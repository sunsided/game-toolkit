use crate::graphics::Graphics;
use crate::painter::Painter;

pub struct Frame {
    pub(crate) encoder: Option<wgpu::CommandEncoder>,
    pub(crate) view: wgpu::TextureView,
    pub(crate) surface_texture: Option<wgpu::SurfaceTexture>,
    /// The virtual-resolution canvas the painter draws into, if any.
    pub(crate) canvas: Option<wgpu::TextureView>,
    /// Size of `canvas`, and whether it is a virtual-resolution canvas (else window-sized).
    pub(crate) canvas_size: [u32; 2],
    pub(crate) canvas_virtual: bool,
    pub clear_color: [f32; 4],
    pub(crate) flushed: bool,
    /// Whether the canvas has been scaled onto the window this frame.
    pub(crate) composited: bool,
    /// Whether indexed colour was on when the frame began (the canvas holds indices).
    pub(crate) indexed: bool,
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
    ///
    /// With indexed colour on ([`Self::target_is_indexed`]) this is the index canvas: it holds
    /// palette indices, not colours. Write them with `index_out` from
    /// `INDEX_WGSL` (feature `indexed`); an RGBA pass would write meaningless indices. Colour
    /// effects belong in a [`PostEffect`](crate::PostEffect), which sees the resolved colours.
    pub fn target_view(&self) -> &wgpu::TextureView {
        self.canvas.as_ref().unwrap_or(&self.view)
    }

    /// Whether [`Self::target_view`] holds palette indices (indexed colour was on when the
    /// frame began) rather than colours. Always `false` without the `indexed` feature.
    pub fn target_is_indexed(&self) -> bool {
        self.indexed
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
