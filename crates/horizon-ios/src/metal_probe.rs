//! Temporary Metal integration probe; this is not the Horizon game renderer.
//! Instantiated only while UIKit is in the foreground on the Winit event loop.

use std::sync::Arc;
use winit::window::Window;

pub(super) struct MetalProbe {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
}

impl MetalProbe {
    pub(super) fn new(window: Arc<Window>) -> Result<Self, String> {
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return Err("Window has no drawable area yet".to_owned());
        }

        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = wgpu::Backends::METAL;
        let instance = wgpu::Instance::new(descriptor);
        let surface = instance
            .create_surface(window)
            .map_err(|error| format!("Metal surface creation failed: {error}"))?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            ..Default::default()
        }))
        .map_err(|error| format!("No compatible Metal adapter: {error}"))?;
        if adapter.get_info().backend != wgpu::Backend::Metal {
            return Err("An unexpected non-Metal backend was selected".to_owned());
        }

        // The wgpu WebGPU defaults request 16 inter-stage shader variables,
        // but an iOS Simulator Metal adapter may only support 15. Request the
        // portable downlevel limits rather than assuming desktop WebGPU limits.
        // Keep the adapter's real texture resolutions for retina iOS screens.
        let required_limits = wgpu::Limits::downlevel_defaults()
            .using_resolution(adapter.limits())
            .using_alignment(adapter.limits());
        let descriptor = wgpu::DeviceDescriptor {
            label: Some("Horizon iOS Metal probe"),
            required_limits,
            ..Default::default()
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&descriptor))
            .map_err(|error| format!("Metal device initialization failed: {error}"))?;
        let mut config = surface
            .get_default_config(&adapter, size.width, size.height)
            .ok_or_else(|| "Metal surface offers no supported configuration".to_owned())?;
        config.present_mode = wgpu::PresentMode::Fifo;
        let max = device.limits().max_texture_dimension_2d;
        config.width = config.width.min(max);
        config.height = config.height.min(max);
        surface.configure(&device, &config);
        Ok(Self {
            surface,
            device,
            queue,
            config,
        })
    }

    pub(super) fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        let max = self.device.limits().max_texture_dimension_2d;
        let width = width.min(max);
        let height = height.min(max);
        if self.config.width == width && self.config.height == height {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    /// Ok(true): a drawable was presented. Ok(false): the swapchain was
    /// reconfigured and needs another redraw before the GPU can be marked ready.
    pub(super) fn draw(&self) -> Result<bool, String> {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                return Ok(false);
            }
            other => return Err(format!("Unable to acquire a Metal frame: {other:?}")),
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Horizon iOS Metal probe"),
            });
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Horizon iOS Metal clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.055,
                            g: 0.13,
                            b: 0.26,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
        Ok(true)
    }
}
