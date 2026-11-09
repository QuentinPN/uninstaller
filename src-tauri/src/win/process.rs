//! Processes: starting the elevated broker, waiting for it, finding a parent.

use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::Path;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{GetProcessId, INFINITE, WaitForSingleObject};
use windows::Win32::UI::Shell::{
    SEE_MASK_FLAG_NO_UI, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW,
};
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;
use windows::core::{HSTRING, PCWSTR, w};

/// A process this one started, held by its handle so its id cannot be reused meanwhile.
pub struct Process(OwnedHandle);

impl Process {
    pub fn id(&self) -> u32 {
        // SAFETY: `self.0` is a live process handle.
        unsafe { GetProcessId(raw(&self.0)) }
    }

    /// Blocks until the process has exited.
    pub fn wait(&self) {
        // SAFETY: `self.0` is a live process handle, which is waitable.
        unsafe { WaitForSingleObject(raw(&self.0), INFINITE) };
    }

    pub fn try_clone(&self) -> io::Result<Self> {
        self.0.try_clone().map(Self)
    }
}

#[cfg(test)]
impl Process {
    /// This very process, for tests that play both ends.
    pub fn current() -> io::Result<Self> {
        use windows::Win32::System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
        };
        // SAFETY: plain call; the handle it returns is owned by the result.
        let handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                false,
                std::process::id(),
            )
        }?;
        // SAFETY: a valid handle that nothing else owns.
        Ok(Self(unsafe { OwnedHandle::from_raw_handle(handle.0) }))
    }
}

impl From<std::process::Child> for Process {
    fn from(child: std::process::Child) -> Self {
        Self(child.into())
    }
}

/// Starts `exe` with `parameters` as administrator: Windows shows its UAC prompt
/// first. A declined prompt fails with `ERROR_CANCELLED`.
pub fn run_as_admin(exe: &Path, parameters: &str) -> io::Result<Process> {
    let file = HSTRING::from(exe);
    let parameters = HSTRING::from(parameters);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_FLAG_NO_UI,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(parameters.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    // SAFETY: `info` is fully initialised and its strings outlive the call.
    unsafe { ShellExecuteExW(&mut info) }?;
    if info.hProcess.is_invalid() {
        return Err(io::Error::other("no process started"));
    }
    // SAFETY: with SEE_MASK_NOCLOSEPROCESS the handle is ours to close.
    Ok(Process(unsafe {
        OwnedHandle::from_raw_handle(info.hProcess.0)
    }))
}

/// The id of the process that started this one. Windows gives an elevated process
/// the program that asked for it as parent, not the UAC service.
pub fn parent_id() -> io::Result<u32> {
    // SAFETY: plain call; the snapshot handle is owned and closed below.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }?;
    // SAFETY: a valid handle that nothing else owns.
    let snapshot = unsafe { OwnedHandle::from_raw_handle(snapshot.0) };
    let me = std::process::id();
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    // SAFETY: `snapshot` is live and `entry` has its size set, as both calls require.
    let mut found = unsafe { Process32FirstW(raw(&snapshot), &mut entry) };
    while found.is_ok() {
        if entry.th32ProcessID == me {
            return Ok(entry.th32ParentProcessID);
        }
        // SAFETY: as above.
        found = unsafe { Process32NextW(raw(&snapshot), &mut entry) };
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "own process not listed",
    ))
}

fn raw(handle: &impl AsRawHandle) -> HANDLE {
    HANDLE(handle.as_raw_handle())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    #[test]
    fn a_child_can_be_waited_for() {
        let child = Command::new("cmd")
            .args(["/c", "exit 0"])
            .stdout(Stdio::null())
            .spawn()
            .expect("spawn");
        let process = Process::from(child);
        assert_ne!(process.id(), 0);
        process.try_clone().expect("clone").wait();
    }

    /// Cross-checks with Windows' own process list, read by PowerShell.
    #[test]
    fn parent_matches_windows() {
        let filter = format!("ProcessId={}", std::process::id());
        let out = Command::new("powershell")
            .args(["-NoProfile", "-Command"])
            .arg(format!(
                "(Get-CimInstance Win32_Process -Filter '{filter}').ParentProcessId"
            ))
            .output()
            .expect("run powershell");
        let expected: u32 = String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse()
            .expect("parent id");
        assert_eq!(parent_id().expect("parent"), expected);
    }
}
