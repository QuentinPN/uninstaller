//! Task Scheduler: the registered tasks and the programs they start; a task's
//! definition saved, removed and registered again for the quarantine.

use std::io;

use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::System::TaskScheduler::{
    IExecAction, IRegisteredTask, ITaskFolder, ITaskService, TASK_ACTION_EXEC, TASK_ACTION_TYPE,
    TASK_CREATE, TASK_ENUM_HIDDEN, TASK_LOGON_PASSWORD, TASK_LOGON_TYPE, TaskScheduler,
};
use windows::Win32::System::Variant::VARIANT;
use windows::core::{BSTR, Interface};

use crate::win::{com, os_error as os};

/// A registered task: its full path (`\Folder\Name`) and the program of each of its
/// "start a program" actions, as written (variables not expanded, maybe quoted).
pub struct Task {
    pub path: String,
    pub programs: Vec<String>,
}

/// Every task the caller can see, hidden ones included; without administrator rights,
/// other accounts' tasks may be missing or show no program.
pub fn tasks() -> Vec<Task> {
    let mut tasks = Vec::new();
    if let Ok((_, root)) = connect() {
        collect(&root, &mut tasks);
    }
    tasks
}

/// The definition of the task at `path` (`\Folder\Name`), as Task Scheduler writes
/// it. A task that logs on with a stored password is refused (`Unsupported`): it could
/// not be registered again without that password.
pub fn export(path: &str) -> io::Result<String> {
    let (_, root) = connect()?;
    let mut logon = TASK_LOGON_TYPE::default();
    // SAFETY: calls on live interfaces; the BSTR and the local outlive their calls.
    let xml = unsafe {
        root.GetTask(&BSTR::from(path)).and_then(|task| {
            task.Definition()?.Principal()?.LogonType(&mut logon)?;
            task.Xml()
        })
    }
    .map_err(os)?;
    if logon == TASK_LOGON_PASSWORD {
        return Err(io::ErrorKind::Unsupported.into());
    }
    Ok(xml.to_string())
}

pub fn exists(path: &str) -> io::Result<bool> {
    let (_, root) = connect()?;
    // SAFETY: call on a live interface; the BSTR outlives it.
    match unsafe { root.GetTask(&BSTR::from(path)) }.map_err(os) {
        Ok(_) => Ok(true),
        Err(missing) if missing.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

/// Removes the task at `path`: all or nothing; denied (`PermissionDenied`) for another
/// account's task without administrator rights.
pub fn delete(path: &str) -> io::Result<()> {
    let (_, root) = connect()?;
    let (folder, name) = path.rsplit_once('\\').ok_or(io::ErrorKind::InvalidInput)?;
    // SAFETY: calls on live interfaces; the BSTRs outlive their calls.
    unsafe {
        match folder {
            "" => Ok(root),
            folder => root.GetFolder(&BSTR::from(folder)),
        }
        .and_then(|folder| folder.DeleteTask(&BSTR::from(name), 0))
    }
    .map_err(os)
}

/// Registers `xml`, from `export`, again at `path`, under the account and logon type
/// it names; folders missing on the way are created.
pub fn register(path: &str, xml: &str) -> io::Result<()> {
    let (service, root) = connect()?;
    let none = VARIANT::default();
    // SAFETY: calls on live interfaces; the BSTRs, variants and locals outlive their calls.
    let registered = unsafe {
        let definition = service.NewTask(0)?;
        definition.SetXmlText(&BSTR::from(xml))?;
        let principal = definition.Principal()?;
        let mut logon = TASK_LOGON_TYPE::default();
        principal.LogonType(&mut logon)?;
        let mut user = BSTR::new();
        principal.UserId(&mut user)?;
        let user = if user.is_empty() {
            VARIANT::default()
        } else {
            VARIANT::from(user)
        };
        root.RegisterTaskDefinition(
            &BSTR::from(path),
            &definition,
            TASK_CREATE.0,
            &user,
            &none,
            logon,
            &none,
        )
    };
    registered.map(|_| ()).map_err(os)
}

/// Task Scheduler on this machine, as the caller, and its root folder.
fn connect() -> io::Result<(ITaskService, ITaskFolder)> {
    com::prepare();
    let none = VARIANT::default();
    // SAFETY: COM is initialised on this thread; empty variants mean this machine, as
    // the caller; the BSTR outlives the call.
    let connected = unsafe {
        CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER).and_then(
            |service: ITaskService| {
                service.Connect(&none, &none, &none, &none)?;
                let root = service.GetFolder(&BSTR::from(r"\"))?;
                Ok((service, root))
            },
        )
    };
    connected.map_err(os)
}

/// The tasks of `folder` and of its subfolders, depth first.
fn collect(folder: &ITaskFolder, tasks: &mut Vec<Task>) {
    // SAFETY: calls on live interfaces; collections count from 1 and each index variant
    // outlives its call.
    unsafe {
        if let Ok(found) = folder.GetTasks(TASK_ENUM_HIDDEN.0) {
            for index in 1..=found.Count().unwrap_or(0) {
                let Ok(task) = found.get_Item(&VARIANT::from(index)) else {
                    continue;
                };
                if let Ok(path) = task.Path() {
                    let programs = programs(&task);
                    tasks.push(Task {
                        path: path.to_string(),
                        programs,
                    });
                }
            }
        }
        if let Ok(folders) = folder.GetFolders(0) {
            for index in 1..=folders.Count().unwrap_or(0) {
                if let Ok(sub) = folders.get_Item(&VARIANT::from(index)) {
                    collect(&sub, tasks);
                }
            }
        }
    }
}

/// The program of each "start a program" action, in order; e-mail, message and COM
/// handler actions start none.
fn programs(task: &IRegisteredTask) -> Vec<String> {
    let mut programs = Vec::new();
    // SAFETY: calls on live interfaces with pointers to locals; actions count from 1.
    unsafe {
        let Ok(actions) = task.Definition().and_then(|d| d.Actions()) else {
            return programs;
        };
        let mut count = 0;
        if actions.Count(&mut count).is_err() {
            return programs;
        }
        for index in 1..=count {
            let Ok(action) = actions.get_Item(index) else {
                continue;
            };
            let mut kind = TASK_ACTION_TYPE::default();
            if action.Type(&mut kind).is_err() || kind != TASK_ACTION_EXEC {
                continue;
            }
            let mut path = BSTR::new();
            if let Ok(exec) = action.cast::<IExecAction>()
                && exec.Path(&mut path).is_ok()
                && !path.is_empty()
            {
                programs.push(path.to_string());
            }
        }
    }
    programs
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every Windows has its own tasks under `\Microsoft\Windows`.
    #[test]
    fn lists_windows_own_tasks() {
        let tasks = tasks();
        assert!(tasks.iter().all(|task| task.path.starts_with('\\')));
        assert!(
            tasks
                .iter()
                .any(|task| task.path.starts_with(r"\Microsoft\Windows\"))
        );
    }
}
