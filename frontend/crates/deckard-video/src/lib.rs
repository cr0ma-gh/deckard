//! Video output: draws core frames into a window with wgpu.
//!
//! Frames are converted to BGRA8 on the CPU (cheap at retro resolutions),
//! uploaded to a texture and drawn aspect-correct with nearest filtering.

mod convert;

use std::sync::Arc;

use deckard_core::VideoFrame;
use winit::event_loop::OwnedDisplayHandle;
use winit::window::Window;

pub use crate::convert::to_bgra8;

#[derive(Debug, thiserror::Error)]
pub enum VideoError {
    #[error("cannot create the rendering surface: {0}")]
    Surface(#[from] wgpu::CreateSurfaceError),
    #[error("no compatible GPU adapter: {0}")]
    Adapter(#[from] wgpu::RequestAdapterError),
    #[error("cannot open the GPU device: {0}")]
    Device(#[from] wgpu::RequestDeviceError),
    #[error("the surface is not supported by the GPU adapter")]
    UnsupportedSurface,
}

/// Result of [`Renderer::render`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presented {
    Yes,
    /// Nothing was shown (window hidden, minimized or surface being rebuilt).
    No,
}

/// Viewport rectangle in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Largest rectangle with `aspect` (width / height) centered in the window.
pub fn fit_aspect(window_width: u32, window_height: u32, aspect: f32) -> Viewport {
    let (w, h) = (window_width as f32, window_height as f32);
    if w <= 0.0 || h <= 0.0 || aspect <= 0.0 {
        return Viewport {
            x: 0.0,
            y: 0.0,
            width: w,
            height: h,
        };
    }
    let (width, height) = if w / h > aspect {
        (h * aspect, h)
    } else {
        (w, w / aspect)
    };
    Viewport {
        x: ((w - width) / 2.0).floor(),
        y: ((h - height) / 2.0).floor(),
        width: width.round(),
        height: height.round(),
    }
}

struct FrameTexture {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
}

/// Draws frames into one window.
pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    texture_format: wgpu::TextureFormat,
    frame: Option<FrameTexture>,
    staging: Vec<u8>,
}

impl Renderer {
    pub fn new(display: OwnedDisplayHandle, window: Arc<Window>) -> Result<Self, VideoError> {
        let instance = wgpu::Instance::new(
            wgpu::InstanceDescriptor::new_with_display_handle_from_env(Box::new(display)),
        );
        let size = window.inner_size();
        let surface = instance.create_surface(window)?;
        // Prefer the discrete GPU on hybrid laptops (the Deck has a single GPU).
        // WGPU_POWER_PREF=high|low and WGPU_BACKEND=vulkan|dx12|gl override the choice.
        let adapter =
            pollster::block_on(
                instance.request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::from_env()
                        .unwrap_or(wgpu::PowerPreference::HighPerformance),
                    compatible_surface: Some(&surface),
                    ..Default::default()
                }),
            )?;
        let adapter_info = adapter.get_info();
        tracing::info!(name = adapter_info.name, backend = ?adapter_info.backend, "GPU adapter");

        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("deckard"),
                required_limits:
                    wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
                ..Default::default()
            }))?;

        tracing::debug!("GPU device created");
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or(VideoError::UnsupportedSurface)?;
        // Vsync: always available, and the frame pacer relies on it.
        config.present_mode = wgpu::PresentMode::Fifo;
        tracing::debug!(format = ?config.format, "configuring surface");
        surface.configure(&device, &config);
        tracing::debug!("surface configured");

        // Sample the raw bytes back unchanged: an sRGB surface encodes on write,
        // so the texture must decode on read.
        let texture_format = if config.format.is_srgb() {
            wgpu::TextureFormat::Bgra8UnormSrgb
        } else {
            wgpu::TextureFormat::Bgra8Unorm
        };

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("frame shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("frame pipeline"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("nearest"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        tracing::debug!("render pipeline ready");
        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            sampler,
            texture_format,
            frame: None,
            staging: Vec::new(),
        })
    }

    /// Must be called when the window size changes.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return; // Minimized: keep the old configuration.
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    /// Uploads a new core frame.
    pub fn upload(&mut self, frame: &VideoFrame<'_>) {
        let (width, height) = (frame.width, frame.height);
        if self
            .frame
            .as_ref()
            .is_none_or(|f| f.width != width || f.height != height)
        {
            self.frame = Some(self.create_frame_texture(width, height));
        }
        let Some(target) = &self.frame else { return };

        to_bgra8(frame, &mut self.staging);
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &target.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &self.staging,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }

    fn create_frame_texture(&self, width: u32, height: u32) -> FrameTexture {
        tracing::debug!(width, height, "creating frame texture");
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("core frame"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.texture_format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("core frame"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        FrameTexture {
            texture,
            bind_group,
            width,
            height,
        }
    }

    /// Draws the last uploaded frame with the given display aspect ratio.
    /// Blocks until the next vertical blank (Fifo presentation).
    pub fn render(&mut self, aspect: f32) -> Presented {
        let surface_texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => t,
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => {
                // Draw this one, rebuild the swapchain for the next.
                self.surface.configure(&self.device, &self.config);
                t
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return Presented::No;
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Presented::No;
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                tracing::error!("surface validation error");
                return Presented::No;
            }
        };
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("frame"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let Some(frame) = &self.frame {
                let vp = fit_aspect(self.config.width, self.config.height, aspect);
                if vp.width >= 1.0 && vp.height >= 1.0 {
                    pass.set_viewport(vp.x, vp.y, vp.width, vp.height, 0.0, 1.0);
                    pass.set_pipeline(&self.pipeline);
                    pass.set_bind_group(0, &frame.bind_group, &[]);
                    pass.draw(0..3, 0..1);
                }
            }
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(surface_texture);
        Presented::Yes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letterbox_and_pillarbox() {
        // 4:3 content in a 16:10 window (Steam Deck): pillarbox.
        let vp = fit_aspect(1280, 800, 4.0 / 3.0);
        assert_eq!((vp.width, vp.height), (1067.0, 800.0));
        assert_eq!((vp.x, vp.y), (106.0, 0.0));

        // 16:9 content in a 4:3 window: letterbox.
        let vp = fit_aspect(800, 600, 16.0 / 9.0);
        assert_eq!((vp.width, vp.height), (800.0, 450.0));
        assert_eq!(vp.y, 75.0);

        // Degenerate input keeps the whole window.
        assert_eq!(fit_aspect(640, 480, 0.0).width, 640.0);
    }
}
