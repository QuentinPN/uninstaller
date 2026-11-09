//! Business logic, independent of Tauri. Windows access goes through `crate::win`.

// `expect` fails the build as soon as the app calls it: remove it then.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "Phase 2: tested now, called by the next slices")
)]
pub mod broker;
pub mod icons;
pub mod inventory;
// `expect` fails the build as soon as the app calls it: remove it then.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "Phase 3: tested now, called by the next slices")
)]
pub mod leftovers;
pub mod profiles;
pub mod protect;
// `expect` fails the build as soon as the app calls it: remove it then.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "Phase 2: tested now, called by the next slices")
)]
pub mod quarantine;
pub mod registry;
pub mod settings;
pub mod sizes;
pub mod store;
#[cfg(test)]
mod test_support;
