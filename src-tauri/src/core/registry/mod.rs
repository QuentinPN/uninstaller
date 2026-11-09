// `expect` fails the build as soon as the app calls it: remove it then.
pub mod paths;
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "Phase 2: tested now, called by the next slices")
)]
pub mod reg_file;
