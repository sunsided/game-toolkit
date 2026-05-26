struct Camera {
    view_proj: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> camera: Camera;

struct VsIn {
    @location(0) v_pos: vec2<f32>,
    @location(1) v_uv:  vec2<f32>,
    @location(2) i_pos: vec2<f32>,
    @location(3) i_size: vec2<f32>,
    @location(4) i_uv_min: vec2<f32>,
    @location(5) i_uv_max: vec2<f32>,
    @location(6) i_color: vec4<f32>,
    @location(7) i_rot:   f32,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    let local = (in.v_pos - vec2<f32>(0.5)) * in.i_size;
    let c = cos(in.i_rot);
    let s = sin(in.i_rot);
    let rotated = vec2<f32>(c * local.x - s * local.y, s * local.x + c * local.y);
    let world = in.i_pos + in.i_size * 0.5 + rotated;

    var out: VsOut;
    out.clip  = camera.view_proj * vec4<f32>(world, 0.0, 1.0);
    out.uv    = mix(in.i_uv_min, in.i_uv_max, in.v_uv);
    out.color = in.i_color;
    return out;
}

@group(1) @binding(0) var t_atlas: texture_2d<f32>;
@group(1) @binding(1) var s_atlas: sampler;

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(t_atlas, s_atlas, in.uv) * in.color;
}
