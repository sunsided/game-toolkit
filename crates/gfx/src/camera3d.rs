//! Perspective 3D camera producing a column-major `view_proj` for the same camera uniform
//! the 2D path uses. Right-handed, looking down -Z, with wgpu's `[0, 1]` depth range.

use crate::transform::{mul, Mat4};

/// A camera with a `view_proj` matrix, implemented by both [`crate::Camera2D`] and
/// [`Camera3D`] so a pipeline can be fed by either.
pub trait Camera {
    fn view_proj(&self) -> Mat4;
}

/// Perspective camera. Set [`Camera3D::eye`]/[`Camera3D::target`] to move it.
#[derive(Copy, Clone, Debug)]
pub struct Camera3D {
    pub eye: [f32; 3],
    pub target: [f32; 3],
    pub up: [f32; 3],
    /// Vertical field of view, in radians.
    pub fov_y: f32,
    pub aspect: f32,
    pub near: f32,
    pub far: f32,
}

impl Camera3D {
    pub fn new(aspect: f32) -> Self {
        Self {
            eye: [0.0, 0.0, 5.0],
            target: [0.0, 0.0, 0.0],
            up: [0.0, 1.0, 0.0],
            fov_y: 60.0_f32.to_radians(),
            aspect,
            near: 0.1,
            far: 100.0,
        }
    }

    pub fn resize(&mut self, w: f32, h: f32) {
        if h > 0.0 {
            self.aspect = w / h;
        }
    }
}

impl Camera for Camera3D {
    fn view_proj(&self) -> Mat4 {
        mul(
            &perspective(self.fov_y, self.aspect, self.near, self.far),
            &look_at(self.eye, self.target, self.up),
        )
    }
}

impl Camera for crate::Camera2D {
    fn view_proj(&self) -> Mat4 {
        crate::Camera2D::view_proj(self)
    }
}

/// Right-handed perspective with a `[0, 1]` depth range (wgpu/Vulkan/Metal convention).
fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fov_y * 0.5).tan();
    [
        [f / aspect, 0.0, 0.0, 0.0],
        [0.0, f, 0.0, 0.0],
        [0.0, 0.0, far / (near - far), -1.0],
        [0.0, 0.0, (near * far) / (near - far), 0.0],
    ]
}

/// Right-handed look-at view matrix.
fn look_at(eye: [f32; 3], target: [f32; 3], up: [f32; 3]) -> Mat4 {
    let z = normalize(sub(eye, target)); // forward is -z, so this points back toward the eye
    let x = normalize(cross(up, z));
    let y = cross(z, x);
    [
        [x[0], y[0], z[0], 0.0],
        [x[1], y[1], z[1], 0.0],
        [x[2], y[2], z[2], 0.0],
        [-dot(x, eye), -dot(y, eye), -dot(z, eye), 1.0],
    ]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = dot(v, v).sqrt().max(1e-6);
    [v[0] / len, v[1] / len, v[2] / len]
}
