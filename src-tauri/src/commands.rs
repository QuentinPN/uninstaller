//! The only IPC boundary. Every command validates its inputs.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::ipc::Channel;

use crate::core::broker::{Client, Reply, Request, RestorePoint, SystemPoint, Whose};
use crate::core::inventory::{self, Kept, Program};
use crate::core::leftovers::{self, Cleaned, ForceFailure, Found, Held, Pick};
use crate::core::quarantine::{self, BatchAction, BatchFailure, BatchSummary};
use crate::core::settings::{self, Settings};
use crate::core::sizes::{self, ProgramSize};
use crate::core::uninstall::{self, UninstallFailure, Uninstalled};
use crate::win::dialog::{self, SaveAs};
use crate::win::process;

/// Facts about the running process, shown in the title bar.
#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    /// Running with an administrator token (ADR 0002: normally false).
    pub elevated: bool,
}

/// The inventory started at launch, taken by the first `list_programs`.
static PREFETCH: Mutex<Option<JoinHandle<Vec<Program>>>> = Mutex::new(None);

/// Starts reading the inventory while WebView2 starts (≈ 400 ms, docs/perf.md), so
/// the first list is ready when the page asks for it.
pub fn prefetch_programs() {
    if let Ok(mut slot) = PREFETCH.lock() {
        *slot = Some(std::thread::spawn(inventory::scan));
    }
}

/// Installed programs, sorted by name (F1): the prefetched inventory the first time,
/// a fresh read after (retry). Async so the registry walk stays off the main thread.
#[tauri::command]
#[specta::specta]
pub async fn list_programs() -> Vec<Program> {
    let prefetched = PREFETCH.lock().ok().and_then(|mut slot| slot.take());
    prefetched
        .and_then(|scan| scan.join().ok())
        .unwrap_or_else(inventory::scan)
}

/// The quarantine batches, newest first (Backups screen); none if the folder does not
/// exist yet.
#[tauri::command]
#[specta::specta]
pub async fn list_quarantine() -> Vec<BatchSummary> {
    quarantine::list(&quarantine::roots(), settings::current().purge_delay())
}

/// The saved settings, or the defaults.
#[tauri::command]
#[specta::specta]
pub fn get_settings() -> Settings {
    settings::current()
}

/// Saves `settings`; false when a value is not one the screen offers or the file
/// cannot be written.
#[tauri::command]
#[specta::specta]
pub fn save_settings(settings: Settings) -> bool {
    settings.is_valid() && settings::path().is_some_and(|path| settings.save(&path).is_ok())
}

/// Runs `action` on the batch the UI names, off the main thread: the elevated broker
/// on its part in the admin root, then the app on its own part.
async fn on_batch(name: String, action: BatchAction) -> Result<(), BatchFailure> {
    tauri::async_runtime::spawn_blocking(move || {
        let holds = |root: Option<PathBuf>| {
            root.filter(|root| quarantine::batch_dir(root, &name).is_some_and(|dir| dir.exists()))
        };
        let (admin, own) = (holds(quarantine::admin_root()), holds(quarantine::root()));
        if admin.is_none() && own.is_none() {
            return Err(BatchFailure::Missing);
        }
        if admin.is_some() {
            let request = Request::Batch {
                action,
                name: name.clone(),
            };
            match elevated(&request) {
                Ok(Reply::Batch(result)) => result?,
                Ok(_) => return Err(BatchFailure::Failed),
                Err(ElevationFailure::Refused) => return Err(BatchFailure::Refused),
                Err(ElevationFailure::Failed) => return Err(BatchFailure::Failed),
            }
        }
        match own {
            Some(root) => Ok(action.run(&root, &name)?),
            None => Ok(()),
        }
    })
    .await
    .map_err(|_| BatchFailure::Failed)?
}

/// Puts a batch's items back, last moved first (F12); stops at the first that cannot
/// go back, the ones before it staying restored.
#[tauri::command]
#[specta::specta]
pub async fn restore_batch(name: String) -> Result<(), BatchFailure> {
    on_batch(name, BatchAction::Restore).await
}

/// Removes a batch for good, after the UI confirmed it with its count and size
/// (ADR 0003).
#[tauri::command]
#[specta::specta]
pub async fn purge_batch(name: String) -> Result<(), BatchFailure> {
    on_batch(name, BatchAction::Purge).await
}

