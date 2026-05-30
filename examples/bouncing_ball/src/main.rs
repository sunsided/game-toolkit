use std::f32::consts::TAU;

use game_toolkit::prelude::*;

/// Beachball longitude panel colors (one per wedge running pole to pole).
const PANELS: [[f32; 3]; 6] = [
    [0.92, 0.20, 0.20], // red
    [0.97, 0.55, 0.15], // orange
    [0.96, 0.85, 0.20], // yellow
    [0.25, 0.70, 0.30], // green
    [0.20, 0.50, 0.90], // blue
    [0.70, 0.30, 0.80], // violet
];

/// Light direction in view space (x right, y down, z toward viewer): upper-left, front.
const LIGHT: [f32; 3] = [-0.50, -0.60, 0.62];

struct BouncingBall {
    pos: [f32; 2],
    vel: [f32; 2],
    size: [f32; 2],
    /// Tumble angles about the view-space X and Y axes, in radians.
    rot_x: f32,
    rot_y: f32,
}

impl Game for BouncingBall {
    fn init(_ctx: &mut Context) -> Result<Self> {
        Ok(Self {
            pos: [100.0, 100.0],
            vel: [320.0, 240.0],
            size: [72.0, 72.0],
            rot_x: 0.4,
            rot_y: 0.0,
        })
    }

    fn update(&mut self, ctx: &mut Context, dt: f32) {
        if ctx.input.key_pressed(Key::Escape) {
            ctx.quit();
        }

        self.pos[0] += self.vel[0] * dt;
        self.pos[1] += self.vel[1] * dt;

        let (w, h) = ctx.gfx.size();
        let (w, h) = (w as f32, h as f32);
        if self.pos[0] < 0.0 {
            self.pos[0] = 0.0;
            self.vel[0] = self.vel[0].abs();
        }
        if self.pos[0] + self.size[0] > w {
            self.pos[0] = w - self.size[0];
            self.vel[0] = -self.vel[0].abs();
        }
        if self.pos[1] < 0.0 {
            self.pos[1] = 0.0;
            self.vel[1] = self.vel[1].abs();
        }
        if self.pos[1] + self.size[1] > h {
            self.pos[1] = h - self.size[1];
            self.vel[1] = -self.vel[1].abs();
        }

        // Tumble: each screen axis of motion rolls the ball about the perpendicular
        // view axis. Because the two angles advance at different rates the rotation
        // axis keeps changing, so the ball tumbles rather than spinning flat. The
        // signs follow velocity, so spin reverses on every bounce.
        let r = self.size[0] * 0.5;
        self.rot_y += (self.vel[0] / r) * dt;
        self.rot_x += (self.vel[1] / r) * dt;
    }

    fn render(&mut self, ctx: &mut Context, frame: &mut Frame) {
        let mut p = frame.painter(&mut ctx.gfx);
        p.clear([0.05, 0.06, 0.09, 1.0]);

        let r = self.size[0] * 0.5;
        let center = [self.pos[0] + r, self.pos[1] + r];
        let rot = rot_matrix(self.rot_x, self.rot_y);

        // Ray-cast the front hemisphere: for each screen sample inside the disc,
        // rebuild the surface point, rotate it into body space to choose a panel,
        // then shade it by its view-space normal. One small rect per sample.
        // Sprite-batched rects sit under the primitive rim drawn afterwards.
        let step = 1.5;
        let inv_r = 1.0 / r;
        let n = (2.0 * r / step).ceil() as i32;
        for iy in 0..=n {
            let dy = -r + iy as f32 * step;
            for ix in 0..=n {
                let dx = -r + ix as f32 * step;
                let zz = r * r - dx * dx - dy * dy;
                if zz <= 0.0 {
                    continue; // outside the silhouette
                }
                let zc = zz.sqrt();

                // View-space surface normal (unit) and the body-space point (length r).
                let nrm = [dx * inv_r, dy * inv_r, zc * inv_r];
                let body = transpose_apply(&rot, [dx, dy, zc]);

                let base = panel_color(body, r);
                let mut color = shade(base, nrm);
                // Feather the silhouette: fade alpha to 0 over the last ~1.2px so
                // the blocky edge anti-aliases into the background instead of
                // ending on a hard, jagged rim. Rects blend with alpha.
                let d = (dx * dx + dy * dy).sqrt();
                color[3] = ((r - d) / 1.2).clamp(0.0, 1.0);

                // Center the cell on its sample so it tiles exactly; a top-left
                // anchor would bias every cell down-right and leak a fringe past
                // the silhouette. Size slightly over `step` to avoid seams.
                let cell = step + 0.3;
                let half = cell * 0.5;
                p.rect([center[0] + dx - half, center[1] + dy - half], [cell, cell], color);
            }
        }
    }
}

/// Pick the beachball surface color for a body-space point: longitude wedges with
/// white caps at the two poles.
fn panel_color(body: [f32; 3], r: f32) -> [f32; 3] {
    let lat = (body[1] / r).clamp(-1.0, 1.0);
    if lat.abs() > 0.82 {
        return [1.0, 1.0, 1.0]; // polar cap
    }
    let lon = body[0].atan2(body[2]); // [-PI, PI]
    let idx = (((lon / TAU) + 0.5) * PANELS.len() as f32).floor() as i32;
    let idx = idx.rem_euclid(PANELS.len() as i32) as usize;
    PANELS[idx]
}

/// Lambert diffuse + ambient with a tight specular highlight, from the view normal.
fn shade(base: [f32; 3], nrm: [f32; 3]) -> [f32; 4] {
    let l = normalize(LIGHT);
    let diff = dot(nrm, l).max(0.0);
    let lit = 0.30 + 0.70 * diff;
    let spec = diff.powf(28.0) * 0.7;
    [
        (base[0] * lit + spec).min(1.0),
        (base[1] * lit + spec).min(1.0),
        (base[2] * lit + spec).min(1.0),
        1.0,
    ]
}

/// Rotation matrix R = Rx(rx) * Ry(ry), returned as three row vectors.
fn rot_matrix(rx: f32, ry: f32) -> [[f32; 3]; 3] {
    let (sx, cx) = rx.sin_cos();
    let (sy, cy) = ry.sin_cos();
    // Rx * Ry
    [
        [cy, 0.0, sy],
        [sx * sy, cx, -sx * cy],
        [-cx * sy, sx, cx * cy],
    ]
}

/// Apply the transpose (inverse, for a rotation) of `m` to vector `v`.
fn transpose_apply(m: &[[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * v[0] + m[1][0] * v[1] + m[2][0] * v[2],
        m[0][1] * v[0] + m[1][1] * v[1] + m[2][1] * v[2],
        m[0][2] * v[0] + m[1][2] * v[1] + m[2][2] * v[2],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = dot(v, v).sqrt().max(1e-6);
    [v[0] / len, v[1] / len, v[2] / len]
}

fn main() -> Result<()> {
    env_logger::init();
    run::<BouncingBall>(AppConfig {
        title: "bouncing_ball".into(),
        width: 800,
        height: 600,
        ..Default::default()
    })
}
