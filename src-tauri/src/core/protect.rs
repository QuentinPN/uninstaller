//! Paths and registry keys that are never one program's own: drive roots, Windows, the
//! folders shared by every program or user, and the keys that list every program's
//! entries. Every operation that walks or removes a program's files or keys asks here
//! first (SECURITY.md).

use std::path::{Component, Path, PathBuf};

use crate::core::registry::paths::UNINSTALL;
use crate::win::registry::Hive;

/// Environment variables naming a shared folder; the folder itself is protected.
const SHARED_FOLDERS: [&str; 11] = [
    "ProgramFiles",
    "ProgramFiles(x86)",
    "ProgramW6432",
    "CommonProgramFiles",
    "CommonProgramFiles(x86)",
    "ProgramData",
    "PUBLIC",
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
    "TEMP",
];

/// Environment variables naming the data folders programs and Windows both write in.
pub const DATA_FOLDERS: [&str; 3] = ["APPDATA", "LOCALAPPDATA", "ProgramData"];
/// Windows' secrets under each data folder's `Microsoft`: refused with all they hold.
const WINDOWS_SECRETS: [&str; 5] = [
    "credentials",
    "crypto",
    "protect",
    "vault",
    "systemcertificates",
];
/// Windows' own folders under each data folder's `Microsoft`, refused themselves only:
/// programs keep their folders and shortcuts inside.
const WINDOWS_DATA: [&str; 5] = [
    "",
    "windows",
    r"windows\start menu",
    r"windows\start menu\programs",
    r"windows\start menu\programs\startup",
];

/// True for a drive root, `%SystemRoot%` or anything under it, a shared folder, the
/// folder holding the user profiles or any account's profile, or a path that is not
/// absolute or goes through `..` (it could climb out of anything checked here).
pub fn is_protected(path: &Path) -> bool {
    let path = normalize(path);
    if !path.is_absolute()
        || path.parent().is_none()
        || path.components().any(|c| c == Component::ParentDir)
    {
        return true;
    }
    let var = |name: &str| std::env::var_os(name).map(|v| normalize(Path::new(&v)));
    if var("SystemRoot").is_some_and(|windows| path.starts_with(windows)) {
        return true;
    }
    let profiles = var("USERPROFILE").and_then(|p| p.parent().map(Path::to_path_buf));
    if profiles
        .as_deref()
        .is_some_and(|p| path.parent() == Some(p))
    {
        return true;
    }
    let microsoft = || {
        DATA_FOLDERS
            .iter()
            .filter_map(|name| var(name))
            .map(|data| data.join("microsoft"))
    };
    if microsoft().any(|ms| WINDOWS_SECRETS.iter().any(|s| path.starts_with(ms.join(s)))) {
        return true;
    }
    SHARED_FOLDERS
        .iter()
        .filter_map(|name| var(name))
        .chain(profiles)
        // Per-user installs (lowercase: `var` normalizes).
        .chain(var("LOCALAPPDATA").map(|l| l.join("programs")))
        .chain(microsoft().flat_map(|ms| WINDOWS_DATA.map(|folder| ms.join(folder))))
        .any(|shared| path == shared)
}

/// Keys under `SOFTWARE` that hold entries of many programs: they and their parents are
/// refused, a program's own subkey is not. A 32-bit view key (`WOW6432Node`) is checked
/// as its native twin.
const SHARED_KEYS: [&str; 18] = [
    UNINSTALL,
    r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run",
    r"SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce",
    r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths",
    r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer",
    r"SOFTWARE\Microsoft\Windows\CurrentVersion\Installer",
    r"SOFTWARE\Microsoft\Windows\CurrentVersion\SharedDLLs",
    r"SOFTWARE\Microsoft\Windows\CurrentVersion\Shell Extensions\Approved",
    r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
    r"SOFTWARE\Policies",
    r"SOFTWARE\Classes\CLSID",
    r"SOFTWARE\Classes\Interface",
    r"SOFTWARE\Classes\TypeLib",
    r"SOFTWARE\Classes\AppID",
    r"SOFTWARE\Classes\*",
    r"SOFTWARE\Classes\Directory",
    r"SOFTWARE\Classes\Folder",
    r"SOFTWARE\Classes\SystemFileAssociations",
];

/// True for a hive or an account root, anything outside `SOFTWARE` (`SYSTEM`, hardware,
/// security, the user's settings), `SOFTWARE` itself, a key in `SHARED_KEYS` or one of
/// its parents, or a malformed path (empty part, leading or trailing separator).
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "Phase 2: tested now, called by the next slices")
)]
pub fn is_protected_key(hive: Hive, path: &str) -> bool {
    let path = path.to_lowercase();
    let mut parts: Vec<&str> = path.split('\\').collect();
    if parts.iter().any(|part| part.is_empty()) {
        return true;
    }
    if matches!(hive, Hive::Users) {
        // HKEY_USERS\<SID>\... reads like HKEY_CURRENT_USER\...
        parts.remove(0);
    }
    if parts.get(1) == Some(&"wow6432node") {
        parts.remove(1);
    }
    if parts.len() < 2 || parts[0] != "software" {
        return true;
    }
    SHARED_KEYS.iter().any(|shared| {
        let shared = shared.to_lowercase();
        let shared: Vec<&str> = shared.split('\\').collect();
        shared.starts_with(&parts)
    })
}

