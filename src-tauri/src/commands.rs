//! The only IPC boundary. Every command validates its inputs.

use serde::Serialize;
use specta::Type;
use std::path::PathBuf;
use std::process::Command;

use crate::core::inventory::{self, Program};

/// Facts about the running process, shown in the title bar.
#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    /// Running with an administrator token (ADR 0002: normally false).
    pub elevated: bool,
}

/// Installed programs, sorted by name (F1). Async so the registry walk stays off the
/// main thread.
#[tauri::command]
#[specta::specta]
pub async fn list_programs() -> Vec<Program> {
    inventory::scan()
}

/// Opens a program's install folder in Explorer. Takes the opaque id, never a path:
/// the folder is read back from the registry. False if it no longer exists.
#[tauri::command]
#[specta::specta]
pub async fn open_install_folder(id: String) -> bool {
    let Some(folder) = inventory::find(&id)
        .and_then(|p| p.install_location)
        .map(|l| PathBuf::from(l.trim_matches('"')))
        .filter(|f| f.is_dir())
    else {
        return false;
    };
    // Explorer hands the folder to the running shell and exits at once.
    Command::new("explorer.exe").arg(folder).status().is_ok()
}

#[tauri::command]
#[specta::specta]
pub fn app_info() -> AppInfo {
    AppInfo {
        elevated: crate::win::token::is_elevated(),
    }
}
