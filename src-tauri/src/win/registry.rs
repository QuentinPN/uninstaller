//! Registry access. Reading is free; writing is only for the quarantine, which checks
//! `core::protect` and keeps a `.reg` copy first (ADR 0003). HKLM needs the broker
//! (ADR 0002).

use std::io;

use windows::Win32::Foundation::{ERROR_SUCCESS, FILETIME, WIN32_ERROR};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, HKEY_USERS, KEY_ALL_ACCESS, KEY_READ,
    KEY_WOW64_32KEY, KEY_WOW64_64KEY, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_ROUTINE_FLAGS,
    REG_SAM_FLAGS, REG_VALUE_TYPE, RRF_NOEXPAND, RRF_RT_REG_DWORD, RRF_RT_REG_EXPAND_SZ,
    RRF_RT_REG_SZ, RegCloseKey, RegCreateKeyExW, RegDeleteKeyExW, RegDeleteTreeW, RegEnumKeyExW,
    RegEnumValueW, RegGetValueW, RegOpenKeyExW, RegQueryInfoKeyW, RegSetValueExW,
};
use windows::core::{HSTRING, PCWSTR, PWSTR};

#[derive(Clone, Copy, Debug)]
pub enum Hive {
    LocalMachine,
    CurrentUser,
    /// HKEY_USERS: one subkey per loaded account hive, named by its SID.
    Users,
}

impl Hive {
    /// The root as regedit shows it.
    pub fn name(self) -> &'static str {
        match self {
            Hive::LocalMachine => "HKEY_LOCAL_MACHINE",
            Hive::CurrentUser => "HKEY_CURRENT_USER",
            Hive::Users => "HKEY_USERS",
        }
    }

    /// Splits a key as regedit shows it (`HKEY_CURRENT_USER\Software\X`) into its hive
    /// and path; `None` for another root.
    pub fn parse(key: &str) -> Option<(Hive, &str)> {
        [Hive::LocalMachine, Hive::CurrentUser, Hive::Users]
            .into_iter()
            .find_map(|hive| {
                let len = hive.name().len();
                let root = key.get(..len)?;
                let rest = &key[len..];
                if !root.eq_ignore_ascii_case(hive.name()) {
                    return None;
                }
                match rest.strip_prefix('\\') {
                    Some(path) => Some((hive, path)),
                    None => rest.is_empty().then_some((hive, "")),
                }
            })
    }

    fn root(self) -> HKEY {
        match self {
            Hive::LocalMachine => HKEY_LOCAL_MACHINE,
            Hive::CurrentUser => HKEY_CURRENT_USER,
            Hive::Users => HKEY_USERS,
        }
    }
}

/// Registry view: explicit, so a 32-bit or 64-bit build reads the same keys.
#[derive(Clone, Copy, Debug)]
pub enum View {
    Native64,
    Wow32,
}

impl View {
    fn flags(self) -> REG_SAM_FLAGS {
        match self {
            View::Native64 => KEY_WOW64_64KEY,
            View::Wow32 => KEY_WOW64_32KEY,
        }
    }
}

/// One value as stored: its name (`""` for the default value), its type number and its raw
/// bytes.
pub struct Value {
    pub name: String,
    pub kind: u32,
    pub data: Vec<u8>,
}

/// An open key, closed on drop.
pub struct Key {
    handle: HKEY,
    access: REG_SAM_FLAGS,
}

impl Key {
    /// Opens `path` under `hive` for reading (`""` opens the hive itself); `None` if it
    /// does not exist or is denied.
    pub fn open(hive: Hive, path: &str, view: View) -> Option<Self> {
        open(hive.root(), path, KEY_READ | view.flags()).ok()
    }

    /// Opens a direct subkey with the same view.
    pub fn subkey(&self, name: &str) -> Option<Self> {
        open(self.handle, name, self.access).ok()
    }

    /// Names of the direct subkeys; one that cannot be read is skipped.
    pub fn subkey_names(&self) -> Vec<String> {
        self.enum_subkeys()
            .map(|(names, _)| names)
            .unwrap_or_default()
    }

