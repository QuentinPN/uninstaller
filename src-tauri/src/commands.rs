//! The only IPC boundary. Every command validates its inputs.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::io::Write;
use std::process::Command;
use std::sync::Mutex;
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::ipc::Channel;

use crate::core::inventory::{self, Program};
use crate::core::sizes::{self, ProgramSize};
use crate::win::dialog::{self, SaveAs};

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
