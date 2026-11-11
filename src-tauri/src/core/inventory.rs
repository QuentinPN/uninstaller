//! Installed-program inventory from the Uninstall keys (F1). Read-only, no elevation.

use serde::Serialize;
use specta::Type;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use crate::core::registry::paths::Source;
use crate::win::registry::Key;

/// Where the program is registered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Scope {
    /// HKLM, 64-bit view.
    Machine64,
    /// HKLM, 32-bit view (WOW6432Node).
    Machine32,
    /// HKCU: installed for the current user only.
    User,
    /// HKU: installed for another account of this PC, whose hive is loaded.
    OtherUser,
}

impl Scope {
    pub(crate) fn tag(self) -> &'static str {
        match self {
            Scope::Machine64 => "hklm64",
            Scope::Machine32 => "hklm32",
            Scope::User => "hkcu",
            Scope::OtherUser => "hku",
        }
    }
}

/// What the entry is. Only `App` is shown by default (F1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    App,
    /// `SystemComponent = 1`: hidden by its installer.
    SystemComponent,
    /// An update or hotfix attached to another product.
    Update,
}

#[derive(Clone, Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Program {
    /// Opaque identifier (`scope\key`, `hku\sid\key`), the only handle the UI sends back.
    pub id: String,
    pub name: String,
    pub publisher: Option<String>,
    pub version: Option<String>,
    /// `yyyy-mm-dd`: `InstallDate`, else the key's last write.
    pub install_date: Option<String>,
    /// `EstimatedSize` in KiB, as declared by the installer.
    pub estimated_size_kib: Option<u32>,
    pub install_location: Option<String>,
    pub uninstall_string: Option<String>,
    pub quiet_uninstall_string: Option<String>,
    pub display_icon: Option<String>,
    pub url: Option<String>,
    pub comments: Option<String>,
    /// Managed by Windows Installer (the key name is the MSI product code).
    pub msi: bool,
    /// Full key path, as regedit shows it.
    pub registry_key: String,
    pub scope: Scope,
    /// The other account's profile folder name, for `Scope::OtherUser`.
    pub profile: Option<String>,
    pub kind: Kind,
    /// Its uninstaller (or, without one, its declared folder) is gone from the disk (F3).
    pub orphan: bool,
}

/// The id tag of a program the user named for a forced uninstall: no entry has it.
const UNLISTED_TAG: &str = "unlisted";

impl Program {
    /// Named by the user for a forced uninstall: the broker never knows it.
    pub fn is_unlisted(id: &str) -> bool {
        id.split_once('\\')
            .is_some_and(|(tag, _)| tag == UNLISTED_TAG)
    }

    /// A program no entry lists, named for a forced uninstall (F6): only its name and
    /// folder are known. It has no uninstaller, and `scope` says nothing.
    pub fn unlisted(name: String, folder: Option<PathBuf>) -> Self {
        Self {
            id: format!(r"{UNLISTED_TAG}\{name}"),
            name,
            publisher: None,
            version: None,
            install_date: None,
            estimated_size_kib: None,
            install_location: folder.map(|f| f.to_string_lossy().into_owned()),
            uninstall_string: None,
            quiet_uninstall_string: None,
            display_icon: None,
            url: None,
            comments: None,
            msi: false,
            registry_key: String::new(),
            scope: Scope::User,
            profile: None,
            kind: Kind::App,
            orphan: true,
        }
    }

    /// `InstallLocation` without the quotes some installers add, if it is a folder.
    pub fn install_folder(&self) -> Option<PathBuf> {
        self.declared_folder().filter(|f| f.is_dir())
    }

    fn declared_folder(&self) -> Option<PathBuf> {
        let location = self.install_location.as_deref()?.trim().trim_matches('"');
        (!location.is_empty()).then(|| PathBuf::from(location))
    }

    /// The uninstaller named by full path is missing or, without an uninstall command,
    /// the declared folder is. A command without a full path (`MsiExec.exe /X…`,
    /// `%SystemRoot%\…`) proves nothing either way.
    fn is_orphan(&self) -> bool {
        match self.uninstall_string.as_deref() {
            Some(command) => command_program(command).is_some_and(|exe| !exe.is_file()),
            None => self.declared_folder().is_some_and(|f| !f.exists()),
        }
    }
}

/// The program file a command line runs, when it names one by full path.
pub fn command_program(command: &str) -> Option<PathBuf> {
    split_command(command).map(|(program, _)| program)
}

