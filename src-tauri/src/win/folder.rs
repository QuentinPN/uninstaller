//! Folders only administrators may change, for what the elevated broker writes: the
//! unelevated app can read them but plant no link or junction in them (SECURITY.md).

use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::{Path, PathBuf};

use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::Security::Authorization::{SE_FILE_OBJECT, SetSecurityInfo};
use windows::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl, GetSecurityDescriptorOwner,
    OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, PSID,
};
use windows::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, CreateDirectoryW, CreateFileW, FILE_ATTRIBUTE_DIRECTORY,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, GetFileInformationByHandle,
    OPEN_EXISTING, READ_CONTROL, WRITE_DAC, WRITE_OWNER,
};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{FOLDERID_ProgramData, KF_FLAG_DEFAULT, SHGetKnownFolderPath};
use windows::core::{BOOL, HSTRING};

use super::security::{self, Descriptor};
use super::{os_error, token};

/// Owners Windows gives its own folders: SYSTEM, Administrators, TrustedInstaller.
const TRUSTED_OWNERS: [&str; 3] = [
    "S-1-5-18",
    "S-1-5-32-544",
    "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464",
];

/// ProgramData as Windows registers it, not as the environment (which this account
/// controls) says.
pub fn program_data() -> Option<PathBuf> {
    // SAFETY: no token means the current account; the returned string is freed below.
    let path =
        unsafe { SHGetKnownFolderPath(&FOLDERID_ProgramData, KF_FLAG_DEFAULT, None) }.ok()?;
    // SAFETY: a NUL-terminated string from the call above.
    let text = unsafe { path.to_string() }.ok();
    // SAFETY: allocated by SHGetKnownFolderPath and freed once, here.
    unsafe { CoTaskMemFree(Some(path.0.cast())) };
    text.map(PathBuf::from)
}

/// Makes `path` a folder that SYSTEM and Administrators own and alone may change, which
/// this account may read. Every folder above it must be a plain folder (no link or
/// junction) owned by Windows or administrators, or nothing is created; an existing
/// `path` must be too, and gets its rights set again. Needs administrator rights.
pub fn admin_only(path: &Path) -> io::Result<()> {
    let ancestors: Vec<&Path> = path.ancestors().skip(1).collect();
    for ancestor in ancestors.into_iter().rev() {
        trusted(&open(ancestor, READ_CONTROL.0)?)?;
    }
    let sid = token::user_sid().ok_or_else(|| io::Error::other("no user SID"))?;
    // Owner Administrators; protected DACL, inherited by everything inside: full
    // control for SYSTEM and Administrators, read and list for this account.
    let descriptor = Descriptor::from_sddl(&format!(
        "O:BAD:PAI(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1200a9;;;{sid})"
    ))?;
    let attributes = descriptor.attributes();
    // SAFETY: the HSTRING is NUL-terminated; `attributes` and its descriptor outlive
    // the call.
    match unsafe { CreateDirectoryW(&HSTRING::from(path.as_os_str()), Some(&attributes)) } {
        Ok(()) => Ok(()),
        Err(error) if error.code() == ERROR_ALREADY_EXISTS.to_hresult() => {
            let folder = open(path, READ_CONTROL.0 | WRITE_DAC.0 | WRITE_OWNER.0)?;
            trusted(&folder)?;
            reapply(&folder, &descriptor)
        }
        Err(error) => Err(os_error(error)),
    }
}

/// `path` itself, never what a link there points to.
fn open(path: &Path, access: u32) -> io::Result<OwnedHandle> {
    // SAFETY: the HSTRING is NUL-terminated and outlives the call.
    let handle = unsafe {
        CreateFileW(
            &HSTRING::from(path.as_os_str()),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            None,
        )
    }
    .map_err(os_error)?;
    // SAFETY: a valid handle that nothing else owns.
    Ok(unsafe { OwnedHandle::from_raw_handle(handle.0) })
}

/// A plain folder owned by Windows or administrators, else `PermissionDenied`.
fn trusted(folder: &OwnedHandle) -> io::Result<()> {
    let handle = HANDLE(folder.as_raw_handle());
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: a live handle and a pointer to a local.
    unsafe { GetFileInformationByHandle(handle, &mut info) }.map_err(os_error)?;
    let plain = info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0
        && info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT.0 == 0;
    if plain && TRUSTED_OWNERS.contains(&security::file_owner(handle)?.as_str()) {
        Ok(())
    } else {
        Err(io::ErrorKind::PermissionDenied.into())
    }
}

/// Sets the owner and protected DACL of `descriptor` on `folder`.
fn reapply(folder: &OwnedHandle, descriptor: &Descriptor) -> io::Result<()> {
    let mut owner = PSID::default();
    let mut dacl: *mut ACL = std::ptr::null_mut();
    let (mut defaulted, mut present) = (BOOL::default(), BOOL::default());
    // SAFETY: a live descriptor; `owner` and `dacl` point inside it, which outlives
    // their use below.
    unsafe {
        GetSecurityDescriptorOwner(descriptor.as_raw(), &mut owner, &mut defaulted)?;
        GetSecurityDescriptorDacl(descriptor.as_raw(), &mut present, &mut dacl, &mut defaulted)?;
    }
    // SAFETY: a live handle opened with WRITE_DAC and WRITE_OWNER; owner and DACL are
    // valid (above).
    unsafe {
        SetSecurityInfo(
            HANDLE(folder.as_raw_handle()),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION
                | DACL_SECURITY_INFORMATION
                | PROTECTED_DACL_SECURITY_INFORMATION,
            Some(owner),
            None,
            Some(dacl),
            None,
        )
    }
    .ok()
    .map_err(os_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn program_data_is_the_registered_folder() {
        let path = program_data().unwrap();
        assert!(
            path.is_absolute() && path.ends_with("ProgramData"),
            "{path:?}"
        );
    }

    /// A folder this account owns (here, under %TEMP%) could be swapped for a
    /// junction: nothing is created below it.
    #[test]
    fn refuses_a_folder_below_one_this_account_owns() {
        let path = std::env::temp_dir().join(format!("uninstaller-admin-{}", std::process::id()));
        let error = admin_only(&path).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(!path.exists());
        fs::create_dir(&path).unwrap();
        let below = path.join("inner");
        assert_eq!(
            admin_only(&below).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        assert!(!below.exists());
        fs::remove_dir(&path).unwrap();
    }

    /// Creates a folder in ProgramData, checks its owner, then removes it.
    #[test]
    #[ignore = "administrateur : VM"]
    fn creates_a_folder_only_administrators_change() {
        let path = program_data().unwrap().join("UninstallerTest");
        admin_only(&path).unwrap();
        admin_only(&path).unwrap();
        let owner =
            security::file_owner(HANDLE(open(&path, READ_CONTROL.0).unwrap().as_raw_handle()));
        assert_eq!(owner.unwrap(), "S-1-5-32-544");
        fs::remove_dir(&path).unwrap();
    }
}
