struct Camera { view_proj: mat4x4<f32> };
@group(0) @binding(0) var<uniform> camera: Camera;

struct VsIn {
    @location(0) v_pos: vec2<f32>,        // unit quad 0..1
    @location(2) i_center: vec2<f32>,
    @location(3) i_radius: f32,
    @location(4) i_thickness: f32,        // 0 = filled disk; >0 = ring (outer = radius)
    @location(5) i_color: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) local: vec2<f32>,        // in pixel space, centered
    @location(1) radius: f32,
    @location(2) thickness: f32,
    @location(3) color: vec4<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    let r = in.i_radius + 1.0;            // expand by 1px for antialiased edge
    let local = (in.v_pos - vec2<f32>(0.5)) * vec2<f32>(2.0 * r);
    let world = in.i_center + local;
    var out: VsOut;
    out.clip = camera.view_proj * vec4<f32>(world, 0.0, 1.0);
    out.local = local;
    out.radius = in.i_radius;
    out.thickness = in.i_thickness;
    out.color = in.i_color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let d = length(in.local);
    let aa = 1.0;
    var alpha: f32;
    if (in.thickness <= 0.0) {
        // filled disk
        alpha = 1.0 - smoothstep(in.radius - aa, in.radius + aa, d);
    } else {
        // ring: keep band between (radius - thickness) .. radius
        let outer = 1.0 - smoothstep(in.radius - aa, in.radius + aa, d);
        let inner = smoothstep(in.radius - in.thickness - aa, in.radius - in.thickness + aa, d);
        alpha = outer * inner;
    }
    if (alpha <= 0.0) { discard; }
    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}
