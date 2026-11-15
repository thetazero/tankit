//! GPU setup and frame rendering. Works with a window surface or headless
//! (offscreen texture -> PNG) so frames can be checked without a display.

use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use winit::window::Window;

/// Everything the renderer needs to draw one frame.
pub struct Frame {
    pub time: f32,
}

pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    surface: Option<(wgpu::Surface<'static>, wgpu::SurfaceConfiguration)>,
    pub format: wgpu::TextureFormat,
    pub width: u32,
    pub height: u32,
}

impl Renderer {
    pub fn new_windowed(window: Arc<Window>) -> Result<Renderer> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance.create_surface(window).context("create surface")?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .context("no compatible GPU adapter")?;
        let (device, queue) = request_device(&adapter)?;

        let (w, h) = (size.width.max(1), size.height.max(1));
        let mut config = surface
            .get_default_config(&adapter, w, h)
            .ok_or_else(|| anyhow!("surface not supported by adapter"))?;
        let caps = surface.get_capabilities(&adapter);
        if let Some(srgb) = caps.formats.iter().copied().find(|f| f.is_srgb()) {
            config.format = srgb;
        }
        config.present_mode = wgpu::PresentMode::AutoVsync;
        config.desired_maximum_frame_latency = 2;
        surface.configure(&device, &config);

        Ok(Renderer { device, queue, format: config.format, width: w, height: h, surface: Some((surface, config)) })
    }

    pub fn new_headless(width: u32, height: u32) -> Result<Renderer> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            ..Default::default()
        }))
        .context("no GPU adapter")?;
        let (device, queue) = request_device(&adapter)?;
        Ok(Renderer {
            device,
            queue,
            surface: None,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            width,
            height,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.width = width;
        self.height = height;
        if let Some((surface, config)) = &mut self.surface {
            config.width = width;
            config.height = height;
            surface.configure(&self.device, config);
        }
    }

    /// Render to the window surface.
    pub fn render(&mut self, frame: &Frame) {
        let Some((surface, config)) = &self.surface else { return };
        let tex = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                surface.configure(&self.device, config);
                return;
            }
            _ => return,
        };
        let view = tex.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });
        self.encode(frame, &view, &mut encoder);
        self.queue.submit([encoder.finish()]);
        self.queue.present(tex);
    }

    /// Render offscreen and read the result back as an image.
    pub fn render_to_image(&mut self, frame: &Frame) -> Result<image::RgbaImage> {
        let (w, h) = (self.width, self.height);
        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let row = (w * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (row * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("shot") });
        self.encode(frame, &view, &mut encoder);
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) },
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.queue.submit([encoder.finish()]);

        readback.map_async(wgpu::MapMode::Read, .., |r| r.expect("map readback"));
        self.device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| anyhow!("poll: {e:?}"))?;
        let data = readback.get_mapped_range(..).map_err(|e| anyhow!("map: {e:?}"))?;
        let mut img = image::RgbaImage::new(w, h);
        for y in 0..h {
            let src = &data[(y * row) as usize..(y * row + w * 4) as usize];
            img.as_mut()[(y * w * 4) as usize..((y + 1) * w * 4) as usize].copy_from_slice(src);
        }
        Ok(img)
    }

    fn encode(&mut self, frame: &Frame, view: &wgpu::TextureView, encoder: &mut wgpu::CommandEncoder) {
        // Placeholder: slow sky-colored pulse proves the loop is alive.
        let pulse = 0.5 + 0.5 * (frame.time * 0.8).sin() as f64;
        let color = wgpu::Color { r: 0.25 + 0.1 * pulse, g: 0.45 + 0.1 * pulse, b: 0.7, a: 1.0 };
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(color), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
}

fn request_device(adapter: &wgpu::Adapter) -> Result<(wgpu::Device, wgpu::Queue)> {
    let info = adapter.get_info();
    log::info!("GPU: {} ({:?})", info.name, info.backend);
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("tankit"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        memory_hints: wgpu::MemoryHints::Performance,
        trace: wgpu::Trace::Off,
    }))
    .context("request device")
}
