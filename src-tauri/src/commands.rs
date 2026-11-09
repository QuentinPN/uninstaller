//! The only IPC boundary. Every command validates its inputs.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::io::Write;
use std::process::Command;
use std::sync::{Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::ipc::Channel;

use crate::core::broker::{Client, Reply, Request, RestorePoint};
use crate::core::inventory::{self, Program};
use crate::core::quarantine::{self, Batch, BatchSummary};
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
    quarantine::root()
        .map(|root| quarantine::list(&root))
        .unwrap_or_default()
}

/// Why a batch action stopped, for the Backups screen to word.
#[derive(Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum BatchFailure {
    /// The batch is gone (purged meanwhile) or its name is not a batch's.
    Missing,
    /// An item's origin exists again: nothing was overwritten.
    Exists,
    /// A kept item changed since it was moved.
    Altered,
    /// The manifest is damaged.
    Corrupt,
    /// The batch is being purged.
    Purging,
    /// Any other read or write error.
    Failed,
}

impl From<quarantine::Error> for BatchFailure {
    fn from(error: quarantine::Error) -> Self {
        use quarantine::Error;
        match error {
            Error::Exists => Self::Exists,
            Error::Altered => Self::Altered,
            Error::Corrupt => Self::Corrupt,
            Error::Purging => Self::Purging,
            Error::Io(e) if e.kind() == std::io::ErrorKind::NotFound => Self::Missing,
            _ => Self::Failed,
        }
    }
}

/// Opens the batch the UI names and runs `action` on it, off the main thread.
async fn on_batch(
    name: String,
    action: impl FnOnce(Batch) -> Result<(), quarantine::Error> + Send + 'static,
) -> Result<(), BatchFailure> {
    let dir = quarantine::root()
        .and_then(|root| quarantine::batch_dir(&root, &name))
        .ok_or(BatchFailure::Missing)?;
    tauri::async_runtime::spawn_blocking(move || Ok(action(Batch::open(&dir)?)?))
        .await
        .map_err(|_| BatchFailure::Failed)?
}

/// Puts a batch's items back, last moved first (F12); stops at the first that cannot
/// go back, the ones before it staying restored.
#[tauri::command]
#[specta::specta]
pub async fn restore_batch(name: String) -> Result<(), BatchFailure> {
    on_batch(name, |mut batch| batch.restore()).await
}

/// Removes a batch for good, after the UI confirmed it with its count and size
/// (ADR 0003).
#[tauri::command]
#[specta::specta]
pub async fn purge_batch(name: String) -> Result<(), BatchFailure> {
    on_batch(name, Batch::purge).await
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
    tauri::async_runtime::spawn_blocking(|| {
        let request = Request::CreateRestorePoint {
            description: RESTORE_POINT_NAME.into(),
        };
        match elevated(&request)? {
            Reply::RestorePoint(point) => Ok(point),
            Reply::Pong { .. } => Err(ElevationFailure::Failed),
        }
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
