//! The app's own files under `%LOCALAPPDATA%\Uninstaller` (ARCHITECTURE.md, Données
//! locales), and how they are written so a crash never leaves half of one.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// The app's folder name, under `%LOCALAPPDATA%` and `%ProgramData%`.
pub const APP_DIR: &str = "Uninstaller";

/// `%LOCALAPPDATA%\Uninstaller`.
pub fn data_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|local| PathBuf::from(local).join(APP_DIR))
}

/// Writes `bytes` and waits until they are on disk.
pub fn write_synced(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// Writes `bytes` in full next to `path`, then renames them over it (ADR 0005): a
/// reader sees the old content or the new, never a mix.
pub fn replace(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut next = path.as_os_str().to_owned();
    next.push(".next");
    write_synced(Path::new(&next), bytes)?;
    fs::rename(next, path)
}
