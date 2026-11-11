//! Size on disk of each program's install folder (F1), computed after the list is shown
//! and handed out in batches as folders finish.

use serde::Serialize;
use specta::Type;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::core::inventory::Program;
use crate::core::protect;

/// Folders walked at once: enough to hide SSD latency without starving the UI.
const WORKERS: usize = 4;
/// Longest wait before the folders finished so far are handed out.
const BATCH_EVERY: Duration = Duration::from_millis(100);

#[derive(Clone, Debug, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProgramSize {
    pub id: String,
    /// Sum of the file sizes under the install folder, in KiB (rounded up).
    pub size_kib: u32,
}

/// Walks every program's install folder once (programs sharing a folder share the
/// result) and calls `send` with batches until all are done. Protected folders, such as
/// a drive root given as install location, are skipped.
pub fn compute(programs: &[Program], mut send: impl FnMut(Vec<ProgramSize>)) {
    let mut ids_by_folder: HashMap<PathBuf, Vec<String>> = HashMap::new();
    for program in programs {
        if let Some(folder) = program
            .install_folder()
            .filter(|f| !protect::is_protected(f))
        {
            ids_by_folder
                .entry(folder)
                .or_default()
                .push(program.id.clone());
        }
    }
    let folders: Vec<_> = ids_by_folder.into_iter().collect();
    let next = AtomicUsize::new(0);
    let (done, results) = mpsc::channel();
    thread::scope(|scope| {
        for _ in 0..WORKERS.min(folders.len()) {
            let done = done.clone();
            scope.spawn(|| {
                let done = done;
                while let Some((folder, ids)) = folders.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let _ = done.send((ids, folder_bytes(folder)));
                }
            });
        }
        drop(done);
        let mut batch = Vec::new();
        let mut since = Instant::now();
        loop {
            let finished = match results.recv_timeout(BATCH_EVERY) {
                Ok((ids, bytes)) => {
                    let size_kib = kib(bytes);
                    batch.extend(ids.iter().map(|id| ProgramSize {
                        id: id.clone(),
                        size_kib,
                    }));
                    false
                }
                Err(mpsc::RecvTimeoutError::Timeout) => false,
                Err(mpsc::RecvTimeoutError::Disconnected) => true,
            };
            if !batch.is_empty() && (finished || since.elapsed() >= BATCH_EVERY) {
                send(std::mem::take(&mut batch));
                since = Instant::now();
            }
            if finished {
                break;
            }
        }
    });
}

/// Bytes as KiB, rounded up, the unit sizes cross the IPC in.
pub fn kib(bytes: u64) -> u32 {
    u32::try_from(bytes.div_ceil(1024)).unwrap_or(u32::MAX)
}

/// Bytes of the file at `path`, or of every file under the folder there; a link
/// itself counts as zero.
pub fn path_bytes(path: &Path) -> u64 {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => folder_bytes(path),
        Ok(meta) if meta.is_file() => meta.len(),
        _ => 0,
    }
}

/// Bytes of every file under `folder`. Junctions and symbolic links are not followed
/// (they may point outside the program, or loop); unreadable entries count as zero.
pub fn folder_bytes(folder: &Path) -> u64 {
    let mut total = 0;
    let mut pending = vec![folder.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            // On Windows both come from the directory listing itself: no extra file open.
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file() {
                total += entry.metadata().map_or(0, |m| m.len());
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::inventory::tests::program;
    use std::fs;

    /// A fresh folder under the system temp dir, removed by the test that made it.
    fn temp_folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("uninstaller-sizes-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("sub")).unwrap();
        dir
    }

    #[test]
    fn sums_nested_files() {
        let dir = temp_folder("sum");
        fs::write(dir.join("a.bin"), vec![0u8; 1500]).unwrap();
        fs::write(dir.join("sub").join("b.bin"), vec![0u8; 600]).unwrap();
        assert_eq!(folder_bytes(&dir), 2100);
        fs::remove_dir_all(&dir).unwrap();
    }

    /// A file counts its own bytes, a junction none of its target's.
    #[test]
    fn measures_a_file_a_folder_or_a_link_itself() {
        let dir = temp_folder("path");
        fs::write(dir.join("sub").join("b.bin"), vec![0u8; 600]).unwrap();
        crate::core::test_support::junction(&dir.join("link"), &dir.join("sub"));
        assert_eq!(path_bytes(&dir.join("sub").join("b.bin")), 600);
        assert_eq!(path_bytes(&dir.join("link")), 0);
        assert_eq!(path_bytes(&dir), 600);
        assert_eq!(path_bytes(&dir.join("absent")), 0);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn batches_cover_every_program_and_skip_protected_folders() {
        let dir = temp_folder("batch");
        fs::write(dir.join("a.bin"), vec![0u8; 2048]).unwrap();
        let folder = dir.to_string_lossy().into_owned();
        let windows = std::env::var("SystemRoot").unwrap();
        let programs = [
            program("one", Some(&folder)),
            program("two", Some(&format!("\"{folder}\""))),
            program("system", Some(&windows)),
            program("none", None),
        ];
        let mut sizes = Vec::new();
        compute(&programs, |batch| sizes.extend(batch));
        sizes.sort_by(|a, b| a.id.cmp(&b.id));
        let expected = |id: &str| ProgramSize {
            id: id.into(),
            size_kib: 2,
        };
        assert_eq!(sizes, [expected("one"), expected("two")]);
        fs::remove_dir_all(&dir).unwrap();
    }
}
