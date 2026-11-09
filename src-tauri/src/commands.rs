//! The only IPC boundary. Every command validates its inputs.

use serde::Serialize;
use specta::Type;

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

#[tauri::command]
#[specta::specta]
pub fn app_info() -> AppInfo {
    AppInfo {
        elevated: crate::win::token::is_elevated(),
    }
}
