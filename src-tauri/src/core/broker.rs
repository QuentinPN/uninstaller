//! The protocol between the app and its elevated broker (ADR 0002): frames of JSON,
//! each prefixed by its length and carrying the protocol version, with one operation
//! from a closed list. Never a shell command, never a free path.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::fs::File;
use std::hash::{BuildHasher, RandomState};
use std::io::{self, ErrorKind, Read, Write};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use crate::core::inventory::{self, Kept};
use crate::core::leftovers::{self, Cleaned, Held};
use crate::core::quarantine::{self, BatchAction, BatchFailure};
use crate::core::registry::paths::SYSTEM_RESTORE;
use crate::core::settings;
use crate::win::process::{self, Process};
use crate::win::registry::{Hive, Key, View};
use crate::win::{pipe, restore, token};

/// Both ends must speak the same version: the broker is the same binary, so a
/// mismatch means something else is on the pipe.
pub const VERSION: u32 = 1;

/// Largest frame accepted, checked before anything is allocated. Room for a
/// `Request::Leftovers` of a few thousand long paths.
const MAX_FRAME: u32 = 1024 * 1024;

/// The broker quits after this long without a request (ADR 0002).
const IDLE: Duration = Duration::from_secs(5 * 60);

/// The programs `Request::Remember` named, as the broker's own inventory listed them.
static REMEMBERED: Kept = Kept::new();

/// What the app asks the broker. Each new elevated operation is a new variant.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum Request {
    /// Checks the channel and the version right after connecting.
    Ping,
    /// A system restore point named `description`, before a change (SECURITY.md 4).
    CreateRestorePoint { description: String },
    /// Restores or purges the part of the batch `name` in the admin root. A name,
    /// never a path: the broker finds the batch itself.
    Batch { action: BatchAction, name: String },
    /// The restore points Windows keeps, for the Backups timeline.
    ListRestorePoints,
    /// Opens Windows' System Restore wizard, where the user picks a point.
    OpenSystemRestore,
    /// Moves the leftovers of `program` (an inventory id) that the app was denied. The
    /// broker scans the program again and takes only the `held` items it finds too.
    Leftovers {
        program: String,
        /// The first pass's batch, which these items join while it takes items.
        batch: Option<String>,
        held: Vec<Held>,
    },
    /// Keeps `program` (an inventory id) as the broker's own inventory lists it, before
    /// its uninstaller removes the entry: `Leftovers` still finds it afterwards. Never
    /// a program description from the app.
    Remember { program: String },
}

/// The broker's answer to one `Request`.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum Reply {
    /// Whether the broker really runs with administrator rights.
    Pong {
        elevated: bool,
    },
    RestorePoint(RestorePoint),
    Batch(Result<(), BatchFailure>),
    /// Newest first; `None` if Windows could not list them.
    RestorePoints(Option<Vec<SystemPoint>>),
    Opened(bool),
    /// `None` if the program or the quarantine folder was not found.
    Cleaned(Option<Cleaned>),
    /// Whether the program was listed, so kept.
    Remembered(bool),
}

/// A restore point in the Backups timeline.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SystemPoint {
    pub sequence: u32,
    pub description: String,
    /// Seconds since the Unix epoch.
    pub created: u32,
}

/// At most this many points in a reply, so it stays well under `MAX_FRAME`; Windows
/// rarely keeps more.
const MAX_POINTS: usize = 32;

/// What became of a `CreateRestorePoint`, also what the Backups screen shows.
#[derive(Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum RestorePoint {
    Created,
    /// Windows created nothing: another point is more recent than its minimum
    /// interval (`SystemRestorePointCreationFrequency`, 24 h by default).
    Skipped,
    /// System Restore is turned off.
    Disabled,
    Failed,
}

#[derive(Serialize, Deserialize)]
struct Frame<T> {
    version: u32,
    body: T,
}

/// Read first, so a frame from another version is refused by its number rather than
/// by a body it cannot parse.
#[derive(Deserialize)]
struct Version {
    version: u32,
}

