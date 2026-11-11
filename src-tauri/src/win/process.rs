//! Processes: starting the elevated broker or an uninstaller, waiting for them and
//! what they start, finding a parent.

use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use windows::Win32::Foundation::{
    ERROR_CANCELLED, ERROR_ELEVATION_REQUIRED, FILETIME, HANDLE, WAIT_OBJECT_0,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    GetExitCodeProcess, GetProcessId, GetProcessTimes, INFINITE, OpenProcess,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, WaitForSingleObject,
};
use windows::Win32::UI::Shell::{
    SEE_MASK_FLAG_NO_UI, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW,
};
use windows::Win32::UI::WindowsAndMessaging::{SHOW_WINDOW_CMD, SW_HIDE, SW_SHOWNORMAL};
use windows::core::{HSTRING, PCWSTR, w};

/// Time between two looks at Windows' process list while waiting for a tree.
const POLL: Duration = Duration::from_millis(250);

/// A process this one started, held by its handle so its id cannot be reused meanwhile.
pub struct Process(OwnedHandle);

impl Process {
    /// The process `id`, with the rights to wait for it and read its times and code.
    fn open(id: u32) -> io::Result<Self> {
        // SAFETY: plain call; the handle it returns is owned by the result.
        let handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                false,
                id,
            )
        }?;
        // SAFETY: a valid handle that nothing else owns.
        Ok(Self(unsafe { OwnedHandle::from_raw_handle(handle.0) }))
    }

    pub fn id(&self) -> u32 {
        // SAFETY: `self.0` is a live process handle.
        unsafe { GetProcessId(raw(&self.0)) }
    }

    /// Blocks until the process has exited.
    pub fn wait(&self) {
        // SAFETY: `self.0` is a live process handle, which is waitable.
        unsafe { WaitForSingleObject(raw(&self.0), INFINITE) };
    }

    pub fn has_exited(&self) -> bool {
        // SAFETY: `self.0` is a live process handle; a zero timeout only polls it.
        let state = unsafe { WaitForSingleObject(raw(&self.0), 0) };
        state == WAIT_OBJECT_0
    }

    pub fn try_clone(&self) -> io::Result<Self> {
        self.0.try_clone().map(Self)
    }

    /// Its exit code, once it has exited.
    pub fn exit_code(&self) -> Option<u32> {
        let mut code = 0;
        // SAFETY: `self.0` is a live process handle with query rights; `code` outlives
        // the call.
        unsafe { GetExitCodeProcess(raw(&self.0), &mut code) }.ok()?;
        self.has_exited().then_some(code)
    }

    /// When it started, in 100 ns units since 1601.
    fn created(&self) -> Option<u64> {
        let mut times = [FILETIME::default(); 4];
        let [created, exited, kernel, user] = &mut times;
        // SAFETY: `self.0` is a live process handle with query rights; the four
        // FILETIMEs outlive the call.
        unsafe { GetProcessTimes(raw(&self.0), created, exited, kernel, user) }.ok()?;
        Some(u64::from(created.dwHighDateTime) << 32 | u64::from(created.dwLowDateTime))
    }
}

#[cfg(test)]
impl Process {
    /// This very process, for tests that play both ends.
    pub fn current() -> io::Result<Self> {
        Self::open(std::process::id())
    }
}

impl From<std::process::Child> for Process {
    fn from(child: std::process::Child) -> Self {
        Self(child.into())
    }
}

/// Starts `exe` with `parameters` as administrator, its window hidden: Windows shows
/// its UAC prompt first. A declined prompt fails with `ERROR_CANCELLED`.
pub fn run_as_admin(exe: &Path, parameters: &str) -> io::Result<Process> {
    run_elevated(exe, parameters, SW_HIDE)
}

/// Starts the program file `exe` with the command-line `arguments`, as given, without
/// a shell. One whose manifest asks for the administrator is started through the UAC
/// prompt instead, with the same arguments.
pub fn start(exe: &Path, arguments: &str) -> io::Result<Process> {
    match std::process::Command::new(exe).raw_arg(arguments).spawn() {
        Ok(child) => Ok(child.into()),
        Err(error) if error.raw_os_error() == Some(ERROR_ELEVATION_REQUIRED.0 as i32) => {
            run_elevated(exe, arguments, SW_SHOWNORMAL)
        }
        Err(error) => Err(error),
    }
}

