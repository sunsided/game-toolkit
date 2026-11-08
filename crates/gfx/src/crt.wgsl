// CRT emulation for the Window post stage. Locks scanlines to the rows of the picture: the
// canvas, placed by `post`, or the effect's source texture (`post.source == 1u`), placed by
// `params.src_rect`. Works with integer and fractional scaling alike.

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
    // 1u: only palette indices in `emissive` glow (needs the indexed canvas).
    bloom_from_indices: u32,
    _pad0: f32,
    emissive: array<vec4<u32>, 2>,
    // Beam model: x, y beam height (sigma, in source lines) of black and of full brightness,
    // z horizontal spot blur (sigma, in source pixels), w = 1 when on.
    beam: vec4<f32>,
    // Monochrome: phosphor colour (linear) in rgb, w = 1 when on.
    mono: vec4<f32>,
    // x: corner radius, as a fraction of half the picture height.
    shape: vec4<f32>,
    // With a source texture: where the picture goes (x, y, w, h in pixels of the stage's
    // image) and which texels it shows (x, y, w, h).
    src_rect: vec4<f32>,
    src_region: vec4<f32>,
};

@group(1) @binding(0) var<uniform> params: CrtParams;

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

// The picture: the canvas, or the source texture. Its placement in the stage's image
// (origin, extent, in pixels) and its size in source pixels.
fn pic_origin() -> vec2<f32> {
    return select(post.canvas_origin, params.src_rect.xy, post.source == 1u);
}

fn pic_extent() -> vec2<f32> {
    return select(post.canvas_extent, params.src_rect.zw, post.source == 1u);
}

fn pic_size() -> vec2<f32> {
    return select(post.canvas_size, params.src_region.zw, post.source == 1u);
}

// Window uv of canvas texel coordinate `t` (in canvas pixels).
fn canvas_uv(t: vec2<f32>) -> vec2<f32> {
    let p = t / post.canvas_size;
    return (post.canvas_origin + p * post.canvas_extent) / post.window_size;
}

// The picture at `t` (in picture pixels), bilinear.
fn pic_linear(t: vec2<f32>) -> vec3<f32> {
    if (post.source == 1u) {
        // Half a texel in from the region's edges: no blending with what lies outside it.
        let inside = clamp(t, vec2<f32>(0.5), params.src_region.zw - 0.5);
        let uv = (params.src_region.xy + inside) / vec2<f32>(textureDimensions(source_tex));
        return textureSampleLevel(source_tex, linear_sampler, uv, 0.0).rgb;
    }
    return textureSampleLevel(input_tex, linear_sampler, canvas_uv(t), 0.0).rgb;
}

// One picture pixel, clamped to the picture.
fn pic_texel(t: vec2<i32>) -> vec3<f32> {
    let c = clamp(t, vec2<i32>(0), vec2<i32>(pic_size()) - 1);
    if (post.source == 1u) {
        return textureLoad(source_tex, vec2<i32>(params.src_region.xy) + c, 0).rgb;
    }
    return textureSampleLevel(input_tex, nearest_sampler, canvas_uv(vec2<f32>(c) + 0.5), 0.0).rgb;
}

// "Sharp bilinear": hard pixel edges, a one-window-pixel blend between picture pixels, so
// fractional scales and curvature don't shimmer.
fn sample_sharp(t: vec2<f32>) -> vec3<f32> {
    let scale = max(pic_extent() / pic_size(), vec2<f32>(1.0));
    let f = fract(t);
    let region = (0.5 - 0.5 / scale) * params.sharpness;
    let g = clamp((f - region) / max(1.0 - 2.0 * region, vec2<f32>(1e-4)), vec2<f32>(0.0), vec2<f32>(1.0));
    return pic_linear(floor(t) + g);
}

// One source line at `x`, blurred horizontally like the beam's spot.
fn beam_line(x: f32, row: i32) -> vec3<f32> {
    // No line above the first or below the last: nothing to add there.
    if (row < 0 || row >= i32(pic_size().y)) {
        return vec3<f32>(0.0);
    }
    let sigma = params.beam.z;
    let x0 = floor(x - 0.5);
    var acc = vec3<f32>(0.0);
    var wsum = 0.0;
    for (var i = -3; i <= 4; i = i + 1) {
        let xi = x0 + f32(i);
        let d = (xi + 0.5) - x;
        let w = exp(-0.5 * d * d / (sigma * sigma));
        acc = acc + pic_texel(vec2<i32>(i32(xi), row)) * w;
        wsum = wsum + w;
    }
    return acc / wsum;
}