/// A fresh pipe name that another process cannot guess.
pub fn pipe_name() -> String {
    let a = RandomState::new().hash_one(std::process::id());
    let b = RandomState::new().hash_one(a);
    format!("uninstaller-broker-{a:016x}{b:016x}")
}

pub fn send(to: &mut impl Write, body: &impl Serialize) -> io::Result<()> {
    let bytes = serde_json::to_vec(&Frame {
        version: VERSION,
        body,
    })?;
    let len = u32::try_from(bytes.len())
        .ok()
        .filter(|len| *len <= MAX_FRAME)
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "frame too large"))?;
    to.write_all(&len.to_le_bytes())?;
    to.write_all(&bytes)?;
    to.flush()
}

/// The next frame's body, or `None` when the other end closed between frames.
pub fn receive<T: DeserializeOwned>(from: &mut impl Read) -> io::Result<Option<T>> {
    let mut len = [0; 4];
    match from.read_exact(&mut len) {
        Err(e) if e.kind() == ErrorKind::UnexpectedEof => return Ok(None),
        read => read?,
    }
    let len = u32::from_le_bytes(len);
    if len > MAX_FRAME {
        return Err(invalid("frame too large"));
    }
    let mut bytes = vec![0; len as usize];
    from.read_exact(&mut bytes)?;
    let Version { version } = serde_json::from_slice(&bytes)?;
    if version != VERSION {
        return Err(invalid("protocol version mismatch"));
    }
    let frame: Frame<T> = serde_json::from_slice(&bytes)?;
    Ok(Some(frame.body))
}

/// Answers requests on `channel` until the other end closes it.
pub fn serve(
    channel: &mut (impl Read + Write),
    mut handle: impl FnMut(Request) -> Reply,
) -> io::Result<()> {
    while let Some(request) = receive(channel)? {
        send(channel, &handle(request))?;
    }
    Ok(())
}

/// The broker mode of the binary (`--broker <pipe>`): no window; quits when the app
/// closes the pipe or after `IDLE` without a request. Returns the exit code.
pub fn main(pipe: &str) -> i32 {
    let watch = Arc::new(Watch::new());
    let watched = Arc::clone(&watch);
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(10));
            if watched.idle() >= IDLE {
                std::process::exit(0);
            }
        }
    });
    match process::parent_id().and_then(|app| run(pipe, app, &watch)) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

/// Connects to the pipe `name`, makes sure process `app` created it, then answers
/// until the pipe closes.
fn run(name: &str, app: u32, watch: &Watch) -> io::Result<()> {
    let mut channel = pipe::connect(name)?;
    if pipe::server_pid(&channel)? != app {
        return Err(refused("pipe created by another process"));
    }
    // The app cannot settle the admin root: the broker does before any request.
    if let Some(root) = quarantine::admin_root()
        .filter(|root| root.exists())
        .and_then(|_| quarantine::secure_admin_root())
    {
        quarantine::settle(&root, settings::current().purge_delay());
    }
    serve(&mut channel, |request| watch.busy(|| answer(request)))
}

/// Carries out one request, elevated.
fn answer(request: Request) -> Reply {
    match request {
        Request::Ping => Reply::Pong {
            elevated: token::is_elevated(),
        },
        Request::CreateRestorePoint { description } => {
            Reply::RestorePoint(restore_point(&description))
        }
        Request::Batch { action, name } => Reply::Batch(
            quarantine::secure_admin_root()
                .ok_or(BatchFailure::Missing)
                .and_then(|root| Ok(action.run(&root, &name)?)),
        ),
        Request::ListRestorePoints => Reply::RestorePoints(restore_points()),
        Request::OpenSystemRestore => Reply::Opened(open_system_restore()),
        Request::Leftovers {
            program,
            batch,
            held,
        } => Reply::Cleaned(clean_held(&program, batch.as_deref(), &held)),
        Request::Remember { program } => Reply::Remembered(remember(&program)),
    }
}

