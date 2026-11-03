//! Winit owns UIKit's sole UIApplicationMain. All callbacks execute on the
//! UIKit main thread; no secondary SwiftUI app event loop is created.

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::ffi::c_void;
use std::sync::{Arc, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::error::EventLoopError;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

#[path = "metal_probe.rs"]
mod metal_probe;
use crate::{ProbeLifecycle, TitleRefresh, TitleTap};
use horizon_client::{
    gfx::{Renderer, panorama::Rotation},
    graphics::GraphicsApi,
    hud::{HudAtlas, HudBatch},
    title::Title,
};
use metal_probe::MetalProbe;

#[derive(Debug)]
enum ShellEvent {
    Wake,
    RendererCompleted,
    TerrainPreviewRequested,
    TerrainPreviewCompleted,
    TerrainPreviewExitRequested,
    TerrainPreviewResetCamera,
    PreviewPhysicsToggle,
    PreviewPhysicsJump,
    PreviewSprintToggle,
    PreviewCrouchToggle,
    PreviewWalk([f32; 2]),
    WorldInspected {
        generation: u64,
        result: Result<PreparedWorldPreview, String>,
    },
}

/// Full CPU-only result delivered from the file/meshing worker. No WGPU
/// objects cross the event channel; UIKit alone uploads GPU state.
struct PreparedWorldPreview {
    world: horizon_client::game_startup::PreparedWorld,
    terrain: Option<horizon_client::game_startup::PreparedTerrain>,
    /// Shared native chunk-map snapshot for read-only collision queries.
    native_world: Option<horizon_client::game_startup::ReadOnlyWorldStage>,
}

impl std::fmt::Debug for PreparedWorldPreview {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedWorldPreview")
            .field("world", &self.world)
            .field("terrain_summary", &self.terrain.as_ref().map(|t| t.summary))
            .field("has_native_world", &self.native_world.is_some())
            .finish()
    }
}

static EVENT_PROXY: OnceLock<EventLoopProxy<ShellEvent>> = OnceLock::new();

/// Expose the existing shared renderer's startup checkpoints on iOS.
/// These messages are emitted by `gfx::startup_step`, which precedes device
/// selection, swapchain configuration and pipeline compilation. The worker
/// reports these without blocking the responsive UIKit event loop.
struct RendererStartupLogger;
static STARTUP_LOGGER: RendererStartupLogger = RendererStartupLogger;

impl log::Log for RendererStartupLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata())
            || !crate::renderer_stage_is_active(
                crate::RENDERER_SWITCH.state(),
                crate::TERRAIN_PREVIEW.state(),
            )
        {
            return;
        }
        let message = record.args().to_string();
        if message.starts_with("Horizon renderer:") {
            report_renderer_stage(&message);
        }
    }

    fn flush(&self) {}
}

fn report_renderer_stage(message: &str) {
    eprintln!("Horizon iOS renderer stage: {message}");
    unsafe { horizon_ios_renderer_stage_changed(message.as_ptr(), message.len()) };
}

pub(super) fn renderer_events_available() -> bool {
    EVENT_PROXY.get().is_some()
}

pub(super) fn request_renderer_event() -> bool {
    EVENT_PROXY
        .get()
        .is_some_and(|proxy| proxy.send_event(ShellEvent::Wake).is_ok())
}

pub(super) fn request_terrain_preview_event() -> bool {
    EVENT_PROXY.get().is_some_and(|proxy| {
        proxy
            .send_event(ShellEvent::TerrainPreviewRequested)
            .is_ok()
    })
}

pub(super) fn request_terrain_exit_event() -> bool {
    EVENT_PROXY.get().is_some_and(|proxy| {
        proxy
            .send_event(ShellEvent::TerrainPreviewExitRequested)
            .is_ok()
    })
}

pub(super) fn request_terrain_camera_reset_event() -> bool {
    EVENT_PROXY.get().is_some_and(|proxy| {
        proxy
            .send_event(ShellEvent::TerrainPreviewResetCamera)
            .is_ok()
    })
}

pub(super) fn request_preview_physics_toggle_event() -> bool {
    EVENT_PROXY
        .get()
        .is_some_and(|proxy| proxy.send_event(ShellEvent::PreviewPhysicsToggle).is_ok())
}

pub(super) fn request_preview_physics_jump_event() -> bool {
    EVENT_PROXY
        .get()
        .is_some_and(|proxy| proxy.send_event(ShellEvent::PreviewPhysicsJump).is_ok())
}

pub(super) fn request_preview_sprint_toggle_event() -> bool {
    EVENT_PROXY
        .get()
        .is_some_and(|proxy| proxy.send_event(ShellEvent::PreviewSprintToggle).is_ok())
}

pub(super) fn request_preview_crouch_toggle_event() -> bool {
    EVENT_PROXY
        .get()
        .is_some_and(|proxy| proxy.send_event(ShellEvent::PreviewCrouchToggle).is_ok())
}

pub(super) fn request_preview_walk_event(axes: [f32; 2]) -> bool {
    EVENT_PROXY
        .get()
        .is_some_and(|proxy| proxy.send_event(ShellEvent::PreviewWalk(axes)).is_ok())
}

pub(super) unsafe fn preview_physics_changed() {
    unsafe { horizon_ios_preview_physics_changed() };
}

fn notify_renderer_worker_complete() {
    if let Some(proxy) = EVENT_PROXY.get() {
        let _ = proxy.send_event(ShellEvent::RendererCompleted);
    }
}

unsafe extern "C" {
    /// Swift installs a UIHostingController as a child of Winit's controller.
    /// The parent remains owned by Winit; Swift must not release it.
    fn horizon_ios_mount_home(view_controller: *mut c_void);
    fn horizon_ios_graphics_changed(state: u32);
    /// Swift copies these temporary bytes and persists the stage synchronously.
    fn horizon_ios_renderer_stage_changed(message: *const u8, length: usize);
    /// Synchronously snapshot a renderer stage before expensive Metal setup.
    fn horizon_ios_renderer_status_changed();
    fn horizon_ios_touch_changed();
    /// Swift copies the UTF-8 bytes before this callback returns.
    fn horizon_ios_report_graphics_error(message: *const u8, length: usize);
    /// Swift copies the diagnostic before this synchronous callback returns.
    fn horizon_ios_resource_import_result(message: *const u8, length: usize, succeeded: i32);
    fn horizon_ios_core_load_result(message: *const u8, length: usize, succeeded: i32);
    /// 1=inspecting, 2=read-only validation succeeded, 3=failed.
    fn horizon_ios_world_inspection_changed(message: *const u8, length: usize, state: i32);
    /// Swift refreshes the optional Jump overlay and diagnostic state.
    fn horizon_ios_preview_physics_changed();
}

