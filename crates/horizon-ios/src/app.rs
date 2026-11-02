//! Winit owns UIKit's sole UIApplicationMain. All callbacks execute on the
//! UIKit main thread; no secondary SwiftUI app event loop is created.

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::ffi::c_void;
use std::sync::{Arc, OnceLock};
use winit::application::ApplicationHandler;
use winit::error::EventLoopError;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

#[path = "metal_probe.rs"]
mod metal_probe;
use crate::ProbeLifecycle;
use horizon_client::{gfx::Renderer, graphics::GraphicsApi, hud::HudBatch};
use metal_probe::MetalProbe;

static EVENT_PROXY: OnceLock<EventLoopProxy<()>> = OnceLock::new();

pub(super) fn renderer_events_available() -> bool {
    EVENT_PROXY.get().is_some()
}

pub(super) fn request_renderer_event() -> bool {
    EVENT_PROXY
        .get()
        .is_some_and(|proxy| proxy.send_event(()).is_ok())
}

unsafe extern "C" {
    /// Swift installs a UIHostingController as a child of Winit's controller.
    /// The parent remains owned by Winit; Swift must not release it.
    fn horizon_ios_mount_home(view_controller: *mut c_void);
    fn horizon_ios_graphics_changed(state: u32);
    fn horizon_ios_touch_changed();
    /// Swift copies the UTF-8 bytes before this callback returns.
    fn horizon_ios_report_graphics_error(message: *const u8, length: usize);
    /// Swift copies the diagnostic before this synchronous callback returns.
    fn horizon_ios_resource_import_result(message: *const u8, length: usize, succeeded: i32);
    fn horizon_ios_core_load_result(message: *const u8, length: usize, succeeded: i32);
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

fn report_graphics_error(message: &str) {
    eprintln!("Horizon iOS graphics: {message}");
    unsafe { horizon_ios_report_graphics_error(message.as_ptr(), message.len()) };
}

#[derive(Default)]
struct Shell {
    window: Option<Arc<Window>>,
    metal: Option<MetalProbe>,
    horizon: Option<Renderer>,
    lifecycle: ProbeLifecycle,
    /// CI opt-in: exercise the real Horizon GPU pipeline without distributing
    /// any proprietary game assets. This is not enabled in normal launches.
    renderer_smoke: bool,
}

impl Shell {
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
    /// This is opt-in because the full Horizon renderer creates many GPU
    /// pipelines. Keep the working Metal probe as the default smoke target.
    /// The probe's surface must be dropped *before* creating the real renderer
    /// on the same UIKit Winit window.
    fn try_horizon_renderer(&mut self) {
        if !crate::RENDERER_SWITCH.start() {
            return;
        }
        let Some(window) = self.window.as_ref().cloned() else {
            crate::RENDERER_SWITCH.finished(false);
            return;
        };
        self.metal = None;
        crate::set_graphics_state(1);
        // Use the exact boot renderer shared with the desktop/Android title
        // screen, not another test WGPU pipeline. Its first drawn frame does
        // not initialize a Minecraft world.
        let atlas = horizon_client::hud::HudAtlas::boot();
        let loaded = horizon_client::game_startup::loading_renderer(
            window.clone(),
            false,
            Some(&atlas),
            None,
            None,
            GraphicsApi::Auto,
        );
        match loaded {
            Ok(renderer) => {
                self.horizon = Some(renderer);
                window.request_redraw();
            }
            Err(error) => {
                report_graphics_error(&format!("Horizon loading renderer: {error}"));
                crate::RENDERER_SWITCH.finished(false);
                self.start_metal();
                window.request_redraw();
            }
        }
        // Refresh SwiftUI even before the first real frame is presented.
        unsafe { graphics_changed(crate::horizon_ios_graphics_state()) };
    }

    fn draw_horizon(&mut self) {
        let Some(renderer) = self.horizon.as_mut() else {
            return;
        };
        let batch = HudBatch::new([renderer.config.width, renderer.config.height]);
        let frame = horizon_client::game_startup::loading_frame(&batch);
        let result = renderer.render(&frame);
        match result {
            Ok(true) => {
                if self.lifecycle.present_frame() {
                    crate::RENDERER_SWITCH.finished(true);
                    crate::set_graphics_state(2);
                }
            }
            Ok(false) => {
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            Err(error) => {
                report_graphics_error(&format!("Horizon first frame: {error}"));
                self.horizon = None;
                crate::RENDERER_SWITCH.finished(false);
                self.start_metal();
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
                unsafe { graphics_changed(crate::horizon_ios_graphics_state()) };
            }
        }
    }
}

impl ApplicationHandler for Shell {
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
        self.lifecycle.suspend();
        crate::clear_touches();
        // Drop the surface and all GPU objects before UIKit enters background.
        self.metal = None;
        self.horizon = None;
        crate::RENDERER_SWITCH.reset();
        crate::set_graphics_state(4);
        crate::shell_suspended();
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
                if self.lifecycle.drawable_size().is_none() {
                    crate::clear_touches();
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
                if self.renderer_smoke
                    && crate::RENDERER_SWITCH.state() == 0
                    && crate::horizon_ios_graphics_state() == 2
                {
                    let _ = crate::RENDERER_SWITCH.request(true, true);
                }
                self.try_horizon_renderer();
                if self.horizon.is_some() {
                    self.draw_horizon();
                } else if let Some(metal) = &self.metal {
                    match metal.draw() {
                        Ok(true) => {
                            if self.lifecycle.present_frame() {
                                crate::set_graphics_state(2);
                                if self.renderer_smoke && crate::RENDERER_SWITCH.state() == 0 {
                                    window.request_redraw();
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
                    let _ = crate::update_touch(
                        touch.id,
                        phase,
                        (touch.location.x as f32, touch.location.y as f32),
                        size,
                    );
                }
            }
            WindowEvent::CloseRequested => event_loop.exit(),
            _ => {}
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: ()) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.metal = None;
        self.horizon = None;
        self.window = None;
        let _ = crate::horizon_ios_shutdown();
        crate::set_graphics_state(0);
    }
}

pub(super) fn run() -> Result<(), EventLoopError> {
    crate::clipboard_bridge::install();
    let event_loop = EventLoop::builder().build()?;
    let _ = EVENT_PROXY.set(event_loop.create_proxy());
    let mut shell = Shell {
        renderer_smoke: std::env::var("HORIZON_IOS_RENDERER_SMOKE").is_ok_and(|value| value == "1"),
        ..Shell::default()
    };
    let _ = crate::horizon_ios_initialize();
    event_loop.run_app(&mut shell)
}
