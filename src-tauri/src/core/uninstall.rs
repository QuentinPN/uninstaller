//! Step 2 of an uninstall (F4): runs the program's own uninstaller, without a shell,
//! and waits for it and for every process it starts.

use serde::Serialize;
use specta::Type;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use crate::core::inventory::{self, Program};
use crate::win::process;

/// A program's uninstaller: a program file and its command-line arguments.
#[derive(Debug, PartialEq)]
pub struct Uninstaller {
    pub exe: PathBuf,
    pub arguments: String,
}

/// Why the uninstaller did not run.
#[derive(Debug, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum UninstallFailure {
    /// The id names no installed program.
    NotFound,
    /// Its entry names no uninstaller.
    NoUninstaller,
    /// Its command names no program file by full path, nor a valid MSI product code.
    Unsupported,
    /// The program file it names is gone from the disk.
    Missing,
    /// It asked for the administrator and the UAC prompt was declined.
    Refused,
    /// Windows would not start it.
    Failed,
}

/// How the uninstaller ended.
#[derive(Debug, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Uninstalled {
    /// Its exit code, kept for the record: installers give codes different meanings.
    /// None while it still runs.
    pub exit_code: Option<u32>,
    /// False when the wait was stopped before it and all it started had ended.
    pub finished: bool,
}

/// What uninstalls `program`: `msiexec /X` with its product code for a Windows
/// Installer entry, else the program file its `UninstallString` names, which must
/// still be on disk.
pub fn uninstaller(program: &Program) -> Result<Uninstaller, UninstallFailure> {
    if program.msi {
        let code = program
            .registry_key
            .rsplit('\\')
            .next()
            .filter(|name| is_product_code(name))
            .ok_or(UninstallFailure::Unsupported)?;
        let system = std::env::var_os("SystemRoot").ok_or(UninstallFailure::Failed)?;
        return Ok(Uninstaller {
            exe: PathBuf::from(system).join(r"System32\msiexec.exe"),
            arguments: format!("/X{code}"),
        });
    }
    let command = program
        .uninstall_string
        .as_deref()
        .filter(|command| !command.trim().is_empty())
        .ok_or(UninstallFailure::NoUninstaller)?;
    let (exe, arguments) =
        inventory::split_command(command).ok_or(UninstallFailure::Unsupported)?;
    if !exe.is_file() {
        return Err(UninstallFailure::Missing);
    }
    Ok(Uninstaller {
        exe,
        arguments: arguments.to_owned(),
    })
}

/// Runs `program`'s uninstaller and waits until it and all it started have ended, or
/// until `stop` turns true.
pub fn run(program: &Program, stop: &AtomicBool) -> Result<Uninstalled, UninstallFailure> {
    let Uninstaller { exe, arguments } = uninstaller(program)?;
    let root = process::start(&exe, &arguments).map_err(|error| {
        if process::is_refused(&error) {
            UninstallFailure::Refused
        } else {
            UninstallFailure::Failed
        }
    })?;
    let finished = process::wait_tree(&root, stop);
    Ok(Uninstalled {
        exit_code: root.exit_code(),
        finished,
    })
}

/// `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}`, hexadecimal digits only.
fn is_product_code(name: &str) -> bool {
    let Some(inner) = name.strip_prefix('{').and_then(|n| n.strip_suffix('}')) else {
        return false;
    };
    let groups: Vec<&str> = inner.split('-').collect();
    groups.iter().map(|g| g.len()).eq([8, 4, 4, 4, 12])
        && groups
            .iter()
            .all(|g| g.chars().all(|c| c.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::inventory::tests::program;

    fn with_command(command: &str) -> Program {
        let mut entry = program("p", None);
        entry.uninstall_string = Some(command.into());
        entry
    }

    #[test]
    fn reads_the_program_file_and_its_arguments() {
        let cmd = std::env::var("ComSpec").unwrap();
        let quoted = uninstaller(&with_command(&format!("\"{cmd}\" /c exit 0"))).unwrap();
        assert_eq!(quoted.exe, PathBuf::from(&cmd));
        assert_eq!(quoted.arguments, "/c exit 0");
        let bare = uninstaller(&with_command(&format!("{cmd}  /S"))).unwrap();
        assert_eq!(
            (bare.exe, bare.arguments.as_str()),
            (PathBuf::from(&cmd), "/S")
        );
        assert_eq!(
            uninstaller(&with_command(r"C:\Absent\Uninstaller\unins000.exe")),
            Err(UninstallFailure::Missing)
        );
        for unsupported in [
            "unins000.exe /S",
            r"%ProgramFiles%\a\u.exe",
            "rundll32 x.dll",
        ] {
            assert_eq!(
                uninstaller(&with_command(unsupported)),
                Err(UninstallFailure::Unsupported),
                "{unsupported}"
            );
        }
        assert_eq!(
            uninstaller(&program("p", None)),
            Err(UninstallFailure::NoUninstaller)
        );
    }

    #[test]
    fn an_msi_entry_runs_msiexec_with_its_checked_product_code() {
        let mut entry = with_command("MsiExec.exe /I{whatever}");
        entry.msi = true;
        entry.registry_key = r"HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\{23170F69-40C1-2702-2201-000001000000}".into();
        let msi = uninstaller(&entry).unwrap();
        assert!(msi.exe.ends_with(r"System32\msiexec.exe"));
        assert_eq!(msi.arguments, "/X{23170F69-40C1-2702-2201-000001000000}");
        for bad in [
            r"Uninstall\{23170F69-40C1-2702-2201-00000100000}",
            r"Uninstall\{23170F69-40C1-2702-2201-00000100000Z}",
            r"Uninstall\7-Zip",
        ] {
            entry.registry_key = bad.into();
            assert_eq!(
                uninstaller(&entry),
                Err(UninstallFailure::Unsupported),
                "{bad}"
            );
        }
    }

    #[test]
    fn runs_the_uninstaller_to_its_end() {
        let cmd = std::env::var("ComSpec").unwrap();
        let done = run(
            &with_command(&format!("\"{cmd}\" /c exit 7")),
            &AtomicBool::new(false),
        );
        assert_eq!(
            done,
            Ok(Uninstalled {
                exit_code: Some(7),
                finished: true
            })
        );
    }
}
