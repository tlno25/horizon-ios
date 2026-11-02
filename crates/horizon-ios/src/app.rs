//! Winit owns UIKit's sole UIApplicationMain. All callbacks execute on the
//! UIKit main thread; no secondary SwiftUI app event loop is created.

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::ffi::c_void;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::error::EventLoopError;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

#[path = "metal_probe.rs"]
mod metal_probe;
use crate::ProbeLifecycle;
use metal_probe::MetalProbe;

unsafe extern "C" {
    /// Swift installs a UIHostingController as a child of Winit's controller.
    /// The parent remains owned by Winit; Swift must not release it.
    fn horizon_ios_mount_home(view_controller: *mut c_void);
    fn horizon_ios_graphics_changed(state: u32);
    /// Swift copies the UTF-8 bytes before this callback returns.
    fn horizon_ios_report_graphics_error(message: *const u8, length: usize);
    /// Swift copies the diagnostic before this synchronous callback returns.
    fn horizon_ios_resource_import_result(message: *const u8, length: usize, succeeded: i32);
}

pub(super) unsafe fn graphics_changed(state: u32) {
    unsafe { horizon_ios_graphics_changed(state) };
}

pub(super) fn report_resource_import_result(message: &str, success: bool) {
    unsafe {
        horizon_ios_resource_import_result(message.as_ptr(), message.len(), i32::from(success));
    }
}

fn report_graphics_error(message: &str) {
    eprintln!("Horizon iOS graphics: {message}");
    unsafe { horizon_ios_report_graphics_error(message.as_ptr(), message.len()) };
}

#[derive(Default)]
struct Shell {
    window: Option<Arc<Window>>,
    metal: Option<MetalProbe>,
    lifecycle: ProbeLifecycle,
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
        if self.metal.is_some() || self.lifecycle.drawable_size().is_none() {
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
        // Drop the surface and all GPU objects before UIKit enters background.
        self.metal = None;
        crate::set_graphics_state(4);
        crate::shell_suspended();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(window) = &self.window else { return };
        if window.id() != id {
            return;
        }
        match event {
            WindowEvent::Resized(size) => {
                let previously_presented = self.lifecycle.has_presented_frame();
                self.lifecycle.resize(size.width, size.height);
                if previously_presented && !self.lifecycle.has_presented_frame() {
                    crate::set_graphics_state(1);
                }
                if let Some(metal) = &mut self.metal {
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
                if let Some(metal) = &self.metal {
                    match metal.draw() {
                        Ok(true) => {
                            if self.lifecycle.present_frame() {
                                crate::set_graphics_state(2);
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
            WindowEvent::CloseRequested => event_loop.exit(),
            _ => {}
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.metal = None;
        self.window = None;
        let _ = crate::horizon_ios_shutdown();
        crate::set_graphics_state(0);
    }
}

pub(super) fn run() -> Result<(), EventLoopError> {
    crate::clipboard_bridge::install();
    let event_loop = EventLoop::builder().build()?;
    let mut shell = Shell::default();
    let _ = crate::horizon_ios_initialize();
    event_loop.run_app(&mut shell)
}
