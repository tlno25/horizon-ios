//! C ABI for the native iOS shell. The game engine is not started by this bridge.
//! UIKit/Winit must own the sole UIApplicationMain invocation.

use std::ffi::{CString, c_char};
use std::sync::OnceLock;
#[cfg(target_os = "ios")]
use std::sync::atomic::AtomicBool;
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
#[cfg(target_os = "ios")]
static TOUCH_STATE: OnceLock<std::sync::Mutex<TouchInput>> = OnceLock::new();
#[cfg(target_os = "ios")]
static ENGINE_HOME: OnceLock<std::path::PathBuf> = OnceLock::new();
#[cfg(target_os = "ios")]
static RESOURCE_IMPORT: std::sync::Mutex<()> = std::sync::Mutex::new(());
#[cfg(target_os = "ios")]
static CORE_LOAD: CoreLoadGate = CoreLoadGate::new();
#[cfg(target_os = "ios")]
static CORE_ASSETS: OnceLock<horizon_client::assets::Assets> = OnceLock::new();
#[cfg(target_os = "ios")]
static PREVIEW_BIOMES: OnceLock<horizon_client::biome::Biomes> = OnceLock::new();
#[cfg(target_os = "ios")]
static PREVIEW_TERRAIN: OnceLock<horizon_client::game_startup::TerrainMeshingResources> =
    OnceLock::new();
#[cfg(target_os = "ios")]
static RENDERER_SWITCH: RendererSwitch = RendererSwitch::new();
#[cfg(target_os = "ios")]
static TERRAIN_PREVIEW: TerrainPreviewSwitch = TerrainPreviewSwitch::new();
#[cfg(target_os = "ios")]
static RENDERER_SMOKE_ENABLED: AtomicBool = AtomicBool::new(false);
/// Opt-in, read-only 20 TPS physics preview. Not actual gameplay.
#[cfg(target_os = "ios")]
static PREVIEW_PHYSICS_ENABLED: AtomicBool = AtomicBool::new(false);

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
    #[cfg(target_os = "ios")]
    RENDERER_SWITCH.reset();
    #[cfg(target_os = "ios")]
    TERRAIN_PREVIEW.reset();
    #[cfg(target_os = "ios")]
    PREVIEW_PHYSICS_ENABLED.store(false, Ordering::Release);
    #[cfg(target_os = "ios")]
    clear_touches();
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

/// Explicit CI-only opt-in for a renderer preview without private game files.
/// Swift only calls this when the simulator has an opt-in marker inside its
/// private cache folder. Does not change the normal app's default behavior.
#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn horizon_ios_enable_renderer_smoke() {
    RENDERER_SMOKE_ENABLED.store(true, Ordering::Release);
}

/// 1 if the private simulator diagnostic marker has been observed by Swift,
/// 0 otherwise. Distinguishes a renderer failure from a missing test request.
#[unsafe(no_mangle)]
pub extern "C" fn horizon_ios_renderer_smoke_enabled() -> u32 {
    #[cfg(target_os = "ios")]
    {
        return u32::from(RENDERER_SMOKE_ENABLED.load(Ordering::Acquire));
    }
    #[cfg(not(target_os = "ios"))]
    0
}

/// 0=Metal probe, 1=requested, 2=constructing the real boot renderer,
/// 3=actual Horizon renderer presented a frame, 4=renderer failed.
/// State 3 does NOT imply gameplay has started.
#[unsafe(no_mangle)]
pub extern "C" fn horizon_ios_renderer_preview_state() -> u32 {
    #[cfg(target_os = "ios")]
    {
        return RENDERER_SWITCH.state();
    }
    #[cfg(not(target_os = "ios"))]
    0
}

