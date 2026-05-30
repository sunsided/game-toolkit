struct Camera {
    view_proj: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> camera: Camera;

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    // Per-instance model matrix (4 columns) + tint.
    @location(2) m0: vec4<f32>,
    @location(3) m1: vec4<f32>,
    @location(4) m2: vec4<f32>,
    @location(5) m3: vec4<f32>,
    @location(6) color: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    let model = mat4x4<f32>(in.m0, in.m1, in.m2, in.m3);
    let world = model * vec4<f32>(in.pos, 1.0);

    var out: VsOut;
    out.clip = camera.view_proj * world;
    // Rotate the normal by the model's upper-left 3x3 (assumes uniform scale).
    out.normal = mat3x3<f32>(in.m0.xyz, in.m1.xyz, in.m2.xyz) * in.normal;
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let l = normalize(vec3<f32>(0.4, 0.8, 0.6));
    let diff = max(dot(n, l), 0.0);
    let lit = 0.25 + 0.75 * diff;
    return vec4<f32>(in.color.rgb * lit, in.color.a);
}