/// Keeps the program `id` as listed now, if it is.
fn remember(id: &str) -> bool {
    inventory::find(id)
        .map(|program| REMEMBERED.keep(program))
        .is_some()
}

/// Scans the program `id` again, elevated, and moves the `held` items it lists too
/// into `batch`.
fn clean_held(id: &str, batch: Option<&str>, held: &[Held]) -> Option<Cleaned> {
    let program = REMEMBERED.find(id)?;
    let found = leftovers::find(&program, &inventory::scan(), &AtomicBool::new(false))?;
    let root = quarantine::secure_admin_root()?;
    leftovers::clean_held(&found, held, &root, batch, &program.name).ok()
}

/// The points WMI lists, newest first; a point whose date cannot be read is left out.
fn restore_points() -> Option<Vec<SystemPoint>> {
    let mut points: Vec<_> = restore::list()
        .ok()?
        .into_iter()
        .filter_map(|point| {
            Some(SystemPoint {
                created: u32::try_from(wmi_time(&point.created)?).ok()?,
                sequence: point.sequence,
                description: point.description,
            })
        })
        .collect();
    points.sort_by_key(|point| std::cmp::Reverse(point.sequence));
    points.truncate(MAX_POINTS);
    Some(points)
}

/// WMI's `yyyymmddHHMMSS.ffffff±UUU` (offset in minutes) to seconds since the Unix
/// epoch.
fn wmi_time(text: &str) -> Option<u64> {
    let (b, sign) = (text.as_bytes(), text.get(21..22)?);
    if text.len() != 25 || b[14] != b'.' || !matches!(sign, "+" | "-") {
        return None;
    }
    let number = |range: std::ops::Range<usize>| -> Option<i64> {
        let part = text.get(range)?;
        part.bytes()
            .all(|c| c.is_ascii_digit())
            .then(|| part.parse().ok())?
    };
    let (year, month, day) = (number(0..4)?, number(4..6)?, number(6..8)?);
    let (hour, minute, second) = (number(8..10)?, number(10..12)?, number(12..14)?);
    let offset = number(22..25)? * if sign == "-" { -1 } else { 1 };
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    // Days since 1970-01-01 from a civil date (Howard Hinnant's algorithm).
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let local = days * 86_400 + hour * 3_600 + minute * 60 + second;
    u64::try_from(local - offset * 60).ok()
}

/// Starts `rstrui.exe`; it runs elevated as the broker does, so no second prompt.
fn open_system_restore() -> bool {
    std::env::var_os("SystemRoot").is_some_and(|windows| {
        std::process::Command::new(std::path::Path::new(&windows).join(r"System32\rstrui.exe"))
            .spawn()
            .is_ok()
    })
}

/// Creates the point, telling a skipped one by the last point's number, which Windows
/// raises with each point it really creates.
fn restore_point(description: &str) -> RestorePoint {
    if description.chars().any(char::is_control) {
        return RestorePoint::Failed;
    }
    let before = last_restore_index();
    match restore::create(description) {
        Ok(_) if before.is_some() && last_restore_index() == before => RestorePoint::Skipped,
        Ok(_) => RestorePoint::Created,
        Err(e) if restore::is_disabled(&e) => RestorePoint::Disabled,
        Err(_) => RestorePoint::Failed,
    }
}

fn last_restore_index() -> Option<u32> {
    Key::open(Hive::LocalMachine, SYSTEM_RESTORE, View::Native64)?.dword("LastIndex")
}

/// When the broker last finished a request; `None` while one runs, so it never quits
/// in the middle of one.
struct Watch(Mutex<Option<Instant>>);

impl Watch {
    fn new() -> Self {
        Self(Mutex::new(Some(Instant::now())))
    }

    fn set(&self, last: Option<Instant>) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = last;
    }

    fn busy<R>(&self, work: impl FnOnce() -> R) -> R {
        self.set(None);
        let result = work();
        self.set(Some(Instant::now()));
        result
    }

    /// How long nothing has run; zero while a request runs.
    fn idle(&self) -> Duration {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .map_or(Duration::ZERO, |last| last.elapsed())
    }
}

