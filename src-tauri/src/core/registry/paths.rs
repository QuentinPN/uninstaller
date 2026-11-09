//! The registry locations of installed programs, defined once.

use crate::core::inventory::Scope;
use crate::win::registry::{Hive, View};

/// Every installed program has a subkey here (per hive and view).
pub const UNINSTALL: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";

/// Where a program can be registered, in reading order. HKCU is not redirected:
/// one view is enough.
pub const UNINSTALL_SOURCES: [(Scope, Hive, View); 3] = [
    (Scope::Machine64, Hive::LocalMachine, View::Native64),
    (Scope::Machine32, Hive::LocalMachine, View::Wow32),
    (Scope::User, Hive::CurrentUser, View::Native64),
];
