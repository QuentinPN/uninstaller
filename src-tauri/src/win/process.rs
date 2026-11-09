//! Queries about other processes.

use std::io;
use std::path::PathBuf;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::core::PWSTR;

/// The full path of the executable process `pid` runs.
pub fn image_path(pid: u32) -> io::Result<PathBuf> {
    // SAFETY: plain call; the handle it returns is closed below.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }?;
    // Long enough for any path, including `\\?\` ones.
    let mut buffer = vec![0u16; 32_768];
    let mut len = buffer.len() as u32;
    // SAFETY: `buffer` holds `len` writable UTF-16 units; on success `len` is set to the
    // number written, without the NUL.
    let queried = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut len,
        )
    };
    // SAFETY: `process` is owned here and closed exactly once.
    let _ = unsafe { CloseHandle(process) };
    queried?;
    Ok(PathBuf::from(String::from_utf16_lossy(
        &buffer[..len as usize],
    )))
}

#[cfg(test)]
mod tests {
    use super::image_path;

    #[test]
    fn finds_this_process_and_refuses_a_missing_one() {
        let path = image_path(std::process::id()).expect("own image");
        let exe = std::env::current_exe().expect("current exe");
        assert_eq!(
            std::fs::canonicalize(path).expect("canonical"),
            std::fs::canonicalize(exe).expect("canonical")
        );
        // Process ids are multiples of 4: 3 is never one.
        assert!(image_path(3).is_err());
    }
}
