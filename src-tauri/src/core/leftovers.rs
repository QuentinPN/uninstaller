//! Leftover scan (F5): the folders a removed program may leave in the shared roots
//! (Program Files, ProgramData, AppData, Start Menus), its shortcuts and its registry
//! keys under `SOFTWARE`, its services and its scheduled tasks. Read only: it proposes,
//! the quarantine moves. Every folder passes `protect::is_protected`, every key
//! `protect::is_protected_key`, every task `protect::is_protected_task`; links and
//! junctions are never listed nor followed.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use specta::Type;

use crate::core::inventory::{self, Program};
use crate::core::protect;
use crate::core::registry::paths;
use crate::win::registry::{Hive, Key, View};
use crate::win::{icon, scheduler, shortcut};

/// How surely a folder belongs to the program (SECURITY.md 5). Ordered from the
/// surest: the Safe depth keeps `Safe`, Moderate adds `Moderate`, Advanced adds all.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Confidence {
    /// At least two independent signals: install folder, product name, publisher.
    Safe,
    /// One signal: the install folder, or a folder named exactly after the product.
    Moderate,
    /// Only part of the name matches; never preselected.
    Advanced,
}

/// Ordered surest first, then by path.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Type)]
pub struct Leftover {
    pub confidence: Confidence,
    pub path: PathBuf,
}

/// A registry key as regedit shows it (`HKEY_CURRENT_USER\Software\Acme\Widget`), a
/// service's name or a scheduled task's path; ordered surest first, then by name.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Type)]
pub struct Named {
    pub confidence: Confidence,
    pub name: String,
}

/// What the scan matches folders against, read from the inventory entry.
pub struct Target {
    product: String,
    publisher: Option<String>,
    install: Option<PathBuf>,
    /// Other programs' install folders, normalized: a folder holding one is never ours.
    others: Vec<PathBuf>,
    /// Other programs' names and publishers, as `key` reduces them: a name one of them
    /// also matches no longer tells whose a folder or key is.
    other_names: Vec<String>,
    other_publishers: Vec<String>,
}

/// Names shorter than this match nothing: too many unrelated folders share them.
const MIN_KEY: usize = 3;
/// Words many unrelated programs start with: alone they are no partial match.
const GENERIC_WORDS: [&str; 18] = [
    "windows",
    "microsoft",
    "common",
    "shared",
    "system",
    "update",
    "updater",
    "program",
    "programs",
    "tools",
    "runtime",
    "driver",
    "drivers",
    "service",
    "services",
    "setup",
    "installer",
    "launcher",
];
/// A partial match needs at least this many characters in common.
const MIN_FRAGMENT: usize = 5;

/// Name parts that tell versions or builds apart, not products.
const ARCH_WORDS: [&str; 6] = ["x64", "x86", "64bit", "32bit", "amd64", "arm64"];
/// Company-form words dropped from a publisher's name.
const COMPANY_WORDS: [&str; 17] = [
    "incorporated",
    "systems",
    "technologies",
    "inc",
    "ltd",
    "llc",
    "gmbh",
    "corp",
    "corporation",
    "co",
    "sa",
    "sas",
    "ag",
    "bv",
    "limited",
    "srl",
    "sarl",
];

/// Everything a program may have left, by kind, each list surest first.
#[derive(Debug, Serialize, Type)]
pub struct Found {
    pub folders: Vec<Leftover>,
    pub shortcuts: Vec<Leftover>,
    pub keys: Vec<Named>,
    pub services: Vec<Named>,
    pub tasks: Vec<Named>,
}

/// Scans every kind of leftover of `program` among `all` the installed programs, read
/// only. `None` once `cancelled` turns true, checked before each kind.
pub fn find(program: &Program, all: &[Program], cancelled: &AtomicBool) -> Option<Found> {
    let target = Target::of(program, all);
    let go = || (!cancelled.load(Ordering::Relaxed)).then_some(());
    go()?;
    let folders = target.scan(&roots());
    go()?;
    let shortcuts = target.shortcuts(&shortcuts(&shortcut_roots()));
    go()?;
    let keys = target.keys(&paths::SOFTWARE_ROOTS);
    go()?;
    let services = target.services(&services());
    go()?;
    let tasks = target.tasks(&scheduled_tasks());
    go()?;
    Some(Found {
        folders,
        shortcuts,
        keys,
        services,
        tasks,
    })
}

