//! Tauri shell. `win` is the only module allowed to use `unsafe`.
#![deny(unsafe_code)]

mod commands;
#[allow(unsafe_code)]
mod win;

use tauri_specta::{Builder, collect_commands};

/// Every IPC command, listed once for both the app and the bindings export.
fn ipc() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new().commands(collect_commands![commands::app_info])
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(ipc().invoke_handler())
        .run(tauri::generate_context!())
        .expect("Tauri failed to start");
}

#[cfg(test)]
mod tests {
    use super::ipc;
    use specta_typescript::Typescript;
    use std::{fs, path::Path};

    /// Regenerates the bindings and fails if they were stale, so drift never passes `npm run verify`.
    #[test]
    fn bindings_are_up_to_date() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/lib/ipc/bindings.ts");
        let before = fs::read_to_string(&path).unwrap_or_default();
        ipc()
            .export(Typescript::default(), &path)
            .expect("export bindings");
        let after = fs::read_to_string(&path).expect("read bindings");
        assert!(
            before == after,
            "src/lib/ipc/bindings.ts was stale and has been regenerated: rerun the tests"
        );
    }
}
