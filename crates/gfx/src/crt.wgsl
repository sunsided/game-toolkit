// CRT emulation for the Window post stage. Locks scanlines to canvas rows using the canvas
// placement in `post`, so it works with integer and fractional scaling alike.

struct CrtParams {
    scanline: f32,
    mask_strength: f32,
    mask_kind: u32,
    mask_size: f32,
    bloom: f32,
    persistence: f32,
    curvature: f32,
    vignette: f32,
    brightness: f32,
    sharpness: f32,
    _pad0: f32,
    _pad1: f32,
};

@group(1) @binding(0) var<uniform> params: CrtParams;

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

// Window uv of canvas texel coordinate `t` (in canvas pixels).
fn canvas_uv(t: vec2<f32>) -> vec2<f32> {
    let p = t / post.canvas_size;
    return (post.canvas_origin + p * post.canvas_extent) / post.window_size;
}

// "Sharp bilinear": hard pixel edges, a one-window-pixel blend between canvas pixels, so
// fractional scales and curvature don't shimmer.
fn sample_sharp(t: vec2<f32>) -> vec3<f32> {
    let scale = max(post.canvas_extent / post.canvas_size, vec2<f32>(1.0));
    let f = fract(t);
    let region = (0.5 - 0.5 / scale) * params.sharpness;
    let g = clamp((f - region) / max(1.0 - 2.0 * region, vec2<f32>(1e-4)), vec2<f32>(0.0), vec2<f32>(1.0));
    return textureSampleLevel(input_tex, linear_sampler, canvas_uv(floor(t) + g), 0.0).rgb;
}

fn mask(frag: vec2<f32>) -> vec3<f32> {
    if (params.mask_kind == 0u) {
        return vec3<f32>(1.0);
    }
    let size = max(params.mask_size, 1.0);
    var x = frag.x / size;
    if (params.mask_kind == 2u) {
        // Shadow mask: triads shift by half a triad on every other row of triads.
        let row = u32(floor(frag.y / size));
        x = x + f32(row % 2u) * 1.5;
    }
    let idx = u32(floor(x)) % 3u;
    let dim = 1.0 - params.mask_strength;
    var m = vec3<f32>(dim);
    m[idx] = 1.0;
    return m;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let frag = in.pos.xy;
    let p0 = (frag - post.canvas_origin) / post.canvas_extent;
    if (any(p0 < vec2<f32>(0.0)) || any(p0 > vec2<f32>(1.0))) {
        // The border: leave it as it is.
        return textureSampleLevel(input_tex, nearest_sampler, in.uv, 0.0);
    }

    // Barrel distortion.
    var c = p0 * 2.0 - 1.0;
    c = c + c * (c.yx * c.yx) * params.curvature;
    let p = c * 0.5 + 0.5;
    if (any(p < vec2<f32>(0.0)) || any(p > vec2<f32>(1.0))) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }

    let t = p * post.canvas_size;
    var color = sample_sharp(t);

    // Bloom: a small blur of the neighbourhood, added on top.
    if (params.bloom > 0.0) {
        var glow = vec3<f32>(0.0);
        let r = 1.5;
        for (var i = 0; i < 8; i = i + 1) {
            let a = f32(i) * 0.785398;
            let o = vec2<f32>(cos(a), sin(a)) * r;
            glow = glow + textureSampleLevel(input_tex, linear_sampler, canvas_uv(t + o), 0.0).rgb;
        }
        color = color + glow / 8.0 * params.bloom;
    }

    // Scanlines: darken towards the edge of each canvas row; bright rows bleed wider.
    let d = abs(fract(t.y) - 0.5) * 2.0;
    let width = mix(1.0, 0.6, clamp(luma(color), 0.0, 1.0));
    let scan = 1.0 - params.scanline * smoothstep(0.0, 1.0, d * width * 1.4);
    color = color * scan * mask(frag) * params.brightness;

    // Vignette.
    let v = clamp(16.0 * p.x * p.y * (1.0 - p.x) * (1.0 - p.y), 0.0, 1.0);
    color = color * mix(1.0, pow(v, 0.3), params.vignette);

    // Phosphor persistence: the previous output fades instead of vanishing.
    if (params.persistence > 0.0) {
        let prev = textureSampleLevel(history_tex, nearest_sampler, in.uv, 0.0).rgb;
        color = max(color, prev * params.persistence);
    }
    return vec4<f32>(color, 1.0);
}
