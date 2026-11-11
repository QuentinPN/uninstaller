//! Step 2 of an uninstall (F4): runs the program's own uninstaller, without a shell,
//! and waits for it and for every process it starts.

use serde::Serialize;
use specta::Type;
use std::io::Read;
use std::path::{Path, PathBuf};
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
    /// The program's entry is still listed: its uninstaller was cancelled or failed,
    /// or, when not `finished`, it still runs.
    pub still_listed: bool,
}

/// Inno Setup names its Uninstall key `<AppId>_is1`.
const INNO_KEY_SUFFIX: &str = "_is1";
const INNO_SILENT: &str = "/VERYSILENT /SUPPRESSMSGBOXES /NORESTART";
const MSI_SILENT: &str = "/qn /norestart";
const NSIS_SILENT: &str = "/S";
/// The manifest of every NSIS installer and uninstaller names its stub.
const NSIS_MARK: &[u8] = b"Nullsoft.NSIS.exehead";
/// The NSIS stub, its resources included, sits at the start of the file.
const NSIS_STUB_BYTES: u64 = 1 << 20;

/// What uninstalls `program`, without windows when `silent` and its installer is
/// known to allow it (F7), else with its own windows: see `interactive` and `quiet`.
pub fn uninstaller(program: &Program, silent: bool) -> Result<Uninstaller, UninstallFailure> {
    let interactive = interactive(program)?;
    Ok(if silent {
        quiet(program, interactive)
    } else {
        interactive
    })
}

/// Without windows: MSI `/qn` with its checked product code; else the entry's
/// `QuietUninstallString` when it runs the same program file as the interactive
/// command; else Inno Setup `/VERYSILENT`, NSIS `/S`; any other installer
/// (InstallShield…) keeps its windows.
fn quiet(program: &Program, interactive: Uninstaller) -> Uninstaller {
    let declared = program
        .quiet_uninstall_string
        .as_deref()
        .and_then(inventory::split_command)
        .filter(|(exe, _)| !program.msi && same_file(exe, &interactive.exe));
    if let Some((exe, arguments)) = declared {
        return Uninstaller {
            exe,
            arguments: arguments.to_owned(),
        };
    }
    let switch = if program.msi {
        MSI_SILENT
    } else if program.registry_key.ends_with(INNO_KEY_SUFFIX) {
        INNO_SILENT
    } else if is_nsis(&interactive.exe) {
        NSIS_SILENT
    } else {
        return interactive;
    };
    let Uninstaller { exe, arguments } = interactive;
    let arguments = if has_switches(&arguments, switch) {
        arguments
    } else {
        format!("{arguments} {switch}").trim_start().to_owned()
    };
    Uninstaller { exe, arguments }
}

/// Windows paths ignore case.
fn same_file(a: &Path, b: &Path) -> bool {
    a.as_os_str().eq_ignore_ascii_case(b.as_os_str())
}

/// Every switch of `switches` is already among `arguments`, in any case.
fn has_switches(arguments: &str, switches: &str) -> bool {
    switches.split(' ').all(|switch| {
        arguments
            .split_whitespace()
            .any(|argument| argument.eq_ignore_ascii_case(switch))
    })
}

/// The program file is an NSIS uninstaller: its stub's manifest names NSIS.
fn is_nsis(exe: &Path) -> bool {
    let mut stub = Vec::new();
    std::fs::File::open(exe)
        .and_then(|file| file.take(NSIS_STUB_BYTES).read_to_end(&mut stub))
        .is_ok_and(|_| {
            stub.windows(NSIS_MARK.len())
                .any(|bytes| bytes == NSIS_MARK)
        })
}

