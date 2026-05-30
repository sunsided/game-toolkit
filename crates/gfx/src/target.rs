//! The color/resolve/depth views a render pass writes to for one frame.
//!
//! Centralizing attachment construction keeps the MSAA resolve and optional depth handling
//! in one place so every batcher's pass agrees on the targets.

/// Depth-stencil state for the built-in 2D pipelines: compatible with a depth attachment of
/// `format` but never testing or writing depth, so enabling a depth buffer leaves the 2D
/// painter's output unchanged. 3D pipelines (issue #2) will use a real depth test instead.
pub(crate) fn no_write_depth(format: wgpu::TextureFormat) -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format,
        depth_write_enabled: Some(false),
        depth_compare: Some(wgpu::CompareFunction::Always),
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState::default(),
    }
}

/// Depth-stencil state for 3D meshes: standard depth test (`Less`) with depth writes on.
pub(crate) fn depth_test(format: wgpu::TextureFormat) -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format,
        depth_write_enabled: Some(true),
        depth_compare: Some(wgpu::CompareFunction::Less),
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState::default(),
    }
}

/// Multisample state for `sample_count` samples (no alpha-to-coverage).
pub(crate) fn multisample(sample_count: u32) -> wgpu::MultisampleState {
    wgpu::MultisampleState {
        count: sample_count,
        mask: !0,
        alpha_to_coverage_enabled: false,
    }
}

/// Borrowed render targets for a frame: the color view drawn into, an optional MSAA resolve
/// target, and an optional depth view.
pub(crate) struct Targets<'a> {
    /// The color attachment drawn into - the MSAA texture when multisampling, else the
    /// surface view.
    pub color: &'a wgpu::TextureView,
    /// `Some(surface)` when multisampling (the MSAA color resolves here), else `None`.
    pub resolve: Option<&'a wgpu::TextureView>,
    /// The depth attachment, when a depth buffer is configured.
    pub depth: Option<&'a wgpu::TextureView>,
}

impl<'a> Targets<'a> {
    pub fn color_attachment(
        &self,
        load: wgpu::LoadOp<wgpu::Color>,
    ) -> wgpu::RenderPassColorAttachment<'a> {
        wgpu::RenderPassColorAttachment {
            view: self.color,
            depth_slice: None,
            resolve_target: self.resolve,
            ops: wgpu::Operations {
                load,
                store: wgpu::StoreOp::Store,
            },
        }
    }

    pub fn depth_attachment(
        &self,
        load: wgpu::LoadOp<f32>,
    ) -> Option<wgpu::RenderPassDepthStencilAttachment<'a>> {
        self.depth.map(|view| wgpu::RenderPassDepthStencilAttachment {
            view,
            depth_ops: Some(wgpu::Operations {
                load,
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        })
    }
}
