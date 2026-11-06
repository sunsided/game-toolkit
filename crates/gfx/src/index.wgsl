// Palette indices on the indexed canvas. The canvas is an sRGB colour target and the index
// rides in its red channel, so a pass that writes indices returns `index_out(i)` (the
// linear value the hardware encodes back to byte `i`), and one that reads the canvas turns a
// loaded texel back into the index with `index_in`. Write with blending off (or alpha 1)
// so indices are never mixed.

fn index_out(i: u32) -> vec4<f32> {
    let c = f32(min(i, 255u)) / 255.0;
    var lin = c / 12.92;
    if (c > 0.04045) {
        lin = pow((c + 0.055) / 1.055, 2.4);
    }
    return vec4<f32>(lin, 0.0, 0.0, 1.0);
}

fn index_in(texel: vec4<f32>) -> u32 {
    let c = texel.r;
    var enc = c * 12.92;
    if (c > 0.0031308) {
        enc = 1.055 * pow(c, 1.0 / 2.4) - 0.055;
    }
    return u32(clamp(round(enc * 255.0), 0.0, 255.0));
}
