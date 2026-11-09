//! The only IPC boundary. Every command validates its inputs.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::io::Write;
use std::process::Command;
use std::sync::{Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::ipc::Channel;

use crate::core::broker::{Client, Reply, Request, RestorePoint, SystemPoint};
use crate::core::inventory::{self, Program};
use crate::core::quarantine::{self, BatchAction, BatchFailure, BatchSummary};
use crate::core::settings::{self, Settings};
use crate::core::sizes::{self, ProgramSize};
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
    let delay = settings::current().purge_delay();
    quarantine::root()
        .map(|root| quarantine::list(&root, delay))
        .unwrap_or_default()
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

/// Runs `action` on the batch the UI names, off the main thread. When Windows denies
/// access (an HKLM key, a file from Program Files), the elevated broker runs it again:
/// items already done stay done and the broker carries on from there.
async fn on_batch(name: String, action: BatchAction) -> Result<(), BatchFailure> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = quarantine::root().ok_or(BatchFailure::Missing)?;
        match action.run(&root, &name) {
            Err(error) if quarantine::needs_admin(&error) => {
                match elevated(&Request::Batch { action, name }) {
                    Ok(Reply::Batch(result)) => result,
                    Ok(_) => Err(BatchFailure::Failed),
                    Err(ElevationFailure::Refused) => Err(BatchFailure::Refused),
                    Err(ElevationFailure::Failed) => Err(BatchFailure::Failed),
                }
            }
            done => Ok(done?),
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
    let mut session = BROKER.lock().unwrap_or_else(PoisonError::into_inner);
    let client = match session.take().filter(Client::is_alive) {
        Some(client) => client,
        None => Client::start()?,
    };
    let reply = session.insert(client).call(request);
    if reply.is_err() {
        *session = None;
    }
    Ok(reply?)
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
