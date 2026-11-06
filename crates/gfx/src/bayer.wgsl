// Ordered (Bayer) dithering thresholds in 0..1. Pass the pixel position plus an offset that
// moves with the world (e.g. the camera's position in canvas pixels), so the pattern stays
// put on the scene instead of crawling when the camera moves:
//   let t = bayer4(vec2<i32>(in.pos.xy) + offset);
//   let lit = value > t;
fn bayer4(p: vec2<i32>) -> f32 {
    let m = array<u32, 16>(0u, 8u, 2u, 10u, 12u, 4u, 14u, 6u, 3u, 11u, 1u, 9u, 15u, 7u, 13u, 5u);
    let q = vec2<u32>(p & vec2<i32>(3));
    return (f32(m[q.y * 4u + q.x]) + 0.5) / 16.0;
}

fn bayer8(p: vec2<i32>) -> f32 {
    // Built from the 4x4 matrix: 4 * M4(p / 2) + M2(p % 2).
    let q = vec2<u32>(p & vec2<i32>(7));
    let m2 = array<u32, 4>(0u, 2u, 3u, 1u);
    let m4 = array<u32, 16>(0u, 8u, 2u, 10u, 12u, 4u, 14u, 6u, 3u, 11u, 1u, 9u, 15u, 7u, 13u, 5u);
    let hi = m4[(q.y >> 1u) * 4u + (q.x >> 1u)];
    let lo = m2[(q.y & 1u) * 2u + (q.x & 1u)];
    return (f32(hi * 4u + lo) + 0.5) / 64.0;
}
