//! Local named pipes for the elevated broker (ADR 0002): one instance, open to this
//! account only, never to remote clients.

use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use windows::Win32::Foundation::{ERROR_PIPE_CONNECTED, HANDLE, HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeClientProcessId, GetNamedPipeServerProcessId,
    PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
};
use windows::core::HSTRING;

use super::token;

/// Buffer size each way; the protocol caps a frame well below it.
const BUFFER: u32 = 64 * 1024;

fn path(name: &str) -> HSTRING {
    HSTRING::from(format!(r"\\.\pipe\{name}"))
}

/// The listening end, created by the unelevated app before it starts the broker.
pub struct Server(OwnedHandle);

impl Server {
    /// Creates the pipe `name`. Fails if any pipe of that name already exists, so a
    /// process that took the name first cannot sit between the app and its broker.
    pub fn create(name: &str) -> io::Result<Self> {
        let sid = token::user_sid().ok_or_else(|| io::Error::other("no user SID"))?;
        // Protected DACL: full access for this account, nothing inherited, nobody else.
        let sddl = HSTRING::from(format!("D:P(A;;GA;;;{sid})"));
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        // SAFETY: `sddl` is NUL-terminated and outlives the call; `descriptor` receives
        // a LocalAlloc'd descriptor, freed below.
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                &sddl,
                SDDL_REVISION_1,
                &mut descriptor,
                None,
            )
        }?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: false.into(),
        };
        // SAFETY: the name is NUL-terminated; `attributes` and the descriptor it points
        // to stay alive until the call returns.
        let handle = unsafe {
            CreateNamedPipeW(
                &path(name),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                BUFFER,
                BUFFER,
                0,
                Some(&attributes),
            )
        };
        let created = if handle.is_invalid() {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        };
        // SAFETY: allocated by the conversion above and freed once; the pipe keeps its
        // own copy of the descriptor.
        unsafe { LocalFree(Some(HLOCAL(descriptor.0))) };
        created?;
        // SAFETY: a valid handle that nothing else owns.
        Ok(Self(unsafe { OwnedHandle::from_raw_handle(handle.0) }))
    }

    /// Waits for the one client, then returns the connection.
    pub fn accept(self) -> io::Result<File> {
        // SAFETY: `self.0` is a live pipe handle opened for synchronous use, hence no
        // OVERLAPPED.
        match unsafe { ConnectNamedPipe(raw(&self.0), None) } {
            // The client may connect between creation and this call.
            Err(e) if e.code() != ERROR_PIPE_CONNECTED.to_hresult() => return Err(e.into()),
            _ => {}
        }
        Ok(File::from(self.0))
    }
}

/// The client end: opens the pipe `name` created by `Server::create`.
pub fn connect(name: &str) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(path(name).to_os_string())
}

/// The process that holds the client end of a connected pipe.
pub fn client_pid(pipe: &File) -> io::Result<u32> {
    let mut pid = 0;
    // SAFETY: `pipe` is a live pipe handle; `pid` is a valid out-pointer.
    unsafe { GetNamedPipeClientProcessId(raw(pipe), &mut pid) }?;
    Ok(pid)
}

/// The process that created a connected pipe.
pub fn server_pid(pipe: &File) -> io::Result<u32> {
    let mut pid = 0;
    // SAFETY: `pipe` is a live pipe handle; `pid` is a valid out-pointer.
    unsafe { GetNamedPipeServerProcessId(raw(pipe), &mut pid) }?;
    Ok(pid)
}

fn raw(handle: &impl AsRawHandle) -> HANDLE {
    HANDLE(handle.as_raw_handle())
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW, GetSecurityInfo, SE_KERNEL_OBJECT,
    };
    use windows::Win32::Security::DACL_SECURITY_INFORMATION;
    use windows::core::PWSTR;

    fn name(test: &str) -> String {
        format!("uninstaller-test-{test}-{}", std::process::id())
    }

    /// The pipe's DACL as SDDL, read back from Windows.
    fn dacl(pipe: &File) -> String {
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        // SAFETY: `pipe` is live; only the descriptor out-pointer is asked for.
        let status = unsafe {
            GetSecurityInfo(
                raw(pipe),
                SE_KERNEL_OBJECT,
                DACL_SECURITY_INFORMATION,
                None,
                None,
                None,
                None,
                Some(&mut descriptor),
            )
        };
        assert!(status.is_ok(), "{status:?}");
        let mut text = PWSTR::null();
        // SAFETY: `descriptor` was just returned; `text` receives a LocalAlloc'd string.
        unsafe {
            ConvertSecurityDescriptorToStringSecurityDescriptorW(
                descriptor,
                SDDL_REVISION_1,
                DACL_SECURITY_INFORMATION,
                &mut text,
                None,
            )
        }
        .expect("sddl");
        // SAFETY: NUL-terminated string returned above.
        let sddl = unsafe { text.to_string() }.expect("utf-16");
        // SAFETY: both were LocalAlloc'd by the calls above, each freed once.
        unsafe {
            LocalFree(Some(HLOCAL(text.0.cast())));
            LocalFree(Some(HLOCAL(descriptor.0)));
        }
        sddl
    }

    #[test]
    fn connects_once_and_only_for_this_account() {
        let name = name("connect");
        let server = Server::create(&name).expect("create");
        assert!(Server::create(&name).is_err(), "second instance refused");
        let accepting = std::thread::spawn(move || server.accept().expect("accept"));
        let client = connect(&name).expect("connect");
        let served = accepting.join().expect("join");

        let me = std::process::id();
        assert_eq!(server_pid(&client).expect("server pid"), me);
        assert_eq!(client_pid(&served).expect("client pid"), me);

        let sid = token::user_sid().expect("sid");
        // Generic access reads back as file rights; one ACE only, protected.
        assert_eq!(dacl(&served), format!("D:P(A;;FA;;;{sid})"));
    }
}
