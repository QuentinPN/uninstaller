//! Shell shortcuts (`.lnk`): what they point to, read without resolving (resolving may
//! search the disk and rewrite the file).

use std::path::{Path, PathBuf};

use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, CoCreateInstance, IPersistFile, STGM_READ,
};
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
use windows::core::{HSTRING, Interface};

use crate::win::com;

/// The file or folder `shortcut` points to, environment variables expanded; `None` if it
/// cannot be read or points to no file (a URL, a shell item).
pub fn target(shortcut: &Path) -> Option<PathBuf> {
    com::prepare();
    // SAFETY: COM is initialised on this thread.
    let link: IShellLinkW =
        unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }.ok()?;
    let file: IPersistFile = link.cast().ok()?;
    // SAFETY: the path outlives the call; read only.
    unsafe { file.Load(&HSTRING::from(shortcut), STGM_READ) }.ok()?;
    let mut buffer = [0_u16; 1024];
    // SAFETY: the buffer's length goes with it; no find data asked for.
    unsafe { link.GetPath(&mut buffer, std::ptr::null_mut(), 0) }.ok()?;
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    (end > 0).then(|| PathBuf::from(String::from_utf16_lossy(&buffer[..end])))
}

/// Writes a shortcut to `target` (tests only: the app never creates shortcuts).
#[cfg(test)]
pub fn create(shortcut: &Path, target: &Path) -> windows::core::Result<()> {
    com::prepare();
    // SAFETY: COM is initialised on this thread.
    let link: IShellLinkW = unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }?;
    // SAFETY: the path outlives the call.
    unsafe { link.SetPath(&HSTRING::from(target)) }?;
    // SAFETY: the path outlives the call.
    unsafe {
        link.cast::<IPersistFile>()?
            .Save(&HSTRING::from(shortcut), true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_target_back() {
        let dir = std::env::temp_dir().join(format!("uninstaller-shortcut-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (lnk, exe) = (dir.join("App.lnk"), dir.join("app.exe"));
        std::fs::write(&exe, b"x").unwrap();
        create(&lnk, &exe).unwrap();
        assert_eq!(target(&lnk), Some(exe));
        assert_eq!(target(&dir.join("missing.lnk")), None);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