impl Target {
    pub fn new(
        product: &str,
        publisher: Option<&str>,
        install: Option<PathBuf>,
        others: Vec<PathBuf>,
    ) -> Self {
        Self {
            product: key(product, &[]),
            publisher: publisher.map(|p| key(p, &COMPANY_WORDS)),
            // An install folder such as `C:\Program Files` says nothing about the program.
            install: install.filter(|folder| !protect::is_protected(folder)),
            others: others.iter().map(|o| protect::normalize(o)).collect(),
            other_names: Vec::new(),
            other_publishers: Vec::new(),
        }
    }

    /// `program` among `all` the installed programs.
    pub fn of(program: &Program, all: &[Program]) -> Self {
        let others = || all.iter().filter(|other| other.id != program.id);
        let target = Self::new(
            &program.name,
            program.publisher.as_deref(),
            program.install_folder(),
            others().filter_map(Program::install_folder).collect(),
        );
        Self {
            other_names: others().map(|other| key(&other.name, &[])).collect(),
            other_publishers: others()
                .filter_map(|other| other.publisher.as_deref())
                .map(|publisher| key(publisher, &COMPANY_WORDS))
                .filter(|publisher| publisher.len() >= MIN_KEY)
                .collect(),
            ..target
        }
    }

    /// Folders under `roots` (and the install folder) that look like the program's,
    /// surest first, then by path.
    pub fn scan(&self, roots: &[PathBuf]) -> Vec<Leftover> {
        if self.product.len() < MIN_KEY {
            return Vec::new();
        }
        let mut found = Vec::new();
        let mut keep = |path: PathBuf, under_publisher: bool| {
            if let Some(confidence) = self.rate(&path, under_publisher)
                && !found.iter().any(|f: &Leftover| same_path(&f.path, &path))
            {
                found.push(Leftover { path, confidence });
            }
        };
        for root in roots {
            for folder in subfolders(root) {
                if self.is_publisher(&folder) {
                    // Shared by the publisher's programs: only its children can be ours.
                    subfolders(&folder).for_each(|child| keep(child, true));
                } else {
                    keep(folder, false);
                }
            }
        }
        if let Some(install) = self.install.clone().filter(|i| real_folder(i)) {
            let under_publisher = install.parent().is_some_and(|p| self.is_publisher(p));
            keep(install, under_publisher);
        }
        sorted(found)
    }

    /// Subkeys of `roots` (`SOFTWARE` keys) named after the program, and of its
    /// publisher's key, surest first, then by key. Names only: values are not read.
    pub fn keys(&self, roots: &[(Hive, &str)]) -> Vec<Named> {
        if self.product.len() < MIN_KEY {
            return Vec::new();
        }
        let mut found = Vec::new();
        let mut keep = |hive: Hive, path: String, name: &str, under_publisher: bool| {
            if !protect::is_protected_key(hive, &path)
                && let Some(confidence) = self.rate_name(name, false, under_publisher)
            {
                let name = format!(r"{}\{path}", hive.name());
                found.push(Named { confidence, name });
            }
        };
        for &(hive, root) in roots {
            let Some(software) = Key::open(hive, root, View::Native64) else {
                continue;
            };
            for name in software.subkey_names() {
                let path = format!(r"{root}\{name}");
                if !self.is_publisher_name(&name) {
                    keep(hive, path, &name, false);
                    continue;
                }
                // Shared by the publisher's programs: only its children can be ours.
                let children = software
                    .subkey(&name)
                    .map(|publisher| publisher.subkey_names());
                for child in children.unwrap_or_default() {
                    keep(hive, format!(r"{path}\{child}"), &child, true);
                }
            }
        }
        sorted(found)
    }

    /// The services whose program file is in the install folder or gone, named after
    /// the program; surest first.
    pub fn services(&self, services: &[Service]) -> Vec<Named> {
        self.rate_all(services, |service| {
            let names: Vec<String> = std::iter::once(&service.name)
                .chain(&service.display)
                .map(|name| key(name, &[]))
                .collect();
            Some(Named {
                confidence: self.rate_pointer(&service.binary, service.binary_exists, &names)?,
                name: service.name.clone(),
            })
        })
    }

