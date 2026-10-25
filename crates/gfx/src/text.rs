use std::collections::{BTreeSet, HashMap};

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
    /// `None` draws above every layer (`Painter::text`), `Some` sorts with the 2D layers.
    layer: Option<i16>,
}

/// Horizontal alignment of text relative to its anchor point, for
/// [`crate::TextDraw::align`].
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum Align {
    /// The anchor is the text's left edge (default).
    #[default]
    Left,
    /// The anchor is the text's horizontal center.
    Center,
    /// The anchor is the text's right edge.
    Right,
}

impl Align {
    /// How far left of the anchor the text starts, as a fraction of its width.
    pub(crate) fn factor(self) -> f32 {
        match self {
            Self::Left => 0.0,
            Self::Center => 0.5,
            Self::Right => 1.0,
        }
    }
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
    /// One renderer per text layer (`None` = above all layers): glyphon draws everything a
    /// renderer prepared in one call, so each layer needs its own vertex buffer.
    renderers: HashMap<Option<i16>, TextRenderer>,
    /// Layers prepared this frame, i.e. the renderers with something to draw.
    prepared: BTreeSet<Option<i16>>,
    sample_count: u32,
    depth_format: Option<wgpu::TextureFormat>,
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

/// Bundled fallback font for platforms without system fonts (the web).
#[cfg(target_arch = "wasm32")]
const DEFAULT_FONT: &[u8] = include_bytes!("../fonts/DejaVuSans.ttf");

/// Native: system fonts via fontconfig/CoreText/DirectWrite. Web: there are no system fonts, so
/// the bundled DejaVu Sans becomes every generic family.
fn new_font_system() -> FontSystem {
    #[cfg(not(target_arch = "wasm32"))]
    {
        FontSystem::new()
    }
    #[cfg(target_arch = "wasm32")]
    {
        let mut fs = FontSystem::new_with_fonts([glyphon::fontdb::Source::Binary(
            std::sync::Arc::new(DEFAULT_FONT),
        )]);
        let db = fs.db_mut();
        db.set_sans_serif_family("DejaVu Sans");
        db.set_serif_family("DejaVu Sans");
        db.set_monospace_family("DejaVu Sans");
        fs
    }
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
        let font_system = new_font_system();
        let swash_cache = SwashCache::new();
        let cache = Cache::new(device);
        let atlas =
            TextAtlas::with_color_mode(device, queue, &cache, surface_format, ColorMode::Accurate);
        let mut viewport = Viewport::new(device, &cache);
        viewport.update(queue, Resolution { width, height });
        Self {
            font_system,
            swash_cache,
            cache,
            atlas,
            viewport,
            renderers: HashMap::new(),
            prepared: BTreeSet::new(),
            sample_count,
            depth_format,
            queued: Vec::new(),
            buffers: HashMap::new(),
            frame: 0,
        }
    }

    /// Register an extra font (TTF/OTF bytes). Text keeps using the default sans-serif family;
    /// the new face is used as a fallback for glyphs the default lacks.
    pub fn load_font_bytes(&mut self, bytes: Vec<u8>) {
        self.font_system.db_mut().load_font_data(bytes);
    }

    pub fn resize(&mut self, queue: &wgpu::Queue, width: u32, height: u32) {
        self.viewport.update(queue, Resolution { width, height });
    }