/// Request transfer from Metal's bootstrap probe to Horizon's *real*
/// loading renderer, reusing the existing UIKit Winit window and event loop.
/// Must be called from the UI; requests are delivered through Winit's proxy.
#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn horizon_ios_preview_game_renderer() -> i32 {
    if !app::renderer_events_available() {
        return -2;
    }
    if !RENDERER_SWITCH.request(CORE_LOAD.state() == 2, horizon_ios_graphics_state() == 2) {
        return -1;
    }
    if app::request_renderer_event() {
        0
    } else {
        RENDERER_SWITCH.reset();
        -2
    }
}

/// Native read-only terrain preview: 0=not ready, 1=meshed/ready,
/// 2=requested, 3=building shared-device renderer, 4=shown, 5=error,
/// 6=returning to title. Never a game session.
#[unsafe(no_mangle)]
pub extern "C" fn horizon_ios_terrain_preview_state() -> u32 {
    #[cfg(target_os = "ios")]
    {
        return TERRAIN_PREVIEW.state();
    }
    #[cfg(not(target_os = "ios"))]
    0
}

#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn horizon_ios_preview_selected_terrain() -> i32 {
    if !app::renderer_events_available() {
        return -2;
    }
    if !TERRAIN_PREVIEW.request() {
        return -1;
    }
    if app::request_terrain_preview_event() {
        0
    } else {
        TERRAIN_PREVIEW.ready();
        -2
    }
}

#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn horizon_ios_leave_terrain_preview() -> i32 {
    if !app::renderer_events_available() {
        return -2;
    }
    if !TERRAIN_PREVIEW.exit_requested() {
        return -1;
    }
    if app::request_terrain_exit_event() {
        0
    } else {
        let _ = TERRAIN_PREVIEW.cancel_exit();
        -2
    }
}

/// Restore the read-only viewer camera to the original saved player pose.
/// The Winit event loop owns the camera; UIKit only queues a reset request.
#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn horizon_ios_reset_terrain_preview_camera() -> i32 {
    if !app::renderer_events_available() {
        return -2;
    }
    if TERRAIN_PREVIEW.state() != 4 {
        return -1;
    }
    if app::request_terrain_camera_reset_event() {
        0
    } else {
        -2
    }
}

/// Whether the opt-in 20 TPS read-only camera physics is active (0/1).
/// Inactive on every ordinary launch and after world/surface teardown.
#[unsafe(no_mangle)]
pub extern "C" fn horizon_ios_preview_physics_enabled() -> u32 {
    #[cfg(target_os = "ios")]
    {
        return u32::from(PREVIEW_PHYSICS_ENABLED.load(Ordering::Acquire));
    }
    #[cfg(not(target_os = "ios"))]
    0
}

#[cfg(target_os = "ios")]
fn set_preview_physics_enabled(enabled: bool) {
    PREVIEW_PHYSICS_ENABLED.store(enabled, Ordering::Release);
    unsafe { app::preview_physics_changed() };
}

/// Toggle physics explicitly; the Winit thread owns its clock and player.
#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn horizon_ios_toggle_preview_physics() -> i32 {
    if TERRAIN_PREVIEW.state() != 4 {
        return -1;
    }
    if app::request_preview_physics_toggle_event() {
        0
    } else {
        -2
    }
}

/// Queue one jump impulse in the next native 20 TPS simulation tick.
#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn horizon_ios_preview_physics_jump() -> i32 {
    if TERRAIN_PREVIEW.state() != 4 || !PREVIEW_PHYSICS_ENABLED.load(Ordering::Acquire) {
        return -1;
    }
    if app::request_preview_physics_jump_event() {
        0
    } else {
        -2
    }
}

/// Number of active touches recorded by the native input bridge.
#[unsafe(no_mangle)]
pub extern "C" fn horizon_ios_touch_count() -> u32 {
    #[cfg(target_os = "ios")]
    {
        return TOUCH_STATE
            .get()
            .and_then(|input| input.lock().ok())
            .map_or(0, |input| input.active_count() as u32);
    }
    #[cfg(not(target_os = "ios"))]
    0
}