    /// The scheduled tasks whose program is in the install folder or gone, named after
    /// the program (the task or its folder); surest first.
    pub fn tasks(&self, tasks: &[ScheduledTask]) -> Vec<Named> {
        self.rate_all(tasks, |task| {
            let names: Vec<String> = task
                .path
                .rsplit('\\')
                .take(2)
                .filter(|name| !name.is_empty())
                .map(|name| key(name, &[]))
                .collect();
            Some(Named {
                confidence: self.rate_pointer(&task.program, task.program_exists, &names)?,
                name: task.path.clone(),
            })
        })
    }

    /// The shortcuts that lead to the program: into its install folder, or named after
    /// it with a target that is gone. A shortcut to a file that still exists elsewhere
    /// is someone else's.
    pub fn shortcuts(&self, shortcuts: &[Shortcut]) -> Vec<Leftover> {
        self.rate_all(shortcuts, |shortcut| {
            Some(Leftover {
                confidence: self.rate_shortcut(shortcut)?,
                path: shortcut.path.clone(),
            })
        })
    }

    /// What `rate` proposes among `items`, surest first; nothing for a product name too
    /// short to tell.
    fn rate_all<I, T: Ord>(&self, items: &[I], rate: impl Fn(&I) -> Option<T>) -> Vec<T> {
        if self.product.len() < MIN_KEY {
            return Vec::new();
        }
        sorted(items.iter().filter_map(rate).collect())
    }

    fn rate_shortcut(&self, shortcut: &Shortcut) -> Option<Confidence> {
        let link = &shortcut.path;
        // The shortcut's own name, and its Start Menu folder's.
        let names: Vec<String> = [link.file_stem(), link.parent().and_then(Path::file_name)]
            .into_iter()
            .flatten()
            .map(|name| key(&name.to_string_lossy(), &[]))
            .collect();
        let target = shortcut.target.as_deref()?;
        self.rate_pointer(target, shortcut.target_exists, &names)
    }

    /// Rates what runs a file (a shortcut, a service) by where it leads, normalized, and
    /// its names, reduced by `key`. One leading to a file still present elsewhere, or
    /// into another program's folder, is someone else's.
    fn rate_pointer(&self, target: &Path, exists: bool, names: &[String]) -> Option<Confidence> {
        let into_install = self
            .install
            .as_deref()
            .is_some_and(|i| target.starts_with(protect::normalize(i)));
        if !into_install && (exists || self.others.iter().any(|o| target.starts_with(o))) {
            return None;
        }
        let exact = names.contains(&self.product);
        let product = exact && !self.shared(&self.product);
        let fragment = !product
            && (exact
                || names
                    .iter()
                    .any(|name| is_fragment(name, &self.product) && !self.shared(name)));
        match (into_install, product) {
            (true, true) => Some(Confidence::Safe),
            (true, false) | (false, true) => Some(Confidence::Moderate),
            _ if fragment => Some(Confidence::Advanced),
            _ => None,
        }
    }

    fn rate(&self, folder: &Path, under_publisher: bool) -> Option<Confidence> {
        let normal = protect::normalize(folder);
        if protect::is_protected(folder)
            || self.is_publisher(folder)
            || self.others.iter().any(|o| o.starts_with(&normal))
            // Above the program's own install folder: may hold more than the program.
            || self.install.iter().any(|i| {
                let i = protect::normalize(i);
                i != normal && i.starts_with(&normal)
            })
        {
            return None;
        }
        let install = self
            .install
            .as_deref()
            .is_some_and(|i| same_path(i, folder));
        self.rate_name(
            &folder.file_name()?.to_string_lossy(),
            install,
            under_publisher,
        )
    }

    /// Rates a folder or key by its name; `install` when it is the install folder. Under
    /// the publisher, the product without the publisher's name counts as the product
    /// ("Google\Chrome" for Google Chrome).
    fn rate_name(&self, name: &str, install: bool, under_publisher: bool) -> Option<Confidence> {
        let name = key(name, &[]);
        let short = self
            .publisher
            .as_deref()
            .and_then(|publisher| self.product.strip_prefix(publisher))
            .filter(|short| short.len() >= MIN_KEY);
        let exact = name == self.product || (under_publisher && short == Some(name.as_str()));
        // "Edge" stands for Microsoft Edge only if no other program extends its name.
        let shared = self.shared(&name) || (exact && self.shared(&self.product));
        let product = exact && !shared;
        let fragment = !product && (exact || (!shared && is_fragment(&name, &self.product)));
        let signals = [install, product, under_publisher && (product || fragment)];
        match signals.into_iter().filter(|&s| s).count() {
            2.. => Some(Confidence::Safe),
            _ if install || product => Some(Confidence::Moderate),
            _ if fragment => Some(Confidence::Advanced),
            _ => None,
        }
    }