/// The app's end of a broker session.
pub struct Client {
    channel: File,
    broker: Process,
}

impl Client {
    /// Starts the broker as administrator (one UAC prompt per session) and connects.
    pub fn start() -> io::Result<Self> {
        let exe = std::env::current_exe()?;
        Self::launch(|name| process::run_as_admin(&exe, &format!("--broker {name}")))
    }

    /// Creates the pipe, has `start` launch the broker on it, then accepts that
    /// process only.
    fn launch(start: impl FnOnce(&str) -> io::Result<Process>) -> io::Result<Self> {
        let name = pipe_name();
        let server = pipe::Server::create(&name)?;
        let broker = start(&name)?;
        // A broker that exits before connecting would leave `accept` waiting: its
        // exit connects in its place, and the id check below refuses that.
        let exited = broker.try_clone()?;
        std::thread::spawn(move || {
            exited.wait();
            let _ = pipe::connect(&name);
        });
        let channel = server.accept()?;
        if pipe::client_pid(&channel)? != broker.id() {
            return Err(refused("pipe opened by another process"));
        }
        Ok(Self { channel, broker })
    }

    /// False once the broker has quit, after its idle time or a crash.
    pub fn is_alive(&self) -> bool {
        !self.broker.has_exited()
    }

    pub fn call(&mut self, request: &Request) -> io::Result<Reply> {
        send(&mut self.channel, request)?;
        receive(&mut self.channel)?
            .ok_or_else(|| io::Error::new(ErrorKind::BrokenPipe, "the broker has quit"))
    }
}

fn refused(why: &str) -> io::Error {
    io::Error::new(ErrorKind::PermissionDenied, why)
}

