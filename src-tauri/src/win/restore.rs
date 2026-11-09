//! System restore points: created by `SRSetRestorePointW`, listed by WMI. Both need
//! administrator rights: only the broker calls them.

use std::io;
use windows::Win32::Foundation::ERROR_SERVICE_DISABLED;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::System::Restore::{
    APPLICATION_UNINSTALL, BEGIN_SYSTEM_CHANGE, END_SYSTEM_CHANGE, RESTOREPOINTINFO_EVENT_TYPE,
    RESTOREPOINTINFOW, SRSetRestorePointW, STATEMGRSTATUS,
};
use windows::Win32::System::Variant::{VARIANT, VariantClear, VariantToString, VariantToUInt32};
use windows::Win32::System::Wmi::{
    IWbemClassObject, IWbemLocator, WBEM_FLAG_FORWARD_ONLY, WBEM_FLAG_RETURN_IMMEDIATELY,
    WBEM_GENERIC_FLAG_TYPE, WBEM_INFINITE, WbemLocator,
};
use windows::core::{BSTR, PCWSTR, w};

use crate::win::com;

/// Longest description, in UTF-16 units, without the NUL.
pub const MAX_DESCRIPTION: usize = 255;

/// Creates a restore point named `description` and returns its sequence number.
/// System Restore turned off fails with `ERROR_SERVICE_DISABLED`.
pub fn create(description: &str) -> io::Result<i64> {
    com::prepare();
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

/// A restore point as WMI lists it. `created` is WMI's
/// `yyyymmddHHMMSS.ffffff±UUU` text, the offset in minutes.
pub struct Listed {
    pub sequence: u32,
    pub description: String,
    pub created: String,
}

/// Every restore point Windows keeps (`root\default:SystemRestore`), in WMI's order.
pub fn list() -> io::Result<Vec<Listed>> {
    com::prepare();
    // SAFETY: COM is initialised on this thread by `com::prepare`.
    let locator: IWbemLocator =
        unsafe { CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER) }?;
    let none = BSTR::new();
    // SAFETY: every string outlives the call; no context object.
    let services = unsafe {
        locator.ConnectServer(
            &BSTR::from(r"ROOT\DEFAULT"),
            &none,
            &none,
            &none,
            0,
            &none,
            None,
        )
    }?;
    let flags = WBEM_GENERIC_FLAG_TYPE(WBEM_FLAG_FORWARD_ONLY.0 | WBEM_FLAG_RETURN_IMMEDIATELY.0);
    // SAFETY: both strings outlive the call; no context object.
    let rows = unsafe {
        services.ExecQuery(
            &BSTR::from("WQL"),
            &BSTR::from("SELECT SequenceNumber, Description, CreationTime FROM SystemRestore"),
            flags,
            None,
        )
    }?;
    let mut points = Vec::new();
    loop {
        let mut row = [None];
        let mut returned = 0;
        // SAFETY: `row` has room for the one object asked for, `returned` is written.
        unsafe { rows.Next(WBEM_INFINITE, &mut row, &mut returned) }.ok()?;
        let Some(row) = row[0].take() else {
            return Ok(points);
        };
        points.push(Listed {
            // SAFETY: `property` hands a VARIANT that `Get` filled.
            sequence: property(&row, w!("SequenceNumber"), |v| unsafe {
                VariantToUInt32(v)
            })?,
            description: property(&row, w!("Description"), text)?,
            created: property(&row, w!("CreationTime"), text)?,
        });
    }
}

/// Reads the property `name` of `row` through `convert`, then frees the value.
fn property<T>(
    row: &IWbemClassObject,
    name: PCWSTR,
    convert: impl FnOnce(&VARIANT) -> windows::core::Result<T>,
) -> io::Result<T> {
    let mut value = VARIANT::default();
    // SAFETY: `name` is a NUL-terminated literal and `value` an empty VARIANT to fill.
    unsafe { row.Get(name, 0, &mut value, None, None) }?;
    let converted = convert(&value);
    // SAFETY: `value` was filled by `Get` and is not used after.
    let _ = unsafe { VariantClear(&mut value) };
    Ok(converted?)
}

fn text(value: &VARIANT) -> windows::core::Result<String> {
    let mut buffer = [0; 1024];
    // SAFETY: `value` is a filled VARIANT; the buffer's length goes with it.
    unsafe { VariantToString(value, &mut buffer) }?;
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    Ok(String::from_utf16_lossy(&buffer[..end]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::win::token;

    /// Without administrator rights Windows refuses the list; COM, the locator and the
    /// connection are still set up on the way.
    #[test]
    fn listing_needs_admin() {
        if !token::is_elevated() {
            assert!(list().is_err());
        }
    }

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