/// A command line as the program file it names by full path and the arguments after.
pub fn split_command(command: &str) -> Option<(PathBuf, &str)> {
    let command = command.trim();
    let (path, arguments) = match command.strip_prefix('"') {
        Some(rest) => rest.split_once('"').unwrap_or((rest, "")),
        // Unquoted, maybe with spaces: up to the first ".exe" (ASCII lowercasing keeps
        // byte offsets).
        None => command.split_at(command.to_ascii_lowercase().find(".exe")? + 4),
    };
    let path = Path::new(path);
    (path.is_absolute() && !path.to_string_lossy().contains('%'))
        .then(|| (path.to_path_buf(), arguments.trim()))
}

/// Values of one Uninstall subkey; implemented by `win::registry::Key` and by test fakes.
pub trait Entry {
    fn string(&self, name: &str) -> Option<String>;
    fn dword(&self, name: &str) -> Option<u32>;
    fn last_write(&self) -> Option<u64>;
}

impl Entry for Key {
    fn string(&self, name: &str) -> Option<String> {
        Key::string(self, name)
    }
    fn dword(&self, name: &str) -> Option<u32> {
        Key::dword(self, name)
    }
    fn last_write(&self) -> Option<u64> {
        Key::last_write(self)
    }
}

/// Reads every source, drops duplicates (same name, version, publisher), sorts by name.
pub fn scan() -> Vec<Program> {
    let mut programs = Vec::new();
    for source in Source::all() {
        let Some(root) = source.open() else {
            continue;
        };
        for name in root.subkey_names() {
            if let Some(entry) = root.subkey(&name) {
                programs.extend(parse(&source, &name, &entry).map(checked));
            }
        }
    }
    dedup_and_sort(programs)
}

/// Reads one program back from its opaque id; `None` if the id is malformed or the
/// key is gone. Lets commands take an id instead of a path from the UI.
pub fn find(id: &str) -> Option<Program> {
    let (source, key_name) = Source::locate(id)?;
    let entry = source.open()?.subkey(key_name)?;
    parse(&source, key_name, &entry).map(checked)
}

/// Programs kept as listed before their uninstaller ran, newest last: their entry is
/// gone once uninstalled, and the leftover scan still needs their name, publisher and
/// folder. A batch uninstall keeps several.
pub struct Kept(Mutex<Vec<Program>>);

impl Kept {
    /// More than any batch: the oldest is dropped past this.
    const MAX: usize = 64;

    pub const fn new() -> Self {
        Self(Mutex::new(Vec::new()))
    }

    /// Keeps `program`, replacing what was kept under its id.
    pub fn keep(&self, program: Program) {
        let mut kept = self.lock();
        kept.retain(|other| other.id != program.id);
        if kept.len() == Self::MAX {
            kept.remove(0);
        }
        kept.push(program);
    }