/// Lowercase, `/` as `\`, no `.` and no trailing separator: Windows paths compare this way.
pub fn normalize(path: &Path) -> PathBuf {
    let text = path.to_string_lossy().replace('/', "\\").to_lowercase();
    Path::new(&text)
        .components()
        .filter(|c| *c != Component::CurDir)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_windows_data_folders() {
        let appdata = PathBuf::from(std::env::var_os("APPDATA").unwrap());
        assert!(is_protected(&appdata.join("Microsoft")));
        assert!(is_protected(&appdata.join(r"Microsoft\Windows\")));
        assert!(is_protected(
            &appdata.join(r"Microsoft\Windows\Start Menu\Programs")
        ));
        assert!(is_protected(&appdata.join(r"Microsoft\Credentials\ABC")));
        assert!(!is_protected(&appdata.join(r"Microsoft\Teams")));
        assert!(!is_protected(
            &appdata.join(r"Microsoft\Windows\Start Menu\Programs\7-Zip")
        ));
    }

    #[test]
    fn refuses_system_and_shared_paths() {
        let env = |name: &str| std::env::var(name).unwrap();
        for path in [
            r"C:\".to_owned(),
            r"D:\".to_owned(),
            "relative".to_owned(),
            env("SystemRoot"),
            format!(r"{}\System32\drivers", env("SystemRoot")),
            env("ProgramFiles"),
            format!(r"{}\", env("ProgramFiles").to_uppercase()),
            env("ProgramData"),
            env("USERPROFILE"),
            env("LOCALAPPDATA"),
            format!(r"{}\Programs", env("LOCALAPPDATA")),
            env("APPDATA").replace('\\', "/"),
            r"C:\Users".to_owned(),
            format!(r"{}\..\Autre", env("USERPROFILE")),
            Path::new(&env("USERPROFILE"))
                .with_file_name("Autre")
                .to_string_lossy()
                .into_owned(),
            format!(r"{}\..\..\Windows", env("ProgramFiles")),
        ] {
            assert!(is_protected(Path::new(&path)), "{path}");
        }
    }

    #[test]
    fn allows_a_program_folder() {
        let env = |name: &str| std::env::var(name).unwrap();
        for path in [
            format!(r"{}\7-Zip", env("ProgramFiles")),
            format!(r"{}\Programs\App", env("LOCALAPPDATA")),
            r"D:\Games\Some Game".to_owned(),
        ] {
            assert!(!is_protected(Path::new(&path)), "{path}");
        }
    }

    #[test]
    fn refuses_shared_keys_and_their_parents() {
        use crate::win::registry::Hive::{CurrentUser, LocalMachine, Users};
        for (hive, path) in [
            (LocalMachine, ""),
            (LocalMachine, "SYSTEM"),
            (LocalMachine, r"SYSTEM\CurrentControlSet\Services\X"),
            (LocalMachine, "SOFTWARE"),
            (LocalMachine, r"SOFTWARE\Microsoft"),
            (LocalMachine, r"SOFTWARE\Microsoft\Windows"),
            (
                LocalMachine,
                r"software\microsoft\windows\currentversion\uninstall",
            ),
            (LocalMachine, r"SOFTWARE\WOW6432Node"),
            (
                LocalMachine,
                r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion",
            ),
            (LocalMachine, r"SOFTWARE\Classes\CLSID"),
            (CurrentUser, "Environment"),
            (CurrentUser, r"Control Panel\Desktop"),
            (
                CurrentUser,
                r"Software\Microsoft\Windows\CurrentVersion\Run",
            ),
            (CurrentUser, "Software\\X\\"),
            (CurrentUser, r"Software\\X"),
            (Users, "S-1-5-21-1"),
            (Users, r"S-1-5-21-1\Software"),
        ] {
            assert!(is_protected_key(hive, path), "{path}");
        }
        for (hive, path) in [
            (LocalMachine, r"SOFTWARE\Vendor\App"),
            (LocalMachine, r"SOFTWARE\WOW6432Node\Vendor"),
            (
                LocalMachine,
                r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\App",
            ),
            (LocalMachine, r"SOFTWARE\Classes\CLSID\{0}"),
            (CurrentUser, r"Software\UninstallerTest"),
            (Users, r"S-1-5-21-1\Software\Vendor"),
        ] {
            assert!(!is_protected_key(hive, path), "{path}");
        }
    }
}
