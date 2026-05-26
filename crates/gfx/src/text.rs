use anyhow::Result;
use glyphon::{
    Attrs, Buffer, Cache, Color, ColorMode, FontSystem, Metrics, Resolution, Shaping, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};

#[derive(Clone)]
struct Queued {
    text: String,
    left: f32,
    top: f32,
    size_px: f32,
    color: [u8; 4],
}

pub(crate) struct TextSystem {
    font_system: FontSystem,
    swash_cache: SwashCache,
    #[allow(dead_code)]
    cache: Cache,
    atlas: TextAtlas,
    viewport: Viewport,
    renderer: TextRenderer,
    queued: Vec<Queued>,
}

impl TextSystem {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        let font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let cache = Cache::new(device);
        let mut atlas = TextAtlas::with_color_mode(
            device,
            queue,
            &cache,
            surface_format,
            ColorMode::Accurate,
        );
        let mut viewport = Viewport::new(device, &cache);
        viewport.update(queue, Resolution { width, height });
        let renderer = TextRenderer::new(
            &mut atlas,
            device,
            wgpu::MultisampleState::default(),
            None,
        );
        Self {
            font_system,
            swash_cache,
            cache,
            atlas,
            viewport,
            renderer,
            queued: Vec::new(),
        }
    }

    pub fn resize(&mut self, queue: &wgpu::Queue, width: u32, height: u32) {
        self.viewport.update(queue, Resolution { width, height });
    }

    pub fn queue(&mut self, text: &str, pos: [f32; 2], size_px: f32, color: [f32; 4]) {
        let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0) as u8;
        self.queued.push(Queued {
            text: text.to_string(),
            left: pos[0],
            top: pos[1],
            size_px,
            color: [c(color[0]), c(color[1]), c(color[2]), c(color[3])],
        });
    }

    pub fn has_pending(&self) -> bool {
        !self.queued.is_empty()
    }

    pub fn flush(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
    ) {
        if self.queued.is_empty() {
            return;
        }

        // Build buffers up-front; the prepare() call borrows them.
        let buffers: Vec<Buffer> = self
            .queued
            .iter()
            .map(|q| {
                let metrics = Metrics::new(q.size_px, q.size_px * 1.2);
                let mut buf = Buffer::new(&mut self.font_system, metrics);
                buf.set_text(
                    &mut self.font_system,
                    &q.text,
                    &Attrs::new(),
                    Shaping::Advanced,
                    None,
                );
                buf.shape_until_scroll(&mut self.font_system, false);
                buf
            })
            .collect();

        let areas: Vec<TextArea> = self
            .queued
            .iter()
            .zip(buffers.iter())
            .map(|(q, buf)| TextArea {
                buffer: buf,
                left: q.left,
                top: q.top,
                scale: 1.0,
                bounds: TextBounds::default(),
                default_color: Color::rgba(q.color[0], q.color[1], q.color[2], q.color[3]),
                custom_glyphs: &[],
            })
            .collect();

        if let Err(e) = self.renderer.prepare(
            device,
            queue,
            &mut self.font_system,
            &mut self.atlas,
            &self.viewport,
            areas,
            &mut self.swash_cache,
        ) {
            log::warn!("glyphon prepare failed: {e:?}");
            self.queued.clear();
            return;
        }

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("text.pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });
        if let Err(e) = self.renderer.render(&self.atlas, &self.viewport, &mut pass) {
            log::warn!("glyphon render failed: {e:?}");
        }
        drop(pass);
        self.queued.clear();
        // Trim atlas if it grew unbounded.
        let _: Result<()> = Ok(());
    }
}
