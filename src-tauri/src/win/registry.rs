//! Read-only registry access. Nothing here writes: changes go through the broker (ADR 0002).

use windows::Win32::Foundation::{ERROR_SUCCESS, FILETIME};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
    REG_ROUTINE_FLAGS, REG_SAM_FLAGS, RRF_NOEXPAND, RRF_RT_REG_DWORD, RRF_RT_REG_EXPAND_SZ,
    RRF_RT_REG_SZ, RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, RegQueryInfoKeyW,
};
use windows::core::{HSTRING, PWSTR};

#[derive(Clone, Copy, Debug)]
pub enum Hive {
    LocalMachine,
    CurrentUser,
}

/// Registry view: explicit, so a 32-bit or 64-bit build reads the same keys.
#[derive(Clone, Copy, Debug)]
pub enum View {
    Native64,
    Wow32,
}

/// An open key, closed on drop.
pub struct Key {
    handle: HKEY,
    access: REG_SAM_FLAGS,
}

impl Key {
    /// Opens `path` under `hive` for reading; `None` if it does not exist or is denied.
    pub fn open(hive: Hive, path: &str, view: View) -> Option<Self> {
        let root = match hive {
            Hive::LocalMachine => HKEY_LOCAL_MACHINE,
            Hive::CurrentUser => HKEY_CURRENT_USER,
        };
        let access = KEY_READ
            | match view {
                View::Native64 => KEY_WOW64_64KEY,
                View::Wow32 => KEY_WOW64_32KEY,
            };
        open(root, path, access)
    }

    /// Opens a direct subkey with the same view.
    pub fn subkey(&self, name: &str) -> Option<Self> {
        open(self.handle, name, self.access)
    }

    /// Names of the direct subkeys.
    pub fn subkey_names(&self) -> Vec<String> {
        let Some(info) = self.info() else {
            return Vec::new();
        };
        let mut names = Vec::with_capacity(info.subkeys as usize);
        let mut buffer = vec![0u16; info.max_subkey_len as usize + 1];
        for index in 0..info.subkeys {
            let mut len = buffer.len() as u32;
            // SAFETY: `buffer` holds `len` u16; the other optional outputs are not requested.
            let status = unsafe {
                RegEnumKeyExW(
                    self.handle,
                    index,
                    Some(PWSTR(buffer.as_mut_ptr())),
                    &mut len,
                    None,
                    None,
                    None,
                    None,
                )
            };
            if status == ERROR_SUCCESS {
                names.push(String::from_utf16_lossy(&buffer[..len as usize]));
            }
        }
        names
    }

    /// A REG_SZ or REG_EXPAND_SZ value (environment variables left unexpanded), trimmed;
    /// `None` if missing, of another type, or empty.
    pub fn string(&self, name: &str) -> Option<String> {
        let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND;
        let mut size = 0u32;
        self.get(name, flags, None, &mut size)?;
        let mut buffer = vec![0u16; (size as usize).div_ceil(2)];
        let mut size = (buffer.len() * 2) as u32;
        self.get(name, flags, Some(buffer.as_mut_ptr().cast()), &mut size)?;
        let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        let value = String::from_utf16_lossy(&buffer[..len]).trim().to_owned();
        (!value.is_empty()).then_some(value)
    }

    /// A REG_DWORD value.
    pub fn dword(&self, name: &str) -> Option<u32> {
        let mut value = 0u32;
        let mut size = 4u32;
        self.get(
            name,
            RRF_RT_REG_DWORD,
            Some((&mut value as *mut u32).cast()),
            &mut size,
        )?;
        Some(value)
    }

    /// Last write time of the key, in FILETIME units (100 ns since 1601-01-01 UTC).
    pub fn last_write(&self) -> Option<u64> {
        self.info().map(|info| info.last_write)
    }

    fn get(
        &self,
        name: &str,
        flags: REG_ROUTINE_FLAGS,
        data: Option<*mut core::ffi::c_void>,
        size: &mut u32,
    ) -> Option<()> {
        let name = HSTRING::from(name);
        // SAFETY: `data`, when given, points to at least `*size` writable bytes owned by the
        // caller; a null data pointer only queries the size.
        let status =
            unsafe { RegGetValueW(self.handle, None, &name, flags, None, data, Some(size)) };
        (status == ERROR_SUCCESS).then_some(())
    }

    fn info(&self) -> Option<KeyInfo> {
        let mut subkeys = 0u32;
        let mut max_subkey_len = 0u32;
        let mut last_write = FILETIME::default();
        // SAFETY: every requested output points to a live local; the rest are not requested.
        let status = unsafe {
            RegQueryInfoKeyW(
                self.handle,
                None,
                None,
                None,
                Some(&mut subkeys),
                Some(&mut max_subkey_len),
                None,
                None,
                None,
                None,
                None,
                Some(&mut last_write),
            )
        };
        (status == ERROR_SUCCESS).then(|| KeyInfo {
            subkeys,
            max_subkey_len,
            last_write: (u64::from(last_write.dwHighDateTime) << 32)
                | u64::from(last_write.dwLowDateTime),
        })
    }
}

impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: `handle` was opened by RegOpenKeyExW and is closed exactly once, here.
        unsafe {
            let _ = RegCloseKey(self.handle);
        }
    }
}

struct KeyInfo {
    subkeys: u32,
    max_subkey_len: u32,
    last_write: u64,
}

fn open(parent: HKEY, path: &str, access: REG_SAM_FLAGS) -> Option<Key> {
    let path = HSTRING::from(path);
    let mut handle = HKEY::default();
    // SAFETY: `parent` is a predefined root or a live key; `handle` is a valid output.
    let status = unsafe { RegOpenKeyExW(parent, &path, None, access, &mut handle) };
    (status == ERROR_SUCCESS).then_some(Key { handle, access })
}

#[cfg(test)]
mod tests {
    use super::{Hive, Key, View};

    /// Present on every Windows install, readable without elevation.
    const CURRENT_VERSION: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";

    #[test]
    fn reads_strings_dwords_and_subkeys() {
        let key = Key::open(Hive::LocalMachine, CURRENT_VERSION, View::Native64).expect("open");
        assert!(key.string("ProductName").is_some());
        assert!(key.dword("CurrentMajorVersionNumber").is_some());
        assert!(
            key.string("CurrentMajorVersionNumber").is_none(),
            "type is checked"
        );
        assert!(key.string("NoSuchValue").is_none());
        assert!(key.last_write().is_some());
        assert!(!key.subkey_names().is_empty());
    }

    #[test]
    fn missing_key_is_none() {
        assert!(
            Key::open(
                Hive::CurrentUser,
                r"SOFTWARE\Uninstaller\Nope",
                View::Native64
            )
            .is_none()
        );
    }
}