    pub fn queue(
        &mut self,
        text: &str,
        pos: [f32; 2],
        size_px: f32,
        color: [f32; 4],
        layer: Option<i16>,
    ) {
        let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0) as u8;
        self.queued.push(Queued {
            text: text.to_string(),
            left: pos[0],
            top: pos[1],
            size_px,
            color: [c(color[0]), c(color[1]), c(color[2]), c(color[3])],
            layer,
        });
    }

    /// Size `[width, height]` of `text` laid out at `size_px`: the widest line, and the lines'
    /// total height. Shapes through the same cache as drawing, so measuring a string and then
    /// drawing it shapes it once.
    pub fn measure(&mut self, text: &str, size_px: f32) -> [f32; 2] {
        let buffer = self.shaped(text, size_px);
        let (mut width, mut height) = (0.0f32, 0.0f32);
        for run in buffer.layout_runs() {
            width = width.max(run.line_w);
            height = height.max(run.line_top + run.line_height);
        }
        [width, height]
    }

    /// The cached shaped buffer for `(text, size_px)`, shaping it on a miss.
    fn shaped(&mut self, text: &str, size_px: f32) -> &Buffer {
        let frame = self.frame;
        let inner = self.buffers.entry(size_key(size_px)).or_default();
        if !inner.contains_key(text) {
            let metrics = Metrics::new(size_px, size_px * 1.2);
            let mut buffer = Buffer::new(&mut self.font_system, metrics);
            buffer.set_text(text, &Attrs::new(), Shaping::Advanced, None);
            buffer.shape_until_scroll(&mut self.font_system, false);
            inner.insert(
                text.to_owned(),
                CachedBuffer {
                    buffer,
                    last_used: frame,
                },
            );
        }
        let cached = inner.get_mut(text).expect("inserted above");
        cached.last_used = frame;
        &cached.buffer
    }

    /// Record every 2D layer that has queued text, for interleaving with the other batchers.
    pub fn collect_layers(&self, out: &mut BTreeSet<i16>) {
        out.extend(self.queued.iter().filter_map(|q| q.layer));
    }

    /// Shape and upload all queued text, one glyphon renderer per layer. Must run before any
    /// [`TextSystem::draw_layer`]: like the other batchers' instance buffers, each renderer's
    /// vertex buffer is written once per frame, outside the encoder's command stream.
    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.prepared.clear();
        if self.queued.is_empty() {
            return;
        }
        self.frame += 1;

        // Build (or refresh) the cached buffer for every queued string. Shaping happens once
        // per distinct (size, text); a repeated string in steady state allocates nothing.
        let queued = std::mem::take(&mut self.queued);
        for q in &queued {
            self.shaped(&q.text, q.size_px);
        }

        let layers: BTreeSet<Option<i16>> = queued.iter().map(|q| q.layer).collect();
        for layer in layers {
            let areas = queued
                .iter()
                .filter(|q| q.layer == layer)
                .map(|q| TextArea {
                    buffer: &self.buffers[&size_key(q.size_px)][&q.text].buffer,
                    left: q.left,
                    top: q.top,
                    scale: 1.0,
                    bounds: TextBounds::default(),
                    default_color: Color::rgba(q.color[0], q.color[1], q.color[2], q.color[3]),
                    custom_glyphs: &[],
                });
            let renderer = self.renderers.entry(layer).or_insert_with(|| {
                TextRenderer::new(
                    &mut self.atlas,
                    device,
                    crate::target::multisample(self.sample_count),
                    self.depth_format.map(crate::target::no_write_depth),
                )
            });
            match renderer.prepare(
                device,
                queue,
                &mut self.font_system,
                &mut self.atlas,
                &self.viewport,
                areas,
                &mut self.swash_cache,
            ) {
                Ok(()) => {
                    self.prepared.insert(layer);
                }
                Err(e) => log::warn!("glyphon prepare failed: {e:?}"),
            }
        }
        // Hand the (now empty) Vec back to keep its allocation for the next frame.
        self.queued = queued;
        self.queued.clear();
    }

    /// Draw the text prepared for `layer` (`None`: the unlayered text above everything) in
    /// its own pass. No-op if that layer has no text this frame.
    pub fn draw_layer(
        &self,
        layer: Option<i16>,
        encoder: &mut wgpu::CommandEncoder,
        targets: &Targets<'_>,
    ) {
        if !self.prepared.contains(&layer) {
            return;
        }
        let Some(renderer) = self.renderers.get(&layer) else {
            return;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("text.pass"),
            color_attachments: &[Some(targets.color_attachment(wgpu::LoadOp::Load))],
            depth_stencil_attachment: targets.depth_attachment(wgpu::LoadOp::Load),
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });
        if let Err(e) = renderer.render(&self.atlas, &self.viewport, &mut pass) {
            log::warn!("glyphon render failed: {e:?}");
        }
    }

    /// End-of-frame housekeeping, after every [`TextSystem::draw_layer`]: evict stale shaped
    /// buffers and free atlas space held by glyphs no longer drawn.
    pub fn finish(&mut self) {
        if self.prepared.is_empty() {
            return;
        }
        self.prepared.clear();
        self.evict_stale();
        self.atlas.trim();
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
