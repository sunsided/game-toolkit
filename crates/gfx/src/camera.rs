use bytemuck::{Pod, Zeroable};

/// 2D orthographic camera. Default coordinate system: top-left origin, pixels.
#[derive(Copy, Clone, Debug)]
pub struct Camera2D {
    pub center: [f32; 2],
    pub zoom: f32,
    pub viewport: [f32; 2],
    /// If true, +Y points down (top-left origin). Matches typical 2D screen coords.
    pub y_down: bool,
}

impl Camera2D {
    pub fn new(viewport_w: f32, viewport_h: f32) -> Self {
        Self {
            center: [viewport_w * 0.5, viewport_h * 0.5],
            zoom: 1.0,
            viewport: [viewport_w, viewport_h],
            y_down: true,
        }
    }

    pub fn resize(&mut self, w: f32, h: f32) {
        // Keep the top-left world coordinate anchored so screen pixels keep
        // mapping to the same world origin as the window grows or shrinks.
        // Without this, `center` stays at the old viewport's midpoint and the
        // projection drifts: the visible edges no longer line up with world
        // [0, size], which makes pixel-space content appear off-centre after a
        // resize. A follow camera overwrites `center` each frame, so it is
        // unaffected.
        let z = self.zoom.max(1e-4);
        let top_left = [
            self.center[0] - self.viewport[0] * 0.5 / z,
            self.center[1] - self.viewport[1] * 0.5 / z,
        ];
        self.viewport = [w, h];
        self.center = [top_left[0] + w * 0.5 / z, top_left[1] + h * 0.5 / z];
    }

    pub fn view_proj(&self) -> [[f32; 4]; 4] {
        let z = self.zoom.max(1e-4);
        let hw = self.viewport[0] * 0.5 / z;
        let hh = self.viewport[1] * 0.5 / z;
        let l = self.center[0] - hw;
        let r = self.center[0] + hw;
        let (b, t) = if self.y_down {
            (self.center[1] + hh, self.center[1] - hh)
        } else {
            (self.center[1] - hh, self.center[1] + hh)
        };
        ortho(l, r, b, t, -1.0, 1.0)
    }
}

fn ortho(l: f32, r: f32, b: f32, t: f32, n: f32, f: f32) -> [[f32; 4]; 4] {
    let rml = r - l;
    let tmb = t - b;
    let fmn = f - n;
    [
        [2.0 / rml, 0.0, 0.0, 0.0],
        [0.0, 2.0 / tmb, 0.0, 0.0],
        [0.0, 0.0, -1.0 / fmn, 0.0],
        [-(r + l) / rml, -(t + b) / tmb, -n / fmn, 1.0],
    ]
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub(crate) struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
}
