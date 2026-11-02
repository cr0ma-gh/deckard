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

/// GPU access lent to overlays.
#[derive(Clone, Copy)]
pub struct Gpu<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    /// Format of the window surface (the overlay's render target).
    pub format: wgpu::TextureFormat,
    pub width: u32,
    pub height: u32,
}

/// Something drawn on top of the game frame, in the same render pass.
pub trait Overlay {
    /// Uploads buffers and textures for this frame (before the pass begins).
    fn prepare(&mut self, gpu: &Gpu<'_>);
    /// Records draw commands. The viewport covers the whole window.
    fn draw(&self, pass: &mut wgpu::RenderPass<'_>);
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

/// Post-processing shader applied over the game framebuffer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ShaderMode {
    #[default]
    None,
    Scanlines,
    Crt,
}

/// How the game frame is scaled and shaded into the window.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Scaling {
    /// Scale by whole multiples of the frame height (sharp pixels, may leave borders).
    pub integer: bool,
    /// Bilinear filtering instead of nearest.
    pub smooth: bool,
    /// Post-processing CRT / retro shader.
    pub shader: ShaderMode,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ShaderParams {
    source_size: [f32; 2],
    target_size: [f32; 2],
    mode: u32,
    curvature: f32,
    scanline_weight: f32,
    mask_strength: f32,
}

/// Like [`fit_aspect`], but the height is a whole multiple of `frame_height`.
/// Falls back to [`fit_aspect`] when even 1x does not fit.
pub fn fit_integer(
    window_width: u32,
    window_height: u32,
    frame_height: u32,
    aspect: f32,
) -> Viewport {
    let fallback = fit_aspect(window_width, window_height, aspect);
    if frame_height == 0 || aspect <= 0.0 {
        return fallback;
    }
    let mut scale = window_height / frame_height;
    while scale > 0 && (frame_height * scale) as f32 * aspect > window_width as f32 {
        scale -= 1;
    }
    if scale == 0 {
        return fallback;
    }
    let height = (frame_height * scale) as f32;
    let width = (height * aspect).round();
    Viewport {
        x: ((window_width as f32 - width) / 2.0).floor(),
        y: ((window_height as f32 - height) / 2.0).floor(),
        width,
        height,
    }
}

struct FrameTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
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
    params_buffer: wgpu::Buffer,
    nearest: wgpu::Sampler,
    linear: wgpu::Sampler,
    scaling: Scaling,
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
        let nearest = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("nearest"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let linear = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("linear"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shader params"),
            size: std::mem::size_of::<ShaderParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        tracing::debug!("render pipeline ready");
        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            params_buffer,
            nearest,
            linear,
            scaling: Scaling::default(),
            texture_format,
            frame: None,
            staging: Vec::new(),
        })
    }

    /// Changes how the game frame is scaled and filtered.
    pub fn set_scaling(&mut self, scaling: Scaling) {
        if scaling == self.scaling {
            return;
        }
        let filter_changed = scaling.smooth != self.scaling.smooth
            || (scaling.shader == ShaderMode::Crt) != (self.scaling.shader == ShaderMode::Crt);
        self.scaling = scaling;
        if filter_changed && let Some(frame) = &self.frame {
            let bind_group = self.frame_bind_group(&frame.view);
            if let Some(frame) = &mut self.frame {
                frame.bind_group = bind_group;
            }
        }
    }

    fn frame_bind_group(&self, view: &wgpu::TextureView) -> wgpu::BindGroup {
        let sampler = if self.scaling.smooth || self.scaling.shader == ShaderMode::Crt {
            &self.linear
        } else {
            &self.nearest
        };
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("core frame"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.params_buffer.as_entire_binding(),
                },
            ],
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
        let bind_group = self.frame_bind_group(&view);
        FrameTexture {
            texture,
            view,
            bind_group,
            width,
            height,
        }
    }

    /// GPU handles for code that draws on top of the game (the menu).
    pub fn gpu(&self) -> Gpu<'_> {
        Gpu {
            device: &self.device,
            queue: &self.queue,
            format: self.config.format,
            width: self.config.width,
            height: self.config.height,
        }
    }

    /// Forgets the last game frame (e.g. when a game is closed).
    pub fn clear_frame(&mut self) {
        self.frame = None;
    }

    /// Draws one frame: the last uploaded game frame (if `game_aspect` is
    /// given) and then `overlay` on top. Blocks until the next vertical blank
    /// (Fifo presentation).
    pub fn render(
        &mut self,
        game_aspect: Option<f32>,
        overlay: Option<&mut dyn Overlay>,
    ) -> Presented {
        let mut overlay = overlay;
        if let Some(o) = overlay.as_deref_mut() {
            o.prepare(&self.gpu());
        }
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
        let game_vp = if let (Some(frame), Some(aspect)) = (&self.frame, game_aspect) {
            let (w, h) = (self.config.width, self.config.height);
            let vp = if self.scaling.integer {
                fit_integer(w, h, frame.height, aspect)
            } else {
                fit_aspect(w, h, aspect)
            };
            if vp.width >= 1.0 && vp.height >= 1.0 {
                let (mode, curvature, scanline_weight, mask_strength) = match self.scaling.shader {
                    ShaderMode::None => (0u32, 0.0f32, 0.0f32, 0.0f32),
                    ShaderMode::Scanlines => (1u32, 0.0f32, 0.35f32, 0.0f32),
                    ShaderMode::Crt => (2u32, 0.04f32, 0.30f32, 0.20f32),
                };
                let params = ShaderParams {
                    source_size: [frame.width as f32, frame.height as f32],
                    target_size: [vp.width, vp.height],
                    mode,
                    curvature,
                    scanline_weight,
                    mask_strength,
                };
                self.queue
                    .write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&params));
                Some(vp)
            } else {
                None
            }
        } else {
            None
        };

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
            if let (Some(frame), Some(vp)) = (&self.frame, game_vp) {
                pass.set_viewport(vp.x, vp.y, vp.width, vp.height, 0.0, 1.0);
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &frame.bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
            if let Some(o) = overlay {
                let (w, h) = (self.config.width as f32, self.config.height as f32);
                pass.set_viewport(0.0, 0.0, w, h, 0.0, 1.0);
                o.draw(&mut pass);
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
    fn integer_scaling() {
        // 240p in 1280x800: 3x (720 px high), width from the aspect ratio.
        let vp = fit_integer(1280, 800, 240, 4.0 / 3.0);
        assert_eq!((vp.width, vp.height), (960.0, 720.0));
        assert_eq!((vp.x, vp.y), (160.0, 40.0));
        // Too wide for 3x: drops to 2x.
        let vp = fit_integer(700, 800, 240, 4.0 / 3.0);
        assert_eq!(vp.height, 480.0);
        // Window smaller than 1x: plain fit.
        assert_eq!(
            fit_integer(200, 100, 240, 4.0 / 3.0),
            fit_aspect(200, 100, 4.0 / 3.0)
        );
    }

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
