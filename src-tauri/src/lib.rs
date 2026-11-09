//! Tauri shell. `win` is the only module allowed to use `unsafe`.
#![deny(unsafe_code)]

mod commands;
mod core;
#[allow(unsafe_code)]
mod win;

use tauri_specta::{Builder, collect_commands};

/// Every IPC command, listed once for both the app and the bindings export.
fn ipc() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new().commands(collect_commands![
        commands::app_info,
        commands::list_programs,
        commands::list_shown,
        commands::list_quarantine,
        commands::restore_batch,
        commands::purge_batch,
        commands::create_restore_point,
        commands::compute_sizes,
        commands::open_install_folder,
        commands::save_export
    ])
}

/// `icon://localhost/<id>` (WebView2: `http://icon.localhost/<id>`): the program's icon
/// as PNG; 404 when it has none.
fn icon_response(path: &str) -> tauri::http::Response<Vec<u8>> {
    let png = core::icons::percent_decode(path.trim_start_matches('/'))
        .and_then(|id| core::icons::png(&id));
    let builder = tauri::http::Response::builder();
    match png {
        Some(body) => builder
            .header("Content-Type", "image/png")
            .header("Cache-Control", "max-age=3600")
            .body(body),
        None => builder.status(404).body(Vec::new()),
    }
    .expect("static response parts are valid")
}

pub fn run() {
    // The elevated broker (ADR 0002) is this same binary: it never starts Tauri.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let [flag, pipe] = args.as_slice()
        && flag == "--broker"
    {
        std::process::exit(core::broker::main(pipe));
    }
    commands::prefetch_programs();
    // Moves a crash interrupted are settled before anything else touches the batches,
    // then batches older than the delay are purged (SECURITY.md 1).
    if let Some(root) = core::quarantine::root() {
        std::thread::spawn(move || {
            core::quarantine::resume_all(&root);
            core::quarantine::purge_expired(
                &root,
                core::quarantine::PURGE_DELAY,
                std::time::SystemTime::now(),
            );
        });
    }
    tauri::Builder::default()
        .invoke_handler(ipc().invoke_handler())
        // Extraction is blocking Win32 work: off the main thread, one task per icon.
        .register_asynchronous_uri_scheme_protocol("icon", |_ctx, request, responder| {
            let path = request.uri().path().to_owned();
            tauri::async_runtime::spawn_blocking(move || responder.respond(icon_response(&path)));
        })
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