/// Why an elevated operation did not run.
#[derive(Debug, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ElevationFailure {
    /// The UAC prompt was refused: nothing changed.
    Refused,
    /// The broker did not start or did not answer.
    Failed,
}

impl From<std::io::Error> for ElevationFailure {
    fn from(error: std::io::Error) -> Self {
        if process::is_refused(&error) {
            Self::Refused
        } else {
            Self::Failed
        }
    }
}

/// The broker session: one UAC prompt, then reused until the broker quits when idle.
static BROKER: Mutex<Option<Client>> = Mutex::new(None);

/// Sends `request` to the elevated broker, starting it (UAC prompt) when none runs.
/// A failed call ends the session, so the next request starts a new one.
fn elevated(request: &Request) -> Result<Reply, ElevationFailure> {
    let mut session = broker();
    let client = match session.take().filter(Client::is_alive) {
        Some(client) => client,
        None => Client::start()?,
    };
    call(&mut session, client, request)
}

/// Sends `request` only to a broker already running and idle: never a UAC prompt,
/// never a wait behind another elevated call.
fn running_broker(request: &Request) -> Option<Reply> {
    let mut session = BROKER.try_lock().ok()?;
    let client = session.take().filter(Client::is_alive)?;
    call(&mut session, client, request).ok()
}

/// A failed call ends the session.
fn call(
    session: &mut Option<Client>,
    client: Client,
    request: &Request,
) -> Result<Reply, ElevationFailure> {
    let reply = session.insert(client).call(request);
    if reply.is_err() {
        *session = None;
    }
    Ok(reply?)
}

fn broker() -> std::sync::MutexGuard<'static, Option<Client>> {
    BROKER.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The name Windows shows for points made from the Backups screen.
const RESTORE_POINT_NAME: &str = "Uninstaller : point créé à la demande";

/// Creates a system restore point through the elevated broker (F12, SECURITY.md 4).
#[tauri::command]
#[specta::specta]
pub async fn create_restore_point() -> Result<RestorePoint, ElevationFailure> {
    let request = Request::CreateRestorePoint {
        description: RESTORE_POINT_NAME.into(),
    };
    ask_broker(request, |reply| match reply {
        Reply::RestorePoint(point) => Some(point),
        _ => None,
    })
    .await
}

/// The restore points Windows keeps, newest first. Listing them needs administrator
/// rights, so the UI asks only when the user does.
#[tauri::command]
#[specta::specta]
pub async fn list_restore_points() -> Result<Vec<SystemPoint>, ElevationFailure> {
    ask_broker(Request::ListRestorePoints, |reply| match reply {
        Reply::RestorePoints(points) => points,
        _ => None,
    })
    .await
}

/// Opens Windows' System Restore wizard, which restores a point after its own
/// confirmation.
#[tauri::command]
#[specta::specta]
pub async fn open_system_restore() -> Result<(), ElevationFailure> {
    ask_broker(Request::OpenSystemRestore, |reply| {
        matches!(reply, Reply::Opened(true)).then_some(())
    })
    .await
}

/// Sends `request` to the elevated broker off the main thread; `read` picks the
/// answer out of the reply, `None` being a failure.
async fn ask_broker<T: Send + 'static>(
    request: Request,
    read: fn(Reply) -> Option<T>,
) -> Result<T, ElevationFailure> {
    tauri::async_runtime::spawn_blocking(move || {
        read(elevated(&request)?).ok_or(ElevationFailure::Failed)
    })
    .await
    .map_err(|_| ElevationFailure::Failed)?
}

/// Names the file where `list_shown` notes the time; set only by
/// `scripts/measure-startup.ps1` (docs/perf.md).
const PERF_LOG: &str = "UNINSTALLER_PERF_LOG";

/// The program list's first rows are painted, `page_ms` after the page started
/// loading. When measuring, appends the time (Unix milliseconds) and `page_ms` to the
/// file named by `UNINSTALLER_PERF_LOG`; otherwise does nothing. The path comes from whoever
/// launched the app, never from the page, which only sends a number.
#[tauri::command]
#[specta::specta]
pub fn list_shown(page_ms: u32) {
    let Some(path) = std::env::var_os(PERF_LOG) else {
        return;
    };
    let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return;
    };
    // A measurement aid: a failed write only leaves the run without its figure.
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| writeln!(file, "{} {page_ms}", now.as_millis()));
}

