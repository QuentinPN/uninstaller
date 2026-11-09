//! Thin safe wrappers over the Windows API. Every `unsafe` block of the crate lives here.

pub mod com;
pub mod dialog;
pub mod icon;
pub mod pipe;
pub mod process;
pub mod registry;
pub mod restore;
pub mod shortcut;
pub mod token;
