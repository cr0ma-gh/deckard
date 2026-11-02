//! Draws a [`Scene`] with wgpu (shapes, covers) and glyphon (text).

mod thumbs;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use bytemuck::{Pod, Zeroable};
use deckard_video::{Gpu, Overlay};
use glyphon::cosmic_text::{Ellipsize, EllipsizeHeightLimit};
use glyphon::{
    Attrs, Buffer, Cache, Color as TextColor, Family, FontSystem, Metrics, Resolution, Shaping,
    SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport, Weight, Wrap,
};

use self::thumbs::{GpuImage, Thumbnails};
pub use self::thumbs::{Rgba, decode_png};
use crate::layout::{Align, Rect, Scene, Text};

const FONT: &[u8] = include_bytes!("../../assets/fonts/Inter.ttf");

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    size: [f32; 2],
    time: f32,
    srgb_target: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct QuadInstance {
    rect: [f32; 4],
    color: [f32; 4],
    radius: f32,
    _pad: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ImageInstance {
    rect: [f32; 4],
    alpha: f32,
    _pad: [f32; 3],
}

/// A GPU buffer that grows as needed.
struct GrowBuffer {
    buffer: Option<wgpu::Buffer>,
    usage: wgpu::BufferUsages,
    label: &'static str,
}

impl GrowBuffer {
    fn new(label: &'static str, usage: wgpu::BufferUsages) -> Self {
        Self {
            buffer: None,
            usage: usage | wgpu::BufferUsages::COPY_DST,
            label,
        }
    }

    fn write(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        let needed = data.len() as u64;
        if self.buffer.as_ref().is_none_or(|b| b.size() < needed) {
            self.buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(self.label),
                size: needed.next_power_of_two().max(256),
                usage: self.usage,
                mapped_at_creation: false,
            }));
        }
        if let Some(buffer) = &self.buffer {
            queue.write_buffer(buffer, 0, data);
        }
    }
}

/// GPU state of one scene layer.
struct LayerGpu {
    quads: GrowBuffer,
    quad_count: u32,
    images: GrowBuffer,
    /// Cover path and instance index of each image to draw.
    image_draws: Vec<(PathBuf, u32)>,
    text: TextRenderer,
}

/// Renders menu scenes; implements [`Overlay`] so it draws in the video pass.
pub struct UiRenderer {
    srgb_target: bool,
    globals: wgpu::Buffer,
    globals_bind: wgpu::BindGroup,
    image_layout: wgpu::BindGroupLayout,
    image_sampler: wgpu::Sampler,
    background: wgpu::RenderPipeline,
    quad_pipeline: wgpu::RenderPipeline,
    image_pipeline: wgpu::RenderPipeline,
    font_system: FontSystem,
    swash: SwashCache,
    atlas: TextAtlas,
    viewport: Viewport,
    text_buffers: HashMap<TextKey, Buffer>,
    layers: Vec<LayerGpu>,
    thumbs: Thumbnails,
    scene: Scene,
    /// Draw the animated background (menu) or not (overlay on a game).
    pub draw_background: bool,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct TextKey {
    text: String,
    size: u32,
    bold: bool,
    max_width: u32,
}

impl TextKey {
    fn of(t: &Text) -> Self {
        Self {
            text: t.text.clone(),
            size: t.size.to_bits(),
            bold: t.bold,
            max_width: t.max_width.round() as u32,
        }
    }
}

impl UiRenderer {
    pub fn new(gpu: &Gpu<'_>) -> Self {
        let device = gpu.device;
        let srgb_target = gpu.format.is_srgb();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ui shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("ui.wgsl").into()),
        });

        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let globals_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui globals"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });
        let image_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui image"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let plain_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui"),
            bind_group_layouts: &[Some(&globals_layout)],
            immediate_size: 0,
        });
        let image_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("ui image"),
                bind_group_layouts: &[Some(&globals_layout), Some(&image_layout)],
                immediate_size: 0,
            });

        let target = [Some(wgpu::ColorTargetState {
            format: gpu.format,
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        })];
        let pipeline = |label: &str,
                        layout: &wgpu::PipelineLayout,
                        vs: &str,
                        fs: &str,
                        buffers: &[Option<wgpu::VertexBufferLayout<'_>>]| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers,
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &target,
                }),
                multiview_mask: None,
                cache: None,
            })
        };

        let quad_attrs = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32];
        let image_attrs = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32];
        let background = pipeline("ui background", &plain_layout, "bg_vs", "bg_fs", &[]);
        let quad_pipeline = pipeline(
            "ui quads",
            &plain_layout,
            "quad_vs",
            "quad_fs",
            &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<QuadInstance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &quad_attrs,
            })],
        );
        let image_pipeline = pipeline(
            "ui images",
            &image_pipeline_layout,
            "image_vs",
            "image_fs",
            &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<ImageInstance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &image_attrs,
            })],
        );
        let image_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ui image"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // Only the bundled font: identical look everywhere, no system scan.
        let mut db = glyphon::fontdb::Database::new();
        db.load_font_data(FONT.to_vec());
        let family = db
            .faces()
            .next()
            .and_then(|f| f.families.first())
            .map(|f| f.0.clone());
        if let Some(family) = family {
            db.set_sans_serif_family(family);
        }
        let font_system = FontSystem::new_with_locale_and_db("en-US".into(), db);
        let cache = Cache::new(device);
        let atlas = TextAtlas::new(device, gpu.queue, &cache, gpu.format);
        let viewport = Viewport::new(device, &cache);

        Self {
            srgb_target,
            globals,
            globals_bind,
            image_layout,
            image_sampler,
            background,
            quad_pipeline,
            image_pipeline,
            font_system,
            swash: SwashCache::new(),
            atlas,
            viewport,
            text_buffers: HashMap::new(),
            layers: Vec::new(),
            thumbs: Thumbnails::new(),
            scene: Scene::default(),
            draw_background: true,
        }
    }

    /// Sets what the next frame shows.
    pub fn set_scene(&mut self, scene: Scene) {
        self.scene = scene;
    }

    /// Creates (or reuses) the shaped buffer for a text.
    fn ensure_text(&mut self, t: &Text) {
        let key = TextKey::of(t);
        if self.text_buffers.contains_key(&key) {
            return;
        }
        let mut buffer = Buffer::new(&mut self.font_system, Metrics::new(t.size, t.size * 1.25));
        buffer.set_wrap(Wrap::None);
        buffer.set_ellipsize(Ellipsize::End(EllipsizeHeightLimit::Lines(1)));
        buffer.set_size(Some(t.max_width.max(1.0)), Some(t.size * 1.6));
        let attrs = Attrs::new().family(Family::SansSerif).weight(if t.bold {
            Weight::BOLD
        } else {
            Weight::NORMAL
        });
        buffer.set_text(&t.text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.font_system, false);
        self.text_buffers.insert(key, buffer);
    }
}

