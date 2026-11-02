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
// 0 = not checked, 1 = missing resources, 2 = manifest found, 3 = error.
// Neither 1 nor 2 means gameplay has started.
static ENGINE_STATE: AtomicU32 = AtomicU32::new(0);
static PLAYER_NAME: OnceLock<CString> = OnceLock::new();

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
    ENGINE_STATE.store(0, Ordering::Release);
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

/// Game engine preflight state; 2 indicates a prepared resource manifest is
/// present, **not** that a game world has started.
#[unsafe(no_mangle)]
pub extern "C" fn horizon_ios_engine_state() -> u32 {
    ENGINE_STATE.load(Ordering::Acquire)
}

/// Stable Rust-owned UTF-8 player name, or NULL before profile preflight.
#[unsafe(no_mangle)]
pub extern "C" fn horizon_ios_player_name() -> *const c_char {
    PLAYER_NAME
        .get()
        .map_or(std::ptr::null(), |name| name.as_ptr())
}

/// Native iOS host supplies its application-support directory (UTF-8 bytes).
/// Returns 0 on successful preflight even when proprietary game assets are
/// absent; errors return -1. Does not start a second UIKit event loop.
#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn horizon_ios_prepare_engine(home: *const u8, length: usize) -> i32 {
    let operation = || -> Result<_, String> {
        if home.is_null() || length == 0 || length > 4096 {
            return Err("Invalid native application-support directory".into());
        }
        let bytes = unsafe { std::slice::from_raw_parts(home, length) };
        let path = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
        let prepared = prepare_engine_directory(
            std::path::Path::new(path),
            |directory| Ok(horizon_client::offline_name::load_or_create(directory)),
            horizon_client::resource_install::ready,
        )?;
        let player =
            CString::new(prepared.player_name).map_err(|_| "Invalid player name".to_owned())?;
        let _ = PLAYER_NAME.set(player);
        ENGINE_STATE.store(
            if prepared.resources_ready { 2 } else { 1 },
            Ordering::Release,
        );
        Ok(())
    };
    match std::panic::catch_unwind(operation) {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => {
            ENGINE_STATE.store(3, Ordering::Release);
            eprintln!("Horizon iOS engine preflight: {error}");
            -1
        }
        Err(_) => {
            ENGINE_STATE.store(3, Ordering::Release);
            eprintln!("Horizon iOS engine preflight panicked");
            -1
        }
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

mod engine_bootstrap;
pub use engine_bootstrap::{EnginePreparation, prepare_engine_directory};