    /// The program `id`: listed, or kept before its uninstaller ran.
    pub fn find(&self, id: &str) -> Option<Program> {
        find(id).or_else(|| self.lock().iter().find(|kept| kept.id == id).cloned())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Program>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Adds what the registry alone cannot tell: whether the program is still on disk.
fn checked(mut program: Program) -> Program {
    program.orphan = program.is_orphan();
    program
}

/// One Uninstall subkey to a program; `None` without a display name.
pub fn parse(source: &Source, key_name: &str, entry: &impl Entry) -> Option<Program> {
    let name = entry.string("DisplayName")?;
    let kind = if entry.dword("SystemComponent") == Some(1) {
        Kind::SystemComponent
    } else if entry.string("ParentKeyName").is_some()
        || entry.string("ReleaseType").is_some_and(|t| {
            UPDATE_RELEASE_TYPES
                .iter()
                .any(|u| t.eq_ignore_ascii_case(u))
        })
    {
        Kind::Update
    } else {
        Kind::App
    };
    Some(Program {
        id: source.id(key_name),
        name,
        publisher: entry.string("Publisher"),
        version: entry.string("DisplayVersion"),
        install_date: entry
            .string("InstallDate")
            .and_then(|d| date_from_compact(&d))
            .or_else(|| entry.last_write().map(date_from_filetime)),
        estimated_size_kib: entry.dword("EstimatedSize").filter(|&s| s > 0),
        install_location: entry.string("InstallLocation"),
        uninstall_string: entry.string("UninstallString"),
        quiet_uninstall_string: entry.string("QuietUninstallString"),
        display_icon: entry.string("DisplayIcon"),
        url: entry
            .string("URLInfoAbout")
            .or_else(|| entry.string("HelpLink")),
        comments: entry.string("Comments"),
        msi: entry.dword("WindowsInstaller") == Some(1),
        registry_key: source.shown_key(key_name),
        scope: source.scope,
        profile: source.profile.as_ref().and_then(|p| p.name.clone()),
        kind,
        orphan: false,
    })
}

const UPDATE_RELEASE_TYPES: [&str; 4] = ["Update", "Hotfix", "Security Update", "Service Pack"];

fn dedup_and_sort(programs: Vec<Program>) -> Vec<Program> {
    let mut seen = HashSet::new();
    let mut unique: Vec<Program> = programs
        .into_iter()
        .filter(|p| {
            seen.insert((
                p.name.to_lowercase(),
                p.version.clone(),
                p.publisher.as_deref().map(str::to_lowercase),
            ))
        })
        .collect();
    unique.sort_by_cached_key(|p| p.name.to_lowercase());
    unique
}

/// `yyyymmdd` (the documented `InstallDate` format) to `yyyy-mm-dd`.
fn date_from_compact(raw: &str) -> Option<String> {
    let digits = raw.as_bytes();
    if digits.len() != 8 || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let (y, m, d) = (&raw[..4], &raw[4..6], &raw[6..]);
    let month: u8 = m.parse().ok()?;
    let day: u8 = d.parse().ok()?;
    ((1..=12).contains(&month) && (1..=31).contains(&day)).then(|| format!("{y}-{m}-{d}"))
}

/// FILETIME (100 ns since 1601-01-01 UTC) to `yyyy-mm-dd` (UTC).
fn date_from_filetime(filetime: u64) -> String {
    const DAYS_1601_TO_1970: i64 = 134_774;
    let days = (filetime / 10_000_000 / 86_400) as i64 - DAYS_1601_TO_1970;
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::core::registry::paths::UNINSTALL_SOURCES;
    use std::collections::HashMap;

    /// A bare program for tests of other modules.
    pub(crate) fn program(id: &str, install_location: Option<&str>) -> Program {
        Program {
            id: id.into(),
            name: id.into(),
            publisher: None,
            version: None,
            install_date: None,
            estimated_size_kib: None,
            install_location: install_location.map(Into::into),
            uninstall_string: None,
            quiet_uninstall_string: None,
            display_icon: None,
            url: None,
            comments: None,
            msi: false,
            registry_key: String::new(),
            scope: Scope::Machine64,
            profile: None,
            kind: Kind::App,
            orphan: false,
        }
    }

    #[test]
    fn kept_programs_are_found_by_id_newest_replacing() {
        let kept = Kept::new();
        assert!(kept.find("gone").is_none());
        kept.keep(program("gone", None));
        kept.keep(program("other", Some(r"C:\Old")));
        kept.keep(program("other", Some(r"C:\New")));
        assert_eq!(kept.find("gone").map(|p| p.id), Some("gone".into()));
        assert_eq!(
            kept.find("other").and_then(|p| p.install_location),
            Some(r"C:\New".into())
        );
        for i in 0..Kept::MAX {
            kept.keep(program(&format!("filler{i}"), None));
        }
        assert!(kept.find("gone").is_none());
    }

    /// The fixed source of `scope`.
    fn at(scope: Scope) -> Source {
        UNINSTALL_SOURCES
            .into_iter()
            .find(|s| s.scope == scope)
            .unwrap()
    }

    #[derive(Default)]
    struct Fake {
        strings: HashMap<&'static str, &'static str>,
        dwords: HashMap<&'static str, u32>,
        last_write: Option<u64>,
    }

    impl Fake {
        fn named(name: &'static str) -> Self {
            let mut fake = Fake::default();
            fake.strings.insert("DisplayName", name);
            fake
        }
        fn s(mut self, k: &'static str, v: &'static str) -> Self {
            self.strings.insert(k, v);
            self
        }
        fn d(mut self, k: &'static str, v: u32) -> Self {
            self.dwords.insert(k, v);
            self
        }
    }

    impl Entry for Fake {
        fn string(&self, name: &str) -> Option<String> {
            self.strings.get(name).map(|v| v.to_string())
        }
        fn dword(&self, name: &str) -> Option<u32> {
            self.dwords.get(name).copied()
        }
        fn last_write(&self) -> Option<u64> {
            self.last_write
        }
    }

    #[test]
    fn without_display_name_is_skipped() {
        assert!(parse(&at(Scope::User), "k", &Fake::default()).is_none());
    }

    #[test]
    fn reads_fields_and_builds_an_opaque_id() {
        let fake = Fake::named("Outil")
            .s("Publisher", "Uninstaller")
            .s("DisplayVersion", "1.2")
            .s("InstallDate", "20240315")
            .d("EstimatedSize", 2048)
            .d("WindowsInstaller", 1);
        let p = parse(&at(Scope::Machine32), "{GUID}", &fake).unwrap();
        assert_eq!(p.id, "hklm32\\{GUID}");
        assert_eq!(p.publisher.as_deref(), Some("Uninstaller"));
        assert_eq!(p.install_date.as_deref(), Some("2024-03-15"));
        assert_eq!(p.estimated_size_kib, Some(2048));
        assert!(p.msi);
        assert_eq!(p.kind, Kind::App);
    }

    #[test]
    fn classifies_system_components_and_updates() {
        let kind = |f: Fake| parse(&at(Scope::Machine64), "k", &f).unwrap().kind;
        assert_eq!(
            kind(Fake::named("A").d("SystemComponent", 1)),
            Kind::SystemComponent
        );
        assert_eq!(
            kind(Fake::named("B").s("ParentKeyName", "Office")),
            Kind::Update
        );
        assert_eq!(
            kind(Fake::named("C").s("ReleaseType", "hotfix")),
            Kind::Update
        );
        assert_eq!(kind(Fake::named("D").d("SystemComponent", 0)), Kind::App);
    }

    #[test]
    fn date_falls_back_to_last_write_when_missing_or_invalid() {
        let mut fake = Fake::named("A").s("InstallDate", "15/03/2024");
        // 2024-03-15T00:00:00Z as FILETIME.
        fake.last_write = Some(133_549_920_000_000_000);
        let p = parse(&at(Scope::User), "k", &fake).unwrap();
        assert_eq!(p.install_date.as_deref(), Some("2024-03-15"));
    }

    #[test]
    fn compact_dates_are_validated() {
        assert_eq!(date_from_compact("20241231").as_deref(), Some("2024-12-31"));
        assert_eq!(date_from_compact("20241301"), None);
        assert_eq!(date_from_compact("2024123"), None);
    }

    #[test]
    fn filetime_epoch_and_leap_day() {
        assert_eq!(date_from_filetime(116_444_736_000_000_000), "1970-01-01");
        assert_eq!(date_from_filetime(133_536_960_000_000_000), "2024-02-29");
    }

    #[test]
    fn duplicates_are_dropped_and_names_sorted() {
        let make = |scope, name: &'static str| parse(&at(scope), "k", &Fake::named(name)).unwrap();
        let out = dedup_and_sort(vec![
            make(Scope::Machine64, "zeta"),
            make(Scope::Machine32, "Alpha"),
            make(Scope::User, "alpha"),
        ]);
        let names: Vec<_> = out.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Alpha", "zeta"]);
    }

    /// Real registry, read-only: the machine has programs and they all have a name.
    #[test]
    fn scans_this_machine() {
        let programs = scan();
        assert!(!programs.is_empty());
        assert!(programs.iter().all(|p| !p.name.is_empty()));
    }

    #[test]
    fn command_program_needs_a_full_path() {
        let exe = |c: &str| command_program(c).map(|p| p.to_string_lossy().into_owned());
        assert_eq!(
            exe(r#""C:\Program Files\App\unins000.exe" /SILENT"#).as_deref(),
            Some(r"C:\Program Files\App\unins000.exe")
        );
        assert_eq!(
            exe(r"C:\Program Files\App\Uninstall.EXE /S").as_deref(),
            Some(r"C:\Program Files\App\Uninstall.EXE")
        );
        assert_eq!(
            split_command(r#""C:\App\u.exe /S"#),
            Some((PathBuf::from(r"C:\App\u.exe /S"), ""))
        );
        for unknown in [
            "MsiExec.exe /X{GUID}",
            r"%SystemRoot%\System32\rundll32.exe x",
            r"rundll32.exe C:\x.dll",
            "",
        ] {
            assert_eq!(exe(unknown), None, "{unknown}");
        }
    }

    #[test]
    fn orphan_when_the_uninstaller_or_the_folder_is_gone() {
        let here = std::env::current_exe().unwrap();
        let gone = r"C:\Nope\Uninstaller-test";
        let orphan = |uninstall: Option<String>, location: Option<&str>| {
            let mut p = program("x", location);
            p.uninstall_string = uninstall;
            p.is_orphan()
        };
        assert!(!orphan(Some(format!(r#""{}" /S"#, here.display())), None));
        assert!(orphan(Some(format!(r#""{gone}\unins000.exe" /S"#)), None));
        assert!(!orphan(Some("MsiExec.exe /X{GUID}".into()), Some(gone)));
        assert!(orphan(None, Some(gone)));
        assert!(!orphan(None, None));
    }

    #[test]
    fn find_reads_back_a_scanned_program_and_rejects_bad_ids() {
        let first = scan().into_iter().next().unwrap();
        assert_eq!(find(&first.id).map(|p| p.name), Some(first.name));
        assert!(find(r"nope\k").is_none());
    }
}
