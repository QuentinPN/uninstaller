//! Quarantine batches (ADR 0003, 0005): nothing is deleted, files and folders are moved
//! into a batch folder whose manifest is written before each move, so a batch can be
//! restored, or resumed after a crash.

use std::fmt;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::protect;

const MANIFEST: &str = "manifest.json";
/// Written in full, then renamed over the manifest (ADR 0005).
const MANIFEST_NEXT: &str = "manifest.json.next";
/// Where moved items are kept, one entry per item, named by its index.
const STORE: &str = "items";

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
    /// A folder or link on another volume than the batch: only files are copied
    /// across volumes so far.
    OtherVolume,
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
            Self::Io(error) => error.fmt(f),
        }
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Per item: `Pending` is written before the move, `Moved` after it, `Restored` once
/// back at its origin (STATE_MACHINES.md).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Pending,
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
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub origin: PathBuf,
    pub kind: Kind,
    /// SHA-256 of a file's content; folders and links have none.
    pub hash: Option<String>,
    pub state: State,
}

#[derive(Serialize, Deserialize)]
struct Manifest {
    /// SHA-256 of `items` as written, to detect a damaged manifest.
    hash: String,
    items: Vec<Item>,
}

/// One cleanup's quarantine folder and its manifest.
pub struct Batch {
    dir: PathBuf,
    items: Vec<Item>,
}

impl Batch {
    /// A new, empty batch at `root\name`; fails if it already exists.
    pub fn create(root: &Path, name: &str) -> Result<Self, Error> {
        fs::create_dir_all(root)?;
        let dir = root.join(name);
        fs::create_dir(&dir)?;
        fs::create_dir(dir.join(STORE))?;
        let batch = Self {
            dir,
            items: Vec::new(),
        };
        batch.save()?;
        Ok(batch)
    }

    /// Reads an existing batch, refusing a manifest that fails its own hash.
    pub fn open(dir: &Path) -> Result<Self, Error> {
        let manifest: Manifest =
            serde_json::from_slice(&fs::read(dir.join(MANIFEST))?).map_err(|_| Error::Corrupt)?;
        if items_hash(&manifest.items)? != manifest.hash {
            return Err(Error::Corrupt);
        }
        Ok(Self {
            dir: dir.to_path_buf(),
            items: manifest.items,
        })
    }

    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// Moves `path` into the batch. The manifest names it `Pending` before the move
    /// and `Moved` after. A link or junction is moved itself, never followed.
    pub fn quarantine(&mut self, path: &Path) -> Result<(), Error> {
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
        if let Err(error) = move_item(path, &self.stored(index), &self.items[index]) {
            // Nothing moved: the item leaves the manifest again.
            self.items.pop();
            self.save()?;
            return Err(error);
        }
        self.set_state(index, State::Moved)
    }

    /// Puts every moved item back, last moved first. Stops at the first item that
    /// cannot go back (its origin exists again, or its content changed); the items
    /// already restored stay restored.
    pub fn restore(&mut self) -> Result<(), Error> {
        for index in (0..self.items.len()).rev() {
            let item = &self.items[index];
            if item.state != State::Moved {
                continue;
            }
            let stored = self.stored(index);
            if fs::symlink_metadata(&item.origin).is_ok() {
                return Err(Error::Exists);
            }
            if let Some(hash) = &item.hash
                && file_hash(&stored)? != *hash
            {
                return Err(Error::Altered);
            }
            if let Some(parent) = item.origin.parent() {
                fs::create_dir_all(parent)?;
            }
            move_item(&stored, &item.origin, item)?;
            self.set_state(index, State::Restored)?;
        }
        Ok(())
    }

    /// Finishes what a crash interrupted. Only the last item can be `Pending` (one
    /// move at a time). While its origin exists it never left: a partial copy in the
    /// batch is dropped and so is the item. Once only the batch holds it, it is
    /// `Moved`. Running it again changes nothing.
    pub fn resume(&mut self) -> Result<(), Error> {
        let Some(index) = self.items.iter().position(|i| i.state == State::Pending) else {
            return Ok(());
        };
        if index + 1 != self.items.len() {
            return Err(Error::Corrupt);
        }
        let stored = self.stored(index);
        let in_batch = fs::symlink_metadata(&stored).is_ok();
        if fs::symlink_metadata(&self.items[index].origin).is_ok() {
            // Only a file is ever copied: a folder or link arrives by rename, whole.
            if in_batch {
                fs::remove_file(&stored)?;
            }
            self.items.pop();
            self.save()
        } else if in_batch {
            self.set_state(index, State::Moved)
        } else {
            Err(Error::Corrupt)
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
            hash: items_hash(&self.items)?,
            items: self.items.clone(),
        };
        let next = self.dir.join(MANIFEST_NEXT);
        let text = serde_json::to_vec_pretty(&manifest).map_err(io::Error::other)?;
        let mut file = File::create(&next)?;
        file.write_all(&text)?;
        // On disk before it replaces the manifest.
        file.sync_all()?;
        drop(file);
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
    let Ok(entries) = fs::read_dir(root) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_dir()))
        .filter(|entry| {
            Batch::open(&entry.path())
                .and_then(|mut batch| batch.resume())
                .is_ok()
        })
        .count()
}

/// Renames `from` to `to`. Across volumes a file is copied (attributes and last
/// write time kept), checked against the item's hash, and only then removed from
/// `from`; on any failure the copy goes and `from` stays.
fn move_item(from: &Path, to: &Path, item: &Item) -> Result<(), Error> {
    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() != io::ErrorKind::CrossesDevices => Err(error.into()),
        Err(_) if item.kind != Kind::File => Err(Error::OtherVolume),
        Err(_) => {
            fs::copy(from, to)?;
            let checked = file_hash(to).map_err(Error::from).and_then(|hash| {
                if Some(&hash) == item.hash.as_ref() {
                    Ok(())
                } else {
                    Err(Error::Altered)
                }
            });
            match checked.and_then(|()| fs::remove_file(from).map_err(Error::from)) {
                Ok(()) => Ok(()),
                Err(error) => {
                    // Our own copy, never the user's file.
                    let _ = fs::remove_file(to);
                    Err(error)
                }
            }
        }
    }
}

fn items_hash(items: &[Item]) -> Result<String, Error> {
    let bytes = serde_json::to_vec(items).map_err(io::Error::other)?;
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
    fn copies_files_across_volumes_and_refuses_folders() {
        let dir = sandbox("volumes");
        let root = other_volume_root("volumes");
        let mut batch = Batch::create(&root, "lot").unwrap();
        let exe = dir.join("app\\app.exe");
        batch.quarantine(&exe).unwrap();
        assert!(!exe.exists());
        assert_eq!(fs::read(batch.stored(0)).unwrap(), b"binary");

        let data = dir.join("app\\data");
        let crosses = fs::rename(&data, root.join("probe")).is_err();
        if !crosses {
            fs::rename(root.join("probe"), &data).unwrap();
        } else {
            assert!(matches!(batch.quarantine(&data), Err(Error::OtherVolume)));
            assert!(data.join("settings.ini").exists());
            assert_eq!(batch.items().len(), 1);
        }

        batch.restore().unwrap();
        assert_eq!(fs::read(&exe).unwrap(), b"binary");
        fs::remove_dir_all(root).unwrap();
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

    #[test]
    fn moves_a_junction_without_following_it() {
        let dir = sandbox("junction");
        let link = dir.join("app\\link");
        let made = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(dir.join("app\\data"))
            .output()
            .unwrap();
        assert!(made.status.success());
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
