//! System restore points (`SRSetRestorePointW`). Needs administrator rights: only the
//! broker calls it.

use std::io;
use std::sync::Once;
use windows::Win32::Foundation::ERROR_SERVICE_DISABLED;
use windows::Win32::System::Com::{
    COINIT_MULTITHREADED, CoInitializeEx, CoInitializeSecurity, EOAC_DYNAMIC_CLOAKING,
    RPC_C_AUTHN_LEVEL_PKT_PRIVACY, RPC_C_IMP_LEVEL_IMPERSONATE,
};
use windows::Win32::System::Restore::{
    APPLICATION_UNINSTALL, BEGIN_SYSTEM_CHANGE, END_SYSTEM_CHANGE, RESTOREPOINTINFO_EVENT_TYPE,
    RESTOREPOINTINFOW, SRSetRestorePointW, STATEMGRSTATUS,
};

/// Longest description, in UTF-16 units, without the NUL.
pub const MAX_DESCRIPTION: usize = 255;

/// Creates a restore point named `description` and returns its sequence number.
/// System Restore turned off fails with `ERROR_SERVICE_DISABLED`.
pub fn create(description: &str) -> io::Result<i64> {
    prepare_com();
    let started = change(BEGIN_SYSTEM_CHANGE, description, 0)?;
    // Nothing runs in between: the point covers the state before what comes next.
    change(END_SYSTEM_CHANGE, "", started)
}

/// Whether `create` failed because System Restore is turned off.
pub fn is_disabled(error: &io::Error) -> bool {
    error.raw_os_error() == Some(ERROR_SERVICE_DISABLED.0 as i32)
}

fn change(event: RESTOREPOINTINFO_EVENT_TYPE, description: &str, sequence: i64) -> io::Result<i64> {
    let units: Vec<u16> = description.encode_utf16().collect();
    if units.len() > MAX_DESCRIPTION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "description too long",
        ));
    }
    // Filled apart: the structure is packed, so its fields cannot be borrowed.
    let mut text = [0; MAX_DESCRIPTION + 1];
    text[..units.len()].copy_from_slice(&units);
    let info = RESTOREPOINTINFOW {
        dwEventType: event,
        dwRestorePtType: APPLICATION_UNINSTALL,
        llSequenceNumber: sequence,
        szDescription: text,
    };
    let mut status = STATEMGRSTATUS::default();
    // SAFETY: both structures are fully initialised and live for the call; the
    // description is NUL-terminated (the array is zeroed past it).
    let done = unsafe { SRSetRestorePointW(&info, &mut status) }.as_bool();
    // Copied out of the packed structure before use.
    let (code, number) = (status.nStatus, status.llSequenceNumber);
    if done {
        Ok(number)
    } else {
        Err(io::Error::from_raw_os_error(code.0 as i32))
    }
}

/// System Restore asks for COM with call security set, so it can take the volume
/// snapshot under the caller's identity.
fn prepare_com() {
    static SECURITY: Once = Once::new();
    // SAFETY: plain call; joining the multithreaded apartment again is harmless.
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    SECURITY.call_once(|| {
        // SAFETY: plain call with no pointers; it fails harmlessly if the process
        // already set its security.
        let _ = unsafe {
            CoInitializeSecurity(
                None,
                -1,
                None,
                None,
                RPC_C_AUTHN_LEVEL_PKT_PRIVACY,
                RPC_C_IMP_LEVEL_IMPERSONATE,
                None,
                EOAC_DYNAMIC_CLOAKING,
                None,
            )
        };
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Refused before Windows is asked, so no point is ever created by this test.
    #[test]
    fn refuses_a_description_too_long() {
        let long = "é".repeat(MAX_DESCRIPTION + 1);
        let refused = change(BEGIN_SYSTEM_CHANGE, &long, 0);
        assert_eq!(
            refused.err().map(|e| e.kind()),
            Some(io::ErrorKind::InvalidInput)
        );
    }
}