fn make_gpu_image(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    srgb: bool,
    rgba: &Rgba,
) -> GpuImage {
    let size = wgpu::Extent3d {
        width: rgba.width,
        height: rgba.height,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("cover"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: if srgb {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        },
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &rgba.pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(rgba.width * 4),
            rows_per_image: Some(rgba.height),
        },
        size,
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("cover"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    });
    GpuImage {
        bind_group,
        width: rgba.width,
        height: rgba.height,
    }
}

/// Largest rectangle with the image's aspect ratio centered in `fit`.
pub fn fit_image(fit: Rect, width: u32, height: u32) -> Rect {
    let scale = (fit.w / width as f32).min(fit.h / height as f32);
    let (w, h) = (width as f32 * scale, height as f32 * scale);
    Rect {
        x: fit.x + (fit.w - w) / 2.0,
        y: fit.y + (fit.h - h) / 2.0,
        w,
        h,
    }
}

fn text_color(c: [f32; 4]) -> TextColor {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    TextColor::rgba(b(c[0]), b(c[1]), b(c[2]), b(c[3]))
}

impl Overlay for UiRenderer {
    fn prepare(&mut self, gpu: &Gpu<'_>) {
        let (device, queue) = (gpu.device, gpu.queue);
        let globals = Globals {
            size: [gpu.width as f32, gpu.height as f32],
            time: self.scene.time,
            srgb_target: if self.srgb_target { 1.0 } else { 0.0 },
        };
        queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));
        self.viewport.update(
            queue,
            Resolution {
                width: gpu.width,
                height: gpu.height,
            },
        );

        // Covers decoded in the background since the last frame.
        let (layout, sampler, srgb) = (&self.image_layout, &self.image_sampler, self.srgb_target);
        self.thumbs.upload(device, queue, |d, q, rgba| {
            make_gpu_image(d, q, layout, sampler, srgb, rgba)
        });

        let scene = std::mem::take(&mut self.scene);
        while self.layers.len() < scene.layers.len() {
            self.layers.push(LayerGpu {
                quads: GrowBuffer::new("ui quads", wgpu::BufferUsages::VERTEX),
                quad_count: 0,
                images: GrowBuffer::new("ui images", wgpu::BufferUsages::VERTEX),
                image_draws: Vec::new(),
                text: TextRenderer::new(
                    &mut self.atlas,
                    device,
                    wgpu::MultisampleState::default(),
                    None,
                ),
            });
        }

        let mut used = HashSet::new();
        for layer in &scene.layers {
            for t in &layer.texts {
                self.ensure_text(t);
                used.insert(TextKey::of(t));
            }
        }
        self.text_buffers.retain(|k, _| used.contains(k));

        for (i, layer) in scene.layers.iter().enumerate() {
            let quads: Vec<QuadInstance> = layer
                .quads
                .iter()
                .map(|q| QuadInstance {
                    rect: [q.rect.x, q.rect.y, q.rect.w, q.rect.h],
                    color: q.color,
                    radius: q.radius,
                    _pad: [0.0; 3],
                })
                .collect();
            let mut images = Vec::new();
            let mut image_draws = Vec::new();
            for img in &layer.images {
                if let Some(gpu_img) = self.thumbs.get(&img.path) {
                    let r = fit_image(img.fit, gpu_img.width, gpu_img.height);
                    image_draws.push((img.path.clone(), images.len() as u32));
                    images.push(ImageInstance {
                        rect: [r.x, r.y, r.w, r.h],
                        alpha: img.alpha,
                        _pad: [0.0; 3],
                    });
                }
            }

            let gpu_layer = &mut self.layers[i];
            gpu_layer
                .quads
                .write(device, queue, bytemuck::cast_slice(&quads));
            gpu_layer.quad_count = quads.len() as u32;
            gpu_layer
                .images
                .write(device, queue, bytemuck::cast_slice(&images));
            gpu_layer.image_draws = image_draws;

            let areas = layer.texts.iter().filter_map(|t| {
                let buffer = self.text_buffers.get(&TextKey::of(t))?;
                let width = buffer.layout_runs().map(|r| r.line_w).fold(0.0, f32::max);
                let left = match t.align {
                    Align::Left => t.x,
                    Align::Center => t.x - width / 2.0,
                    Align::Right => t.x - width,
                };
                Some(TextArea {
                    buffer,
                    left,
                    top: t.y,
                    scale: 1.0,
                    bounds: TextBounds {
                        left: left.floor() as i32,
                        top: t.y.floor() as i32,
                        right: (left + t.max_width).ceil() as i32,
                        bottom: (t.y + t.size * 1.6).ceil() as i32,
                    },
                    default_color: text_color(t.color),
                    custom_glyphs: &[],
                })
            });
            if let Err(e) = gpu_layer.text.prepare(
                device,
                queue,
                &mut self.font_system,
                &mut self.atlas,
                &self.viewport,
                areas,
                &mut self.swash,
            ) {
                tracing::warn!("text preparation failed: {e}");
            }
        }
        for unused in self.layers.iter_mut().skip(scene.layers.len()) {
            unused.quad_count = 0;
            unused.image_draws.clear();
        }
        self.atlas.trim();
        self.scene = scene;
    }

    fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_bind_group(0, &self.globals_bind, &[]);
        if self.draw_background {
            pass.set_pipeline(&self.background);
            pass.draw(0..3, 0..1);
        }
        for (i, layer) in self.layers.iter().enumerate().take(self.scene.layers.len()) {
            if let (Some(buf), true) = (&layer.quads.buffer, layer.quad_count > 0) {
                pass.set_pipeline(&self.quad_pipeline);
                pass.set_bind_group(0, &self.globals_bind, &[]);
                pass.set_vertex_buffer(0, buf.slice(..));
                pass.draw(0..6, 0..layer.quad_count);
            }
            if let (Some(buf), false) = (&layer.images.buffer, layer.image_draws.is_empty()) {
                pass.set_pipeline(&self.image_pipeline);
                pass.set_bind_group(0, &self.globals_bind, &[]);
                pass.set_vertex_buffer(0, buf.slice(..));
                for (path, instance) in &layer.image_draws {
                    if let Some(img) = self.thumbs.ready(path) {
                        pass.set_bind_group(1, &img.bind_group, &[]);
                        pass.draw(0..6, *instance..*instance + 1);
                    }
                }
            }
            if let Err(e) = layer.text.render(&self.atlas, &self.viewport, pass) {
                tracing::warn!(layer = i, "text rendering failed: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn images_fit_centered() {
        let fit = Rect {
            x: 0.0,
            y: 0.0,
            w: 400.0,
            h: 400.0,
        };
        let r = fit_image(fit, 200, 100);
        assert_eq!((r.w, r.h), (400.0, 200.0));
        assert_eq!((r.x, r.y), (0.0, 100.0));
        let r = fit_image(fit, 100, 200);
        assert_eq!((r.w, r.h, r.x), (200.0, 400.0, 100.0));
    }
}
