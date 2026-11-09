// `expect` fails the build as soon as the app calls it: remove it then.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "Phase 2: tested now, called by the next slices")
)]
pub mod export;
pub mod paths;
