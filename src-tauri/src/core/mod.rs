//! Business logic, independent of Tauri. Windows access goes through `crate::win`.

pub mod icons;
pub mod inventory;
pub mod profiles;
pub mod protect;
// `expect` fails the build as soon as the app calls it: remove it then.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "Phase 2: tested now, called by the next slices")
)]
pub mod quarantine;
pub mod registry;
pub mod sizes;