pub(super) unsafe fn graphics_changed(state: u32) {
    unsafe { horizon_ios_graphics_changed(state) };
}

pub(super) fn report_resource_import_result(message: &str, success: bool) {
    unsafe {
        horizon_ios_resource_import_result(message.as_ptr(), message.len(), i32::from(success));
    }
}

pub(super) fn report_core_load_result(message: &str, success: bool) {
    unsafe { horizon_ios_core_load_result(message.as_ptr(), message.len(), i32::from(success)) };
}

pub(super) fn notify_touch_changed() {
    unsafe { horizon_ios_touch_changed() };
}

fn report_world_inspection(message: &str, state: i32) {
    eprintln!("Horizon iOS world inspection: {message}");
    unsafe { horizon_ios_world_inspection_changed(message.as_ptr(), message.len(), state) };
}

fn report_graphics_error(message: &str) {
    eprintln!("Horizon iOS graphics: {message}");
    unsafe { horizon_ios_report_graphics_error(message.as_ptr(), message.len()) };
}

/// Title visuals come from native Horizon, not duplicated SwiftUI buttons.
/// After a world has been inspected, users can explicitly switch to a static,
/// textured GPU preview. Gameplay and saving are still disabled.
struct PreparedTitle {
    atlas: HudAtlas,
    screen: Title,
    rotation: Rotation,
}

struct PreparedRenderer {
    renderer: Renderer,
    title: Option<PreparedTitle>,
}

#[derive(Default)]
struct Shell {
    window: Option<Arc<Window>>,
    metal: Option<MetalProbe>,
    horizon: Option<Renderer>,
    title: Option<PreparedTitle>,
    title_refresh: TitleRefresh,
    title_tap: TitleTap,
    world_inspection_pending: bool,
    world_inspection_generation: u64,
    /// The saved centre column, kept for startup diagnostics. The separate
    /// read-only native World owns Arc references to up to nine saved columns.
    initial_column: Option<horizon_client::chunk::Chunk>,
    /// Native CPU meshes for up to nine saved columns, staged until the
    /// terrain renderer has the corresponding block sprite atlas.
    terrain: Option<horizon_client::game_startup::PreparedTerrain>,
    /// Native resident World, retaining the same Arc chunks as the CPU mesher.
    /// Read-only: no ticks, writes, session locks or live game simulation.
    native_world: Option<horizon_client::game_startup::ReadOnlyWorldStage>,
    /// Avoid showing an error popup for every collision touch while the
    /// selected saved world has an unsupported contextual block shape.
    preview_collision_failed: bool,
    /// Saved first-person camera, supplied only with a validated CPU mesh.
    terrain_camera: Option<crate::PreviewCamera>,
    /// Opt-in native living gravity/jump, kept separate from saved player data.
    preview_physics: Option<horizon_client::preview_physics::PreviewPhysics>,
    preview_clock: Option<horizon_client::clock::FixedStep>,
    preview_last_tick: Option<Instant>,
    preview_jump_queued: bool,
    /// Camera-relative walking input, sampled on the Winit tick thread.
    preview_walk: [f32; 2],
    /// Sprint is an explicit read-only preview setting, not a saved ability.
    preview_sprint: bool,
    /// The original title renderer retains its GPU resources while a textured
    /// terrain replacement is active. Both share one WGPU device.
    title_renderer: Option<Renderer>,
    /// Offscreen textured pipeline preparation, no window surface on worker.
    terrain_job: Option<JoinHandle<Result<Renderer, String>>>,
    /// GPU pipelines are compiled offscreen on a worker. Winit/SwiftUI must
    /// remain responsive during this work; only surface attachment is on UIKit.
    renderer_job: Option<JoinHandle<Result<PreparedRenderer, String>>>,
    lifecycle: ProbeLifecycle,
    /// CI opt-in: exercise the real Horizon GPU pipeline without distributing
    /// any proprietary game assets. This is not enabled in normal launches.
    renderer_smoke: bool,
}

impl Shell {
    fn stop_preview_physics(&mut self) {
        if self.preview_physics.take().is_some() {
            crate::set_preview_physics_enabled(false);
        }
        self.preview_clock = None;
        self.preview_last_tick = None;
        self.preview_jump_queued = false;
        self.preview_walk = [0.0, 0.0];
        self.preview_sprint = false;
        crate::set_preview_sprinting(false);
        crate::set_preview_crouching(false);
    }

    fn toggle_preview_physics(&mut self) {
        if self.preview_physics.is_some() {
            self.stop_preview_physics();
        } else if self.title_renderer.is_some()
            && self.native_world.is_some()
            && self.terrain_camera.is_some()
            && crate::TERRAIN_PREVIEW.state() == 4
        {
            let position = self.terrain_camera.as_ref().expect("checked").position();
            self.preview_physics = Some(horizon_client::preview_physics::PreviewPhysics::new(
                position,
            ));
            self.preview_clock = Some(horizon_client::clock::FixedStep::default());
            self.preview_last_tick = Some(Instant::now());
            crate::set_preview_physics_enabled(true);
        }
    }

