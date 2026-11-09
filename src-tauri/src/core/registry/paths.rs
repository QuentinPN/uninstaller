//! The registry locations of installed programs, defined once.

use crate::core::inventory::Scope;
use crate::win::registry::{Hive, View};

/// The Uninstall key under a `SOFTWARE` root (native or WOW6432Node).
macro_rules! uninstall {
    ($software:literal) => {
        concat!($software, r"\Microsoft\Windows\CurrentVersion\Uninstall")
    };
}

/// Every installed program has a subkey here (per hive and view).
pub const UNINSTALL: &str = uninstall!("SOFTWARE");

/// Where a program can be registered, in reading order. HKCU is not redirected:
/// one view is enough.
pub const UNINSTALL_SOURCES: [(Scope, Hive, View); 3] = [
    (Scope::Machine64, Hive::LocalMachine, View::Native64),
    (Scope::Machine32, Hive::LocalMachine, View::Wow32),
    (Scope::User, Hive::CurrentUser, View::Native64),
];

/// A program's key as regedit shows it (the 32-bit view is WOW6432Node).
pub fn shown_key(scope: Scope, key_name: &str) -> String {
    let parent = match scope {
        Scope::Machine64 => concat!(r"HKEY_LOCAL_MACHINE\", uninstall!("SOFTWARE")),
        Scope::Machine32 => concat!(r"HKEY_LOCAL_MACHINE\", uninstall!(r"SOFTWARE\WOW6432Node")),
        Scope::User => concat!(r"HKEY_CURRENT_USER\", uninstall!("SOFTWARE")),
    };
    format!(r"{parent}\{key_name}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shown_key_names_the_hive_and_the_32_bit_view() {
        assert_eq!(
            shown_key(Scope::Machine32, "App"),
            r"HKEY_LOCAL_MACHINE\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\App"
        );
        assert_eq!(
            shown_key(Scope::User, "App"),
            r"HKEY_CURRENT_USER\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\App"
        );
    }
}