/// Opens a program's install folder in Explorer. Takes the opaque id, never a path:
/// the folder is read back from the registry. False if it no longer exists.
#[tauri::command]
#[specta::specta]
pub async fn open_install_folder(id: String) -> bool {
    let Some(folder) = inventory::find(&id).and_then(|p| p.install_folder()) else {
        return false;
    };
    // Explorer hands the folder to the running shell and exits at once.
    Command::new("explorer.exe").arg(folder).status().is_ok()
}

/// Size on disk of every program with an install folder, sent in batches on `on_sizes`
/// as folders finish (F1). Resolves once all are sent.
#[tauri::command]
#[specta::specta]
pub async fn compute_sizes(on_sizes: Channel<Vec<ProgramSize>>) {
    let programs = inventory::scan();
    // Disk walk: on a blocking thread, not on the async runtime.
    let _ = tauri::async_runtime::spawn_blocking(move || {
        sizes::compute(&programs, |batch| {
            // The window may be gone; nothing to do then.
            let _ = on_sizes.send(batch);
        });
    })
    .await;
}

/// The leftover scan running, if any: `cancel_leftovers` or the next scan stops it.
static LEFTOVER_SCAN: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);

/// Raised to stop waiting for the uninstaller started last.
static UNINSTALL_WAIT: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);

/// The programs whose uninstaller ran, as listed before.
static UNINSTALLED: Kept = Kept::new();

/// The last scan that answered, for `quarantine_leftovers` to read the picks from: the
/// UI sends back places in it, never paths.
static LAST_LEFTOVERS: Mutex<Option<Scanned>> = Mutex::new(None);
static SCAN_SERIAL: AtomicU32 = AtomicU32::new(0);

/// What the last `quarantine_leftovers` left for want of rights, for
/// `quarantine_held_leftovers`.
static HELD_LEFTOVERS: Mutex<Option<Pending>> = Mutex::new(None);

#[derive(Clone, PartialEq)]
struct Pending {
    /// The program's inventory id: the broker scans it again.
    program: String,
    /// The batch the first pass filled, which the held items join.
    batch: Option<String>,
    held: Vec<Held>,
}

struct Scanned {
    serial: u32,
    program: String,
    /// The program's name, which names the quarantine batch.
    label: String,
    found: Found,
}

/// A scan's answer and its number, which `quarantine_leftovers` takes back.
#[derive(Serialize, Type)]
pub struct Scan {
    pub serial: u32,
    pub found: Found,
}

/// Why a leftover scan found nothing.
#[derive(Debug, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum LeftoverFailure {
    /// The id names no installed program.
    NotFound,
    Cancelled,
    /// The scan stopped on an unexpected error.
    Failed,
}

/// Leftovers of the program `id` names (F5), read only, off the async runtime: an
/// installed one, or the one `run_uninstaller` ran last. A new scan stops the one
/// running.
#[tauri::command]
#[specta::specta]
pub async fn scan_leftovers(id: String) -> Result<Scan, LeftoverFailure> {
    let cancelled = fresh_flag(&LEFTOVER_SCAN);
    tauri::async_runtime::spawn_blocking(move || {
        let program = UNINSTALLED.find(&id).ok_or(LeftoverFailure::NotFound)?;
        let all = inventory::scan();
        let found =
            leftovers::find(&program, &all, &cancelled).ok_or(LeftoverFailure::Cancelled)?;
        let mut last = last_leftovers();
        // Replaced by a newer scan while finishing: that one's list is the one shown.
        if cancelled.load(Ordering::Relaxed) {
            return Err(LeftoverFailure::Cancelled);
        }
        let serial = SCAN_SERIAL.fetch_add(1, Ordering::Relaxed) + 1;
        *last = Some(Scanned {
            serial,
            program: id,
            label: program.name,
            found: found.clone(),
        });
        Ok(Scan { serial, found })
    })
    .await
    .unwrap_or(Err(LeftoverFailure::Failed))
}

