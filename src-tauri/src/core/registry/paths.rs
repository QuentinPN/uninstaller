//! The registry locations of installed programs, defined once.

use crate::core::inventory::Scope;
use crate::core::profiles::{self, Profile};
use crate::win::registry::{Hive, Key, View};

/// The Uninstall key under a `SOFTWARE` root (native or WOW6432Node).
macro_rules! uninstall {
    ($software:literal) => {
        concat!($software, r"\Microsoft\Windows\CurrentVersion\Uninstall")
    };
}

/// System Restore settings and state, under HKLM (`LastIndex`: the last point's number).
pub const SYSTEM_RESTORE: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\SystemRestore";

/// Where programs keep their settings, one subkey per publisher or product, with the
/// 32-bit twins as regedit shows them; all read in the native view.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "Phase 3: tested now, called by the next slices")
)]
pub const SOFTWARE_ROOTS: [(Hive, &str); 4] = [
    (Hive::LocalMachine, "SOFTWARE"),
    (Hive::LocalMachine, r"SOFTWARE\WOW6432Node"),
    (Hive::CurrentUser, "Software"),
    (Hive::CurrentUser, r"Software\WOW6432Node"),
];

/// One subkey per service and driver, under HKLM.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "Phase 3: tested now, called by the next slices")
)]
pub const SERVICES: &str = r"SYSTEM\CurrentControlSet\Services";

/// Every installed program has a subkey here (per hive and view).
pub const UNINSTALL: &str = uninstall!("SOFTWARE");
/// The same key as regedit shows it in the 32-bit view.
const UNINSTALL_WOW32: &str = uninstall!(r"SOFTWARE\WOW6432Node");

/// One Uninstall key: a hive, a view and, for another account, its profile.
#[derive(Clone, Debug)]
pub struct Source {
    pub scope: Scope,
    hive: Hive,
    view: View,
    pub profile: Option<Profile>,
}

/// Where a program can be registered on every PC, in reading order. User hives are not
/// redirected: one view is enough.
pub const UNINSTALL_SOURCES: [Source; 3] = [
    Source::fixed(Scope::Machine64, Hive::LocalMachine, View::Native64),
    Source::fixed(Scope::Machine32, Hive::LocalMachine, View::Wow32),
    Source::fixed(Scope::User, Hive::CurrentUser, View::Native64),
];

impl Source {
    const fn fixed(scope: Scope, hive: Hive, view: View) -> Self {
        Source {
            scope,
            hive,
            view,
            profile: None,
        }
    }

    /// The Uninstall key in another account's loaded hive.
    pub fn other_user(profile: Profile) -> Self {
        Source {
            scope: Scope::OtherUser,
            hive: Hive::Users,
            view: View::Native64,
            profile: Some(profile),
        }
    }

    /// The fixed sources, then one per other loaded account.
    pub fn all() -> impl Iterator<Item = Source> {
        UNINSTALL_SOURCES
            .into_iter()
            .chain(profiles::others().into_iter().map(Source::other_user))
    }

    pub fn open(&self) -> Option<Key> {
        let path = match &self.profile {
            Some(p) => format!(r"{}\{UNINSTALL}", p.sid),
            None => UNINSTALL.to_owned(),
        };
        Key::open(self.hive, &path, self.view)
    }

    /// Opaque id of a program: `tag\key`, or `hku\sid\key` for another account.
    pub fn id(&self, key_name: &str) -> String {
        format!(r"{}{key_name}", self.prefix())
    }

    /// Reads an id back to its source and key name; `None` if malformed.
    pub fn locate(id: &str) -> Option<(Source, &str)> {
        let (tag, rest) = id.split_once('\\')?;
        let (source, key_name) = if tag == Scope::OtherUser.tag() {
            let (sid, key_name) = rest.split_once('\\')?;
            (Source::other_user(profiles::profile(sid)?), key_name)
        } else {
            let source = UNINSTALL_SOURCES
                .into_iter()
                .find(|s| s.scope.tag() == tag)?;
            (source, rest)
        };
        (!key_name.is_empty() && !key_name.contains('\\')).then_some((source, key_name))
    }

    /// A program's key as regedit shows it (the 32-bit view is WOW6432Node).
    pub fn shown_key(&self, key_name: &str) -> String {
        let uninstall = match self.view {
            View::Native64 => UNINSTALL,
            View::Wow32 => UNINSTALL_WOW32,
        };
        let sid = self
            .profile
            .as_ref()
            .map(|p| format!(r"{}\", p.sid))
            .unwrap_or_default();
        format!(r"{}\{sid}{uninstall}\{key_name}", self.hive.name())
    }

    fn prefix(&self) -> String {
        match &self.profile {
            Some(p) => format!(r"{}\{}\", self.scope.tag(), p.sid),
            None => format!(r"{}\", self.scope.tag()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SID: &str = "S-1-5-21-1-2-3-1002";

    fn other() -> Source {
        Source::other_user(Profile {
            sid: SID.into(),
            name: None,
        })
    }

    #[test]
    fn shown_key_names_the_hive_the_account_and_the_32_bit_view() {
        assert_eq!(
            UNINSTALL_SOURCES[1].shown_key("App"),
            r"HKEY_LOCAL_MACHINE\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\App"
        );
        assert_eq!(
            UNINSTALL_SOURCES[2].shown_key("App"),
            r"HKEY_CURRENT_USER\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\App"
        );
        assert_eq!(
            other().shown_key("App"),
            format!(r"HKEY_USERS\{SID}\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\App")
        );
    }

    #[test]
    fn ids_read_back_to_their_source() {
        for source in [UNINSTALL_SOURCES[0].clone(), other()] {
            let id = source.id("{GUID}");
            let (found, key_name) = Source::locate(&id).expect(&id);
            assert_eq!((found.scope, key_name), (source.scope, "{GUID}"));
            assert_eq!(found.shown_key(key_name), source.shown_key(key_name));
        }
        assert_eq!(other().id("k"), format!(r"hku\{SID}\k"));
    }

    #[test]
    fn malformed_ids_are_rejected() {
        for bad in [
            "",
            "hklm64",
            r"hklm64\",
            r"nope\k",
            r"hkcu\a\b",
            r"hku\k",
            r"hku\S-1-5-18\k",
            r"hku\S-1-5-21-1_Classes\k",
            &format!(r"hku\{SID}\"),
            &format!(r"hku\{SID}\a\b"),
        ] {
            assert!(Source::locate(bad).is_none(), "{bad}");
        }
    }
}