/// With its own windows: `msiexec /X` with its product code for a Windows Installer
/// entry, else the program file its `UninstallString` names, which must still be on
/// disk.
fn interactive(program: &Program) -> Result<Uninstaller, UninstallFailure> {
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

/// Runs `program`'s uninstaller, without windows when `silent` allows it, and waits
/// until it and all it started have ended, or until `stop` turns true.
pub fn run(
    program: &Program,
    silent: bool,
    stop: &AtomicBool,
) -> Result<Uninstalled, UninstallFailure> {
    let Uninstaller { exe, arguments } = uninstaller(program, silent)?;
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
        still_listed: inventory::find(&program.id).is_some(),
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
        let quoted = uninstaller(&with_command(&format!("\"{cmd}\" /c exit 0")), false).unwrap();
        assert_eq!(quoted.exe, PathBuf::from(&cmd));
        assert_eq!(quoted.arguments, "/c exit 0");
        let bare = uninstaller(&with_command(&format!("{cmd}  /S")), false).unwrap();
        assert_eq!(
            (bare.exe, bare.arguments.as_str()),
            (PathBuf::from(&cmd), "/S")
        );
        assert_eq!(
            uninstaller(&with_command(r"C:\Absent\Uninstaller\unins000.exe"), false),
            Err(UninstallFailure::Missing)
        );
        for unsupported in [
            "unins000.exe /S",
            r"%ProgramFiles%\a\u.exe",
            "rundll32 x.dll",
        ] {
            assert_eq!(
                uninstaller(&with_command(unsupported), true),
                Err(UninstallFailure::Unsupported),
                "{unsupported}"
            );
        }
        assert_eq!(
            uninstaller(&program("p", None), true),
            Err(UninstallFailure::NoUninstaller)
        );
    }

    #[test]
    fn an_msi_entry_runs_msiexec_with_its_checked_product_code() {
        let mut entry = with_command("MsiExec.exe /I{whatever}");
        entry.msi = true;
        entry.registry_key = r"HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\{23170F69-40C1-2702-2201-000001000000}".into();
        let msi = uninstaller(&entry, false).unwrap();
        assert!(msi.exe.ends_with(r"System32\msiexec.exe"));
        assert_eq!(msi.arguments, "/X{23170F69-40C1-2702-2201-000001000000}");
        for bad in [
            r"Uninstall\{23170F69-40C1-2702-2201-00000100000}",
            r"Uninstall\{23170F69-40C1-2702-2201-00000100000Z}",
            r"Uninstall\7-Zip",
        ] {
            entry.registry_key = bad.into();
            assert_eq!(
                uninstaller(&entry, true),
                Err(UninstallFailure::Unsupported),
                "{bad}"
            );
        }
    }

    #[test]
    fn silent_uses_the_quiet_command_or_the_installer_switches() {
        let cmd = std::env::var("ComSpec").unwrap();
        let silent = |entry: &Program| uninstaller(entry, true).unwrap().arguments;

        let mut msi = with_command("MsiExec.exe /I{whatever}");
        msi.msi = true;
        msi.registry_key = r"Uninstall\{23170F69-40C1-2702-2201-000001000000}".into();
        // Its product code, checked, even when the entry declares another command.
        msi.quiet_uninstall_string = Some(format!("\"{cmd}\" /c"));
        assert_eq!(
            silent(&msi),
            "/X{23170F69-40C1-2702-2201-000001000000} /qn /norestart"
        );

        let mut inno = with_command(&format!("\"{cmd}\""));
        inno.registry_key = r"Uninstall\Uninstaller Fixture_is1".into();
        assert_eq!(silent(&inno), "/VERYSILENT /SUPPRESSMSGBOXES /NORESTART");
        inno.uninstall_string = Some(format!(
            "\"{cmd}\" /verysilent /suppressmsgboxes /norestart"
        ));
        assert_eq!(silent(&inno), "/verysilent /suppressmsgboxes /norestart");

        // Declared by the entry: preferred, unless it names no program file on disk.
        inno.quiet_uninstall_string = Some(format!("\"{cmd}\" /quiet"));
        assert_eq!(silent(&inno), "/quiet");
        let elsewhere = std::env::current_exe().unwrap();
        inno.quiet_uninstall_string = Some(format!("\"{}\" /quiet", elsewhere.display()));
        assert_eq!(silent(&inno), "/verysilent /suppressmsgboxes /norestart");
        inno.quiet_uninstall_string = Some(r"C:\Absent\Uninstaller\u.exe /quiet".into());
        assert_eq!(silent(&inno), "/verysilent /suppressmsgboxes /norestart");

        let dir = std::env::temp_dir().join("Uninstaller-nsis-test");
        std::fs::create_dir_all(&dir).unwrap();
        let nsis = dir.join("uninst.exe");
        std::fs::write(&nsis, [&b"MZ stub "[..], NSIS_MARK, b" data"].concat()).unwrap();
        let other = dir.join("other.exe");
        std::fs::write(&other, b"MZ InstallShield").unwrap();
        let nsis_entry = with_command(&format!("\"{}\" _?=C:\\App", nsis.display()));
        let other_entry = with_command(&format!("\"{}\" -x", other.display()));
        let (nsis_args, other_args) = (silent(&nsis_entry), silent(&other_entry));
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(nsis_args, r"_?=C:\App /S");
        // Unknown installer: its own windows, unchanged.
        assert_eq!(other_args, "-x");
        assert_eq!(
            uninstaller(&inno, false).unwrap().arguments,
            "/verysilent /suppressmsgboxes /norestart"
        );
    }

    #[test]
    fn runs_the_uninstaller_to_its_end() {
        let cmd = std::env::var("ComSpec").unwrap();
        let done = run(
            &with_command(&format!("\"{cmd}\" /c exit 7")),
            false,
            &AtomicBool::new(false),
        );
        assert_eq!(
            done,
            Ok(Uninstalled {
                exit_code: Some(7),
                finished: true,
                still_listed: false
            })
        );
    }
}