    /// Drive only the temporary camera body from Horizon's own 20 TPS clock.
    /// Never advance the World's block or entity simulation, and never replay
    /// suspended time. A missing/unmeshed target fails closed.
    fn tick_preview_physics(&mut self, now: Instant) {
        let Some(last) = self.preview_last_tick.replace(now) else {
            return;
        };
        let Some(clock) = self.preview_clock.as_mut() else {
            return;
        };
        let elapsed = now
            .saturating_duration_since(last)
            .min(Duration::from_millis(500));
        let ticks = clock.advance(elapsed);
        let mut changed = false;
        let mut error = None;
        for _ in 0..ticks {
            let outcome = (|| -> Result<bool, String> {
                let motion = self
                    .preview_physics
                    .as_mut()
                    .ok_or("Preview physics is not active")?;
                let stage = self
                    .native_world
                    .as_ref()
                    .ok_or("Native world was released")?;
                let resources = crate::PREVIEW_TERRAIN
                    .get()
                    .ok_or("Collision table was released")?;
                let jump = std::mem::take(&mut self.preview_jump_queued);
                let yaw = self
                    .terrain_camera
                    .as_ref()
                    .ok_or("Preview camera disappeared")?
                    .rotation()[0];
                motion
                    .tick_with_controls(
                        stage,
                        &resources.collisions,
                        jump,
                        self.preview_walk,
                        yaw,
                        Some(&resources.movement),
                        self.preview_sprint,
                    )
                    .map_err(|e| e.to_string())?;
                let camera = self
                    .terrain_camera
                    .as_mut()
                    .ok_or("Preview camera disappeared")?;
                if (motion.position()[1] - camera.saved_position()[1]).abs() > 16.0 {
                    return Err("Preview motion reached the vertical safety boundary".to_owned());
                }
                let previous_view = camera.position();
                camera.set_simulated_position(motion.position());
                let changed = camera.position() != previous_view;
                let blocked_at_edge = camera.position() != motion.position();
                if blocked_at_edge {
                    // Reject momentum that would carry the camera into a
                    // missing or unmeshed chunk. Continue rather than abort.
                    motion.sync_camera(camera.position());
                }
                Ok(changed)
            })();
            match outcome {
                Ok(moved) => changed |= moved,
                Err(message) => {
                    error = Some(message);
                    break;
                }
            }
        }
        if let Some(message) = error {
            self.stop_preview_physics();
            report_graphics_error(&format!("Physics preview stopped: {message}"));
        }
        if changed {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }

    /// Request the desktop loader's read-only world preparation stage.
    /// A native World with saved chunks is constructed for collision queries,
    /// but no simulation loop, session lock or save writer is started.
    /// Never block UIKit on file access.
    fn inspect_selected_world(&mut self, folder: String) {
        if self.world_inspection_pending {
            return;
        }
        let Some(home) = crate::ENGINE_HOME.get() else {
            report_world_inspection("Game home is unavailable", 3);
            return;
        };
        let Some(proxy) = EVENT_PROXY.get() else {
            report_world_inspection("Winit event loop is unavailable", 3);
            return;
        };
        self.world_inspection_pending = true;
        self.stop_preview_physics();
        self.initial_column = None;
        self.terrain = None;
        self.native_world = None;
        self.preview_collision_failed = false;
        self.terrain_camera = None;
        crate::TERRAIN_PREVIEW.reset();
        self.world_inspection_generation = self.world_inspection_generation.wrapping_add(1);
        let generation = self.world_inspection_generation;
        let saves = home.join("saves");
        let prepared_resources = home
            .join(".local/horizon-client")
            .join(horizon_core::VANILLA_VERSION);
        report_world_inspection("Inspecting selected world (read-only)...", 1);
        let worker = std::thread::Builder::new()
            .name("Horizon iOS world inspection".to_owned())
            .spawn(move || {
                let result = std::panic::catch_unwind(|| -> Result<_, String> {
                    // Probe save compatibility first. Only construct the
                    // resource-backed resident decoder for a saved overworld
                    // chunk and an already loaded core registry.
                    let basic =
                        horizon_client::game_startup::inspect_existing_world(&saves, &folder)
                            .map_err(|error| error.to_string())?;
                    let native_ready = matches!(
                        basic.initial_chunk_disk,
                        Some(horizon_client::game_startup::InitialChunkDiskState::Saved { .. })
                    );
                    let Some(core) = crate::CORE_ASSETS.get().filter(|_| native_ready) else {
                        return Ok(PreparedWorldPreview {
                            world: horizon_client::game_startup::PreparedWorld {
                                inspection: basic,
                                initial_column: None,
                            },
                            terrain: None,
                            native_world: None,
                        });
                    };
                    let biomes = if let Some(biomes) = crate::PREVIEW_BIOMES.get() {
                        biomes
                    } else {
                        let loaded = horizon_client::biome::Biomes::load(&prepared_resources)
                            .map_err(|error| error.to_string())?;
                        let _ = crate::PREVIEW_BIOMES.set(loaded);
                        crate::PREVIEW_BIOMES
                            .get()
                            .ok_or("Could not cache prepared biome tables")?
                    };
                    let world = horizon_client::game_startup::prepare_existing_world_with_registry(
                        &saves,
                        &folder,
                        core.registry,
                        biomes,
                    )
                    .map_err(|error| error.to_string())?;
                    let (terrain, native_world) = if let Some(column) = &world.initial_column {
                        // Full block models and sprite atlas are loaded only on
                        // demand, never during the title's initial GPU boot.
                        let assets = if let Some(assets) = crate::PREVIEW_TERRAIN.get() {
                            assets
                        } else {
                            let loaded =
                                horizon_client::game_startup::TerrainMeshingResources::load(
                                    &prepared_resources,
                                    core,
                                )
                                .map_err(|error| error.to_string())?;
                            let _ = crate::PREVIEW_TERRAIN.set(loaded);
                            crate::PREVIEW_TERRAIN
                                .get()
                                .ok_or("Cannot cache terrain assets")?
                        };
                        let region = horizon_client::game_startup::load_saved_initial_region(
                            &saves.join(&folder),
                            column,
                            core.registry,
                            biomes,
                        )
                        .map_err(|error| error.to_string())?;
                        let air = core
                            .registry
                            .default_state("minecraft:air")
                            .ok_or("No air state in prepared native block registry")?;
                        let staged = horizon_client::game_startup::ReadOnlyWorldStage::from_region(
                            &region,
                            world.inspection.seed,
                            air,
                        )
                        .map_err(|error| error.to_string())?;
                        let meshes = horizon_client::game_startup::mesh_prepared_column(
                            &region,
                            world.inspection.seed,
                            core,
                            biomes,
                            assets,
                        )
                        .map_err(|error| error.to_string())?;
                        (Some(meshes), Some(staged))
                    } else {
                        (None, None)
                    };
                    Ok(PreparedWorldPreview {
                        world,
                        terrain,
                        native_world,
                    })
                })
                .unwrap_or_else(|payload| Err(crate::describe_renderer_panic(payload.as_ref())));
                let _ = proxy.send_event(ShellEvent::WorldInspected { generation, result });
            });
        if let Err(error) = worker {
            self.world_inspection_pending = false;
            report_world_inspection(&format!("Cannot start inspection: {error}"), 3);
        }
    }

    fn mount_home(&self) -> Result<(), String> {
        let window = self.window.as_ref().ok_or("Winit window not created")?;
        let handle = window.window_handle().map_err(|error| error.to_string())?;
        let RawWindowHandle::UiKit(handle) = handle.as_raw() else {
            return Err("Winit did not return a UIKit view".to_owned());
        };
        let parent = handle
            .ui_view_controller
            .ok_or("Winit controller unavailable")?;
        // Metal inserts a CAMetalLayer on the Winit view. Mount SwiftUI last
        // so UIKit's overlay is never hidden behind that layer.
        unsafe { horizon_ios_mount_home(parent.as_ptr()) };
        Ok(())
    }

    fn start_metal(&mut self) {
        let Some(window) = &self.window else { return };
        if self.metal.is_some()
            || self.horizon.is_some()
            || self.lifecycle.drawable_size().is_none()
        {
            return;
        }
        crate::set_graphics_state(1);
        match MetalProbe::new(window.clone()) {
            Ok(probe) => {
                self.metal = Some(probe);
                // Device creation is not proof that a Metal frame was
                // presented. Keep state 1 until the next redraw succeeds.
                window.request_redraw();
            }
            Err(error) => {
                report_graphics_error(&format!("Metal initialization: {error}"));
                crate::set_graphics_state(3);
                // Do not exit UIKit; the SwiftUI shell should remain usable.
            }
        }
    }
    /// Compile Horizon's substantial GPU pipeline set offscreen on a worker.
    /// Keep presenting the working Metal probe until preparation completes.
    /// UIKit alone owns the live surface and connects it after the worker wakes
    /// the existing Winit event loop. No second event loop is ever created.
    fn try_horizon_renderer(&mut self) {
        if self.renderer_job.is_some() || !crate::RENDERER_SWITCH.start() {
            return;
        }
        report_renderer_stage("Horizon renderer: background pipeline preparation requested");
        unsafe { horizon_ios_renderer_status_changed() };
        let Some(window) = self.window.as_ref().cloned() else {
            self.fail_horizon_renderer("Horizon window was not created");
            return;
        };
        let title_root = crate::title_resources_root(
            crate::ENGINE_HOME.get().map(|home| home.as_path()),
            crate::CORE_LOAD.state() == 2,
            self.renderer_smoke || crate::horizon_ios_renderer_smoke_enabled() != 0,
        );
        let saves_dir = crate::ENGINE_HOME.get().map(|home| home.join("saves"));
        let job = std::thread::Builder::new()
            .name("Horizon iOS GPU preparation".to_owned())
            .spawn(move || {
                // A complete boot renderer contains WGPU objects only. The
                // window surface is neither created nor touched on this thread.
                // Catch Rust panics so the event loop receives a failure, too.
                let loaded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    // Once the shared block registry is loaded, draw with
                    // Minecraft's prepared UI atlas and title panorama.
                    // The resource-free Simulator smoke remains unchanged.
                    let prepared = if let Some(root) = title_root.as_deref() {
                        report_renderer_stage("Horizon renderer: loading prepared title textures");
                        let atlas = HudAtlas::build(root).map_err(|error| error.to_string())?;
                        let saves = saves_dir.as_deref().ok_or("Missing iOS game home")?;
                        Some((
                            PreparedTitle {
                                atlas,
                                screen: Title::new(saves.to_path_buf(), root),
                                rotation: Rotation::default(),
                            },
                            horizon_client::gfx::panorama::Faces::load(root),
                        ))
                    } else {
                        None
                    };
                    let boot = prepared.is_none().then(HudAtlas::boot);
                    let (atlas, panorama) = match prepared.as_ref() {
                        Some((title, panorama)) => (&title.atlas, Some(panorama)),
                        None => (boot.as_ref().expect("boot atlas built"), None),
                    };
                    report_renderer_stage("Horizon renderer: compiling GPU pipelines offscreen");
                    let renderer = horizon_client::game_startup::loading_renderer(
                        window,
                        true,
                        Some(atlas),
                        panorama,
                        None,
                        GraphicsApi::Auto,
                    )
                    .map_err(|error| error.to_string())?;
                    Ok(PreparedRenderer {
                        renderer,
                        title: prepared.map(|(title, _)| title),
                    })
                }))
                .unwrap_or_else(|payload| Err(crate::describe_renderer_panic(payload.as_ref())));
                // Wake the *one* UIKit/Winit event loop, never create another.
                notify_renderer_worker_complete();
                loaded
            });
        match job {
            Ok(job) => self.renderer_job = Some(job),
            Err(error) => self.fail_horizon_renderer(&format!("Cannot start GPU worker: {error}")),
        }
    }

    fn fail_horizon_renderer(&mut self, message: &str) {
        // Do not overwrite the last actual GPU stage: it tells us where the
        // worker panicked, and remains in the Simulator's diagnostic JSON.
        report_graphics_error(message);
        crate::RENDERER_SWITCH.finished(false);
        unsafe { horizon_ios_renderer_status_changed() };
        self.start_metal();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
        unsafe { graphics_changed(crate::horizon_ios_graphics_state()) };
    }

    /// This runs only on the Winit thread, when the GPU worker wakes it.
    fn complete_horizon_renderer(&mut self) {
        // The RendererCompleted event is sent immediately before the worker
        // exits. `is_finished` could still be false at event delivery; skipping
        // it here would lose the only completion notification forever.
        let job = self.renderer_job.take().expect("checked worker exists");
        let loaded = job
            .join()
            .unwrap_or_else(|payload| Err(crate::describe_renderer_panic(payload.as_ref())));
        // A backgrounded or restarted UIKit drawable must never receive the
        // old worker's surface, even if that worker finished successfully.
        if self.lifecycle.drawable_size().is_none() || crate::RENDERER_SWITCH.state() != 2 {
            report_renderer_stage("Horizon renderer: discarding stale GPU preparation");
            crate::RENDERER_SWITCH.reset();
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            return;
        }
        let result = loaded.and_then(|mut prepared| {
            report_renderer_stage("Horizon renderer: attaching surface on UIKit thread");
            self.metal = None;
            crate::set_graphics_state(1);
            prepared
                .renderer
                .attach_prepared_surface()
                .map_err(|error| error.to_string())?;
            Ok(prepared)
        });
        match result {
            Ok(prepared) => {
                report_renderer_stage("Horizon renderer: GPU initialization complete");
                self.title = prepared.title;
                self.horizon = Some(prepared.renderer);
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            Err(error) => self.fail_horizon_renderer(&format!("Horizon boot renderer: {error}")),
        }
    }

    /// Opt-in static terrain view. Keep drawing the title while the shared
    /// WGPU device compiles a second textured pipeline on a background thread.
    fn try_terrain_preview(&mut self) {
        if crate::TERRAIN_PREVIEW.state() != 2 {
            return;
        }
        if self.terrain_job.is_some()
            || self.title_renderer.is_some()
            || self.horizon.is_none()
            || self.title.is_none()
            || self.terrain.is_none()
            || self.initial_column.is_none()
            || self.terrain_camera.is_none()
            || self.lifecycle.drawable_size().is_none()
        {
            self.fail_terrain_preview("Native terrain preview is unavailable or still preparing");
            return;
        }
        let Some(window) = self.window.as_ref().cloned() else {
            self.fail_terrain_preview("Cannot start terrain without the iOS window");
            return;
        };
        let context = self
            .horizon
            .as_ref()
            .expect("checked title renderer")
            .background_context();
        report_renderer_stage(
            "Horizon renderer: preparing native terrain sprite pipelines on shared GPU",
        );
        if !crate::TERRAIN_PREVIEW.start() {
            return;
        }
        if let Some(camera) = self.terrain_camera.as_mut() {
            camera.clear();
        }
        unsafe { horizon_ios_renderer_status_changed() };
        let job = std::thread::Builder::new()
            .name("Horizon iOS terrain GPU preparation".to_owned())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let resources = crate::PREVIEW_TERRAIN
                        .get()
                        .ok_or("Terrain assets not loaded")?;
                    horizon_client::game_startup::loading_terrain_renderer_with_context(
                        window,
                        &resources.render,
                        context,
                    )
                    .map_err(|error| error.to_string())
                }))
                .unwrap_or_else(|payload| Err(crate::describe_renderer_panic(payload.as_ref())));
                if let Some(proxy) = EVENT_PROXY.get() {
                    let _ = proxy.send_event(ShellEvent::TerrainPreviewCompleted);
                }
                result
            });
        match job {
            Ok(job) => self.terrain_job = Some(job),
            Err(error) => {
                self.fail_terrain_preview(&format!("Cannot start terrain GPU worker: {error}"))
            }
        }
    }

    fn fail_terrain_preview(&mut self, message: &str) {
        report_graphics_error(message);
        crate::TERRAIN_PREVIEW.failed();
        unsafe { horizon_ios_renderer_status_changed() };
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    /// Transfer the one live surface to the textured renderer on UIKit's
    /// event thread, then upload the previously prepared CPU section meshes.
    fn complete_terrain_preview(&mut self) {
        let Some(job) = self.terrain_job.take() else {
            return;
        };
        let loaded = job
            .join()
            .unwrap_or_else(|payload| Err(crate::describe_renderer_panic(payload.as_ref())));
        // Suspended / replaced worlds cannot accept a stale GPU result.
        if crate::TERRAIN_PREVIEW.state() != 3
            || self.lifecycle.drawable_size().is_none()
            || self.terrain.is_none()
            || self.terrain_camera.is_none()
        {
            return;
        }
        let mut prepared = match loaded {
            Ok(prepared) => prepared,
            Err(error) => {
                self.fail_terrain_preview(&format!("Native terrain GPU preparation: {error}"));
                return;
            }
        };
        let Some(mut old_title) = self.horizon.take() else {
            self.fail_terrain_preview("Native title renderer disappeared during preparation");
            return;
        };
        report_renderer_stage("Horizon renderer: transferring surface and uploading saved terrain");
        let upload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            prepared
                .activate_replacement(&mut old_title)
                .map_err(|error| error.to_string())?;
            let terrain = self.terrain.as_ref().ok_or("Saved mesh was released")?;
            horizon_client::game_startup::upload_prepared_terrain(&mut prepared, terrain);
            Ok::<(), String>(())
        }))
        .unwrap_or_else(|payload| Err(crate::describe_renderer_panic(payload.as_ref())));
        match upload {
            Ok(()) => {
                report_renderer_stage(
                    "Horizon renderer: native terrain uploaded, awaiting first frame",
                );
                self.title_renderer = Some(old_title);
                self.horizon = Some(prepared);
                self.title_tap.clear();
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            Err(error) => {
                // Drop its surface *before* restoring the original title.
                drop(prepared);
                match old_title.recreate_surface() {
                    Ok(()) => self.horizon = Some(old_title),
                    Err(restore) => {
                        report_graphics_error(&format!("Cannot restore title surface: {restore}"));
                        self.title = None;
                        crate::RENDERER_SWITCH.finished(false);
                        self.start_metal();
                    }
                }
                self.fail_terrain_preview(&format!("Native terrain GPU upload: {error}"));
            }
        }
    }

    /// Return to the still-prepared title renderer. Never reinterpret a
    /// preview as a joined or running world; the save remains untouched.
    fn restore_title_renderer(&mut self) {
        if crate::TERRAIN_PREVIEW.state() != 6 {
            return;
        }
        self.finish_terrain_preview(true, None);
    }

    fn finish_terrain_preview(&mut self, requested_exit: bool, error: Option<&str>) {
        self.stop_preview_physics();
        let Some(mut title_renderer) = self.title_renderer.take() else {
            self.fail_terrain_preview("Title renderer was not retained");
            return;
        };
        if let Some(camera) = self.terrain_camera.as_mut() {
            camera.clear();
        }
        // The terrain renderer owns the current surface. Drop it first.
        self.horizon = None;
        match title_renderer.recreate_surface() {
            Ok(()) => {
                self.horizon = Some(title_renderer);
                if requested_exit {
                    crate::TERRAIN_PREVIEW.exit_finished(true);
                } else {
                    crate::TERRAIN_PREVIEW.failed();
                }
                if let Some(error) = error {
                    report_graphics_error(error);
                }
                unsafe { horizon_ios_renderer_status_changed() };
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            Err(restore) => {
                self.title = None;
                crate::RENDERER_SWITCH.finished(false);
                self.fail_terrain_preview(&format!("Title surface recovery failed: {restore}"));
                self.start_metal();
            }
        }
    }

    fn draw_horizon(&mut self) {
        let Some(renderer) = self.horizon.as_mut() else {
            return;
        };
        let mut batch = HudBatch::new([renderer.config.width, renderer.config.height]);
        let terrain_active = self.title_renderer.is_some();
        let result: Result<bool, String> = if terrain_active {
            // A static view of the saved camera, not a running world.
            let Some(camera) = self.terrain_camera.as_ref() else {
                self.finish_terrain_preview(false, Some("Saved preview camera disappeared"));
                return;
            };
            let frame = horizon_client::game_startup::terrain_preview_frame_with_pose(
                &batch,
                camera.position(),
                camera.rotation(),
                self.preview_physics
                    .as_ref()
                    .is_some_and(|body| body.crouching()),
            );
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| renderer.render(&frame)))
                .map_err(|panic| crate::describe_renderer_panic(panic.as_ref()))
                .and_then(|result| result.map_err(|error| error.to_string()))
        } else if let Some(title) = self.title.as_mut() {
            let elapsed = title.screen.started.elapsed().as_secs_f32();
            let frame = horizon_client::game_startup::loading_title_frame(
                &mut batch,
                &title.screen,
                &title.atlas,
                &mut title.rotation,
                elapsed,
            );
            renderer.render(&frame).map_err(|error| error.to_string())
        } else {
            // Resource-free Simulator probe retains its exact old behavior.
            renderer
                .render(&horizon_client::game_startup::loading_frame(&batch))
                .map_err(|error| error.to_string())
        };
        match result {
            Ok(true) => {
                if self.lifecycle.present_frame() {
                    if terrain_active && crate::TERRAIN_PREVIEW.state() == 3 {
                        if crate::TERRAIN_PREVIEW.presented() {
                            unsafe { horizon_ios_renderer_status_changed() };
                        }
                    }
                    if crate::RENDERER_SWITCH.state() != 3 {
                        crate::RENDERER_SWITCH.finished(true);
                        unsafe { horizon_ios_renderer_status_changed() };
                    }
                    crate::set_graphics_state(2);
                }
            }
            Ok(false) => {
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            Err(error) => {
                if terrain_active {
                    self.finish_terrain_preview(
                        false,
                        Some(&format!("Horizon terrain preview frame: {error}")),
                    );
                    return;
                }
                report_graphics_error(&format!("Horizon first frame: {error}"));
                self.horizon = None;
                self.title = None;
                self.terrain_camera = None;
                crate::TERRAIN_PREVIEW.reset();
                crate::RENDERER_SWITCH.finished(false);
                unsafe { horizon_ios_renderer_status_changed() };
                self.start_metal();
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
                unsafe { graphics_changed(crate::horizon_ios_graphics_state()) };
            }
        }
    }
}