/// Number of accepted touch transitions since app launch.
#[unsafe(no_mangle)]
pub extern "C" fn horizon_ios_touch_events() -> u64 {
    #[cfg(target_os = "ios")]
    {
        return TOUCH_STATE
            .get()
            .and_then(|input| input.lock().ok())
            .map_or(0, |input| input.event_count());
    }
    #[cfg(not(target_os = "ios"))]
    0
}

#[cfg(target_os = "ios")]
fn update_touch(id: u64, phase: TouchPhase, point: (f32, f32), size: (u32, u32)) -> bool {
    let input = TOUCH_STATE.get_or_init(|| std::sync::Mutex::new(TouchInput::default()));
    let accepted = input
        .lock()
        .is_ok_and(|mut state| state.handle(id, phase, point, size));
    if accepted {
        app::notify_touch_changed();
    }
    accepted
}

#[cfg(target_os = "ios")]
fn clear_touches() {
    if let Some(input) = TOUCH_STATE.get() {
        if let Ok(mut state) = input.lock() {
            state.clear();
        }
        app::notify_touch_changed();
    }
}

/// Diagnostic SwiftUI touch pad. Phases: 0=start, 1=move, 2=end, 3=cancel.
/// The reserved ID cannot collide with Winit's ordinary touch IDs.
#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn horizon_ios_debug_touch(
    phase: u32,
    x: f32,
    y: f32,
    width: u32,
    height: u32,
) -> i32 {
    let phase = match phase {
        0 => TouchPhase::Started,
        1 => TouchPhase::Moved,
        2 => TouchPhase::Ended,
        3 => TouchPhase::Cancelled,
        _ => return -1,
    };
    i32::from(update_touch(u64::MAX, phase, (x, y), (width, height)))
}

/// Game engine preflight state; 2 indicates a prepared resource manifest is
/// present, **not** that a game world has started.
#[unsafe(no_mangle)]
pub extern "C" fn horizon_ios_engine_state() -> u32 {
    ENGINE_STATE.load(Ordering::Acquire)
}

/// 0=not started, 1=loading, 2=block registry loaded, 3=failed.
/// State 2 does not mean a game world is running.
#[unsafe(no_mangle)]
pub extern "C" fn horizon_ios_core_load_state() -> u32 {
    #[cfg(target_os = "ios")]
    {
        return CORE_LOAD.state();
    }
    #[cfg(not(target_os = "ios"))]
    0
}

/// The title preview reads from the same prepared-resource root as the core
/// registry, but never touches private game files during resource-free CI
/// smoke tests. Constructing the GPU renderer remains on its worker thread.
#[must_use]
pub fn title_resources_root(
    home: Option<&std::path::Path>,
    core_ready: bool,
    diagnostic_smoke: bool,
) -> Option<std::path::PathBuf> {
    if !core_ready || diagnostic_smoke {
        return None;
    }
    Some(
        home?
            .join(".local/horizon-client")
            .join(horizon_core::VANILLA_VERSION),
    )
}

