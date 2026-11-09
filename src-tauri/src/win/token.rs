//! Access-token queries for the current process.

use windows::Win32::Foundation::{CloseHandle, HANDLE, HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows::Win32::Security::{
    GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TOKEN_USER, TokenElevation, TokenUser,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::core::PWSTR;

/// Whether the current process runs elevated. Any failure reads as "not elevated":
/// callers then go through the broker, which is the safe side.
pub fn is_elevated() -> bool {
    with_token(|token| {
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
        queried.is_ok() && elevation.TokenIsElevated != 0
    })
    .unwrap_or(false)
}

/// The SID of the account running this process, as text (`S-1-5-21-…`).
pub fn user_sid() -> Option<String> {
    with_token(|token| {
        let mut size = 0u32;
        // SAFETY: without a buffer the call only reports the size it needs; the expected
        // "insufficient buffer" error is ignored.
        let _ = unsafe { GetTokenInformation(token, TokenUser, None, 0, &mut size) };
        // u64 items: aligned for TOKEN_USER and the SID that follows it.
        let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
        // SAFETY: `buffer` holds at least `size` writable bytes.
        unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                Some(buffer.as_mut_ptr().cast()),
                size,
                &mut size,
            )
        }
        .ok()?;
        // SAFETY: on success the buffer starts with a TOKEN_USER whose SID points inside
        // the buffer, which outlives every use of it below.
        let sid = unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid };
        let mut text = PWSTR::null();
        // SAFETY: `sid` is valid (above); `text` receives a LocalAlloc'd string.
        unsafe { ConvertSidToStringSidW(sid, &mut text) }.ok()?;
        // SAFETY: `text` is the NUL-terminated string just returned.
        let value = unsafe { text.to_string() }.ok();
        // SAFETY: `text` was allocated by ConvertSidToStringSidW and is freed once, here.
        unsafe { LocalFree(Some(HLOCAL(text.0.cast()))) };
        value
    })
    .flatten()
}

/// Runs `query` on this process's token, opened for querying and closed afterwards.
fn with_token<R>(query: impl FnOnce(HANDLE) -> R) -> Option<R> {
    let mut token = HANDLE::default();
    // SAFETY: GetCurrentProcess returns a pseudo-handle that needs no closing;
    // `token` is a valid out-pointer for the duration of the call.
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }.ok()?;
    let result = query(token);
    // SAFETY: `token` is owned by this function and closed exactly once.
    let _ = unsafe { CloseHandle(token) };
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::{is_elevated, user_sid};
    use std::process::Command;

    fn whoami(arg: &str) -> String {
        let out = Command::new("whoami")
            .arg(arg)
            .output()
            .expect("run whoami");
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// Cross-checks with Windows itself: an elevated token carries the
    /// "High Mandatory Level" label (SID S-1-16-12288).
    #[test]
    fn matches_whoami_integrity_level() {
        assert_eq!(is_elevated(), whoami("/groups").contains("S-1-16-12288"));
    }

    #[test]
    fn user_sid_matches_whoami() {
        let sid = user_sid().expect("sid");
        assert!(sid.starts_with("S-1-"), "{sid}");
        assert!(whoami("/user").contains(&sid), "{sid}");
    }
}
