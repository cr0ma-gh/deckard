// Draws the core frame into the current viewport (set by the renderer to the
// letterboxed area). One full-screen triangle, no vertex buffer.

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VsOut {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: VsOut;
    out.position = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    out.uv = uv;
    return out;
}

struct Params {
    source_size: vec2<f32>,
    target_size: vec2<f32>,
    mode: u32,
    curvature: f32,
    scanline_weight: f32,
    mask_strength: f32,
};

@group(0) @binding(0) var frame_texture: texture_2d<f32>;
@group(0) @binding(1) var frame_sampler: sampler;
@group(0) @binding(2) var<uniform> params: Params;

const PI: f32 = 3.141592653589793;

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Mode 0: None (direct output)
    if (params.mode == 0u) {
        return vec4<f32>(textureSample(frame_texture, frame_sampler, in.uv).rgb, 1.0);
    }

    var uv = in.uv;

    // Mode 2: CRT (with barrel distortion/curvature)
    if (params.mode == 2u && params.curvature > 0.0) {
        let c = uv - vec2<f32>(0.5, 0.5);
        let r2 = dot(c, c);
        uv = uv + c * (r2 * params.curvature);
        // Black border outside curved tube
        if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
            return vec4<f32>(0.0, 0.0, 0.0, 1.0);
        }
    }

    let col = textureSample(frame_texture, frame_sampler, uv).rgb;

    // Scanlines (based on native source frame lines)
    let line = fract(uv.y * params.source_size.y);
    let scan = 1.0 - params.scanline_weight * 0.5 * (1.0 - cos(line * 2.0 * PI));
    var out_rgb = col * scan;

    // Mode 2 additional features: RGB phosphor mask + vignette
    if (params.mode == 2u) {
        // RGB phosphor triad mask based on viewport screen pixel coordinates
        let mask_col = u32(in.position.x) % 3u;
        var mask = vec3<f32>(1.0 - params.mask_strength);
        if (mask_col == 0u) {
            mask.r = 1.0;
        } else if (mask_col == 1u) {
            mask.g = 1.0;
        } else {
            mask.b = 1.0;
        }

        // Vignette (smooth corner darkening)
        let vig = clamp(16.0 * uv.x * (1.0 - uv.x) * uv.y * (1.0 - uv.y), 0.0, 1.0);
        let vig_factor = pow(vig, 0.12);

        // Compensate for brightness lost to scanlines and mask
        out_rgb = out_rgb * mask * vig_factor * 1.25;
    }

    return vec4<f32>(out_rgb, 1.0);
}