/// Execute the first real CPU asset loading stage used by the native game.
/// May take time: call only from a background worker, never the UIKit thread.
/// One successful load is cached for the lifetime of the process; an invalid
/// resource bundle can be fixed and retried. No Winit event loop is created.
#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn horizon_ios_load_core_assets() -> i32 {
    if !CORE_LOAD.try_begin() {
        return if CORE_LOAD.state() == 2 { 0 } else { -2 };
    }
    let result = std::panic::catch_unwind(|| -> Result<_, String> {
        let home = ENGINE_HOME
            .get()
            .ok_or("Engine home has not been prepared")?;
        let _guard = RESOURCE_IMPORT.lock().map_err(|error| error.to_string())?;
        if !horizon_client::resource_install::ready(home) {
            return Err("Import prepared game resources before loading the registry".into());
        }
        let root = home
            .join(".local/horizon-client")
            .join(horizon_core::VANILLA_VERSION);
        horizon_client::game_startup::load_core_registry(&root).map_err(|error| error.to_string())
    });
    let (success, message) = match result {
        Ok(Ok(assets)) => {
            let count = assets.registry.len();
            if CORE_ASSETS.set(assets).is_err() {
                (false, "Game data has already been loaded".to_owned())
            } else {
                (true, format!("Loaded {count} block registry states"))
            }
        }
        Ok(Err(error)) => (false, error),
        Err(_) => (false, "Block registry loading panicked".to_owned()),
    };
    CORE_LOAD.finish(success);
    // Rust strings are borrowed only for the duration of this synchronous
    // callback. Swift copies the text before dispatching to UIKit's thread.
    let message: String = message.chars().take(2048).collect();
    app::report_core_load_result(&message, success);
    if success { 0 } else { -1 }
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
        let home = std::path::Path::new(path);
        if ENGINE_HOME
            .get()
            .is_some_and(|saved| saved.as_path() != home)
        {
            return Err("Engine sandbox home cannot change while the app is running".into());
        }
        let prepared = prepare_engine_directory(
            home,
            |directory| Ok(horizon_client::offline_name::load_or_create(directory)),
            horizon_client::resource_install::ready,
        )?;
        let player =
            CString::new(prepared.player_name).map_err(|_| "Invalid player name".to_owned())?;
        let _ = PLAYER_NAME.set(player);
        let _ = ENGINE_HOME.set(home.to_path_buf());
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

/// Called by Swift after a Files.app ZIP selection. This operation can
/// extract multiple gigabytes: call it from a worker, not the UIKit thread.
/// The native callback reports a copied diagnostic string on completion.
#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn horizon_ios_import_resources(path: *const u8, length: usize) -> i32 {
    let operation = || -> Result<(), String> {
        if path.is_null() || length == 0 || length > 4096 {
            return Err("Invalid ZIP archive path".into());
        }
        let raw = unsafe { std::slice::from_raw_parts(path, length) };
        let text = std::str::from_utf8(raw).map_err(|e| e.to_string())?;
        let archive = std::path::Path::new(text);
        if !archive.is_absolute() || text.as_bytes().contains(&0) {
            return Err("Resource archive path must be absolute UTF-8".into());
        }
        let home = ENGINE_HOME.get().ok_or("Engine home was not prepared")?;
        let _guard = RESOURCE_IMPORT.lock().map_err(|e| e.to_string())?;
        horizon_client::resource_install::import_archive(archive, home)
            .map_err(|e| e.to_string())?;
        if !horizon_client::resource_install::ready(home) {
            return Err("Imported resources manifest was not found".into());
        }
        ENGINE_STATE.store(2, Ordering::Release);
        Ok(())
    };
    let result = match std::panic::catch_unwind(operation) {
        Ok(value) => value,
        Err(_) => Err("Resource import panicked".into()),
    };
    match result {
        Ok(()) => {
            app::report_resource_import_result("Prepared resources imported", true);
            0
        }
        Err(message) => {
            eprintln!("Horizon iOS resource import: {message}");
            app::report_resource_import_result(&message, false);
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

mod title_refresh;
pub use title_refresh::TitleRefresh;
mod title_tap;
pub use title_tap::TitleTap;

mod probe_lifecycle;
pub use probe_lifecycle::ProbeLifecycle;

#[cfg(any(test, target_os = "ios"))]
mod clipboard_bridge;

mod engine_bootstrap;
pub use engine_bootstrap::{CoreLoadGate, EnginePreparation, prepare_engine_directory};
mod touch_input;
pub use touch_input::{TouchInput, TouchPhase};
mod renderer_switch;
pub use renderer_switch::{RendererSwitch, describe_renderer_panic, renderer_stage_is_active};
mod terrain_preview;
pub use terrain_preview::TerrainPreviewSwitch;
mod preview_camera;
pub use preview_camera::PreviewCamera;
