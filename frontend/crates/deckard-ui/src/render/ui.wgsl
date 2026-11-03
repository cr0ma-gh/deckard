// Menu drawing: animated background, rounded quads and cover images.
// Colors arrive in sRGB; when the target is an sRGB surface they are
// linearized here so the hardware encoding gives back the intended color.

struct Globals {
    size: vec2<f32>,
    time: f32,
    srgb_target: f32,
};

@group(0) @binding(0) var<uniform> g: Globals;

fn to_target(c: vec3<f32>) -> vec3<f32> {
    if (g.srgb_target > 0.5) {
        return pow(max(c, vec3<f32>(0.0)), vec3<f32>(2.2));
    }
    return c;
}

fn to_clip(p: vec2<f32>) -> vec4<f32> {
    return vec4<f32>(p.x / g.size.x * 2.0 - 1.0, 1.0 - p.y / g.size.y * 2.0, 0.0, 1.0);
}

// Two triangles per instance, corners in 0..1.
fn corner(i: u32) -> vec2<f32> {
    var c = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
    );
    return c[i];
}

// ---------------------------------------------------------------- background

struct BgOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn bg_vs(@builtin(vertex_index) i: u32) -> BgOut {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var out: BgOut;
    out.position = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    out.uv = uv;
    return out;
}

// Soft glowing band following a slow wave.
fn wave(uv: vec2<f32>, speed: f32, freq: f32, offset: f32, height: f32) -> f32 {
    let y = 0.62 + offset + sin(uv.x * freq + g.time * speed) * height
        + sin(uv.x * freq * 2.3 - g.time * speed * 0.7) * height * 0.35;
    let d = abs(uv.y - y);
    return exp(-d * d * 900.0) * 0.55 + exp(-d * 60.0) * 0.12;
}

@fragment
fn bg_fs(in: BgOut) -> @location(0) vec4<f32> {
    // Steam Deck tones (see theme.rs): dark blue-grey, Steam blue glow.
    let top = vec3<f32>(0.090, 0.114, 0.145);
    let bottom = vec3<f32>(0.043, 0.055, 0.071);
    var c = mix(top, bottom, in.uv.y);
    // Light vignette.
    let v = distance(in.uv, vec2<f32>(0.45, 0.4));
    c *= 1.15 - v * 0.45;
    let glow = wave(in.uv, 0.22, 3.1, 0.0, 0.05) + wave(in.uv, 0.17, 2.4, 0.03, 0.04);
    c += vec3<f32>(0.10, 0.62, 1.0) * glow * 0.55;
    return vec4<f32>(to_target(c), 1.0);
}

// ---------------------------------------------------------------- quads

struct QuadIn {
    @location(0) rect: vec4<f32>,
    @location(1) color: vec4<f32>,
    @location(2) radius: f32,
};

struct QuadOut {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) half_size: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) radius: f32,
};

@vertex
fn quad_vs(@builtin(vertex_index) vi: u32, q: QuadIn) -> QuadOut {
    let c = corner(vi);
    var out: QuadOut;
    out.position = to_clip(q.rect.xy + c * q.rect.zw);
    out.half_size = q.rect.zw * 0.5;
    out.local = (c - vec2<f32>(0.5)) * q.rect.zw;
    out.color = q.color;
    out.radius = min(q.radius, min(out.half_size.x, out.half_size.y));
    return out;
}

@fragment
fn quad_fs(in: QuadOut) -> @location(0) vec4<f32> {
    // Signed distance to a rounded box, anti-aliased over one pixel.
    let q = abs(in.local) - in.half_size + vec2<f32>(in.radius);
    let d = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - in.radius;
    let coverage = clamp(0.5 - d, 0.0, 1.0);
    return vec4<f32>(to_target(in.color.rgb), in.color.a * coverage);
}

// ---------------------------------------------------------------- images

struct ImageIn {
    @location(0) rect: vec4<f32>,
    @location(1) alpha: f32,
};

struct ImageOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) alpha: f32,
};

@group(1) @binding(0) var image_texture: texture_2d<f32>;
@group(1) @binding(1) var image_sampler: sampler;

@vertex
fn image_vs(@builtin(vertex_index) vi: u32, img: ImageIn) -> ImageOut {
    let c = corner(vi);
    var out: ImageOut;
    out.position = to_clip(img.rect.xy + c * img.rect.zw);
    out.uv = c;
    out.alpha = img.alpha;
    return out;
}

@fragment
fn image_fs(in: ImageOut) -> @location(0) vec4<f32> {
    // The texture uses an sRGB format on sRGB targets: no conversion needed.
    let c = textureSample(image_texture, image_sampler, in.uv);
    return vec4<f32>(c.rgb, c.a * in.alpha);
}
