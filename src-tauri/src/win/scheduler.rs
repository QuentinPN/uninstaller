//! Task Scheduler: the registered tasks and the programs they start, read only.

use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::System::TaskScheduler::{
    IExecAction, IRegisteredTask, ITaskFolder, ITaskService, TASK_ACTION_EXEC, TASK_ACTION_TYPE,
    TASK_ENUM_HIDDEN, TaskScheduler,
};
use windows::Win32::System::Variant::VARIANT;
use windows::core::{BSTR, Interface};

use crate::win::com;

/// A registered task: its full path (`\Folder\Name`) and the program of each of its
/// "start a program" actions, as written (variables not expanded, maybe quoted).
pub struct Task {
    pub path: String,
    pub programs: Vec<String>,
}

/// Every task the caller can see, hidden ones included; without administrator rights,
/// other accounts' tasks may be missing or show no program.
pub fn tasks() -> Vec<Task> {
    com::prepare();
    // SAFETY: COM is initialised on this thread.
    let service: Option<ITaskService> =
        unsafe { CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER) }.ok();
    let none = VARIANT::default();
    let mut tasks = Vec::new();
    // SAFETY: empty variants mean this machine, as the caller; the BSTR outlives the call.
    let root = service.and_then(|service| unsafe {
        service.Connect(&none, &none, &none, &none).ok()?;
        service.GetFolder(&BSTR::from(r"\")).ok()
    });
    if let Some(root) = root {
        collect(&root, &mut tasks);
    }
    tasks
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
