//! COM set-up shared by the wrappers that create COM objects.

use std::sync::Once;
use windows::Win32::System::Com::{
    COINIT_MULTITHREADED, CoInitializeEx, CoInitializeSecurity, EOAC_DYNAMIC_CLOAKING,
    RPC_C_AUTHN_LEVEL_PKT_PRIVACY, RPC_C_IMP_LEVEL_IMPERSONATE,
};

/// Joins the multithreaded apartment on this thread, and sets the process's call
/// security before its first COM object: it can be set only once, and System Restore
/// needs it to take the volume snapshot under the caller's identity. The apartment is
/// never left: the thread keeps it until it ends, and a thread already in one keeps
/// its own.
pub fn prepare() {
    static SECURITY: Once = Once::new();
    // SAFETY: plain call; joining again, or failing because the thread is in a
    // single-threaded apartment, is harmless.
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    SECURITY.call_once(|| {
        // SAFETY: plain call with no pointers; it fails harmlessly if the process
        // already set its security.
        let _ = unsafe {
            CoInitializeSecurity(
                None,
                -1,
                None,
                None,
                RPC_C_AUTHN_LEVEL_PKT_PRIVACY,
                RPC_C_IMP_LEVEL_IMPERSONATE,
                None,
                EOAC_DYNAMIC_CLOAKING,
                None,
            )
        };
    });
}
