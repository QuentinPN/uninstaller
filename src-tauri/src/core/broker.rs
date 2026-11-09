//! The protocol between the app and its elevated broker (ADR 0002): frames of JSON,
//! each prefixed by its length and carrying the protocol version, with one operation
//! from a closed list. Never a shell command, never a free path.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::hash::{BuildHasher, RandomState};
use std::io::{self, ErrorKind, Read, Write};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use crate::win::{pipe, process};

/// Both ends must speak the same version: the broker is the same binary, so a
/// mismatch means something else is on the pipe.
pub const VERSION: u32 = 1;

/// Largest frame accepted, checked before anything is allocated.
const MAX_FRAME: u32 = 64 * 1024;

/// The broker quits after this long without a request (ADR 0002).
const IDLE: Duration = Duration::from_secs(5 * 60);

/// What the app asks the broker. Each new elevated operation is a new variant.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum Request {
    /// Checks the channel and the version right after connecting.
    Ping,
}

/// The broker's answer to one `Request`.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum Reply {
    Pong,
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
    match run(pipe, &watch) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

/// Connects to the app's pipe `name`, makes sure the app is this same program, then
/// answers until the pipe closes.
fn run(name: &str, watch: &Watch) -> io::Result<()> {
    let mut channel = pipe::connect(name)?;
    if !same_program(pipe::server_pid(&channel)?)? {
        return Err(io::Error::new(
            ErrorKind::PermissionDenied,
            "pipe created by another program",
        ));
    }
    serve(&mut channel, |request| watch.busy(|| answer(request)))
}

/// Whether process `pid` runs this very executable.
fn same_program(pid: u32) -> io::Result<bool> {
    let theirs = std::fs::canonicalize(process::image_path(pid)?)?;
    Ok(theirs == std::fs::canonicalize(std::env::current_exe()?)?)
}

/// Carries out one request, elevated.
fn answer(request: Request) -> Reply {
    match request {
        Request::Ping => Reply::Pong,
    }
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

fn invalid(why: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, why)
}

#[cfg(test)]
mod tests {
    use super::*;
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
            serve(&mut channel, |Request::Ping| Reply::Pong)
        });
        let mut app = pipe::connect(&name).expect("connect");
        for _ in 0..2 {
            send(&mut app, &Request::Ping).expect("send");
            assert_eq!(receive(&mut app).expect("reply"), Some(Reply::Pong));
        }
        drop(app);
        broker.join().expect("join").expect("serve ends cleanly");
    }

    #[test]
    fn broker_answers_this_program_until_the_pipe_closes() {
        let name = pipe_name();
        let server = pipe::Server::create(&name).expect("create");
        let broker = std::thread::spawn(move || run(&name, &Watch::new()));
        let mut app = server.accept().expect("accept");
        send(&mut app, &Request::Ping).expect("send");
        assert_eq!(receive(&mut app).expect("reply"), Some(Reply::Pong));
        drop(app);
        broker.join().expect("join").expect("ends cleanly");
    }

    #[test]
    fn same_program_tells_this_binary_from_another() {
        assert!(same_program(std::process::id()).expect("self"));
        let mut other = std::process::Command::new("ping")
            .args(["-n", "5", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .expect("spawn ping");
        let checked = same_program(other.id());
        let _ = other.kill();
        let _ = other.wait();
        assert!(!checked.expect("ping"));
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
