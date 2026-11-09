//! The Windows "Save as" dialog.

use std::ffi::c_void;
use std::path::PathBuf;

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance,
    CoInitializeEx, CoTaskMemFree, CoUninitialize,
};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FOS_FORCEFILESYSTEM, FOS_OVERWRITEPROMPT, FileSaveDialog, IFileSaveDialog, SIGDN_FILESYSPATH,
};
use windows::core::{HSTRING, PCWSTR};

/// What the dialog shows: its title, the suggested file name and the one file type.
pub struct SaveAs<'a> {
    pub title: &'a str,
    pub file_name: &'a str,
    pub type_name: &'a str,
    /// Without the dot, e.g. `csv`.
    pub extension: &'a str,
}

/// Asks the user where to save; `None` when cancelled or when the dialog fails.
/// Blocks until it closes, on a thread of its own (COM needs a single-threaded
/// apartment). `owner` is the window's HWND as an integer, 0 for none.
pub fn ask_save_path(owner: isize, request: SaveAs<'_>) -> Option<PathBuf> {
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                // SAFETY: first COM call on this new thread; balanced below.
                let init = unsafe {
                    CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE)
                };
                if init.is_err() {
                    return None;
                }
                let path = show(owner, &request);
                // SAFETY: balances the successful CoInitializeEx once `show` has
                // released every COM object it created.
                unsafe { CoUninitialize() };
                path
            })
            .join()
            .ok()
            .flatten()
    })
}

fn show(owner: isize, request: &SaveAs<'_>) -> Option<PathBuf> {
    let title = HSTRING::from(request.title);
    let file_name = HSTRING::from(request.file_name);
    let type_name = HSTRING::from(request.type_name);
    let pattern = HSTRING::from(format!("*.{}", request.extension));
    let extension = HSTRING::from(request.extension);
    let types = [COMDLG_FILTERSPEC {
        pszName: PCWSTR(type_name.as_ptr()),
        pszSpec: PCWSTR(pattern.as_ptr()),
    }];
    let owner = (owner != 0).then_some(HWND(owner as *mut c_void));
    // SAFETY: COM is initialized on this thread (`ask_save_path`); every string passed
    // outlives the calls (the dialog copies them); the returned path string is
    // CoTaskMem-allocated and freed once, after being copied.
    unsafe {
        let dialog: IFileSaveDialog =
            CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        dialog.SetTitle(&title).ok()?;
        dialog.SetFileName(&file_name).ok()?;
        dialog.SetFileTypes(&types).ok()?;
        dialog.SetDefaultExtension(&extension).ok()?;
        let options = dialog.GetOptions().ok()?;
        dialog
            .SetOptions(options | FOS_OVERWRITEPROMPT | FOS_FORCEFILESYSTEM)
            .ok()?;
        // Cancelling comes back as an error: nothing to save.
        dialog.Show(owner).ok()?;
        let text = dialog
            .GetResult()
            .ok()?
            .GetDisplayName(SIGDN_FILESYSPATH)
            .ok()?;
        let path = text.to_string().ok();
        CoTaskMemFree(Some(text.0 as *const c_void));
        path.map(PathBuf::from)
    }
}

#[cfg(test)]
mod tests {
    use super::{SaveAs, ask_save_path};
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, IDCANCEL, IDOK, PostMessageW, WM_COMMAND,
    };
    use windows::core::HSTRING;

    /// Opens the real dialog and answers it with a window message (no mouse or
    /// keyboard). It shows on screen, so it only runs on request:
    /// `cargo test -- --ignored dialog`.
    fn answer(title: &str, file_name: &str, button: i32) -> Option<std::path::PathBuf> {
        std::thread::scope(|scope| {
            scope.spawn(|| {
                let title = HSTRING::from(title);
                for _ in 0..100 {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    // SAFETY: plain window lookup and message post; the window may be
                    // gone already, which only makes the calls fail.
                    unsafe {
                        if let Ok(window) = FindWindowW(windows::core::w!("#32770"), &title) {
                            // Created before it is ready: give it time to take the answer.
                            std::thread::sleep(std::time::Duration::from_millis(500));
                            let _ = PostMessageW(
                                Some(window),
                                WM_COMMAND,
                                WPARAM(button as usize),
                                LPARAM(0),
                            );
                            return;
                        }
                    }
                }
            });
            ask_save_path(
                0,
                SaveAs {
                    title,
                    file_name,
                    type_name: "Texte (*.txt)",
                    extension: "txt",
                },
            )
        })
    }

    #[test]
    #[ignore = "shows a window"]
    fn dialog_returns_the_chosen_path_or_none_when_cancelled() {
        let dir = std::env::temp_dir();
        let target = dir.join("uninstaller-dialog-test");
        let chosen = answer("Uninstaller test OK", &target.to_string_lossy(), IDOK.0);
        assert_eq!(chosen, Some(dir.join("uninstaller-dialog-test.txt")));
        assert_eq!(answer("Uninstaller test cancel", "x", IDCANCEL.0), None);
    }
}
