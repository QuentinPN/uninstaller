//! Quarantine batches (ADR 0003, 0005): nothing is deleted, files and folders are moved
//! into a batch folder and registry keys saved there as `.reg` before they go. The
//! manifest is written before each step, so a batch can be restored, or resumed after
//! a crash.

use std::fmt;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;

use super::protect;
use super::registry::reg_file;
use super::sizes;
use crate::win::registry::{self, Hive, View};

const MANIFEST: &str = "manifest.json";
/// Written in full, then renamed over the manifest (ADR 0005).
const MANIFEST_NEXT: &str = "manifest.json.next";
/// Where moved items are kept, one entry per item, named by its index.
const STORE: &str = "items";
/// How long a batch stays restorable before it is purged (SECURITY.md: default 14
/// days, a setting later).
pub const PURGE_DELAY: Duration = Duration::from_secs(14 * 24 * 3600);

#[derive(Debug)]
pub enum Error {
    /// `core/protect.rs` refuses the path: nothing was written or moved.
    Protected,
    /// Restoring would overwrite something that exists now.
    Exists,
    /// A moved file no longer has the content it was moved with.
    Altered,
    /// The manifest does not match its own hash.
    Corrupt,
    /// A link, or a folder holding one, on another volume than the batch: a link is
    /// only ever renamed, never recreated.
    OtherVolume,
    /// A key, subkey or value could not be read: nothing was saved or removed.
    Unreadable,
    /// The batch is being purged: nothing goes in or comes back out.
    Purging,
    Io(io::Error),
}

/// For logs (ADR 0003: refusals are logged); the UI words its own messages.
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Protected => f.write_str("protected path"),
            Self::Exists => f.write_str("origin exists"),
            Self::Altered => f.write_str("moved file altered"),
            Self::Corrupt => f.write_str("manifest corrupt"),
            Self::OtherVolume => f.write_str("other volume"),
            Self::Unreadable => f.write_str("key unreadable"),
            Self::Purging => f.write_str("batch being purged"),
            Self::Io(error) => error.fmt(f),
        }
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Per item: `Pending` is written before the move, `Copied` once a copy (across
/// volumes, or a key's `.reg`) is verified and before the origin is removed, `Moved`
/// after, `Restored` once back at its origin (STATE_MACHINES.md).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Pending,
    Copied,
    Moved,
    Restored,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    File,
    Folder,
    /// A symbolic link or junction, moved as the link itself.
    Link,
    /// A registry key with its subkeys, kept as a `.reg` file. Its origin is the key as
    /// regedit names it, in the 64-bit view (32-bit keys go through `WOW6432Node`).
    Key,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub origin: PathBuf,
    pub kind: Kind,
    /// SHA-256 of a file's content or of a key's `.reg`; folders and links have none.
    pub hash: Option<String>,
    pub state: State,
}

/// `Active` holds items; `Purging` is written before the batch folder goes, so a purge
/// a crash interrupted is finished at startup (STATE_MACHINES.md).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BatchState {
    Active,
    Purging,
}

#[derive(Serialize, Deserialize)]
struct Manifest {
    /// SHA-256 of `created`, `state` and `items` as written, to detect a damaged
    /// manifest.
    hash: String,
    /// Seconds since the Unix epoch; the purge delay counts from here.
    created: u64,
    state: BatchState,
    items: Vec<Item>,
}

/// What the Backups screen shows of a batch. Times are seconds since the Unix epoch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BatchSummary {
    /// The batch folder's name.
    pub name: String,
    pub created: u32,
    /// When it is due to be purged (`created` + `PURGE_DELAY`).
    pub purge_on: u32,
    /// Files, folders and links not restored.
    pub files: u32,
    /// Registry keys not restored.
    pub keys: u32,
    /// What its kept items take on disk.
    pub size_kib: u32,
    pub purging: bool,
}

/// One cleanup's quarantine folder and its manifest.
pub struct Batch {
    dir: PathBuf,
    created: u64,
    state: BatchState,
    items: Vec<Item>,
}

