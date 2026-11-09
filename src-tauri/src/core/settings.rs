//! The user's settings, kept in `settings.json` (ARCHITECTURE.md, Données locales).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use specta::Type;

use super::store;

/// The purge delays the Backups screen offers, in days (SECURITY.md 1).
pub const PURGE_DAYS: [u32; 5] = [7, 14, 30, 60, 90];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// How long a quarantine batch stays restorable, one of `PURGE_DAYS`.
    pub purge_days: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { purge_days: 14 }
    }
}

impl Settings {
    pub fn purge_delay(&self) -> Duration {
        Duration::from_secs(u64::from(self.purge_days) * 24 * 3600)
    }

    /// The settings saved at `path`. A missing or unreadable file, or a value the
    /// screen does not offer, gives the defaults: a setting never blocks the app.
    pub fn load(path: &Path) -> Self {
        fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Self>(&bytes).ok())
            .filter(Self::is_valid)
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        store::replace(path, &text)
    }

    pub fn is_valid(&self) -> bool {
        PURGE_DAYS.contains(&self.purge_days)
    }
}

/// `%LOCALAPPDATA%\Uninstaller\settings.json`.
pub fn path() -> Option<PathBuf> {
    store::data_dir().map(|dir| dir.join("settings.json"))
}

/// The saved settings, or the defaults when there is no data folder.
pub fn current() -> Settings {
    path().map(|path| Settings::load(&path)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("uninstaller-settings-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir.join("settings.json")
    }

    #[test]
    fn saved_settings_load_back() {
        let path = temp_file("round");
        let settings = Settings { purge_days: 30 };
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path), settings);
        assert_eq!(settings.purge_delay(), Duration::from_secs(30 * 86_400));
        assert!(!path.with_extension("json.next").exists());
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn bad_files_give_the_defaults() {
        let path = temp_file("bad");
        assert_eq!(Settings::load(&path), Settings::default());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        for text in ["{", "{}", r#"{"purgeDays": 3}"#, r#"{"purgeDays": "14"}"#] {
            fs::write(&path, text).unwrap();
            assert_eq!(Settings::load(&path), Settings::default(), "{text}");
        }
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn only_offered_delays_are_valid() {
        assert!(Settings::default().is_valid());
        assert!(PURGE_DAYS.contains(&Settings::default().purge_days));
        assert!(!Settings { purge_days: 0 }.is_valid());
        assert!(!Settings { purge_days: 15 }.is_valid());
    }
}