    /// Another program has this name, a name starting with it ("Python" for every
    /// Python component) or this publisher: the name alone is no proof.
    fn shared(&self, name: &str) -> bool {
        self.other_names
            .iter()
            .any(|other| other == name || is_fragment(name, other))
            || self
                .other_publishers
                .iter()
                .any(|publisher| publisher == name)
    }

    fn is_publisher(&self, folder: &Path) -> bool {
        folder
            .file_name()
            .is_some_and(|name| self.is_publisher_name(&name.to_string_lossy()))
    }

    fn is_publisher_name(&self, name: &str) -> bool {
        self.publisher.as_deref().is_some_and(|publisher| {
            publisher.len() >= MIN_KEY && key(name, &COMPANY_WORDS) == publisher
        })
    }
}

/// A shortcut file and where it leads, read once for every program rated against it.
pub struct Shortcut {
    path: PathBuf,
    /// Normalized (`protect::normalize`); `None` for a URL or a shell item.
    target: Option<PathBuf>,
    target_exists: bool,
}

/// A Windows service whose program file is named by full path, read once for every
/// program rated against it.
pub struct Service {
    name: String,
    display: Option<String>,
    /// Normalized (`protect::normalize`).
    binary: PathBuf,
    binary_exists: bool,
}

/// The services (not the drivers) run from a full path outside the protected folders:
/// Windows' own, under `System32` or named through `%SystemRoot%`, are left out.
pub fn services() -> Vec<Service> {
    let Some(root) = Key::open(Hive::LocalMachine, paths::SERVICES, View::Native64) else {
        return Vec::new();
    };
    root.subkey_names()
        .into_iter()
        .filter_map(|name| {
            let service = root.subkey(&name)?;
            // SERVICE_WIN32_OWN_PROCESS | SERVICE_WIN32_SHARE_PROCESS: drivers lack both.
            if service.dword("Type")? & 0x30 == 0 {
                return None;
            }
            let binary = inventory::command_program(&service.string("ImagePath")?)?;
            if protect::is_protected(&binary) {
                return None;
            }
            Some(Service {
                display: service.string("DisplayName"),
                binary_exists: binary.is_file(),
                binary: protect::normalize(&binary),
                name,
            })
        })
        .collect()
}

/// A scheduled task and the program its first "start a program" action runs, read once
/// for every program rated against it.
pub struct ScheduledTask {
    /// As the Task Scheduler shows it: `\Folder\Name`.
    path: String,
    /// Normalized (`protect::normalize`).
    program: PathBuf,
    program_exists: bool,
}

/// The tasks outside Windows' own folder that start a program named by full path,
/// outside the protected folders.
pub fn scheduled_tasks() -> Vec<ScheduledTask> {
    scheduler::tasks()
        .into_iter()
        .filter(|task| !protect::is_protected_task(&task.path))
        .filter_map(|task| {
            let program = icon::expand_env(task.programs.first()?);
            let program = PathBuf::from(program.trim().trim_matches('"'));
            if !program.is_absolute() || protect::is_protected(&program) {
                return None;
            }
            Some(ScheduledTask {
                program_exists: program.is_file(),
                program: protect::normalize(&program),
                path: task.path,
            })
        })
        .collect()
}

/// The shortcuts in `roots` and their direct subfolders, protected ones left out.
pub fn shortcuts(roots: &[PathBuf]) -> Vec<Shortcut> {
    roots
        .iter()
        .flat_map(|root| std::iter::once(root.clone()).chain(subfolders(root)))
        .flat_map(|folder| links(&folder))
        .filter(|link| !protect::is_protected(link))
        .map(|path| {
            let target = shortcut::target(&path).map(|t| protect::normalize(&t));
            let target_exists = target.as_ref().is_some_and(|t| t.exists());
            Shortcut {
                path,
                target,
                target_exists,
            }
        })
        .collect()
}

/// The roots a program's folders usually sit in, each once.
pub fn roots() -> Vec<PathBuf> {
    let vars = ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"];
    unique(
        vars.iter()
            .chain(&protect::DATA_FOLDERS)
            .filter_map(std::env::var_os)
            .map(PathBuf::from)
            .chain(env_path("LOCALAPPDATA", "Programs"))
            .chain(start_menus()),
    )
}

