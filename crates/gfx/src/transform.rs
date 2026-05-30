//! Minimal column-major 4x4 matrix helpers for building 3D model transforms.
//!
//! Column-major matches WGSL's `mat4x4<f32>`: each inner `[f32; 4]` is a column, and the
//! shader applies a matrix as `m * vec4(pos, 1.0)`.

/// Column-major 4x4 matrix.
pub type Mat4 = [[f32; 4]; 4];

pub fn identity() -> Mat4 {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

/// Matrix product `a * b` (the transform `b` is applied first, then `a`).
pub fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut out = [[0.0f32; 4]; 4];
    for (col, out_col) in out.iter_mut().enumerate() {
        for (row, slot) in out_col.iter_mut().enumerate() {
            let mut s = 0.0;
            for k in 0..4 {
                s += a[k][row] * b[col][k];
            }
            *slot = s;
        }
    }
    out
}

pub fn translation(t: [f32; 3]) -> Mat4 {
    let mut m = identity();
    m[3] = [t[0], t[1], t[2], 1.0];
    m
}

pub fn scale(s: [f32; 3]) -> Mat4 {
    [
        [s[0], 0.0, 0.0, 0.0],
        [0.0, s[1], 0.0, 0.0],
        [0.0, 0.0, s[2], 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

pub fn rotation_x(angle: f32) -> Mat4 {
    let (s, c) = angle.sin_cos();
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, c, s, 0.0],
        [0.0, -s, c, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

pub fn rotation_y(angle: f32) -> Mat4 {
    let (s, c) = angle.sin_cos();
    [
        [c, 0.0, -s, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [s, 0.0, c, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

pub fn rotation_z(angle: f32) -> Mat4 {
    let (s, c) = angle.sin_cos();
    [
        [c, s, 0.0, 0.0],
        [-s, c, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}