/// A program no entry lists, from the name or folder the user typed (F6): kept like
/// an uninstalled one, so `scan_leftovers` searches its leftovers.
#[tauri::command]
#[specta::specta]
pub async fn force_unlisted(name: String, folder: String) -> Result<Program, ForceFailure> {
    tauri::async_runtime::spawn_blocking(move || {
        let program = leftovers::unlisted(&name, &folder, &inventory::scan())?;
        UNINSTALLED.keep(program.clone());
        Ok(program)
    })
    .await
    .unwrap_or(Err(ForceFailure::Failed))
}

/// Stops the leftover scan running; its `scan_leftovers` answers `Cancelled`.
#[tauri::command]
#[specta::specta]
pub fn cancel_leftovers() {
    raise_flag(&LEFTOVER_SCAN);
}

/// Runs the uninstaller of the program `id` names (F4, step 2), without windows when
/// `silent` and its installer allows it (F7), and answers once it and all it started
/// have ended, or `stop_uninstaller` was called. Starting another stops the wait for
/// the previous one, which keeps running.
#[tauri::command]
#[specta::specta]
pub async fn run_uninstaller(id: String, silent: bool) -> Result<Uninstalled, UninstallFailure> {
    let stop = fresh_flag(&UNINSTALL_WAIT);
    tauri::async_runtime::spawn_blocking(move || {
        let program = inventory::find(&id).ok_or(UninstallFailure::NotFound)?;
        UNINSTALLED.keep(program.clone());
        // The broker's leftover pass needs the program once its entry is gone. Only a
        // broker already running (after the restore point) is told: no extra prompt.
        running_broker(&Request::Remember {
            program: id.clone(),
        });
        uninstall::run(&program, silent, &stop)
    })
    .await
    .unwrap_or(Err(UninstallFailure::Failed))
}

/// Stops waiting for the uninstaller (one that opened a browser page, say): its
/// `run_uninstaller` answers with `finished` false. The uninstaller is left running.
#[tauri::command]
#[specta::specta]
pub fn stop_uninstaller() {
    raise_flag(&UNINSTALL_WAIT);
}

/// Why picked leftovers were not moved.
#[derive(Debug, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum CleanFailure {
    /// `serial` is not the last scan's, or its items were already sent: scan again.
    Stale,
    /// The quarantine folder could not be created or written.
    Failed,
}

/// Moves the `picks` of the scan `serial` into a new quarantine batch (F5, ADR 0003),
/// off the async runtime. Once per scan: the UI scans again to show what is left.
#[tauri::command]
#[specta::specta]
pub async fn quarantine_leftovers(serial: u32, picks: Vec<Pick>) -> Result<Cleaned, CleanFailure> {
    tauri::async_runtime::spawn_blocking(move || {
        let scanned = take_scan(serial).ok_or(CleanFailure::Stale)?;
        let root = quarantine::root().ok_or(CleanFailure::Failed)?;
        let (cleaned, held) = leftovers::clean(&scanned.found, &picks, &root, &scanned.label)
            .map_err(|_| CleanFailure::Failed)?;
        *held_leftovers() = (!held.is_empty()).then_some(Pending {
            program: scanned.program,
            batch: cleaned.batch.clone(),
            held,
        });
        Ok(cleaned)
    })
    .await
    .unwrap_or(Err(CleanFailure::Failed))
}

/// Moves what the last `quarantine_leftovers` counted `needsAdmin`, through the
/// elevated broker (UAC prompt), which scans the program again and takes only what
/// it finds too. Kept for another try until the broker has answered.
#[tauri::command]
#[specta::specta]
pub async fn quarantine_held_leftovers() -> Result<Cleaned, ElevationFailure> {
    let pending = held_leftovers().clone().ok_or(ElevationFailure::Failed)?;
    let request = Request::Leftovers {
        program: whose(&pending.program).ok_or(ElevationFailure::Failed)?,
        batch: pending.batch.clone(),
        held: pending.held.clone(),
    };
    let cleaned = ask_broker(request, |reply| match reply {
        Reply::Cleaned(cleaned) => cleaned,
        _ => None,
    })
    .await?;
    // A quarantine done meanwhile may have left newer items: keep those.
    let mut held = held_leftovers();
    if held.as_ref() == Some(&pending) {
        *held = None;
    }
    Ok(cleaned)
}