impl Batch {
    /// A new, empty batch at `root\name`; fails if it already exists.
    pub fn create(root: &Path, name: &str) -> Result<Self, Error> {
        fs::create_dir_all(root)?;
        let dir = root.join(name);
        fs::create_dir(&dir)?;
        fs::create_dir(dir.join(STORE))?;
        let created = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |age| age.as_secs());
        let batch = Self {
            dir,
            created,
            state: BatchState::Active,
            items: Vec::new(),
        };
        batch.save()?;
        Ok(batch)
    }

    /// Reads an existing batch, refusing a manifest that fails its own hash.
    pub fn open(dir: &Path) -> Result<Self, Error> {
        let manifest: Manifest =
            serde_json::from_slice(&fs::read(dir.join(MANIFEST))?).map_err(|_| Error::Corrupt)?;
        if content_hash(manifest.created, manifest.state, &manifest.items)? != manifest.hash {
            return Err(Error::Corrupt);
        }
        Ok(Self {
            dir: dir.to_path_buf(),
            created: manifest.created,
            state: manifest.state,
            items: manifest.items,
        })
    }

    pub fn items(&self) -> &[Item] {
        &self.items
    }

    pub fn summary(&self) -> BatchSummary {
        let left = || self.items.iter().filter(|i| i.state != State::Restored);
        let keys = left().filter(|i| i.kind == Kind::Key).count();
        let files = left().count() - keys;
        let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        let secs = |s: u64| u32::try_from(s).unwrap_or(u32::MAX);
        BatchSummary {
            name: self
                .dir
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            created: secs(self.created),
            purge_on: secs(self.created.saturating_add(PURGE_DELAY.as_secs())),
            files: count(files),
            keys: count(keys),
            size_kib: sizes::kib(sizes::folder_bytes(&self.dir.join(STORE))),
            purging: self.state == BatchState::Purging,
        }
    }

    /// Moves `path` into the batch. The manifest names it `Pending` before the move
    /// and `Moved` after. A link or junction is moved itself, never followed.
    pub fn quarantine(&mut self, path: &Path) -> Result<(), Error> {
        self.active()?;
        if protect::is_protected(path) {
            return Err(Error::Protected);
        }
        let meta = fs::symlink_metadata(path)?;
        let (kind, hash) = if meta.is_symlink() {
            (Kind::Link, None)
        } else if meta.is_dir() {
            (Kind::Folder, None)
        } else {
            (Kind::File, Some(file_hash(path)?))
        };
        let index = self.items.len();
        self.items.push(Item {
            origin: path.to_path_buf(),
            kind,
            hash,
            state: State::Pending,
        });
        self.save()?;
        let stored = self.stored(index);
        let moved = match fs::rename(path, &stored) {
            Err(error) if error.kind() == io::ErrorKind::CrossesDevices => {
                copy_verified(path, &stored)
            }
            renamed => renamed.map_err(Error::from),
        };
        if let Err(error) = moved {
            // Nothing left the origin: the item leaves the manifest again.
            let _ = remove_tree(&stored);
            self.items.pop();
            self.save()?;
            return Err(error);
        }
        if fs::symlink_metadata(path).is_ok() {
            // Copied across volumes: the origin goes only once the copy is recorded.
            self.set_state(index, State::Copied)?;
            remove_tree(path)?;
        }
        self.set_state(index, State::Moved)
    }

    /// Saves the key named as regedit shows it (`HKEY_CURRENT_USER\Software\X`) into the
    /// batch as a `.reg`, then removes it: `Pending` before the save, `Copied` once the
    /// file is checked, `Moved` once the key is gone.
    pub fn quarantine_key(&mut self, key: &str) -> Result<(), Error> {
        self.active()?;
        let (hive, path) = Hive::parse(key).ok_or(Error::Protected)?;
        if protect::is_protected_key(hive, path) {
            return Err(Error::Protected);
        }
        let saved = reg_file::export(hive, path, View::Native64).ok_or(Error::Unreadable)?;
        let hash = hex(&Sha256::digest(&saved));
        let index = self.items.len();
        self.items.push(Item {
            origin: PathBuf::from(key),
            kind: Kind::Key,
            hash: Some(hash.clone()),
            state: State::Pending,
        });
        self.save()?;
        let stored = self.stored(index);
        let written = write_synced(&stored, &saved)
            .map_err(Error::from)
            .and_then(|()| {
                if file_hash(&stored)? == hash {
                    Ok(())
                } else {
                    Err(Error::Altered)
                }
            });
        if let Err(error) = written {
            let _ = fs::remove_file(&stored);
            self.items.pop();
            self.save()?;
            return Err(error);
        }
        self.set_state(index, State::Copied)?;
        registry::delete_tree(hive, path, View::Native64)?;
        self.set_state(index, State::Moved)
    }

    /// Puts every moved item back, last moved first. Stops at the first item that
    /// cannot go back (its origin exists again, or its content changed); the items
    /// already restored stay restored.
    pub fn restore(&mut self) -> Result<(), Error> {
        self.active()?;
        for index in (0..self.items.len()).rev() {
            let item = &self.items[index];
            if item.state != State::Moved {
                continue;
            }
            let stored = self.stored(index);
            if origin_exists(item)? {
                return Err(Error::Exists);
            }
            if let Some(hash) = &item.hash
                && file_hash(&stored)? != *hash
            {
                return Err(Error::Altered);
            }
            match item.kind {
                Kind::Key => import_key(item, &stored)?,
                _ => put_back(&stored, &item.origin)?,
            }
            self.set_state(index, State::Restored)?;
        }
        Ok(())
    }

    /// Finishes what a crash interrupted. Only the last item can be unsettled (one
    /// move at a time). `Pending` with its origin still there never left: a partial
    /// copy in the batch is dropped and so is the item; with only the batch holding
    /// it, it is `Moved` once its content checks out. `Copied` has a verified copy:
    /// what remains of the origin goes and it is `Moved`. A `Purging` batch is
    /// purged. Running it again changes nothing.
    pub fn resume(&mut self) -> Result<(), Error> {
        if self.state == BatchState::Purging {
            return self.remove();
        }
        let Some(index) = self
            .items
            .iter()
            .position(|i| matches!(i.state, State::Pending | State::Copied))
        else {
            return Ok(());
        };
        if index + 1 != self.items.len() {
            return Err(Error::Corrupt);
        }
        let stored = self.stored(index);
        let item = &self.items[index];
        let in_batch = fs::symlink_metadata(&stored).is_ok();
        let at_origin = origin_exists(item)?;
        match (item.state, at_origin, in_batch) {
            (State::Copied, _, true) => {
                if at_origin {
                    remove_origin(item)?;
                }
                self.set_state(index, State::Moved)
            }
            (State::Pending, true, _) => {
                if in_batch {
                    remove_tree(&stored)?;
                }
                self.items.pop();
                self.save()
            }
            (State::Pending, false, true) => {
                if let Some(hash) = &item.hash
                    && file_hash(&stored)? != *hash
                {
                    return Err(Error::Corrupt);
                }
                self.set_state(index, State::Moved)
            }
            _ => Err(Error::Corrupt),
        }
    }

    /// Removes the batch for good (ADR 0003: after the delay, or on a confirmed
    /// request): an interrupted step is finished first, then `Purging` is written,
    /// then the moved items go, then the manifest and the folder.
    pub fn purge(mut self) -> Result<(), Error> {
        self.resume()?;
        self.state = BatchState::Purging;
        self.save()?;
        self.remove()
    }

    fn remove(&self) -> Result<(), Error> {
        match fs::remove_dir_all(self.dir.join(STORE)) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error.into()),
            _ => {}
        }
        fs::remove_dir_all(&self.dir)?;
        Ok(())
    }

    fn active(&self) -> Result<(), Error> {
        match self.state {
            BatchState::Active => Ok(()),
            BatchState::Purging => Err(Error::Purging),
        }
    }

    fn stored(&self, index: usize) -> PathBuf {
        self.dir.join(STORE).join(index.to_string())
    }

    fn set_state(&mut self, index: usize, state: State) -> Result<(), Error> {
        self.items[index].state = state;
        self.save()
    }

    /// Writes the manifest next to it, then renames it over the old one.
    fn save(&self) -> Result<(), Error> {
        let manifest = Manifest {
            hash: content_hash(self.created, self.state, &self.items)?,
            created: self.created,
            state: self.state,
            items: self.items.clone(),
        };
        let next = self.dir.join(MANIFEST_NEXT);
        let text = serde_json::to_vec_pretty(&manifest).map_err(io::Error::other)?;
        write_synced(&next, &text)?;
        fs::rename(next, self.dir.join(MANIFEST))?;
        Ok(())
    }
}

