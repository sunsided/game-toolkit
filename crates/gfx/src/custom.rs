//! Game-recorded passes interleaved with the painter's layers ([`Painter::custom`]).
//!
//! [`Painter::custom`]: crate::Painter::custom

use crate::target::{self, Targets};

/// A pass queued with [`Painter::custom`](crate::Painter::custom), run when its layer draws.
pub(crate) type CustomFn = Box<dyn FnOnce(&mut CustomPass<'_>)>;

/// Queued custom passes, in call order per layer.
#[derive(Default)]
pub(crate) struct CustomPasses {
    pending: Vec<(i16, CustomFn)>,
}

impl CustomPasses {
    pub fn push(&mut self, layer: i16, f: CustomFn) {
        self.pending.push((layer, f));
    }

    pub fn collect_layers(&self, out: &mut std::collections::BTreeSet<i16>) {
        out.extend(self.pending.iter().map(|(layer, _)| *layer));
    }

    /// Take this layer's passes, in the order they were queued.
    pub fn take_layer(&mut self, layer: i16) -> Vec<CustomFn> {
        let (this, rest) = std::mem::take(&mut self.pending)
            .into_iter()
            .partition(|(l, _)| *l == layer);
        self.pending = rest;
        this.into_iter().map(|(_, f)| f).collect()
    }

    pub fn clear(&mut self) {
        self.pending.clear();
    }
}

/// What a [`Painter::custom`](crate::Painter::custom) closure gets: the frame's encoder and
/// the painter's render target at that point in the frame (the canvas, the window, or the
/// MSAA texture resolving into either).
///
/// With indexed colour on ([`Self::is_indexed`]) the target holds palette indices: return
/// `index_out(i)` from `INDEX_WGSL` (feature `indexed`) instead of a colour.
///
/// The easiest way to draw is [`Self::begin_render_pass`], which attaches the target the same
/// way the painter does (load, store, MSAA resolve, depth). Build the pipeline for it with
/// [`Self::format`], [`Self::multisample`] and [`Self::depth_stencil`], so it matches the
/// attachments whatever `AppConfig::msaa_samples` and `depth_format` are.
pub struct CustomPass<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub encoder: &'a mut wgpu::CommandEncoder,
    pub(crate) targets: Targets<'a>,
    pub(crate) format: wgpu::TextureFormat,
    pub(crate) sample_count: u32,
    pub(crate) depth_format: Option<wgpu::TextureFormat>,
    pub(crate) size: [u32; 2],
    pub(crate) indexed: bool,
}

impl<'a> CustomPass<'a> {
    /// A render pass onto the painter's target that keeps what is already there (the clear
    /// colour and lower layers), with the MSAA resolve and depth attachment the painter uses.
    pub fn begin_render_pass(&mut self, label: &str) -> wgpu::RenderPass<'_> {
        self.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(self.targets.color_attachment(wgpu::LoadOp::Load))],
            depth_stencil_attachment: self.targets.depth_attachment(wgpu::LoadOp::Load),
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        })
    }

    /// The colour view drawn into: the MSAA texture when multisampling, else the canvas (or
    /// the window without one). For passes set up by hand; see [`Self::resolve_target`].
    pub fn view(&self) -> &wgpu::TextureView {
        self.targets.color
    }

    /// `Some(view)` when multisampling: the MSAA colour resolves into it.
    pub fn resolve_target(&self) -> Option<&wgpu::TextureView> {
        self.targets.resolve
    }

    /// The depth view, when `AppConfig::depth_format` is set.
    pub fn depth_view(&self) -> Option<&wgpu::TextureView> {
        self.targets.depth
    }

    /// The target's colour format (always sRGB; see `Graphics::surface_format`).
    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    /// Multisample state matching the target, for the pipeline.
    pub fn multisample(&self) -> wgpu::MultisampleState {
        target::multisample(self.sample_count)
    }

    /// Depth-stencil state matching the target, for the pipeline: `None` without a depth
    /// buffer, else one that neither tests nor writes depth (like the painter's 2D layers).
    pub fn depth_stencil(&self) -> Option<wgpu::DepthStencilState> {
        self.depth_format.map(target::no_write_depth)
    }

    /// Whether the target holds palette indices (indexed colour is on) rather than colours.
    pub fn is_indexed(&self) -> bool {
        self.indexed
    }

    /// Size of the target in pixels: the virtual resolution, else the window's size.
    pub fn size(&self) -> [u32; 2] {
        self.size
    }
}