    /// The subkey names and how many subkeys the key holds.
    fn enum_subkeys(&self) -> Option<(Vec<String>, u32)> {
        let info = self.info()?;
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
        Some((names, info.subkeys))
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
}

impl Key {
    /// Every subkey name, or `None` if one cannot be read.
    pub fn all_subkey_names(&self) -> Option<Vec<String>> {
        let (names, count) = self.enum_subkeys()?;
        (names.len() == count as usize).then_some(names)
    }

    /// Every value with its raw bytes, in the order the registry lists them, or `None`
    /// if one cannot be read (including one that grew since the key was measured).
    pub fn values(&self) -> Option<Vec<Value>> {
        let info = self.info()?;
        let mut values = Vec::with_capacity(info.values as usize);
        let mut name = vec![0u16; info.max_value_name_len as usize + 1];
        let mut data = vec![0u8; info.max_value_len as usize];
        for index in 0..info.values {
            let mut name_len = name.len() as u32;
            let mut data_len = data.len() as u32;
            let mut kind = 0u32;
            // SAFETY: `name` holds `name_len` u16 and `data` holds `data_len` bytes, both
            // owned here; `kind` is a live local.
            let status = unsafe {
                RegEnumValueW(
                    self.handle,
                    index,
                    Some(PWSTR(name.as_mut_ptr())),
                    &mut name_len,
                    None,
                    Some(&mut kind),
                    Some(data.as_mut_ptr()),
                    Some(&mut data_len),
                )
            };
            if status != ERROR_SUCCESS {
                return None;
            }
            values.push(Value {
                name: String::from_utf16_lossy(&name[..name_len as usize]),
                kind,
                data: data[..data_len as usize].to_vec(),
            });
        }
        Some(values)
    }

    /// Opens `path`, creating it and its missing parents, for reading and writing.
    pub fn create(hive: Hive, path: &str, view: View) -> io::Result<Self> {
        let name = HSTRING::from(path);
        let access = KEY_READ | KEY_WRITE | view.flags();
        let mut handle = HKEY::default();
        // SAFETY: the root is predefined and `handle` is a valid output; no class, security
        // attributes or disposition are requested.
        let status = unsafe {
            RegCreateKeyExW(
                hive.root(),
                &name,
                None,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                access,
                None,
                &mut handle,
                None,
            )
        };
        check(status)?;
        Ok(Key { handle, access })
    }

    /// Writes a value as stored, type and bytes unchanged.
    pub fn set(&self, value: &Value) -> io::Result<()> {
        let name = HSTRING::from(value.name.as_str());
        // SAFETY: the key is live and open for writing; `value.data` is borrowed for the call.
        check(unsafe {
            RegSetValueExW(
                self.handle,
                &name,
                None,
                REG_VALUE_TYPE(value.kind),
                Some(&value.data),
            )
        })
    }
}

impl Key {
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
        let mut values = 0u32;
        let mut max_value_name_len = 0u32;
        let mut max_value_len = 0u32;
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
                Some(&mut values),
                Some(&mut max_value_name_len),
                Some(&mut max_value_len),
                None,
                Some(&mut last_write),
            )
        };
        (status == ERROR_SUCCESS).then(|| KeyInfo {
            subkeys,
            max_subkey_len,
            values,
            max_value_name_len,
            max_value_len,
            last_write: (u64::from(last_write.dwHighDateTime) << 32)
                | u64::from(last_write.dwLowDateTime),
        })
    }
}

