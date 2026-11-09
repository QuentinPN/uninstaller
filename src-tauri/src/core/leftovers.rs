//! Leftover scan (F5), files part: the folders a removed program may leave in the
//! shared roots (Program Files, ProgramData, AppData, Start Menus) and its shortcuts.
//! Read only: it proposes, the quarantine moves. Every candidate passes `protect::is_protected`; links and junctions are never
//! listed nor followed.

use std::fs;
use std::path::{Path, PathBuf};

use crate::core::inventory::Program;
use crate::core::protect;
use crate::win::shortcut;

/// How surely a folder belongs to the program (SECURITY.md 5). Ordered from the
/// surest: the Safe depth keeps `Safe`, Moderate adds `Moderate`, Advanced adds all.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
    /// At least two independent signals: install folder, product name, publisher.
    Safe,
    /// One signal: the install folder, or a folder named exactly after the product.
    Moderate,
    /// Only part of the name matches; never preselected.
    Advanced,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Leftover {
    pub path: PathBuf,
    pub confidence: Confidence,
}

/// What the scan matches folders against, read from the inventory entry.
pub struct Target {
    product: String,
    publisher: Option<String>,
    install: Option<PathBuf>,
    /// Other programs' install folders, normalized: a folder holding one is never ours.
    others: Vec<PathBuf>,
    /// Another installed program has the same name (other versions, editions): the name
    /// alone no longer tells whose a folder is.
    twin: bool,
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
            twin: false,
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
            twin: others().any(|other| key(&other.name, &[]) == target.product),
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

    /// The shortcuts that lead to the program: into its install folder, or named after
    /// it with a target that is gone. A shortcut to a file that still exists elsewhere
    /// is someone else's.
    pub fn shortcuts(&self, shortcuts: &[Shortcut]) -> Vec<Leftover> {
        if self.product.len() < MIN_KEY {
            return Vec::new();
        }
        let found = shortcuts
            .iter()
            .filter_map(|shortcut| {
                Some(Leftover {
                    confidence: self.rate_shortcut(shortcut)?,
                    path: shortcut.path.clone(),
                })
            })
            .collect();
        sorted(found)
    }

    fn rate_shortcut(&self, shortcut: &Shortcut) -> Option<Confidence> {
        let link = &shortcut.path;
        let into_install = self
            .install
            .as_deref()
            .is_some_and(|i| shortcut.leads_into(&protect::normalize(i)));
        if !into_install
            && (shortcut.target_exists || self.others.iter().any(|o| shortcut.leads_into(o)))
        {
            return None;
        }
        // The shortcut's own name, and its Start Menu folder's.
        let names: Vec<String> = [link.file_stem(), link.parent().and_then(Path::file_name)]
            .into_iter()
            .flatten()
            .map(|name| key(&name.to_string_lossy(), &[]))
            .collect();
        let exact = names.contains(&self.product);
        let product = exact && !self.twin;
        let fragment =
            !product && (exact || names.iter().any(|name| is_fragment(name, &self.product)));
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
        let name = key(&folder.file_name()?.to_string_lossy(), &[]);
        let install = self
            .install
            .as_deref()
            .is_some_and(|i| same_path(i, folder));
        let exact = name == self.product;
        let product = exact && !self.twin;
        let fragment = !product && (exact || is_fragment(&name, &self.product));
        let signals = [install, product, under_publisher && (product || fragment)];
        match signals.into_iter().filter(|&s| s).count() {
            2.. => Some(Confidence::Safe),
            _ if install || product => Some(Confidence::Moderate),
            _ if fragment => Some(Confidence::Advanced),
            _ => None,
        }
    }

    fn is_publisher(&self, folder: &Path) -> bool {
        self.publisher.as_deref().is_some_and(|publisher| {
            publisher.len() >= MIN_KEY
                && folder
                    .file_name()
                    .is_some_and(|n| key(&n.to_string_lossy(), &COMPANY_WORDS) == publisher)
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

impl Shortcut {
    /// Its target is in `folder`, given normalized.
    fn leads_into(&self, folder: &Path) -> bool {
        self.target.as_ref().is_some_and(|t| t.starts_with(folder))
    }
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

/// Surest first, then by path.
fn sorted(mut found: Vec<Leftover>) -> Vec<Leftover> {
    found.sort_by(|a, b| (a.confidence, &a.path).cmp(&(b.confidence, &b.path)));
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
        target.twin = true;
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
        }
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
