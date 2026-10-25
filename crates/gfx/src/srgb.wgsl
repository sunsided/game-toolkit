// Shared helpers, prepended to the shaders that need them: each one's module is built from
// `concat!(include_str!("srgb.wgsl"), include_str!("<shader>.wgsl"))`, so there is one copy.

// Painter colors are sRGB (what a color picker shows); the frame is an sRGB target that
// expects linear output, so decode them before use.
fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

