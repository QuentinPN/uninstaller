//! Installed-program inventory from the Uninstall keys (F1). Read-only, no elevation.

use serde::Serialize;
use specta::Type;
use std::collections::HashSet;

use crate::core::registry::paths::{UNINSTALL, UNINSTALL_SOURCES};
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
}

impl Scope {
    fn tag(self) -> &'static str {
        match self {
            Scope::Machine64 => "hklm64",
            Scope::Machine32 => "hklm32",
            Scope::User => "hkcu",
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
    /// Opaque identifier (`scope\key`), the only handle the UI sends back.
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
    pub scope: Scope,
    pub kind: Kind,
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
    for (scope, hive, view) in UNINSTALL_SOURCES {
        let Some(root) = Key::open(hive, UNINSTALL, view) else {
            continue;
        };
        for name in root.subkey_names() {
            if let Some(entry) = root.subkey(&name) {
                programs.extend(parse(scope, &name, &entry));
            }
        }
    }
    dedup_and_sort(programs)
}

/// One Uninstall subkey to a program; `None` without a display name.
pub fn parse(scope: Scope, key_name: &str, entry: &impl Entry) -> Option<Program> {
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
        id: format!("{}\\{key_name}", scope.tag()),
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
        scope,
        kind,
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
mod tests {
    use super::*;
    use std::collections::HashMap;

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
        assert!(parse(Scope::User, "k", &Fake::default()).is_none());
    }

    #[test]
    fn reads_fields_and_builds_an_opaque_id() {
        let fake = Fake::named("Outil")
            .s("Publisher", "Uninstaller")
            .s("DisplayVersion", "1.2")
            .s("InstallDate", "20240315")
            .d("EstimatedSize", 2048)
            .d("WindowsInstaller", 1);
        let p = parse(Scope::Machine32, "{GUID}", &fake).unwrap();
        assert_eq!(p.id, "hklm32\\{GUID}");
        assert_eq!(p.publisher.as_deref(), Some("Uninstaller"));
        assert_eq!(p.install_date.as_deref(), Some("2024-03-15"));
        assert_eq!(p.estimated_size_kib, Some(2048));
        assert!(p.msi);
        assert_eq!(p.kind, Kind::App);
    }

    #[test]
    fn classifies_system_components_and_updates() {
        let kind = |f: Fake| parse(Scope::Machine64, "k", &f).unwrap().kind;
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
        let p = parse(Scope::User, "k", &fake).unwrap();
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
        let make = |scope, name: &'static str| parse(scope, "k", &Fake::named(name)).unwrap();
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
}
