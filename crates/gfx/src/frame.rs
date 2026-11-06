use crate::graphics::Graphics;
use crate::painter::Painter;

pub struct Frame {
    pub(crate) encoder: Option<wgpu::CommandEncoder>,
    pub(crate) view: wgpu::TextureView,
    pub(crate) surface_texture: Option<wgpu::SurfaceTexture>,
    /// The virtual-resolution canvas the painter draws into, if any.
    pub(crate) canvas: Option<wgpu::TextureView>,
    pub clear_color: [f32; 4],
    pub(crate) flushed: bool,
    /// Whether the canvas has been scaled onto the window this frame.
    pub(crate) composited: bool,
}

impl Frame {
    /// Begin a 2D rendering pass. Returns a [`Painter`] that flushes on drop.
    pub fn painter<'a>(&'a mut self, gfx: &'a mut Graphics) -> Painter<'a> {
        Painter::new(self, gfx)
    }

    pub fn encoder_mut(&mut self) -> Option<&mut wgpu::CommandEncoder> {
        self.encoder.as_mut()
    }

    /// The window's (surface's) view. With a virtual resolution the painter draws into the
    /// canvas instead ([`Self::target_view`]), which is scaled onto this view by
    /// [`Graphics::flush_pending`] or at the end of the frame.
    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    /// The view the painter draws into: the virtual-resolution canvas when one is set, else
    /// the window's view.
    pub fn target_view(&self) -> &wgpu::TextureView {
        self.canvas.as_ref().unwrap_or(&self.view)
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
