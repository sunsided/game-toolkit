// Composites the vello-rendered offscreen texture onto the surface with a fullscreen
// triangle. The vello target holds premultiplied-alpha, sRGB-encoded color, so the pipeline
// blends with PREMULTIPLIED_ALPHA_BLENDING and the shader decodes to linear for the sRGB
// frame target (which encodes it again on write).

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VsOut {
    // Oversized fullscreen triangle; uv runs 0..2 and covers the screen over 0..1.
    let x = f32((idx << 1u) & 2u);
    let y = f32(idx & 2u);
    var out: VsOut;
    out.uv = vec2<f32>(x, y);
    out.pos = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, in.uv);
    if (c.a <= 0.0) {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(srgb_to_linear(c.rgb / c.a) * c.a, c.a);
}
