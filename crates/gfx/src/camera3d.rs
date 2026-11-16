//! 3D camera (perspective or orthographic) producing a column-major `view_proj` for the same camera uniform
//! the 2D path uses. Right-handed, looking down -Z, with wgpu's `[0, 1]` depth range.

use crate::camera::ortho;
use crate::transform::{Mat4, mul};

/// A camera with a `view_proj` matrix, implemented by both [`crate::Camera2D`] and
/// [`Camera3D`] so a pipeline can be fed by either.
pub trait Camera {
    fn view_proj(&self) -> Mat4;
}

/// 3D camera: perspective by default, orthographic when [`Camera3D::ortho_height`] is set.
/// Set [`Camera3D::eye`]/[`Camera3D::target`] to move it.
#[derive(Copy, Clone, Debug)]
pub struct Camera3D {
    pub eye: [f32; 3],
    pub target: [f32; 3],
    pub up: [f32; 3],
    /// Vertical field of view, in radians. Unused while [`Self::ortho_height`] is set.
    pub fov_y: f32,
    /// `Some(h)` switches to an orthographic projection showing `h` world units vertically
    /// (and `h * aspect` horizontally), centred on the view axis: no foreshortening, for
    /// isometric and top-down views. Smaller is more zoomed in. `None` (the default) is
    /// perspective.
    pub ortho_height: Option<f32>,
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
            ortho_height: None,
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
        let proj = match self.ortho_height {
            Some(h) => {
                let hh = h * 0.5;
                let hw = hh * self.aspect;
                ortho(-hw, hw, -hh, hh, self.near, self.far)
            }
            None => perspective(self.fov_y, self.aspect, self.near, self.far),
        };
        mul(&proj, &look_at(self.eye, self.target, self.up))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Project a world-space point to normalized device coordinates.
    fn ndc(cam: &Camera3D, p: [f32; 3]) -> [f32; 3] {
        let m = cam.view_proj();
        let row = |i: usize| m[0][i] * p[0] + m[1][i] * p[1] + m[2][i] * p[2] + m[3][i];
        let w = row(3);
        [row(0) / w, row(1) / w, row(2) / w]
    }

    fn close(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-5)
    }

    #[test]
    fn ortho_has_no_foreshortening_and_fills_the_height() {
        // Default camera sits at z = 5 looking down -z; near 0.1, far 100.
        let mut cam = Camera3D::new(2.0);
        cam.ortho_height = Some(4.0);
        // The top-right corner of the 8x4 view volume, on the near and the far plane.
        assert!(close(ndc(&cam, [4.0, 2.0, 4.9]), [1.0, 1.0, 0.0]));
        assert!(close(ndc(&cam, [4.0, 2.0, -95.0]), [1.0, 1.0, 1.0]));
        // Perspective shrinks the same point with distance.
        cam.ortho_height = None;
        assert!(ndc(&cam, [4.0, 2.0, -95.0])[0] < ndc(&cam, [4.0, 2.0, 4.9])[0]);
    }
}
