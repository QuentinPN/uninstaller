//! Thin safe wrappers over the Windows API. Every `unsafe` block of the crate lives here.

pub mod dialog;
pub mod icon;
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "Phase 2: tested now, called by the next slices")
)]
pub mod pipe;
pub mod process;
pub mod registry;
pub mod restore;
pub mod token;
