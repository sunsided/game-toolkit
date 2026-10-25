struct Camera { view_proj: mat4x4<f32> };
@group(0) @binding(0) var<uniform> camera: Camera;

@group(1) @binding(0) var t_tex: texture_2d<f32>;
@group(1) @binding(1) var s_tex: sampler;

struct VsIn {
    @location(0) pos: vec2<f32>,
    @location(1) uvq: vec3<f32>,          // (u*q, v*q, q); q = 1 for affine UVs
    @location(2) color: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uvq: vec3<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = camera.view_proj * vec4<f32>(in.pos, 0.0, 1.0);
    out.uvq = in.uvq;
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Dividing after interpolation makes UVs perspective-correct for quads (see quad_uvq).
    // Decoding after interpolation blends gradients in sRGB, as CSS gradients do.
    let color = vec4<f32>(srgb_to_linear(in.color.rgb), in.color.a);
    return textureSample(t_tex, s_tex, in.uvq.xy / in.uvq.z) * color;
}

// Painter colors are sRGB (what a color picker shows); the frame is an sRGB target that
// expects linear output, so decode them before use.
fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}
