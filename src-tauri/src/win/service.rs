//! Service Control Manager: a service stopped and removed for the quarantine, which
//! keeps its registry key as a `.reg` beforehand.

use std::io;
use std::thread;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::ERROR_SERVICE_NOT_ACTIVE;
use windows::Win32::Storage::FileSystem::DELETE;
use windows::Win32::System::Services::{
    CloseServiceHandle, ControlService, DeleteService, OpenSCManagerW, OpenServiceW,
    QueryServiceStatus, SC_HANDLE, SC_MANAGER_CONNECT, SERVICE_CONTROL_STOP, SERVICE_QUERY_STATUS,
    SERVICE_STATUS, SERVICE_STOP, SERVICE_STOPPED,
};
use windows::core::{HSTRING, PCWSTR};

use super::os_error;

/// How long a service gets to stop before its removal is given up.
const STOP_WAIT: Duration = Duration::from_secs(30);

/// Stops the service `name` if it runs, then removes it. Denied (`PermissionDenied`)
/// without administrator rights; a service that does not stop in time stays
/// (`TimedOut`). Windows drops its key once the last handle to it closes.
pub fn delete(name: &str) -> io::Result<()> {
    let manager = Handle::manager(SC_MANAGER_CONNECT)?;
    let access = DELETE.0 | SERVICE_STOP | SERVICE_QUERY_STATUS;
    // SAFETY: a live manager handle; the HSTRING outlives the call.
    let service =
        Handle(unsafe { OpenServiceW(manager.0, &HSTRING::from(name), access) }.map_err(os_error)?);
    let mut status = SERVICE_STATUS::default();
    // SAFETY: a live service handle and a pointer to a local.
    match unsafe { ControlService(service.0, SERVICE_CONTROL_STOP, &mut status) } {
        Err(error) if error.code() == ERROR_SERVICE_NOT_ACTIVE.to_hresult() => {}
        stopped => {
            stopped.map_err(os_error)?;
            let started = Instant::now();
            while status.dwCurrentState != SERVICE_STOPPED {
                if started.elapsed() > STOP_WAIT {
                    return Err(io::ErrorKind::TimedOut.into());
                }
                thread::sleep(Duration::from_millis(250));
                // SAFETY: a live service handle and a pointer to a local.
                unsafe { QueryServiceStatus(service.0, &mut status) }.map_err(os_error)?;
            }
        }
    }
    // SAFETY: a live service handle opened with `DELETE`.
    unsafe { DeleteService(service.0) }.map_err(os_error)
}

/// A service or manager handle, closed when dropped.
struct Handle(SC_HANDLE);

impl Handle {
    /// The Service Control Manager of this machine.
    fn manager(access: u32) -> io::Result<Self> {
        // SAFETY: null names mean this machine and its active database.
        let handle = unsafe { OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), access) };
        handle.map(Self).map_err(os_error)
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle came from `OpenSCManagerW` or `OpenServiceW` and is closed once.
        let _ = unsafe { CloseServiceHandle(self.0) };
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use windows::Win32::System::Services::{
        CreateServiceW, SC_MANAGER_CREATE_SERVICE, SERVICE_DEMAND_START, SERVICE_ERROR_NORMAL,
        SERVICE_WIN32_OWN_PROCESS,
    };

    /// Registers a service that is never started (administrator rights).
    pub fn create(name: &str, binary: &str) -> io::Result<()> {
        let manager = Handle::manager(SC_MANAGER_CREATE_SERVICE)?;
        // SAFETY: a live manager handle; the HSTRINGs outlive the call; null optional
        // strings mean none and LocalSystem.
        let service = unsafe {
            CreateServiceW(
                manager.0,
                &HSTRING::from(name),
                &HSTRING::from(name),
                SERVICE_QUERY_STATUS,
                SERVICE_WIN32_OWN_PROCESS,
                SERVICE_DEMAND_START,
                SERVICE_ERROR_NORMAL,
                &HSTRING::from(binary),
                PCWSTR::null(),
                None,
                PCWSTR::null(),
                PCWSTR::null(),
                PCWSTR::null(),
            )
        };
        service.map(|handle| drop(Handle(handle))).map_err(os_error)
    }

    /// Without rights, nothing is stopped or removed: Windows refuses the handle.
    #[test]
    fn removal_needs_rights_or_a_service() {
        let error = delete("UninstallerTestAbsent").unwrap_err();
        // ERROR_SERVICE_DOES_NOT_EXIST with rights, ERROR_ACCESS_DENIED without.
        assert!(matches!(error.raw_os_error(), Some(1060 | 5)));
    }
}
