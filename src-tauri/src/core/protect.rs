//! Paths that are never one program's own: drive roots, Windows, and the folders shared
//! by every program or user. Every operation that walks or removes a program's files
//! asks here first (SECURITY.md).

use std::path::{Component, Path, PathBuf};

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

/// True for a drive root, `%SystemRoot%` or anything under it, a shared folder, the
/// folder holding the user profiles, or a path that is not absolute.
pub fn is_protected(path: &Path) -> bool {
    let path = normalize(path);
    if !path.is_absolute() || path.parent().is_none() {
        return true;
    }
    let var = |name: &str| std::env::var_os(name).map(|v| normalize(Path::new(&v)));
    if var("SystemRoot").is_some_and(|windows| path.starts_with(windows)) {
        return true;
    }
    let profiles = var("USERPROFILE").and_then(|p| p.parent().map(Path::to_path_buf));
    SHARED_FOLDERS
        .iter()
        .filter_map(|name| var(name))
        .chain(profiles)
        // Per-user installs (lowercase: `var` normalizes).
        .chain(var("LOCALAPPDATA").map(|l| l.join("programs")))
        .any(|shared| path == shared)
}

/// Lowercase, `/` as `\`, no `.` and no trailing separator: Windows paths compare this way.
fn normalize(path: &Path) -> PathBuf {
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
}