impl ApplicationHandler<ShellEvent> for Shell {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        crate::shell_resumed();
        self.lifecycle.resume();
        if let Some(window) = &self.window {
            let size = window.inner_size();
            self.lifecycle.resize(size.width, size.height);
            self.start_metal();
            if let Err(error) = self.mount_home() {
                eprintln!("Cannot reattach the SwiftUI host: {error}");
            }
            return;
        }
        let window = match event_loop.create_window(Window::default_attributes()) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                eprintln!("Cannot create the iOS window: {error}");
                event_loop.exit();
                return;
            }
        };
        let size = window.inner_size();
        self.lifecycle.resize(size.width, size.height);
        self.window = Some(window);
        self.start_metal();
        if let Err(error) = self.mount_home() {
            eprintln!("Cannot mount SwiftUI in Winit's UIKit window: {error}");
            event_loop.exit();
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.stop_preview_physics();
        self.lifecycle.suspend();
        crate::clear_touches();
        // Drop the surface and all GPU objects before UIKit enters background.
        self.metal = None;
        self.horizon = None;
        self.title = None;
        self.title_tap.clear();
        self.world_inspection_pending = false;
        self.initial_column = None;
        self.terrain = None;
        self.native_world = None;
        self.preview_collision_failed = false;
        self.terrain_camera = None;
        self.title_renderer = None;
        crate::TERRAIN_PREVIEW.reset();
        self.world_inspection_generation = self.world_inspection_generation.wrapping_add(1);
        crate::RENDERER_SWITCH.reset();
        crate::set_graphics_state(4);
        crate::shell_suspended();
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // The opt-in body is driven at a fixed 20 TPS without spinning the
        // display at full speed. UIKit background/suspension drops this clock.
        let now = Instant::now();
        if self.preview_physics.is_some() && crate::TERRAIN_PREVIEW.state() == 4 {
            self.tick_preview_physics(now);
        }
        // Title splash/panorama need animation, but an unconditional redraw
        // in about_to_wait would spin the UIKit event loop at full speed.
        let title_visible = self.title.is_some()
            && self.horizon.is_some()
            && self.title_renderer.is_none()
            && self.lifecycle.drawable_size().is_some();
        if self.title_refresh.due(now, title_visible) {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
        let deadline = self.title_refresh.next_deadline();
        let deadline = if self.preview_physics.is_some() && crate::TERRAIN_PREVIEW.state() == 4 {
            Some(deadline.map_or(now + Duration::from_millis(25), |next| {
                next.min(now + Duration::from_millis(25))
            }))
        } else {
            deadline
        };
        event_loop.set_control_flow(match deadline {
            Some(next) => ControlFlow::WaitUntil(next),
            None => ControlFlow::Wait,
        });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(window) = self.window.as_ref().cloned() else {
            return;
        };
        if window.id() != id {
            return;
        }
        match event {
            WindowEvent::Resized(size) => {
                let previously_presented = self.lifecycle.has_presented_frame();
                self.lifecycle.resize(size.width, size.height);
                if let Some(camera) = self.terrain_camera.as_mut() {
                    camera.clear();
                }
                if self.lifecycle.drawable_size().is_none() {
                    crate::clear_touches();
                    self.title_tap.clear();
                }
                if previously_presented && !self.lifecycle.has_presented_frame() {
                    crate::set_graphics_state(1);
                }
                if let Some(renderer) = &mut self.horizon {
                    if self.lifecycle.drawable_size().is_some() {
                        renderer.resize(size.width, size.height);
                        window.request_redraw();
                    }
                } else if let Some(metal) = &mut self.metal {
                    if self.lifecycle.drawable_size().is_some() {
                        metal.resize(size.width, size.height);
                        window.request_redraw();
                    }
                } else if self.lifecycle.drawable_size().is_some()
                    && crate::horizon_ios_graphics_state() != 3
                {
                    self.start_metal();
                }
            }
            WindowEvent::RedrawRequested if self.lifecycle.drawable_size().is_some() => {
                // A CLI-only Simulator diagnostic: after a successful Metal
                // probe frame, try the same renderer the native game uses for
                // its boot screen. No Minecraft files or game world required.
                if (self.renderer_smoke || crate::horizon_ios_renderer_smoke_enabled() != 0)
                    && crate::RENDERER_SWITCH.state() == 0
                    && crate::horizon_ios_graphics_state() == 2
                {
                    let _ = crate::RENDERER_SWITCH.request(true, true);
                }
                self.try_horizon_renderer();
                if self.horizon.is_some() {
                    self.draw_horizon();
                } else if let Some(metal) = &self.metal {
                    let mut handoff_after_present = false;
                    match metal.draw() {
                        Ok(true) => {
                            if self.lifecycle.present_frame() {
                                crate::set_graphics_state(2);
                                if (self.renderer_smoke
                                    || crate::horizon_ios_renderer_smoke_enabled() != 0)
                                    && crate::RENDERER_SWITCH.state() == 0
                                {
                                    // On iOS a redraw requested from inside
                                    // RedrawRequested can be coalesced. Start
                                    // the handoff in this same event instead
                                    // of assuming a second event will arrive.
                                    handoff_after_present = crate::RENDERER_SWITCH
                                        .request_smoke_after_probe_frame(true, true);
                                }
                            }
                        }
                        Ok(false) => window.request_redraw(),
                        Err(error) => {
                            report_graphics_error(&format!("Metal presentation: {error}"));
                            crate::set_graphics_state(3);
                            self.metal = None;
                        }
                    }
                    if handoff_after_present {
                        self.try_horizon_renderer();
                    }
                }
            }
            WindowEvent::Touch(touch) => {
                if let Some(size) = self.lifecycle.drawable_size() {
                    use winit::event::TouchPhase as WinitPhase;
                    let phase = match touch.phase {
                        WinitPhase::Started => crate::TouchPhase::Started,
                        WinitPhase::Moved => crate::TouchPhase::Moved,
                        WinitPhase::Ended => crate::TouchPhase::Ended,
                        WinitPhase::Cancelled => crate::TouchPhase::Cancelled,
                    };
                    let point = (touch.location.x as f32, touch.location.y as f32);
                    if crate::update_touch(touch.id, phase, point, size) {
                        if self.title_renderer.is_some() {
                            // Read-only native World collision queries bind
                            // to the exact saved chunks used by the mesher.
                            // Only the transient view camera may move.
                            if let Some(camera) = self.terrain_camera.as_mut() {
                                let from = camera.position();
                                if camera.touch(touch.id, phase, point, size) {
                                    let target = camera.position();
                                    if target != from {
                                        let outcome = self
                                            .native_world
                                            .as_ref()
                                            .zip(crate::PREVIEW_TERRAIN.get())
                                            .ok_or("Native preview collision world is not ready")
                                            .and_then(|(stage, resources)| {
                                                stage
                                                    .resolve_preview_movement_with_pose(
                                                        &resources.collisions,
                                                        from,
                                                        target,
                                                        0.0,
                                                        false,
                                                        self.preview_physics
                                                            .as_ref()
                                                            .is_some_and(|body| body.crouching()),
                                                    )
                                                    .map_err(|_| "Native preview collision failed")
                                            });
                                        match outcome {
                                            Ok(resolved) => {
                                                if !camera.set_collision_resolved_position(resolved)
                                                {
                                                    let _ = camera
                                                        .set_collision_resolved_position(from);
                                                }
                                            }
                                            Err(error) => {
                                                let _ =
                                                    camera.set_collision_resolved_position(from);
                                                if !self.preview_collision_failed {
                                                    self.preview_collision_failed = true;
                                                    report_graphics_error(error);
                                                }
                                            }
                                        }
                                    }
                                    if let Some(motion) = self.preview_physics.as_mut() {
                                        if let Some(camera) = &self.terrain_camera {
                                            if target != from {
                                                motion.sync_camera(camera.position());
                                            }
                                        }
                                    }
                                    window.request_redraw();
                                }
                            }
                        } else if self.title.is_some()
                            && self.horizon.is_some()
                            && self.title_renderer.is_none()
                            && crate::TERRAIN_PREVIEW.state() != 3
                        {
                            let tap = self.title_tap.handle(touch.id, phase, point, size);
                            let swipe = self.title_tap.take_scroll();
                            if tap.is_some() || swipe.is_some() {
                                let renderer =
                                    self.horizon.as_ref().expect("title renderer exists");
                                let batch =
                                    HudBatch::new([renderer.config.width, renderer.config.height]);
                                let view = (batch.width, batch.height);
                                if let Some(release) = tap {
                                    let pointer =
                                        (release.0 / batch.scale, release.1 / batch.scale);
                                    let request = self.title.as_ref().and_then(|title| {
                                        title
                                            .screen
                                            .preview_inspect_request(pointer.0, pointer.1, view)
                                    });
                                    if let Some(folder) = request {
                                        self.inspect_selected_world(folder);
                                    } else if self.title.as_mut().is_some_and(|title| {
                                        title.screen.preview_tap(pointer.0, pointer.1, view)
                                    }) {
                                        // Leaving the selection invalidates any
                                        // file/meshing worker still in flight.
                                        self.world_inspection_generation =
                                            self.world_inspection_generation.wrapping_add(1);
                                        self.world_inspection_pending = false;
                                        self.stop_preview_physics();
                                        self.initial_column = None;
                                        self.terrain = None;
                                        self.native_world = None;
                                        self.preview_collision_failed = false;
                                        self.terrain_camera = None;
                                        crate::TERRAIN_PREVIEW.reset();
                                        unsafe { horizon_ios_renderer_status_changed() };
                                        window.request_redraw();
                                    }
                                } else if let Some((dy, start)) = swipe {
                                    if self.title.as_mut().is_some_and(|title| {
                                        title.screen.preview_scroll(
                                            dy / batch.scale,
                                            (start.0 / batch.scale, start.1 / batch.scale),
                                            view,
                                        )
                                    }) {
                                        window.request_redraw();
                                    }
                                }
                            }
                        } else {
                            self.title_tap.clear();
                        }
                    }
                }
            }
            WindowEvent::CloseRequested => event_loop.exit(),
            _ => {}
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: ShellEvent) {
        match event {
            ShellEvent::RendererCompleted => self.complete_horizon_renderer(),
            ShellEvent::TerrainPreviewRequested => self.try_terrain_preview(),
            ShellEvent::TerrainPreviewCompleted => self.complete_terrain_preview(),
            ShellEvent::TerrainPreviewExitRequested => self.restore_title_renderer(),
            ShellEvent::PreviewPhysicsToggle => self.toggle_preview_physics(),
            ShellEvent::PreviewPhysicsJump => {
                if self.preview_physics.is_some() && crate::TERRAIN_PREVIEW.state() == 4 {
                    self.preview_jump_queued = true;
                }
            }
            ShellEvent::PreviewWalk(axes) => {
                if self.preview_physics.is_some() && crate::TERRAIN_PREVIEW.state() == 4 {
                    self.preview_walk = axes;
                }
            }
            ShellEvent::PreviewSprintToggle => {
                if self.preview_physics.is_some() && crate::TERRAIN_PREVIEW.state() == 4 {
                    // A crouched viewer may not sprint.
                    if self
                        .preview_physics
                        .as_ref()
                        .is_some_and(|body| body.crouching())
                    {
                        return;
                    }
                    self.preview_sprint = !self.preview_sprint;
                    crate::set_preview_sprinting(self.preview_sprint);
                }
            }
            ShellEvent::PreviewCrouchToggle => {
                if crate::TERRAIN_PREVIEW.state() == 4
                    && let (Some(body), Some(stage), Some(resources)) = (
                        self.preview_physics.as_mut(),
                        self.native_world.as_ref(),
                        crate::PREVIEW_TERRAIN.get(),
                    )
                {
                    let enable_crouch = !body.crouching();
                    match body.set_crouching(enable_crouch, stage, &resources.collisions) {
                        Ok(true) => {
                            let crouching = body.crouching();
                            crate::set_preview_crouching(crouching);
                            if crouching {
                                self.preview_sprint = false;
                                crate::set_preview_sprinting(false);
                            }
                            if let Some(window) = &self.window {
                                window.request_redraw();
                            }
                        }
                        Ok(false) => report_graphics_error(
                            "Cannot stand: a saved block obstructs the standing player body",
                        ),
                        Err(error) => report_graphics_error(&format!(
                            "Cannot change read-only crouch: {error}"
                        )),
                    }
                }
            }
            ShellEvent::TerrainPreviewResetCamera => {
                if crate::TERRAIN_PREVIEW.state() == 4 && self.title_renderer.is_some() {
                    if let Some(camera) = self.terrain_camera.as_mut() {
                        camera.reset_view();
                        if let Some(motion) = self.preview_physics.as_mut() {
                            motion.sync_camera(camera.position());
                        }
                        if let Some(clock) = self.preview_clock.as_mut() {
                            clock.reset();
                        }
                        self.preview_last_tick = Some(Instant::now());
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }
                }
            }
            ShellEvent::WorldInspected { generation, result } => {
                if generation == self.world_inspection_generation
                    && self.lifecycle.drawable_size().is_some()
                {
                    self.world_inspection_pending = false;
                    match result {
                        Ok(loaded) => {
                            let PreparedWorldPreview {
                                world: prepared,
                                terrain,
                                native_world,
                            } = loaded;
                            self.terrain = terrain;
                            self.native_world = native_world;
                            self.preview_collision_failed = false;
                            let horizon_client::game_startup::PreparedWorld {
                                inspection: world,
                                initial_column,
                            } = prepared;
                            // The Winit thread owns the native column after the
                            // worker finishes. Later stages will transfer it
                            // into the terrain/chunk manager, not save it.
                            self.initial_column = initial_column;
                            self.terrain_camera =
                                if self.terrain.is_some() && self.initial_column.is_some() {
                                    world.saved_position.zip(world.saved_rotation).map(
                                        |(position, rotation)| {
                                            crate::PreviewCamera::new(position, rotation)
                                        },
                                    )
                                } else {
                                    None
                                };
                            if let (Some(camera), Some(terrain)) =
                                (self.terrain_camera.as_mut(), self.terrain.as_ref())
                            {
                                // Only loaded, successfully meshed columns
                                // become traversable by the read-only viewer.
                                for (key, _) in &terrain.meshes {
                                    camera.allow_rendered_chunk([key.0, key.2]);
                                }
                            }
                            if self.terrain_camera.is_some()
                                && self.horizon.is_some()
                                && self.title.is_some()
                            {
                                crate::TERRAIN_PREVIEW.ready();
                            } else {
                                crate::TERRAIN_PREVIEW.reset();
                            }
                            unsafe { horizon_ios_renderer_status_changed() };
                            let safe_name: String = world
                                .name
                                .chars()
                                .filter(|c| !c.is_control())
                                .take(96)
                                .collect();
                            let mode = match world.game_mode {
                                1 => "Creative",
                                2 => "Adventure",
                                3 => "Spectator",
                                _ => "Survival",
                            };
                            let resident = world.resident_chunk_preview.as_ref().map_or(
                                " No resident block/biome ID conversion available.".to_owned(),
                                |r| format!(
                                    " Native resident chunk: {} sections, block state ID {}, biome ID {}, approx. {} section heap bytes. Held: {}. No terrain rendered.",
                                    r.resident_sections, r.block_state_id, r.biome_id, r.heap_bytes,
                                    self.initial_column.is_some(),
                                ),
                            );
                            let native_status = self.native_world.as_ref().map_or(
                                " Native collision world not staged.".to_owned(),
                                |stage| format!(
                                    " Native read-only collision world: {} saved chunks (0 writes, 0 simulation ticks).",
                                    stage.saved_columns(),
                                ),
                            );
                            let mesh_status = self.terrain.as_ref().map_or(
                                " CPU terrain mesh not prepared.".to_owned(),
                                |prepared| format!(
                                    " CPU mesh ready: {} of {} saved columns ({} skipped by budget), {} visible sections, {} solid quads, {} translucent quads (read-only; GPU preview available after preparing).",
                                    prepared.meshed_columns,
                                    prepared.saved_neighbors + 1,
                                    prepared.skipped_columns,
                                    prepared.summary.sections,
                                    prepared.summary.solid_quads,
                                    prepared.summary.translucent_quads,
                                ),
                            );
                            let disk_probe = match &world.initial_chunk_disk {
                                Some(horizon_client::game_startup::InitialChunkDiskState::Saved { data_version, sections, block_below_player }) => {
                                    let sampled = block_below_player.as_deref().map_or(
                                        "No saved block section at the player's feet".to_owned(),
                                        |name| {
                                            let safe_name: String = name.chars()
                                                .filter(|ch| !ch.is_control())
                                                .take(96)
                                                .collect();
                                            format!("Block below saved feet: {safe_name}")
                                        },
                                    );
                                    format!(" Anvil chunk sections decoded: {sections} (data version {}). {sampled}.", data_version.map_or_else(|| "unknown".to_owned(), |n| n.to_string()))
                                }
                                Some(horizon_client::game_startup::InitialChunkDiskState::Missing) => {
                                    " No saved chunk exists at this position; generation will be required.".to_owned()
                                }
                                None => " No overworld chunk was inspected.".to_owned(),
                            };
                            let initial_location = match (world.saved_position, world.initial_chunk) {
                                (Some([x, y, z]), Some([cx, cz])) => format!(
                                    " Saved player at ({x:.1}, {y:.1}, {z:.1}); first chunk ({cx}, {cz}).{disk_probe}{resident}{native_status}{mesh_status}"
                                ),
                                _ => " No valid saved player location; spawn search will be required before gameplay.".to_owned(),
                            };
                            let dimension: String = world
                                .dimension
                                .chars()
                                .filter(|c| !c.is_control())
                                .take(96)
                                .collect();
                            report_world_inspection(
                                &format!(
                                    "World '{safe_name}' verified: {mode}, seed {}, dimension {dimension}.{initial_location} Gameplay is not implemented yet; no save files were changed.",
                                    world.seed
                                ),
                                2,
                            );
                        }
                        Err(error) => {
                            self.terrain = None;
                            self.native_world = None;
                            self.preview_collision_failed = false;
                            self.terrain_camera = None;
                            self.initial_column = None;
                            crate::TERRAIN_PREVIEW.reset();
                            unsafe { horizon_ios_renderer_status_changed() };
                            report_world_inspection(&format!("World inspection failed: {error}"), 3)
                        }
                    }
                }
            }
            ShellEvent::Wake => {}
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.metal = None;
        self.horizon = None;
        self.title_renderer = None;
        self.title = None;
        self.terrain = None;
        self.terrain_camera = None;
        crate::TERRAIN_PREVIEW.reset();
        self.window = None;
        let _ = crate::horizon_ios_shutdown();
        crate::set_graphics_state(0);
    }
}

pub(super) fn run() -> Result<(), EventLoopError> {
    crate::clipboard_bridge::install();
    if log::set_logger(&STARTUP_LOGGER).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
    let event_loop = EventLoop::<ShellEvent>::with_user_event().build()?;
    let _ = EVENT_PROXY.set(event_loop.create_proxy());
    let mut shell = Shell {
        renderer_smoke: std::env::var("HORIZON_IOS_RENDERER_SMOKE").is_ok_and(|value| value == "1"),
        ..Shell::default()
    };
    let _ = crate::horizon_ios_initialize();
    event_loop.run_app(&mut shell)
}
