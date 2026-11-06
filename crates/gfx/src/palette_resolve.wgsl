// Turns the indexed canvas (palette index in the red channel) into colour through the
// palette lookup texture, applying palette cycles and the crossfade between two rows.

struct Resolve {
    row_a: u32,
    row_b: u32,
    blend: f32,
    cycle_count: u32,
    cycles: array<vec4<u32>, 16>,
};

@group(0) @binding(0) var indices: texture_2d<f32>;
@group(0) @binding(1) var palette: texture_2d<f32>;
@group(0) @binding(2) var<uniform> r: Resolve;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VsOut {
    let x = f32((idx << 1u) & 2u);
    let y = f32(idx & 2u);
    var out: VsOut;
    out.pos = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    var i = index_in(textureLoad(indices, vec2<i32>(in.pos.xy), 0));
    for (var k = 0u; k < r.cycle_count; k = k + 1u) {
        let c = r.cycles[k];
        if (i >= c.x && i < c.x + c.y) {
            i = c.x + (i - c.x + c.z) % c.y;
            break;
        }
    }
    let a = textureLoad(palette, vec2<i32>(i32(i), i32(r.row_a)), 0);
    let b = textureLoad(palette, vec2<i32>(i32(i), i32(r.row_b)), 0);
    return vec4<f32>(mix(a, b, r.blend).rgb, 1.0);
}