/// `%LOCALAPPDATA%\Uninstaller\quarantine` (ARCHITECTURE.md, Données locales).
pub fn root() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|local| {
        PathBuf::from(local)
            .join("Uninstaller")
            .join("quarantine")
    })
}

/// At startup: resumes every batch under `root`. A batch that cannot be read is left
/// as it is, for the Backups screen to show; returns how many were resumed.
pub fn resume_all(root: &Path) -> usize {
    batch_dirs(root)
        .filter(|dir| {
            Batch::open(dir)
                .and_then(|mut batch| batch.resume())
                .is_ok()
        })
        .count()
}

/// The batch named `name` under `root`, if `name` is a plain folder name: the UI only
/// sends back names it was given, but never a path.
pub fn batch_dir(root: &Path, name: &str) -> Option<PathBuf> {
    let plain = !matches!(name, "" | "." | "..") && !name.contains(['\\', '/', ':']);
    plain.then(|| root.join(name))
}

/// Every batch under `root` that can be read, newest first.
pub fn list(root: &Path) -> Vec<BatchSummary> {
    let mut batches: Vec<_> = batch_dirs(root)
        .filter_map(|dir| Batch::open(&dir).ok())
        .map(|batch| batch.summary())
        .collect();
    batches.sort_by(|a, b| b.created.cmp(&a.created).then_with(|| a.name.cmp(&b.name)));
    batches
}

