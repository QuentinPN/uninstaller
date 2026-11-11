//! Security descriptors and SIDs, shared by the broker's pipe and its folders.

use std::io;

use windows::Win32::Foundation::{HANDLE, HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo,
    SDDL_REVISION_1, SE_FILE_OBJECT,
};
use windows::Win32::Security::{
    OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES,
};
use windows::core::{HSTRING, PWSTR};

/// A security descriptor built from SDDL, freed when dropped.
pub struct Descriptor(PSECURITY_DESCRIPTOR);

impl Descriptor {
    pub fn from_sddl(sddl: &str) -> io::Result<Self> {
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        // SAFETY: the HSTRING is NUL-terminated and outlives the call; `descriptor`
        // receives a LocalAlloc'd descriptor, freed on drop.
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                &HSTRING::from(sddl),
                SDDL_REVISION_1,
                &mut descriptor,
                None,
            )
        }?;
        Ok(Self(descriptor))
    }

    pub fn as_raw(&self) -> PSECURITY_DESCRIPTOR {
        self.0
    }

    /// Attributes that create an object with this descriptor; valid while `self` is.
    pub fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0.0,
            bInheritHandle: false.into(),
        }
    }
}

impl Drop for Descriptor {
    fn drop(&mut self) {
        // SAFETY: allocated by the conversion in `from_sddl` and freed once, here.
        unsafe { LocalFree(Some(HLOCAL(self.0.0))) };
    }
}

/// `sid` as text (`S-1-5-…`).
pub fn sid_string(sid: PSID) -> Option<String> {
    let mut text = PWSTR::null();
    // SAFETY: the caller passes a valid SID; `text` receives a LocalAlloc'd string.
    unsafe { ConvertSidToStringSidW(sid, &mut text) }.ok()?;
    // SAFETY: `text` is the NUL-terminated string just returned.
    let value = unsafe { text.to_string() }.ok();
    // SAFETY: `text` was allocated by ConvertSidToStringSidW and is freed once, here.
    unsafe { LocalFree(Some(HLOCAL(text.0.cast()))) };
    value
}

/// The owner of the file or folder open as `handle` (opened with `READ_CONTROL`).
pub fn file_owner(handle: HANDLE) -> io::Result<String> {
    let mut owner = PSID::default();
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    // SAFETY: a live handle; `owner` points inside `descriptor`, LocalAlloc'd by the
    // call and freed below, after the last use of `owner`.
    unsafe {
        GetSecurityInfo(
            handle,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            Some(&mut owner),
            None,
            None,
            None,
            Some(&mut descriptor),
        )
    }
    .ok()?;
    let text = sid_string(owner);
    // SAFETY: allocated by GetSecurityInfo and freed once, here.
    unsafe { LocalFree(Some(HLOCAL(descriptor.0))) };
    text.ok_or_else(|| io::Error::other("unreadable owner"))
}