/// Where shortcuts sit: both Start Menus and both Desktops, each once.
pub fn shortcut_roots() -> Vec<PathBuf> {
    let desktops = [
        env_path("USERPROFILE", "Desktop"),
        env_path("PUBLIC", "Desktop"),
    ];
    unique(start_menus().chain(desktops.into_iter().flatten()))
}

/// The user's and every user's Start Menu program folders.
fn start_menus() -> impl Iterator<Item = PathBuf> {
    ["APPDATA", "ProgramData"]
        .into_iter()
        .filter_map(|var| env_path(var, r"Microsoft\Windows\Start Menu\Programs"))
}

/// `rest` under the folder the environment variable `var` names.
fn env_path(var: &str, rest: &str) -> Option<PathBuf> {
    std::env::var_os(var).map(|folder| Path::new(&folder).join(rest))
}

fn unique(paths: impl Iterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut kept: Vec<PathBuf> = Vec::new();
    for path in paths {
        if !kept.iter().any(|k| same_path(k, &path)) {
            kept.push(path);
        }
    }
    kept
}

fn sorted<T: Ord>(mut found: Vec<T>) -> Vec<T> {
    found.sort();
    found
}

/// A name reduced to what identifies it: lowercase letters, digits, `+` and `#`, with
/// parenthesized parts, versions, architectures and the given words left out.
/// "7-Zip 23.01 (x64)" → "7zip", "Acme, Inc." (company words) → "acme".
fn key(name: &str, drop: &[&str]) -> String {
    let mut depth = 0_u32;
    let outside: String = name
        .chars()
        .filter(|&c| {
            match c {
                '(' | '[' => depth += 1,
                ')' | ']' => depth = depth.saturating_sub(1),
                _ => return depth == 0,
            }
            false
        })
        .collect();
    outside
        .split(|c: char| c.is_whitespace() || c == ',')
        .map(|word| {
            word.chars()
                .filter(|c| c.is_alphanumeric() || matches!(c, '+' | '#'))
                .flat_map(char::to_lowercase)
                .collect::<String>()
        })
        .filter(|word| {
            !word.is_empty()
                && !is_version(word)
                && !ARCH_WORDS.contains(&word.as_str())
                && !drop.contains(&word.as_str())
        })
        .collect()
}

/// "2301", "v21" (dots are already gone): a word that only numbers a release.
fn is_version(word: &str) -> bool {
    let digits = word.strip_prefix('v').unwrap_or(word);
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

/// One name starts with the other and the common part is long enough.
fn is_fragment(a: &str, b: &str) -> bool {
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    short.len() >= MIN_FRAGMENT && long.starts_with(short) && !GENERIC_WORDS.contains(&short)
}

/// Same folder, whatever the case or a trailing separator.
fn same_path(a: &Path, b: &Path) -> bool {
    protect::normalize(a) == protect::normalize(b)
}

/// A folder that is not a link or junction.
fn real_folder(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_dir() && !m.is_symlink())
}

/// The shortcut files (`.lnk`) directly in `folder`.
fn links(folder: &Path) -> impl Iterator<Item = PathBuf> + use<> {
    fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file()))
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("lnk"))
        })
}

