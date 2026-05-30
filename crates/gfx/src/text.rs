use std::collections::HashMap;

use crate::target::Targets;
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

/// A shaped `cosmic_text::Buffer` kept across frames, tagged with the frame it was last
/// drawn on so stale entries can be evicted.
struct CachedBuffer {
    buffer: Buffer,
    last_used: u64,
}

/// Drop cached buffers untouched for this many frames (~4s at 60fps). Bounds memory when
/// text changes every frame (e.g. a live counter) without thrashing steady-state HUD text.
const EVICT_AFTER_FRAMES: u64 = 240;

pub(crate) struct TextSystem {
    font_system: FontSystem,
    swash_cache: SwashCache,
    #[allow(dead_code)]
    cache: Cache,
    atlas: TextAtlas,
    viewport: Viewport,
    renderer: TextRenderer,
    queued: Vec<Queued>,
    /// Shaped buffers cached across frames, keyed by quantized size then text so a cache hit
    /// needs no allocation. Nested maps let the inner lookup borrow `&str` directly.
    buffers: HashMap<u32, HashMap<String, CachedBuffer>>,
    frame: u64,
}

/// Quantize a pixel size into a stable integer cache key (hundredths of a pixel).
fn size_key(size_px: f32) -> u32 {
    (size_px.max(0.0) * 100.0).round() as u32
}

impl TextSystem {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        sample_count: u32,
        depth_format: Option<wgpu::TextureFormat>,
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
            crate::target::multisample(sample_count),
            depth_format.map(crate::target::no_write_depth),
        );
        Self {
            font_system,
            swash_cache,
            cache,
            atlas,
            viewport,
            renderer,
            queued: Vec::new(),
            buffers: HashMap::new(),
            frame: 0,
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

    /// Render all queued text in one pass. Glyphon prepares a single internal vertex buffer
    /// per call, so (unlike sprites and circles) text is not interleaved by layer; it always
    /// draws on top of the sprite and circle layers.
    pub fn flush(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        targets: &Targets,
    ) {
        if self.queued.is_empty() {
            return;
        }
        self.frame += 1;

        // Build (or refresh) the cached buffer for every queued string. Shaping happens once
        // per distinct (size, text); a repeated string in steady state allocates nothing.
        for q in &self.queued {
            let inner = self.buffers.entry(size_key(q.size_px)).or_default();
            if let Some(c) = inner.get_mut(&q.text) {
                c.last_used = self.frame;
            } else {
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
                inner.insert(
                    q.text.clone(),
                    CachedBuffer {
                        buffer: buf,
                        last_used: self.frame,
                    },
                );
            }
        }

        let areas: Vec<TextArea> = self
            .queued
            .iter()
            .map(|q| TextArea {
                buffer: &self.buffers[&size_key(q.size_px)][&q.text].buffer,
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
            color_attachments: &[Some(targets.color_attachment(wgpu::LoadOp::Load))],
            depth_stencil_attachment: targets.depth_attachment(wgpu::LoadOp::Load),
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });
        if let Err(e) = self.renderer.render(&self.atlas, &self.viewport, &mut pass) {
            log::warn!("glyphon render failed: {e:?}");
        }
        drop(pass);
        self.queued.clear();
        self.evict_stale();
    }

    /// Drop buffers not drawn within [`EVICT_AFTER_FRAMES`] so the cache stays bounded.
    fn evict_stale(&mut self) {
        let frame = self.frame;
        self.buffers.retain(|_, inner| {
            inner.retain(|_, c| c.last_used + EVICT_AFTER_FRAMES >= frame);
            !inner.is_empty()
        });
    }
}
