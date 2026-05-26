use crate::graphics::Graphics;
use crate::painter::Painter;

pub struct Frame {
    pub(crate) encoder: Option<wgpu::CommandEncoder>,
    pub(crate) view: wgpu::TextureView,
    pub(crate) surface_texture: Option<wgpu::SurfaceTexture>,
    pub clear_color: [f32; 4],
    pub(crate) flushed: bool,
}

impl Frame {
    /// Begin a 2D rendering pass. Returns a [`Painter`] that flushes on drop.
    pub fn painter<'a>(&'a mut self, gfx: &'a mut Graphics) -> Painter<'a> {
        Painter::new(self, gfx)
    }

    pub fn encoder_mut(&mut self) -> Option<&mut wgpu::CommandEncoder> {
        self.encoder.as_mut()
    }

    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    /// Split borrow of the view and encoder so overlays (egui, custom passes) can record
    /// into the same frame without fighting the borrow checker.
    pub fn view_and_encoder(&mut self) -> (&wgpu::TextureView, &mut wgpu::CommandEncoder) {
        (
            &self.view,
            self.encoder
                .as_mut()
                .expect("frame encoder taken/dropped before view_and_encoder"),
        )
    }
}
