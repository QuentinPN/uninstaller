//! Helpers shared by the tests of several modules.

use std::path::Path;

/// Makes `link` a junction to `target` (no administrator rights needed).
pub fn junction(link: &Path, target: &Path) {
    let made = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .unwrap();
    assert!(made.status.success());
}