// Vertical beam profile at distance `d` (in lines) from a line of colour `c`: brighter
// lines are taller.
fn beam_profile(d: f32, c: vec3<f32>) -> vec3<f32> {
    let l = clamp(max(c.r, max(c.g, c.b)), 0.0, 1.0);
    let w = mix(params.beam.x, params.beam.y, sqrt(l));
    return c * exp(-0.5 * (d * d) / (w * w));
}

// The beam model at picture position `t`: the two nearest lines' stripes of light. The gaps
// only show where there are enough output pixels per line for them.
fn beam(t: vec2<f32>, px_per_line: f32) -> vec3<f32> {
    let line = t.y - 0.5;
    let y0 = floor(line);
    let fy = line - y0;
    let a = beam_line(t.x, i32(y0));
    let b = beam_line(t.x, i32(y0) + 1);
    let scan = params.scanline * smoothstep(1.4, 2.6, px_per_line);
    let flat = select(b, a, fy < 0.5);
    let lit = beam_profile(fy, a) + beam_profile(1.0 - fy, b);
    // Make up the average brightness lost to the gaps. Only that: stripes wide enough to
    // overlap are left to add up, so bright areas glow and bleed as on a real tube.
    let avg_w = mix(params.beam.x, params.beam.y, 0.5);
    let norm = 1.0 / clamp(avg_w * 2.5066, 0.35, 1.0);
    return mix(flat, lit * norm, scan);
}

// How much picture texel `t` contributes to bloom: 1 for everything by default, else whether
// its palette index is in the emissive set (canvas only).
fn bloom_weight(t: vec2<f32>) -> f32 {
    if (params.bloom_from_indices == 0u || post.source == 1u) {
        return 1.0;
    }
    let i = index_at(vec2<i32>(floor(t)));
    let word = params.emissive[i / 128u][(i / 32u) % 4u];
    return f32((word >> (i % 32u)) & 1u);
}

fn mask(frag: vec2<f32>) -> vec3<f32> {
    if (params.mask_kind == 0u) {
        return vec3<f32>(1.0);
    }
    let size = max(params.mask_size, 1.0);
    let dim = 1.0 - params.mask_strength;
    var x = frag.x / size;
    var gap = 1.0;
    if (params.mask_kind == 2u) {
        // Shadow mask: triads shift by half a triad on every other row of triads.
        let row = u32(floor(frag.y / size));
        x = x + f32(row % 2u) * 1.5;
    } else if (params.mask_kind == 3u) {
        // Slot mask: every other triad column shifted down by half a slot, short gaps
        // between the slots.
        let col = floor(frag.x / (3.0 * size));
        let slot_h = 4.5 * size;
        let y = frag.y + (col % 2.0) * slot_h * 0.5;
        if (fract(y / slot_h) < 0.22) {
            gap = dim;
        }
    }
    let idx = u32(floor(x)) % 3u;
    var m = vec3<f32>(dim);
    m[idx] = 1.0;
    return m * gap;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let frag = in.pos.xy;
    let extent = pic_extent();
    let p0 = (frag - pic_origin()) / extent;
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
    // Rounded corners.
    let r = params.shape.x;
    if (r > 0.0) {
        let aspect = extent.x / extent.y;
        let q = abs(p - 0.5) * 2.0;
        let e = max(q - vec2<f32>(1.0 - r / aspect, 1.0 - r), vec2<f32>(0.0)) * vec2<f32>(aspect, 1.0);
        if (length(e) > r) {
            return vec4<f32>(0.0, 0.0, 0.0, 1.0);
        }
    }

    let size = pic_size();
    let t = p * size;
    let beam_on = params.beam.w > 0.5;
    var color: vec3<f32>;
    if (beam_on) {
        color = beam(t, extent.y / size.y);
    } else {
        color = sample_sharp(t);
    }

    // Bloom: a small blur of the neighbourhood, added on top.
    if (params.bloom > 0.0) {
        var glow = vec3<f32>(0.0);
        let r = 1.5;
        for (var i = 0; i < 8; i = i + 1) {
            let a = f32(i) * 0.785398;
            let o = vec2<f32>(cos(a), sin(a)) * r;
            glow = glow + pic_linear(t + o) * bloom_weight(t + o);
        }
        color = color + glow / 8.0 * params.bloom;
    }

    // Scanlines without the beam model: darken towards the edge of each row; bright rows
    // bleed wider.
    if (!beam_on) {
        let d = abs(fract(t.y) - 0.5) * 2.0;
        let width = mix(1.0, 0.6, clamp(luma(color), 0.0, 1.0));
        color = color * (1.0 - params.scanline * smoothstep(0.0, 1.0, d * width * 1.4));
    }

    // A monochrome monitor shows brightness in its phosphor colour.
    if (params.mono.w > 0.5) {
        color = vec3<f32>(luma(color)) * params.mono.rgb;
    }
    color = color * mask(frag) * params.brightness;

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
