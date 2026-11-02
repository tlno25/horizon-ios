//! C ABI for the native iOS shell. The game engine is not started by this bridge.
//! UIKit/Winit must own the sole UIApplicationMain invocation.

use std::ffi::{CString, c_char};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};

/// Increment when changing the ABI signatures or meanings.
const ABI_VERSION: u32 = 1;

/// 0 = uninitialized, 1 = shell ready, 2 = suspended.
static SHELL_STATE: AtomicU32 = AtomicU32::new(0);
static GAME_VERSION: OnceLock<CString> = OnceLock::new();
// 0 = not attempted, 1 = initializing, 2 = ready, 3 = failed, 4 = suspended.
static GRAPHICS_STATE: AtomicU32 = AtomicU32::new(0);

/// Returns the ABI revision supported by this static library.
#[unsafe(no_mangle)]
pub extern "C" fn horizon_ios_abi_version() -> u32 {
    ABI_VERSION
}

/// UTF-8, NUL-terminated and owned by Rust for the lifetime of the process.
/// Callers must never free or modify the returned pointer.
#[unsafe(no_mangle)]
pub extern "C" fn horizon_ios_game_version() -> *const c_char {
    GAME_VERSION
        .get_or_init(|| CString::new(horizon_core::version::VANILLA_VERSION).unwrap_or_default())
        .as_ptr()
}

/// Initialize the shell bridge, but not the Minecraft engine. Idempotent.
/// Return value 0 means success.
#[unsafe(no_mangle)]
pub extern "C" fn horizon_ios_initialize() -> i32 {
    let _ = SHELL_STATE.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire);
    0
}

/// Current shell status: 0 = not initialized, 1 = ready, 2 = suspended.
/// This is not a gameplay-running status.
#[unsafe(no_mangle)]
pub extern "C" fn horizon_ios_state() -> u32 {
    SHELL_STATE.load(Ordering::Acquire)
}

/// Shell teardown. Does not yet stop or destroy a running game engine.
#[unsafe(no_mangle)]
pub extern "C" fn horizon_ios_shutdown() -> i32 {
    SHELL_STATE.store(0, Ordering::Release);
    0
}

#[cfg(target_os = "ios")]
fn shell_resumed() {
    SHELL_STATE.store(1, Ordering::Release);
}

#[cfg(target_os = "ios")]
fn shell_suspended() {
    SHELL_STATE.store(2, Ordering::Release);
}

/// Metal probe status, distinct from the main Minecraft engine status.
#[unsafe(no_mangle)]
pub extern "C" fn horizon_ios_graphics_state() -> u32 {
    GRAPHICS_STATE.load(Ordering::Acquire)
}

#[cfg(target_os = "ios")]
fn set_graphics_state(state: u32) {
    if GRAPHICS_STATE.swap(state, Ordering::AcqRel) != state {
        unsafe { app::graphics_changed(state) };
    }
}

// Winit owns UIApplicationMain; Swift does not declare an @main entry point.
#[cfg(target_os = "ios")]
mod app;

/// Called once from iOS main.m on the main thread. UIApplicationMain does not
/// return during normal operation. No Rust unwinding may cross this C ABI.
#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn horizon_ios_run() -> i32 {
    match std::panic::catch_unwind(app::run) {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => {
            eprintln!("Horizon iOS event loop: {error}");
            1
        }
        Err(_) => {
            eprintln!("Horizon iOS event loop panicked");
            1
        }
    }
}

mod probe_lifecycle;
pub use probe_lifecycle::ProbeLifecycle;

#[cfg(any(test, target_os = "ios"))]
mod clipboard_bridge;