/// The folders under `root`, each a batch or what is left of one; none if `root`
/// cannot be read.
fn batch_dirs(root: &Path) -> impl Iterator<Item = PathBuf> {
    fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_dir()))
        .map(|entry| entry.path())
}

/// Purges every batch under `root` created at least `delay` before `now`, and any
/// `Purging` one. A batch that cannot be read is left for the Backups screen; returns
/// how many were purged.
pub fn purge_expired(root: &Path, delay: Duration, now: SystemTime) -> usize {
    let Some(limit) = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .and_then(|age| age.checked_sub(delay))
    else {
        return 0;
    };
    batch_dirs(root)
        .filter_map(|dir| Batch::open(&dir).ok())
        .filter(|batch| batch.state == BatchState::Purging || batch.created <= limit.as_secs())
        .map(Batch::purge)
        .filter(Result::is_ok)
        .count()
}

/// Writes `bytes` and waits until they are on disk.
fn write_synced(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// A key item's origin, split as `Hive::parse` does; a manifest naming no known hive
/// is corrupt.
fn origin_key(item: &Item) -> Result<(Hive, &str), Error> {
    item.origin
        .to_str()
        .and_then(Hive::parse)
        .ok_or(Error::Corrupt)
}

fn origin_exists(item: &Item) -> Result<bool, Error> {
    Ok(match item.kind {
        Kind::Key => {
            let (hive, path) = origin_key(item)?;
            registry::key_exists(hive, path, View::Native64)?
        }
        _ => fs::symlink_metadata(&item.origin).is_ok(),
    })
}

fn remove_origin(item: &Item) -> Result<(), Error> {
    match item.kind {
        Kind::Key => {
            let (hive, path) = origin_key(item)?;
            registry::delete_tree(hive, path, View::Native64)?;
        }
        _ => remove_tree(&item.origin)?,
    }
    Ok(())
}

/// Moves `stored` back to `origin`, copying across volumes; a partial copy goes.
fn put_back(stored: &Path, origin: &Path) -> Result<(), Error> {
    if let Some(parent) = origin.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::rename(stored, origin) {
        Err(error) if error.kind() == io::ErrorKind::CrossesDevices => {
            if let Err(error) = copy_verified(stored, origin) {
                let _ = remove_tree(origin);
                return Err(error);
            }
            remove_tree(stored)?;
        }
        renamed => renamed?,
    }
    Ok(())
}

/// Recreates a key from its `.reg`, which may only name the key and its subkeys; on
/// failure what was recreated goes, since the key did not exist before.
fn import_key(item: &Item, stored: &Path) -> Result<(), Error> {
    let (hive, path) = origin_key(item)?;
    let keys = reg_file::parse(&fs::read(stored)?).ok_or(Error::Corrupt)?;
    let inside = |key: &reg_file::RegKey| {
        let own = key.path.to_lowercase();
        let root = path.to_lowercase();
        key.hive.name() == hive.name() && (own == root || own.starts_with(&format!(r"{root}\")))
    };
    if !keys.iter().all(inside) {
        return Err(Error::Corrupt);
    }
    if let Err(error) = reg_file::import(&keys, View::Native64) {
        let _ = registry::delete_tree(hive, path, View::Native64);
        return Err(error.into());
    }
    fs::remove_file(stored)?;
    Ok(())
}

/// Copies a file or a folder tree across volumes (attributes and last write time
/// kept), each file checked by SHA-256 against its source. Leaves `from` alone; the
/// caller removes a partial `to`.
fn copy_verified(from: &Path, to: &Path) -> Result<(), Error> {
    let meta = fs::symlink_metadata(from)?;
    if meta.is_symlink() {
        return Err(Error::OtherVolume);
    }
    if meta.is_dir() {
        fs::create_dir(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            copy_verified(&entry.path(), &to.join(entry.file_name()))?;
        }
        return Ok(());
    }
    fs::copy(from, to)?;
    if file_hash(from)? != file_hash(to)? {
        return Err(Error::Altered);
    }
    Ok(())
}

/// Removes a file, a link (not its target) or a folder tree: the origin once its
/// copy is verified, or a partial copy of ours.
fn remove_tree(path: &Path) -> io::Result<()> {
    if fs::symlink_metadata(path)?.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

fn content_hash(created: u64, state: BatchState, items: &[Item]) -> Result<String, Error> {
    let bytes = serde_json::to_vec(&(created, state, items)).map_err(io::Error::other)?;
    Ok(hex(&Sha256::digest(bytes)))
}

fn file_hash(path: &Path) -> io::Result<String> {
    let mut hasher = Sha256::new();
    io::copy(&mut File::open(path)?, &mut hasher)?;
    Ok(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh folder under %TEMP% standing in for a program's files (TESTING.md:
    /// destructive tests only touch what they created).
    fn sandbox(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("uninstaller-tests")
            .join(format!("{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("app\\data")).unwrap();
        fs::write(dir.join("app\\app.exe"), b"binary").unwrap();
        fs::write(dir.join("app\\data\\settings.ini"), b"[x]\ny=1").unwrap();
        dir
    }

    #[test]
    fn purges_only_expired_batches() {
        let dir = sandbox("purge");
        let root = dir.join("quarantine");
        let mut old = Batch::create(&root, "old").unwrap();
        old.quarantine(&dir.join("app\\app.exe")).unwrap();
        fs::create_dir(dir.join("target")).unwrap();
        fs::write(dir.join("target\\kept.txt"), b"kept").unwrap();
        junction(&dir.join("app\\link"), &dir.join("target"));
        old.quarantine(&dir.join("app\\link")).unwrap();
        old.created -= PURGE_DELAY.as_secs();
        old.save().unwrap();
        let mut recent = Batch::create(&root, "recent").unwrap();
        recent.quarantine(&dir.join("app\\data")).unwrap();
        fs::create_dir(root.join("damaged")).unwrap();
        fs::write(root.join("damaged\\manifest.json"), b"{").unwrap();

        let listed = list(&root);
        assert_eq!(
            listed.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(),
            ["recent", "old"]
        );
        assert_eq!((listed[1].files, listed[1].keys), (2, 0));
        assert_eq!(listed[1].size_kib, 1);
        assert_eq!(
            u64::from(listed[1].purge_on),
            u64::from(listed[1].created) + PURGE_DELAY.as_secs()
        );
        assert_eq!((listed[0].files, listed[0].size_kib), (1, 1));

        assert_eq!(purge_expired(&root, PURGE_DELAY, SystemTime::now()), 1);
        assert!(!root.join("old").exists());
        assert!(!dir.join("app\\app.exe").exists());
        assert_eq!(fs::read(dir.join("target\\kept.txt")).unwrap(), b"kept");
        assert_eq!(Batch::open(&root.join("recent")).unwrap().items().len(), 1);
        assert!(root.join("damaged").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn batch_names_never_leave_the_root() {
        let root = Path::new(r"C:\q");
        assert_eq!(batch_dir(root, "lot 1"), Some(root.join("lot 1")));
        for name in ["", ".", "..", r"..\x", "a/b", r"C:\Windows", "C:x"] {
            assert_eq!(batch_dir(root, name), None, "{name}");
        }
    }

    #[test]
    fn interrupted_purge_finishes_at_startup() {
        let dir = sandbox("purge-resume");
        let root = dir.join("quarantine");
        let mut batch = Batch::create(&root, "lot").unwrap();
        batch.quarantine(&dir.join("app\\data")).unwrap();
        batch.state = BatchState::Purging;
        batch.save().unwrap();
        let mut batch = Batch::open(&root.join("lot")).unwrap();
        assert!(matches!(batch.restore(), Err(Error::Purging)));
        assert!(matches!(
            batch.quarantine(&dir.join("app\\app.exe")),
            Err(Error::Purging)
        ));
        assert!(dir.join("app\\app.exe").exists());
        assert_eq!(resume_all(&root), 1);
        assert!(!root.join("lot").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn round_trip_puts_files_and_folders_back() {
        let dir = sandbox("round-trip");
        let mut batch = Batch::create(&dir.join("quarantine"), "lot").unwrap();
        batch.quarantine(&dir.join("app\\app.exe")).unwrap();
        batch.quarantine(&dir.join("app\\data")).unwrap();
        assert!(!dir.join("app\\app.exe").exists());
        assert!(!dir.join("app\\data").exists());
        assert!(batch.items().iter().all(|i| i.state == State::Moved));

        let mut reopened = Batch::open(&dir.join("quarantine\\lot")).unwrap();
        assert_eq!(reopened.items(), batch.items());
        reopened.restore().unwrap();
        assert_eq!(fs::read(dir.join("app\\app.exe")).unwrap(), b"binary");
        assert_eq!(
            fs::read(dir.join("app\\data\\settings.ini")).unwrap(),
            b"[x]\ny=1"
        );
        assert!(reopened.items().iter().all(|i| i.state == State::Restored));
        fs::remove_dir_all(dir).unwrap();
    }

    /// A test key (`HKCU\Software\UninstallerTest\...`, nothing else) with values
    /// and a subkey; returns its regedit name and its `.reg` before any change.
    fn test_registry_key(name: &str) -> (String, String, Vec<u8>) {
        let path = registry::test_key::fresh(name);
        let deeper =
            registry::Key::create(Hive::CurrentUser, &format!(r"{path}\Sub"), View::Native64)
                .unwrap();
        for (name, kind, data) in [("", 1, vec![0x41, 0, 0, 0]), ("n", 4, vec![5, 0, 0, 0])] {
            deeper
                .set(&registry::Value {
                    name: name.to_owned(),
                    kind,
                    data,
                })
                .unwrap();
        }
        drop(deeper);
        let before = reg_file::export(Hive::CurrentUser, &path, View::Native64).unwrap();
        (format!(r"HKEY_CURRENT_USER\{path}"), path, before)
    }

    #[test]
    fn round_trip_saves_removes_and_reimports_a_key() {
        let dir = sandbox("key");
        let (key, path, before) = test_registry_key("quarantine");
        let mut batch = Batch::create(&dir.join("quarantine"), "lot").unwrap();
        batch.quarantine_key(&key).unwrap();
        assert!(!registry::key_exists(Hive::CurrentUser, &path, View::Native64).unwrap());
        assert_eq!(batch.items()[0].kind, Kind::Key);
        assert_eq!(batch.items()[0].state, State::Moved);
        assert_eq!(fs::read(batch.stored(0)).unwrap(), before);

        assert!(matches!(batch.quarantine_key(&key), Err(Error::Unreadable)));
        for refused in [r"HKEY_CURRENT_USER\Software", r"HKEY_CLASSES_ROOT\x", "x"] {
            assert!(matches!(
                batch.quarantine_key(refused),
                Err(Error::Protected)
            ));
        }
        assert_eq!(batch.items().len(), 1);

        registry::Key::create(Hive::CurrentUser, &path, View::Native64).unwrap();
        assert!(matches!(batch.restore(), Err(Error::Exists)));
        registry::delete_tree(Hive::CurrentUser, &path, View::Native64).unwrap();
        batch.restore().unwrap();
        assert!(reg_file::export(Hive::CurrentUser, &path, View::Native64).unwrap() == before);
        assert!(!batch.stored(0).exists());
        assert_eq!(batch.items()[0].state, State::Restored);
        registry::test_key::clear(&path);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn resume_removes_a_saved_key() {
        let dir = sandbox("key-resume");
        let (key, path, before) = test_registry_key("resume");
        let mut batch = Batch::create(&dir.join("quarantine"), "lot").unwrap();
        batch.items.push(Item {
            origin: PathBuf::from(&key),
            kind: Kind::Key,
            hash: Some(hex(&Sha256::digest(&before))),
            state: State::Copied,
        });
        batch.save().unwrap();
        fs::write(batch.stored(0), &before).unwrap();
        assert_eq!(resume_all(&dir.join("quarantine")), 1);
        assert!(!registry::key_exists(Hive::CurrentUser, &path, View::Native64).unwrap());
        let mut batch = Batch::open(&dir.join("quarantine\\lot")).unwrap();
        assert_eq!(batch.items()[0].state, State::Moved);
        batch.restore().unwrap();
        assert!(reg_file::export(Hive::CurrentUser, &path, View::Native64).unwrap() == before);
        registry::test_key::clear(&path);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn refuses_protected_paths_without_touching_the_manifest() {
        let dir = sandbox("protected");
        let mut batch = Batch::create(&dir.join("quarantine"), "lot").unwrap();
        let windows = std::env::var("SystemRoot").unwrap();
        for path in [windows.as_str(), r"C:\", "relative"] {
            assert!(matches!(
                batch.quarantine(Path::new(path)),
                Err(Error::Protected)
            ));
        }
        assert!(
            Batch::open(&dir.join("quarantine\\lot"))
                .unwrap()
                .items()
                .is_empty()
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn restore_never_overwrites_nor_restores_altered_content() {
        let dir = sandbox("no-overwrite");
        let mut batch = Batch::create(&dir.join("quarantine"), "lot").unwrap();
        let exe = dir.join("app\\app.exe");
        batch.quarantine(&exe).unwrap();
        fs::write(&exe, b"reinstalled").unwrap();
        assert!(matches!(batch.restore(), Err(Error::Exists)));
        assert_eq!(fs::read(&exe).unwrap(), b"reinstalled");

        fs::rename(&exe, dir.join("other.exe")).unwrap();
        fs::write(batch.stored(0), b"tampered").unwrap();
        assert!(matches!(batch.restore(), Err(Error::Altered)));
        assert!(!exe.exists());
        fs::remove_dir_all(dir).unwrap();
    }

    /// A batch whose last item was written `Pending`, as a crash would leave it.
    fn interrupted(dir: &Path, moved: bool) -> Batch {
        let mut batch = Batch::create(&dir.join("quarantine"), "lot").unwrap();
        let exe = dir.join("app\\app.exe");
        batch.items.push(Item {
            origin: exe.clone(),
            kind: Kind::File,
            hash: Some(file_hash(&exe).unwrap()),
            state: State::Pending,
        });
        batch.save().unwrap();
        if moved {
            fs::rename(&exe, batch.stored(0)).unwrap();
        }
        batch
    }

    #[test]
    fn resumes_an_interrupted_move_either_way() {
        let dir = sandbox("resume");
        let root = dir.join("quarantine");

        interrupted(&dir, false);
        assert_eq!(resume_all(&root), 1);
        assert!(Batch::open(&root.join("lot")).unwrap().items().is_empty());
        assert!(dir.join("app\\app.exe").exists());
        fs::remove_dir_all(&root).unwrap();

        interrupted(&dir, true);
        assert_eq!(resume_all(&root), 1);
        assert_eq!(resume_all(&root), 1, "idempotent");
        let mut batch = Batch::open(&root.join("lot")).unwrap();
        assert_eq!(batch.items()[0].state, State::Moved);
        batch.restore().unwrap();
        assert_eq!(fs::read(dir.join("app\\app.exe")).unwrap(), b"binary");
        assert_eq!(resume_all(&dir.join("absent")), 0);
        fs::remove_dir_all(dir).unwrap();
    }

    /// A batch root on the repository's volume (D: here), while `sandbox` is on
    /// %TEMP%'s (C:). On a machine with one volume the test still passes, by rename.
    fn other_volume_root(name: &str) -> PathBuf {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("uninstaller-tests")
            .join(format!("{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        root
    }

    #[test]
    fn copies_files_and_folders_across_volumes_but_no_links() {
        let dir = sandbox("volumes");
        let root = other_volume_root("volumes");
        let mut batch = Batch::create(&root, "lot").unwrap();
        let exe = dir.join("app\\app.exe");
        let data = dir.join("app\\data");
        batch.quarantine(&exe).unwrap();
        batch.quarantine(&data).unwrap();
        assert!(!exe.exists() && !data.exists());
        assert_eq!(fs::read(batch.stored(0)).unwrap(), b"binary");
        assert_eq!(
            fs::read(batch.stored(1).join("settings.ini")).unwrap(),
            b"[x]\ny=1"
        );
        batch.restore().unwrap();
        assert_eq!(fs::read(&exe).unwrap(), b"binary");
        assert_eq!(fs::read(data.join("settings.ini")).unwrap(), b"[x]\ny=1");
        assert!(!batch.stored(1).exists());

        let crosses = fs::rename(&exe, root.join("probe")).is_err();
        if crosses {
            junction(&dir.join("app\\link"), &data);
            assert!(matches!(
                batch.quarantine(&dir.join("app")),
                Err(Error::OtherVolume)
            ));
            assert!(data.join("settings.ini").exists());
            assert!(!batch.stored(2).exists());
            assert_eq!(batch.items().len(), 2);
            fs::remove_dir(dir.join("app\\link")).unwrap();
        } else {
            fs::rename(root.join("probe"), &exe).unwrap();
        }
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn resume_finishes_a_verified_copy() {
        let dir = sandbox("copied");
        let data = dir.join("app\\data");
        let mut batch = Batch::create(&dir.join("quarantine"), "lot").unwrap();
        batch.items.push(Item {
            origin: data.clone(),
            kind: Kind::Folder,
            hash: None,
            state: State::Copied,
        });
        batch.save().unwrap();
        copy_verified(&data, &batch.stored(0)).unwrap();
        assert_eq!(resume_all(&dir.join("quarantine")), 1);
        assert!(!data.exists());
        let mut batch = Batch::open(&dir.join("quarantine\\lot")).unwrap();
        assert_eq!(batch.items()[0].state, State::Moved);
        batch.restore().unwrap();
        assert!(data.join("settings.ini").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn resume_drops_a_partial_copy_while_the_origin_exists() {
        let dir = sandbox("partial");
        let batch = interrupted(&dir, false);
        fs::write(batch.stored(0), b"bin").unwrap();
        assert_eq!(resume_all(&dir.join("quarantine")), 1);
        assert!(
            Batch::open(&dir.join("quarantine\\lot"))
                .unwrap()
                .items()
                .is_empty()
        );
        assert!(!batch.stored(0).exists());
        assert_eq!(fs::read(dir.join("app\\app.exe")).unwrap(), b"binary");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn detects_a_damaged_manifest() {
        let dir = sandbox("corrupt");
        let mut batch = Batch::create(&dir.join("quarantine"), "lot").unwrap();
        batch.quarantine(&dir.join("app\\app.exe")).unwrap();
        let manifest = dir.join("quarantine\\lot").join(MANIFEST);
        let text = fs::read_to_string(&manifest).unwrap();
        fs::write(&manifest, text.replace("\"moved\"", "\"restored\"")).unwrap();
        assert!(matches!(
            Batch::open(&dir.join("quarantine\\lot")),
            Err(Error::Corrupt)
        ));
        fs::remove_dir_all(dir).unwrap();
    }

    fn junction(link: &Path, target: &Path) {
        let made = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()
            .unwrap();
        assert!(made.status.success());
    }

    #[test]
    fn moves_a_junction_without_following_it() {
        let dir = sandbox("junction");
        let link = dir.join("app\\link");
        junction(&link, &dir.join("app\\data"));
        let mut batch = Batch::create(&dir.join("quarantine"), "lot").unwrap();
        batch.quarantine(&link).unwrap();
        assert!(!link.exists());
        assert!(dir.join("app\\data\\settings.ini").exists());
        batch.restore().unwrap();
        assert!(link.join("settings.ini").exists());
        fs::remove_dir(&link).unwrap();
        fs::remove_dir_all(dir).unwrap();
    }
}