/// The real subfolders of `folder`; none if it cannot be read.
fn subfolders(folder: &Path) -> impl Iterator<Item = PathBuf> {
    fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| {
            entry
                .file_type()
                .is_ok_and(|t| t.is_dir() && !t.is_symlink())
        })
        .map(|entry| entry.path())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::test_support::junction;
    use crate::win::registry;

    #[test]
    fn keys_keep_what_names_the_product() {
        assert_eq!(key("7-Zip 23.01 (x64)", &[]), "7zip");
        assert_eq!(key("Notepad++ v8.6 [64-bit]", &[]), "notepad++");
        assert_eq!(key("Acme, Inc.", &COMPANY_WORDS), "acme");
        assert_eq!(key("Microsoft Corporation", &COMPANY_WORDS), "microsoft");
        assert_eq!(key("Widget Pro 2.1 x64", &[]), "widgetpro");
    }

    /// Folders under %TEMP% stand in for Program Files and AppData; nothing is changed
    /// outside them.
    #[test]
    fn rates_folders_by_their_signals() {
        let dir = std::env::temp_dir()
            .join("uninstaller-tests")
            .join(format!("leftovers-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let (files, data) = (dir.join("files"), dir.join("data"));
        for folder in [
            r"files\Acme\Widget Pro",
            r"files\Acme\Gadget",
            r"files\Other",
            r"data\Widget Pro",
            r"data\WidgetPro Cache",
            r"data\Widg",
            r"data\Acme Inc",
            r"data\WidgetPro Hub\Gadget",
            r"elsewhere",
        ] {
            fs::create_dir_all(dir.join(folder)).unwrap();
        }
        junction(&data.join("Widget Pro Link"), &dir.join("elsewhere"));
        let install = files.join(r"Acme\Widget Pro");
        // As some installers declare it, with a trailing separator.
        let declared = files.join(r"Acme\Widget Pro\");
        let hub = vec![data.join(r"WidgetPro Hub\Gadget")];
        let target = Target::new(
            "Widget Pro 2.1 (x64)",
            Some("Acme Inc."),
            Some(declared),
            hub,
        );

        let found = target.scan(&[files.clone(), data.clone()]);

        assert_eq!(
            found,
            [
                Leftover {
                    path: install,
                    confidence: Confidence::Safe
                },
                Leftover {
                    path: data.join("Widget Pro"),
                    confidence: Confidence::Moderate
                },
                Leftover {
                    path: data.join("WidgetPro Cache"),
                    confidence: Confidence::Advanced
                },
            ]
        );
        fs::remove_dir_all(dir).unwrap();
    }

    /// Shortcuts written under %TEMP%, standing in for a Start Menu.
    #[test]
    fn rates_shortcuts_by_where_they_lead() {
        let dir = std::env::temp_dir()
            .join("uninstaller-tests")
            .join(format!("shortcuts-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let (menu, install, other) = (dir.join("menu"), dir.join("Widget Pro"), dir.join("Other"));
        for folder in [&menu.join("Widget Pro"), &install, &other] {
            fs::create_dir_all(folder).unwrap();
        }
        fs::write(install.join("widget.exe"), b"x").unwrap();
        fs::write(other.join("other.exe"), b"x").unwrap();
        let gone = dir.join("gone.exe");
        let links = [
            ("Widget Pro.lnk", install.join("widget.exe")),
            (r"Widget Pro\Manual.lnk", gone.clone()),
            ("WidgetPro Help.lnk", gone.clone()),
            ("Widget Pro Other.lnk", other.join("other.exe")),
            ("Unrelated.lnk", gone),
        ];
        for (name, target) in &links {
            shortcut::create(&menu.join(name), target).unwrap();
        }
        let target = Target::new("Widget Pro", None, Some(install), vec![other]);

        let found = target.shortcuts(&shortcuts(std::slice::from_ref(&menu)));

        let expected: Vec<_> = [
            ("Widget Pro.lnk", Confidence::Safe),
            (r"Widget Pro\Manual.lnk", Confidence::Moderate),
            ("WidgetPro Help.lnk", Confidence::Advanced),
        ]
        .into_iter()
        .map(|(name, confidence)| Leftover {
            path: menu.join(name),
            confidence,
        })
        .collect();
        assert_eq!(found, expected);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_protected_install_folder_is_ignored() {
        let program_files = PathBuf::from(std::env::var_os("ProgramFiles").unwrap());
        let target = Target::new("Widget Pro", None, Some(program_files), Vec::new());
        assert_eq!(target.install, None);
    }

    #[test]
    fn a_folder_above_the_install_folder_is_not_proposed() {
        let dir = std::env::temp_dir()
            .join("uninstaller-tests")
            .join(format!("above-{}", std::process::id()));
        let install = dir.join(r"Foobar\x64");
        fs::create_dir_all(&install).unwrap();
        let target = Target::new("Foobar", None, Some(install.clone()), vec![]);
        let found = target.scan(std::slice::from_ref(&dir));
        assert_eq!(
            found,
            [Leftover {
                path: install,
                confidence: Confidence::Moderate
            }]
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_shared_name_is_only_a_partial_match() {
        let dir = std::env::temp_dir()
            .join("uninstaller-tests")
            .join(format!("twins-{}", std::process::id()));
        fs::create_dir_all(dir.join(r"Acme\Local DB")).unwrap();
        let mut target = Target::new("Local DB 2019", Some("Acme"), None, vec![]);
        assert_eq!(
            target.scan(std::slice::from_ref(&dir))[0].confidence,
            Confidence::Safe
        );
        target.other_names = vec!["localdb".to_owned()];
        assert_eq!(
            target.scan(std::slice::from_ref(&dir))[0].confidence,
            Confidence::Advanced
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn never_proposes_a_protected_folder() {
        let program_files = PathBuf::from(std::env::var_os("ProgramFiles").unwrap());
        let target = Target::new("Program Files", None, Some(program_files.clone()), vec![]);
        let parent = program_files.parent().unwrap().to_path_buf();
        assert!(target.scan(&[parent]).is_empty());
    }

    /// Read only, on this machine's programs: nothing proposed is protected, and folders
    /// are real folders, not links.
    /// `--nocapture` prints what each program would get, for a manual look.
    #[test]
    fn real_programs_get_no_protected_folder() {
        let (roots, links) = (roots(), shortcuts(&shortcut_roots()));
        let (software, services, tasks) = (paths::SOFTWARE_ROOTS, services(), scheduled_tasks());
        assert!(tasks.iter().all(|task| {
            !protect::is_protected_task(&task.path) && !protect::is_protected(&task.program)
        }));
        let all = crate::core::inventory::scan();
        for program in &all {
            let target = Target::of(program, &all);
            let found = target.shortcuts(&links);
            for leftover in target.scan(&roots).into_iter().chain(found) {
                println!(
                    "{} · {:?} · {}",
                    program.name,
                    leftover.confidence,
                    leftover.path.display()
                );
                assert!(!protect::is_protected(&leftover.path));
                assert!(real_folder(&leftover.path) || leftover.path.is_file());
            }
            for leftover in target.keys(&software) {
                println!(
                    "{} · {:?} · {}",
                    program.name, leftover.confidence, leftover.name
                );
                let (hive, path) = Hive::parse(&leftover.name).unwrap();
                assert!(!protect::is_protected_key(hive, path));
            }
            for leftover in target.services(&services) {
                let service = &leftover.name;
                println!(
                    "{} · {:?} · service {service}",
                    program.name, leftover.confidence
                );
            }
            for leftover in target.tasks(&tasks) {
                let task = &leftover.name;
                println!("{} · {:?} · task {task}", program.name, leftover.confidence);
            }
        }
    }

    #[test]
    fn a_name_other_programs_share_is_not_proposed() {
        let mut target = Target::new("Python 3.12 Core", None, None, vec![]);
        assert_eq!(
            target.rate_name("Python", false, false),
            Some(Confidence::Advanced)
        );
        target.other_names = vec!["pythondocs".to_owned()];
        assert_eq!(target.rate_name("Python", false, false), None);
        target.other_publishers = vec!["pythoncore".to_owned()];
        assert_eq!(
            target.rate_name("Python Core", false, false),
            Some(Confidence::Advanced)
        );
    }

    /// Keys under `HKCU\Software\UninstallerTest` stand in for `SOFTWARE`.
    #[test]
    fn rates_registry_keys_by_name() {
        let root = registry::test_key::fresh("leftover-keys");
        let subkeys = [
            "Acme Widget",
            r"Acme\Widget",
            r"Acme\Other",
            "Acme Widget Tools",
            "Unrelated",
        ];
        for sub in subkeys {
            Key::create(Hive::CurrentUser, &format!(r"{root}\{sub}"), View::Native64).unwrap();
        }
        let target = Target::new("Acme Widget 2.0", Some("Acme, Inc."), None, Vec::new());

        let found = target.keys(&[(Hive::CurrentUser, &root)]);

        let expected: Vec<_> = [
            (r"Acme\Widget", Confidence::Safe),
            ("Acme Widget", Confidence::Moderate),
            ("Acme Widget Tools", Confidence::Advanced),
        ]
        .into_iter()
        .map(|(sub, confidence)| Named {
            confidence,
            name: format!(r"HKEY_CURRENT_USER\{root}\{sub}"),
        })
        .collect();
        registry::test_key::clear(&root);
        assert_eq!(found, expected);
    }

    #[test]
    fn a_short_name_another_program_extends_is_no_proof() {
        let mut target = Target::new(
            "Microsoft Edge",
            Some("Microsoft Corporation"),
            None,
            vec![],
        );
        assert_eq!(
            target.rate_name("Edge", false, true),
            Some(Confidence::Safe)
        );
        target.other_names = vec!["microsoftedgeupdate".to_owned()];
        assert_eq!(
            target.rate_name("Edge", false, true),
            Some(Confidence::Advanced)
        );
    }

    /// A shortcut to a shell item has no file to tell whose it is: its name is no proof.
    #[test]
    fn a_shortcut_without_a_file_target_is_not_proposed() {
        let shortcut = Shortcut {
            path: std::env::temp_dir().join("Widget Pro.lnk"),
            target: None,
            target_exists: false,
        };
        let target = Target::new("Widget Pro", None, None, vec![]);
        assert!(target.shortcuts(&[shortcut]).is_empty());
    }

    #[test]
    fn a_cancelled_scan_stops_and_a_full_one_answers() {
        let program = crate::core::inventory::tests::program("Qpn Absent Widget", None);
        assert!(find(&program, &[], &AtomicBool::new(true)).is_none());
        let found = find(&program, &[], &AtomicBool::new(false)).unwrap();
        let kinds = [found.folders.len(), found.shortcuts.len(), found.keys.len()];
        assert_eq!(kinds, [0; 3]);
        assert!(found.services.is_empty() && found.tasks.is_empty());
    }

    #[test]
    fn rates_scheduled_tasks_by_their_program() {
        let dir = std::env::temp_dir()
            .join("uninstaller-tests")
            .join(format!("tasks-{}", std::process::id()));
        let install = dir.join("Widget Pro");
        let task = |path: &str, program: PathBuf, exists| ScheduledTask {
            path: path.to_owned(),
            program: protect::normalize(&program),
            program_exists: exists,
        };
        let tasks = [
            task(r"\Widget Pro Update", install.join("up.exe"), true),
            task(r"\Widget Pro\Daily", dir.join(r"gone\up.exe"), false),
            task(r"\WidgetProTelemetry", dir.join(r"gone\t.exe"), false),
            task(r"\Widget Pro Agent", dir.join(r"elsewhere\a.exe"), true),
            task(r"\Unrelated", dir.join(r"gone\u.exe"), false),
        ];
        let target = Target::new("Widget Pro", None, Some(install), vec![]);

        let found = target.tasks(&tasks);

        let expected: Vec<_> = [
            (r"\Widget Pro Update", Confidence::Moderate),
            (r"\Widget Pro\Daily", Confidence::Moderate),
            (r"\WidgetProTelemetry", Confidence::Advanced),
        ]
        .into_iter()
        .map(|(name, confidence)| Named {
            confidence,
            name: name.to_owned(),
        })
        .collect();
        assert_eq!(found, expected);
    }

    #[test]
    fn rates_services_by_their_program_file() {
        let dir = std::env::temp_dir()
            .join("uninstaller-tests")
            .join(format!("services-{}", std::process::id()));
        let (install, other) = (dir.join("Widget Pro"), dir.join("Other"));
        let service = |name: &str, display: Option<&str>, binary: PathBuf, exists| Service {
            name: name.to_owned(),
            display: display.map(str::to_owned),
            binary: protect::normalize(&binary),
            binary_exists: exists,
        };
        let services = [
            service(
                "WidgetSvc",
                Some("Widget Pro"),
                install.join("svc.exe"),
                true,
            ),
            service("Widget Pro", None, dir.join(r"gone\svc.exe"), false),
            service("WidgetProUpdate", None, dir.join(r"gone\up.exe"), false),
            service("Widget Pro Agent", None, other.join("agent.exe"), false),
            service("Widget Pro", None, dir.join(r"elsewhere\svc.exe"), true),
            service("Unrelated", None, install.join("x.exe"), true),
        ];
        let target = Target::new("Widget Pro", None, Some(install), vec![other]);

        let found = target.services(&services);

        let expected: Vec<_> = [
            ("WidgetSvc", Confidence::Safe),
            ("Unrelated", Confidence::Moderate),
            ("Widget Pro", Confidence::Moderate),
            ("WidgetProUpdate", Confidence::Advanced),
        ]
        .into_iter()
        .map(|(name, confidence)| Named {
            confidence,
            name: name.to_owned(),
        })
        .collect();
        assert_eq!(found, expected);
    }

    #[test]
    fn short_names_match_nothing() {
        let target = Target::new("Go", None, None, vec![]);
        assert!(target.scan(&roots()).is_empty());
        let appdata = PathBuf::from(std::env::var_os("APPDATA").unwrap()).join("Microsoft");
        let sdk = Target::new("Windows SDK Signing Tools", Some("Microsoft"), None, vec![]);
        assert!(sdk.scan(&[appdata]).is_empty());
    }
}
