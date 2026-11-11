//! Thin safe wrappers over the Windows API. Every `unsafe` block of the crate lives here.

pub mod com;
pub mod dialog;
pub mod icon;
pub mod pipe;
pub mod process;
pub mod registry;
pub mod restore;
pub mod scheduler;
pub mod service;
pub mod shortcut;
pub mod token;

use std::io;

use windows::core::{Error, HRESULT};

/// A Windows error as `io::Error`, with its own kind (`NotFound`, `PermissionDenied`)
/// when the HRESULT wraps a Win32 code.
pub fn os_error(error: Error) -> io::Error {
    const WIN32: u32 = 0x8007_0000;
    let HRESULT(code) = error.code();
    let code = code.cast_unsigned();
    if code & 0xFFFF_0000 == WIN32 {
        io::Error::from_raw_os_error((code & 0xFFFF).cast_signed())
    } else {
        error.into()
    }
}
