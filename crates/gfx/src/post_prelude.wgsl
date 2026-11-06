// game-toolkit post-effect prelude: prepended to every ShaderEffect.

struct PostGlobals {
    size: vec2<f32>,
    canvas_size: vec2<f32>,
    window_size: vec2<f32>,
    canvas_origin: vec2<f32>,
    canvas_extent: vec2<f32>,
    time: f32,
    frame: u32,
    // 1u when `indices_tex` holds the indexed canvas's palette indices.
    indexed: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};

@group(0) @binding(0) var input_tex: texture_2d<f32>;
@group(0) @binding(1) var nearest_sampler: sampler;
@group(0) @binding(2) var linear_sampler: sampler;
@group(0) @binding(3) var<uniform> post: PostGlobals;
@group(0) @binding(4) var history_tex: texture_2d<f32>;
@group(0) @binding(5) var indices_tex: texture_2d<f32>;

// The palette index at canvas pixel `p` (before cycling), with indexed colour on
// (`post.indexed == 1u`). `index_in` comes from the index helpers appended to this prelude.
fn index_at(p: vec2<i32>) -> u32 {
    let size = vec2<i32>(textureDimensions(indices_tex));
    return index_in(textureLoad(indices_tex, clamp(p, vec2<i32>(0), size - 1), 0));
}

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn post_vs_main(@builtin(vertex_index) idx: u32) -> VsOut {
    let x = f32((idx << 1u) & 2u);
    let y = f32(idx & 2u);
    var out: VsOut;
    out.uv = vec2<f32>(x, y);
    out.pos = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    return out;
}
