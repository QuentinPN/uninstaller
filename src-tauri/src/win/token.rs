//! Access-token queries for the current process.

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// Whether the current process runs elevated. Any failure reads as "not elevated":
/// callers then go through the broker, which is the safe side.
pub fn is_elevated() -> bool {
    let mut token = HANDLE::default();
    // SAFETY: GetCurrentProcess returns a pseudo-handle that needs no closing;
    // `token` is a valid out-pointer for the duration of the call.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }.is_err() {
        return false;
    }
    let mut elevation = TOKEN_ELEVATION::default();
    let mut returned = 0u32;
    // SAFETY: `token` was opened with TOKEN_QUERY; the buffer is a live TOKEN_ELEVATION
    // and its exact size is passed, as TokenElevation requires.
    let queried = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            Some(std::ptr::from_mut(&mut elevation).cast()),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        )
    };
    // SAFETY: `token` is owned by this function and closed exactly once.
    let _ = unsafe { CloseHandle(token) };
    queried.is_ok() && elevation.TokenIsElevated != 0
}

#[cfg(test)]
mod tests {
    use super::is_elevated;
    use std::process::Command;

    /// Cross-checks with Windows itself: an elevated token carries the
    /// "High Mandatory Level" label (SID S-1-16-12288).
    #[test]
    fn matches_whoami_integrity_level() {
        let out = Command::new("whoami")
            .arg("/groups")
            .output()
            .expect("run whoami");
        let high = String::from_utf8_lossy(&out.stdout).contains("S-1-16-12288");
        assert_eq!(is_elevated(), high);
    }
}