/// Removes the key at `path` with its values and subkeys. Never a hive: an empty path is
/// refused. The caller has checked `core::protect` and kept a `.reg` copy.
pub fn delete_tree(hive: Hive, path: &str, view: View) -> io::Result<()> {
    if path.is_empty() {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let key = open(hive.root(), path, KEY_ALL_ACCESS | view.flags())?;
    // SAFETY: a live key opened with every right; no subkey name, so only its contents go.
    check(unsafe { RegDeleteTreeW(key.handle, PCWSTR::null()) })?;
    drop(key);
    delete_empty(hive, path, view)
}

/// Whether the key at `path` may be removed: opened with every right, as `delete_tree`
/// does, so an access denied comes before anything is saved or changed.
pub fn check_delete(hive: Hive, path: &str, view: View) -> io::Result<()> {
    open(hive.root(), path, KEY_ALL_ACCESS | view.flags()).map(drop)
}

/// Whether the key at `path` exists; an error other than "not found" (access denied)
/// is not taken for an answer.
pub fn key_exists(hive: Hive, path: &str, view: View) -> io::Result<bool> {
    match open(hive.root(), path, KEY_READ | view.flags()) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

/// Removes a key that has no subkeys left (Windows refuses otherwise).
fn delete_empty(hive: Hive, path: &str, view: View) -> io::Result<()> {
    let (parent, name) = path.rsplit_once('\\').unwrap_or(("", path));
    let parent = open(hive.root(), parent, KEY_READ | view.flags())?;
    let name = HSTRING::from(name);
    // SAFETY: `parent` is a live key; the view flag selects the same registry view.
    check(unsafe { RegDeleteKeyExW(parent.handle, &name, view.flags().0, None) })
}

fn check(status: WIN32_ERROR) -> io::Result<()> {
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(status.0 as i32))
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
    values: u32,
    /// In characters, without the terminating null.
    max_value_name_len: u32,
    /// In bytes.
    max_value_len: u32,
    last_write: u64,
}

fn open(parent: HKEY, path: &str, access: REG_SAM_FLAGS) -> io::Result<Key> {
    let path = HSTRING::from(path);
    let mut handle = HKEY::default();
    // SAFETY: `parent` is a predefined root or a live key; `handle` is a valid output.
    check(unsafe { RegOpenKeyExW(parent, &path, None, access, &mut handle) })?;
    Ok(Key { handle, access })
}

/// Test keys live under `HKCU\Software\UninstallerTest` only, one per test and
/// process, so tests never touch anything else.
#[cfg(test)]
pub mod test_key {
    use super::{Hive, View, delete_empty, delete_tree};

    const PARENT: &str = r"Software\UninstallerTest";

    /// A fresh path for `name`, cleared of a failed run's leftovers.
    pub fn fresh(name: &str) -> String {
        let path = format!(r"{PARENT}\{name}-{}", std::process::id());
        let _ = delete_tree(Hive::CurrentUser, &path, View::Native64);
        path
    }

    /// Removes the test key, and the parent once no other test uses it.
    pub fn clear(path: &str) {
        let _ = delete_tree(Hive::CurrentUser, path, View::Native64);
        let _ = delete_empty(Hive::CurrentUser, PARENT, View::Native64);
    }
}

#[cfg(test)]
mod tests {
    use super::{Hive, Key, Value, View, delete_tree, test_key};

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
    fn a_machine_key_cannot_be_removed_without_elevation() {
        // Only opens the key: nothing changes, elevated or not.
        let denied = super::check_delete(Hive::LocalMachine, "SOFTWARE", View::Native64).is_err();
        assert_eq!(denied, !crate::win::token::is_elevated());
    }

    #[test]
    fn parses_regedit_roots() {
        assert!(matches!(
            Hive::parse(r"hkey_current_user\Software\X"),
            Some((Hive::CurrentUser, r"Software\X"))
        ));
        assert!(matches!(Hive::parse("HKEY_USERS"), Some((Hive::Users, ""))));
        assert!(Hive::parse(r"HKEY_CURRENT_USERS\X").is_none());
        assert!(Hive::parse(r"HKEY_CLASSES_ROOT\X").is_none());
    }

    #[test]
    fn creates_writes_and_deletes_a_tree() {
        let path = test_key::fresh("tree");
        let key = Key::create(Hive::CurrentUser, &format!(r"{path}\a\b"), View::Native64).unwrap();
        key.set(&Value {
            name: "n".to_owned(),
            kind: 4,
            data: vec![7, 0, 0, 0],
        })
        .unwrap();
        drop(key);
        let read = Key::open(Hive::CurrentUser, &format!(r"{path}\a\b"), View::Native64).unwrap();
        assert_eq!(read.dword("n"), Some(7));
        drop(read);
        assert!(delete_tree(Hive::CurrentUser, "", View::Native64).is_err());
        delete_tree(Hive::CurrentUser, &path, View::Native64).unwrap();
        assert!(Key::open(Hive::CurrentUser, &path, View::Native64).is_none());
        test_key::clear(&path);
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