fn invalid(why: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, why)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wmi_times_read_as_utc() {
        assert_eq!(wmi_time("19700101000000.000000+000"), Some(0));
        assert_eq!(wmi_time("20240229123000.000000-000"), Some(1_709_209_800));
        // Two hours ahead of UTC: the same instant two hours earlier on the clock.
        assert_eq!(wmi_time("20240229143000.000000+120"), Some(1_709_209_800));
        for bad in [
            "",
            "20241301000000.000000-000",
            "2024022912300.0000000-000",
            "20240229123000.000000*000",
            "20240229123000.000000-0a0",
        ] {
            assert_eq!(wmi_time(bad), None, "{bad}");
        }
    }
    use std::io::Cursor;

    fn frame(json: &str) -> Vec<u8> {
        let mut bytes = (json.len() as u32).to_le_bytes().to_vec();
        bytes.extend_from_slice(json.as_bytes());
        bytes
    }

    #[test]
    fn frames_round_trip_and_end_cleanly() {
        let mut bytes = Vec::new();
        send(&mut bytes, &Request::Ping).expect("send");
        assert_eq!(bytes[4..], *br#"{"version":1,"body":"Ping"}"#);
        let mut from = Cursor::new(bytes);
        assert_eq!(receive(&mut from).expect("ping"), Some(Request::Ping));
        assert_eq!(receive::<Request>(&mut from).expect("end"), None);
    }

    #[test]
    fn many_held_leftovers_fit_in_a_frame() {
        let held: Vec<Held> = (0..2000)
            .map(|i| {
                Held::Path(format!(r"C:\Program Files\{}\{i}", "Vendor Name ".repeat(12)).into())
            })
            .chain((0..1000).map(|i| {
                Held::Key(format!(
                    r"HKEY_LOCAL_MACHINE\SOFTWARE\{}\{i}",
                    "Vendor ".repeat(12)
                ))
            }))
            .chain((0..100).map(|i| Held::Task(format!(r"\Vendor\Update {i}"))))
            .chain((0..100).map(|i| Held::Service(format!("VendorUpdate{i}"))))
            .collect();
        let request = Request::Leftovers {
            program: "id".into(),
            batch: Some("Widget".into()),
            held,
        };
        let mut bytes = Vec::new();
        send(&mut bytes, &request).expect("send");
        assert_eq!(
            receive(&mut Cursor::new(bytes)).expect("receive"),
            Some(request)
        );
        let reply = Reply::Cleaned(Some(Cleaned {
            batch: Some("Widget".into()),
            moved: 2,
            needs_admin: 0,
            failed: 1,
        }));
        let mut bytes = Vec::new();
        send(&mut bytes, &reply).expect("send");
        assert_eq!(
            receive(&mut Cursor::new(bytes)).expect("receive"),
            Some(reply)
        );
    }

    #[test]
    fn refuses_other_versions_unknown_operations_and_huge_frames() {
        let refused = |bytes: Vec<u8>| receive::<Request>(&mut Cursor::new(bytes)).is_err();
        assert!(refused(frame(r#"{"version":2,"body":"Ping"}"#)));
        assert!(refused(frame(r#"{"version":2,"body":{"Later":1}}"#)));
        assert!(refused(frame(r#"{"version":1,"body":"Format"}"#)));
        assert!(refused(frame(r#"{"body":"Ping"}"#)));
        // Only the length is read: nothing that size is allocated.
        assert!(refused(u32::MAX.to_le_bytes().to_vec()));
        // A frame cut short is an error, not a clean end.
        assert!(refused(
            frame(r#"{"version":1,"body":"Ping"}"#)[..10].to_vec()
        ));
    }

    #[test]
    fn an_unlisted_program_is_not_remembered() {
        assert_eq!(
            answer(Request::Remember {
                program: "unknown".into()
            }),
            Reply::Remembered(false)
        );
        assert!(REMEMBERED.find("unknown").is_none());
    }

    #[test]
    fn pipe_names_differ() {
        assert_ne!(pipe_name(), pipe_name());
        assert!(pipe_name().starts_with("uninstaller-broker-"));
    }

    /// Both ends over a real pipe, without elevation: the broker side runs on a thread.
    #[test]
    fn serves_over_a_named_pipe_until_closed() {
        let name = pipe_name();
        let server = pipe::Server::create(&name).expect("create");
        let broker = std::thread::spawn(move || {
            let mut channel = server.accept().expect("accept");
            serve(&mut channel, |_| Reply::Pong { elevated: false })
        });
        let mut app = pipe::connect(&name).expect("connect");
        for _ in 0..2 {
            send(&mut app, &Request::Ping).expect("send");
            assert_eq!(
                receive(&mut app).expect("reply"),
                Some(Reply::Pong { elevated: false })
            );
        }
        drop(app);
        broker.join().expect("join").expect("serve ends cleanly");
    }

    /// An in-process broker on a thread stands for the elevated one.
    fn in_process(name: &str) -> io::Result<Process> {
        let name = name.to_owned();
        std::thread::spawn(move || run(&name, std::process::id(), &Watch::new()));
        Process::current()
    }

    fn child(args: &[&str]) -> io::Result<Process> {
        let (program, args) = args.split_first().expect("program");
        let child = std::process::Command::new(program)
            .args(args)
            .stdout(std::process::Stdio::null())
            .spawn()?;
        Ok(child.into())
    }

    #[test]
    fn client_talks_to_its_broker() {
        let mut client = Client::launch(in_process).expect("launch");
        assert!(client.is_alive());
        for _ in 0..2 {
            assert_eq!(
                client.call(&Request::Ping).expect("call"),
                Reply::Pong {
                    elevated: token::is_elevated()
                }
            );
        }
    }

    #[test]
    fn client_refuses_another_process_on_the_pipe() {
        let mut stranger = None;
        let launched = Client::launch(|name| {
            stranger = Some(pipe::connect(name)?);
            child(&["ping", "-n", "5", "127.0.0.1"])
        });
        assert!(stranger.is_some());
        assert_eq!(
            launched.err().map(|e| e.kind()),
            Some(ErrorKind::PermissionDenied)
        );
    }

    /// Refused one way or another (its stand-in may close first), but never waits.
    #[test]
    fn client_does_not_wait_for_a_broker_that_died() {
        assert!(Client::launch(|_| child(&["cmd", "/c", "exit 1"])).is_err());
    }

    #[test]
    fn broker_refuses_a_pipe_from_another_process() {
        let name = pipe_name();
        let server = pipe::Server::create(&name).expect("create");
        let broker = std::thread::spawn(move || run(&name, 4, &Watch::new()));
        let _app = server.accept().expect("accept");
        let refused = broker.join().expect("join");
        assert_eq!(
            refused.err().map(|e| e.kind()),
            Some(ErrorKind::PermissionDenied)
        );
    }

    /// The real elevated broker, `uninstaller.exe` next to the test binary: shows
    /// the UAC prompt, so run by hand after `cargo build`.
    fn launch_app() -> io::Result<Client> {
        let exe = std::env::current_exe()?;
        let app = exe
            .parent()
            .and_then(|deps| deps.parent())
            .expect("target dir")
            .join("uninstaller.exe");
        Client::launch(|name| process::run_as_admin(&app, &format!("--broker {name}")))
    }

    /// Accept the prompt.
    fn elevated_client() -> Client {
        launch_app().expect("launch")
    }

    #[test]
    #[ignore = "administrateur : affiche l'invite UAC"]
    fn elevated_broker_answers() {
        assert_eq!(
            elevated_client().call(&Request::Ping).expect("call"),
            Reply::Pong { elevated: true }
        );
    }

    /// Changes nothing: the batch does not exist. Accept the prompt.
    #[test]
    #[ignore = "administrateur : affiche l'invite UAC"]
    fn elevated_broker_runs_batch_actions() {
        let request = Request::Batch {
            action: BatchAction::Restore,
            name: "uninstaller-lot-absent".into(),
        };
        assert_eq!(
            elevated_client().call(&request).expect("call"),
            Reply::Batch(Err(BatchFailure::Missing))
        );
    }

    /// Read only: lists the points and checks they come newest first. Accept the prompt.
    #[test]
    #[ignore = "administrateur : affiche l'invite UAC"]
    fn elevated_broker_lists_restore_points() {
        let Reply::RestorePoints(Some(points)) = elevated_client()
            .call(&Request::ListRestorePoints)
            .expect("call")
        else {
            panic!("no list");
        };
        assert!(points.windows(2).all(|p| p[0].sequence > p[1].sequence));
        assert!(points.iter().all(|p| p.created > 1_600_000_000));
        println!("{points:#?}");
    }

    /// Refuse the UAC prompt (« Non »): the app must tell it from other failures.
    #[test]
    #[ignore = "administrateur : refuser l'invite UAC"]
    fn refused_prompt_is_reported() {
        let refused = launch_app()
            .err()
            .expect("the prompt was accepted, not refused");
        assert!(process::is_refused(&refused), "{refused:?}");
    }

    /// Creates a real restore point on this PC: only with the user's consent.
    #[test]
    #[ignore = "administrateur : crée un vrai point de restauration"]
    fn elevated_broker_creates_a_restore_point() {
        let before = last_restore_index();
        let request = Request::CreateRestorePoint {
            description: "Uninstaller : essai du broker".into(),
        };
        assert_eq!(
            elevated_client().call(&request).expect("call"),
            Reply::RestorePoint(RestorePoint::Created)
        );
        assert_ne!(last_restore_index(), before);
    }

    #[test]
    fn control_characters_never_reach_windows() {
        assert_eq!(restore_point("a\nb"), RestorePoint::Failed);
    }

    #[test]
    fn idle_time_stops_while_a_request_runs() {
        let watch = Watch::new();
        std::thread::sleep(Duration::from_millis(20));
        assert!(watch.idle() >= Duration::from_millis(20));
        watch.busy(|| assert_eq!(watch.idle(), Duration::ZERO));
        assert!(watch.idle() < Duration::from_millis(20));
    }
}