fn run_elevated(exe: &Path, parameters: &str, show: SHOW_WINDOW_CMD) -> io::Result<Process> {
    let file = HSTRING::from(exe);
    let parameters = HSTRING::from(parameters);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_FLAG_NO_UI,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(parameters.as_ptr()),
        nShow: show.0,
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

/// Whether `run_as_admin` failed because the user refused the UAC prompt. The code
/// comes either as the Win32 error or wrapped in an HRESULT.
pub fn is_refused(error: &io::Error) -> bool {
    let code = error.raw_os_error();
    code == Some(ERROR_CANCELLED.0 as i32) || code == Some(ERROR_CANCELLED.to_hresult().0)
}

/// Blocks until `root`, every process it started and those started in turn have
/// exited (many uninstallers hand over to a copy of themselves in %TEMP%), or until
/// `stop` turns true; returns whether they all ended. Windows gives an elevated
/// process the program that asked for it as parent, so the tree crosses the UAC
/// prompt. Children are found by polling: one that starts and ends between two looks
/// is not waited for, and its own children are missed.
pub fn wait_tree(root: &Process, stop: &AtomicBool) -> bool {
    let (Some(created), Ok(root)) = (root.created(), root.try_clone()) else {
        root.wait();
        return true;
    };
    // Held open, so none of their ids is reused while the tree is followed.
    let mut tree = vec![(root.id(), created, root)];
    loop {
        if stop.load(Ordering::Relaxed) {
            return false;
        }
        let born: Vec<_> = processes()
            .unwrap_or_default()
            .into_iter()
            .filter(|(id, _)| tree.iter().all(|(known, ..)| known != id))
            .filter_map(|(id, parent)| {
                let (_, parent_created, _) = tree.iter().find(|(known, ..)| *known == parent)?;
                let child = Process::open(id).ok()?;
                let created = child.created()?;
                // An older process naming a parent whose id was reused is not its child.
                (created >= *parent_created).then_some((id, created, child))
            })
            .collect();
        if born.is_empty() && tree.iter().all(|(.., process)| process.has_exited()) {
            return true;
        }
        tree.extend(born);
        std::thread::sleep(POLL);
    }
}

/// The id of the process that started this one. Windows gives an elevated process
/// the program that asked for it as parent, not the UAC service.
pub fn parent_id() -> io::Result<u32> {
    let me = std::process::id();
    processes()?
        .into_iter()
        .find_map(|(id, parent)| (id == me).then_some(parent))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "own process not listed"))
}

/// Every running process, as its id and its parent's.
fn processes() -> io::Result<Vec<(u32, u32)>> {
    // SAFETY: plain call; the snapshot handle is owned and closed below.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }?;
    // SAFETY: a valid handle that nothing else owns.
    let snapshot = unsafe { OwnedHandle::from_raw_handle(snapshot.0) };
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut all = Vec::new();
    // SAFETY: `snapshot` is live and `entry` has its size set, as both calls require.
    let mut found = unsafe { Process32FirstW(raw(&snapshot), &mut entry) };
    while found.is_ok() {
        all.push((entry.th32ProcessID, entry.th32ParentProcessID));
        // SAFETY: as above.
        found = unsafe { Process32NextW(raw(&snapshot), &mut entry) };
    }
    Ok(all)
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
        assert!(process.has_exited());
        assert!(!Process::current().expect("current").has_exited());
    }

    /// `cmd` hands a `ping` of about two seconds over to itself in the background and
    /// exits at once: the wait lasts until the ping ends, and gives its code.
    #[test]
    fn waits_for_what_the_process_started() {
        let cmd = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap())
            .join(r"System32\cmd.exe");
        let begun = std::time::Instant::now();
        let root = start(&cmd, r#"/c start "" /b ping -n 3 127.0.0.1 >nul & exit 3"#).unwrap();
        assert!(wait_tree(&root, &AtomicBool::new(false)));
        assert!(begun.elapsed() >= Duration::from_millis(1500));
        assert_eq!(root.exit_code(), Some(3));
        let stopped = start(&cmd, "/c ping -n 3 127.0.0.1 >nul").unwrap();
        assert!(!wait_tree(&stopped, &AtomicBool::new(true)));
        assert_eq!(stopped.exit_code(), None);
    }

    #[test]
    fn a_refused_prompt_is_told_apart() {
        assert!(is_refused(&io::Error::from_raw_os_error(1223)));
        assert!(is_refused(
            &windows::core::Error::from(ERROR_CANCELLED).into()
        ));
        assert!(!is_refused(&io::Error::from_raw_os_error(5)));
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