/// The program `id` as the broker is told it: its id, or, for one the user named,
/// what the user typed.
fn whose(id: &str) -> Option<Whose> {
    if !Program::is_unlisted(id) {
        return Some(Whose::Listed(id.to_owned()));
    }
    let program = UNINSTALLED.find(id)?;
    Some(Whose::Unlisted {
        name: program.name,
        folder: program.install_location.unwrap_or_default(),
    })
}

fn held_leftovers() -> std::sync::MutexGuard<'static, Option<Pending>> {
    HELD_LEFTOVERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// The last scan, taken out, if it is the scan `serial`.
fn take_scan(serial: u32) -> Option<Scanned> {
    let mut last = last_leftovers();
    last.as_ref()
        .is_some_and(|s| s.serial == serial)
        .then(|| last.take())?
}

fn last_leftovers() -> std::sync::MutexGuard<'static, Option<Scanned>> {
    LAST_LEFTOVERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

type Flag = Mutex<Option<Arc<AtomicBool>>>;

/// A fresh stop flag for new work in `slot`, the previous work's flag raised.
fn fresh_flag(slot: &Flag) -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    raise(
        slot.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .replace(Arc::clone(&flag)),
    );
    flag
}

/// Raises and forgets the flag of the work running in `slot`.
fn raise_flag(slot: &Flag) {
    raise(slot.lock().unwrap_or_else(PoisonError::into_inner).take());
}

fn raise(flag: Option<Arc<AtomicBool>>) {
    if let Some(flag) = flag {
        flag.store(true, Ordering::Relaxed);
    }
}

/// File types of a list export (F19).
#[derive(Clone, Copy, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ExportFormat {
    Html,
    Csv,
    Txt,
}

impl ExportFormat {
    fn extension(self) -> &'static str {
        match self {
            ExportFormat::Html => "html",
            ExportFormat::Csv => "csv",
            ExportFormat::Txt => "txt",
        }
    }
}

/// An export built by the UI (it has the labels, the order and the measured sizes),
/// with the texts of the save dialog.
#[derive(Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Export {
    pub format: ExportFormat,
    pub content: String,
    pub dialog_title: String,
    pub file_name: String,
    pub type_name: String,
}

/// Asks where to save the export, then writes it. The path only ever comes from the
/// system dialog, never from the UI. `Ok(None)` when the user cancels.
#[tauri::command]
#[specta::specta]
pub async fn save_export(
    window: tauri::WebviewWindow,
    request: Export,
) -> Result<Option<String>, String> {
    let owner = window.hwnd().map_or(0, |hwnd| hwnd.0 as isize);
    tauri::async_runtime::spawn_blocking(move || {
        let save_as = SaveAs {
            title: &request.dialog_title,
            file_name: &request.file_name,
            type_name: &request.type_name,
            extension: request.format.extension(),
        };
        let Some(path) = dialog::ask_save_path(owner, save_as) else {
            return Ok(None);
        };
        std::fs::write(&path, request.content).map_err(|e| e.to_string())?;
        Ok(Some(path.display().to_string()))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
#[specta::specta]
pub fn app_info() -> AppInfo {
    AppInfo {
        elevated: crate::win::token::is_elevated(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No broker runs in the unit tests: `running_broker` must not start one.
    #[test]
    fn only_a_running_broker_is_told() {
        assert!(running_broker(&Request::Ping).is_none());
        assert!(broker().is_none());
    }

    #[test]
    fn new_work_or_a_stop_raises_the_running_one_s_flag() {
        let slot = Flag::default();
        let first = fresh_flag(&slot);
        let second = fresh_flag(&slot);
        assert!(first.load(Ordering::Relaxed));
        assert!(!second.load(Ordering::Relaxed));
        raise_flag(&slot);
        assert!(second.load(Ordering::Relaxed));
        assert!(slot.lock().unwrap().is_none());
    }

    #[test]
    fn picks_are_taken_once_from_the_last_scan_only() {
        let found = || Found {
            folders: Vec::new(),
            shortcuts: Vec::new(),
            keys: Vec::new(),
            services: Vec::new(),
            tasks: Vec::new(),
        };
        *last_leftovers() = Some(Scanned {
            serial: 7,
            program: "id".into(),
            label: "Widget".into(),
            found: found(),
        });
        assert!(take_scan(6).is_none());
        assert!(take_scan(7).is_some());
        assert!(take_scan(7).is_none());
    }
}
