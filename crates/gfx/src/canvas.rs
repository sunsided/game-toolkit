//! Virtual resolution: the painter draws into a fixed-size canvas texture, which is then
//! scaled onto the window.

/// How the virtual-resolution canvas is scaled onto the window. See
/// [`GraphicsConfig::virtual_resolution`](crate::GraphicsConfig::virtual_resolution).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum ScaleMode {
    /// Scale by the largest whole number that fits, so every canvas pixel becomes an equal
    /// square of window pixels; the rest of the window is border. Falls back to [`Fit`]
    /// when the window is smaller than the canvas.
    ///
    /// [`Fit`]: ScaleMode::Fit
    #[default]
    Integer,
    /// Scale as large as fits while keeping the aspect ratio, by a fractional factor if need
    /// be (e.g. 800 × 600 on a 1080p screen). Canvas pixels then differ in size by one window
    /// pixel; [`ScaleFilter::Linear`] or a post-process filter hides that.
    Fit,
    /// Fill the whole window, distorting the aspect ratio.
    Stretch,
}

/// Texture filter for scaling the canvas onto the window.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum ScaleFilter {
    /// Crisp pixels (default).
    #[default]
    Nearest,
    /// Smooth, slightly blurry; can look better with [`ScaleMode::Fit`].
    Linear,
}

impl ScaleFilter {
    /// Position in the `[nearest, linear]` sampler and bind-group arrays. A match rather than
    /// `as usize`, so a new variant fails to compile here instead of picking the wrong one.
    pub(crate) const fn index(self) -> usize {
        match self {
            Self::Nearest => 0,
            Self::Linear => 1,
        }
    }
}

/// Where the canvas lands in the window, in physical window pixels.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct CanvasRect {
    /// Top-left corner.
    pub origin: [f32; 2],
    /// Width and height.
    pub size: [f32; 2],
}

impl CanvasRect {
    /// Map a window position (e.g. a raw cursor position) to canvas pixels for a canvas of
    /// `canvas` size. Positions in the border map outside `0..canvas`.
    pub fn window_to_canvas(&self, canvas: [u32; 2], pos: [f32; 2]) -> [f32; 2] {
        let s = self.canvas_per_window(canvas);
        [
            (pos[0] - self.origin[0]) * s[0],
            (pos[1] - self.origin[1]) * s[1],
        ]
    }

    /// Canvas pixels per window pixel along each axis, for scaling deltas.
    pub fn canvas_per_window(&self, canvas: [u32; 2]) -> [f32; 2] {
        [
            canvas[0] as f32 / self.size[0].max(1e-6),
            canvas[1] as f32 / self.size[1].max(1e-6),
        ]
    }
}

/// Place a `canvas`-sized image in a `window`-sized one with `mode`. Origins and sizes are
/// whole pixels so the edges stay sharp.
pub(crate) fn layout(window: [u32; 2], canvas: [u32; 2], mode: ScaleMode) -> CanvasRect {
    let (ww, wh) = (window[0].max(1) as f32, window[1].max(1) as f32);
    let (cw, ch) = (canvas[0].max(1) as f32, canvas[1].max(1) as f32);
    let fit = (ww / cw).min(wh / ch);
    let scale = match mode {
        ScaleMode::Stretch => {
            return CanvasRect {
                origin: [0.0, 0.0],
                size: [ww, wh],
            };
        }
        ScaleMode::Integer if fit >= 1.0 => fit.floor(),
        ScaleMode::Integer | ScaleMode::Fit => fit,
    };
    let size = [(cw * scale).round(), (ch * scale).round()];
    CanvasRect {
        origin: [
            ((ww - size[0]) * 0.5).floor(),
            ((wh - size[1]) * 0.5).floor(),
        ],
        size,
    }
}

/// An offscreen colour texture the toolkit renders into and samples from, with blit bind
/// groups for both filters.
pub(crate) struct RenderTexture {
    pub size: [u32; 2],
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    /// Indexed by [`ScaleFilter::index`].
    pub blit_bind_groups: [wgpu::BindGroup; 2],
}

impl RenderTexture {
    pub fn new(
        device: &wgpu::Device,
        blit: &Blit,
        format: wgpu::TextureFormat,
        size: [u32; 2],
        label: &str,
    ) -> Self {
        let size = [size[0].max(1), size[1].max(1)];
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let blit_bind_groups = blit.bind_groups(device, &view);
        Self {
            size,
            texture,
            view,
            blit_bind_groups,
        }
    }
}

/// The offscreen scene the painter draws into: the virtual-resolution canvas, or a
/// window-sized stand-in while post effects need the scene as a texture.
pub(crate) struct Canvas {
    pub tex: RenderTexture,
    /// `true` for the window-sized stand-in (no virtual resolution): it is resized with the
    /// window and placed 1:1.
    pub follows_window: bool,
}

impl Canvas {
    pub fn size(&self) -> [u32; 2] {
        self.tex.size
    }
}

/// A full-screen-triangle copy of one texture into a viewport of another, nearest or linear.
pub(crate) struct Blit {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    samplers: [wgpu::Sampler; 2],
}

impl Blit {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blit.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("blit.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit.bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blit.layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blit.pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = |filter| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("blit.sampler"),
                mag_filter: filter,
                min_filter: filter,
                ..Default::default()
            })
        };
        Self {
            pipeline,
            layout,
            samplers: [
                sampler(wgpu::FilterMode::Nearest),
                sampler(wgpu::FilterMode::Linear),
            ],
        }
    }

    /// Bind groups sampling `src` with the nearest and the linear sampler, indexed by
    /// [`ScaleFilter::index`].
    pub fn bind_groups(
        &self,
        device: &wgpu::Device,
        src: &wgpu::TextureView,
    ) -> [wgpu::BindGroup; 2] {
        self.samplers.each_ref().map(|sampler| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("blit.bg"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(src),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                ],
            })
        })
    }

    /// Clear `dst` to `clear` and draw the bound source into `rect`.
    pub fn draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        bind_group: &wgpu::BindGroup,
        dst: &wgpu::TextureView,
        rect: CanvasRect,
        clear: wgpu::Color,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("blit.pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: dst,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });
        if rect.size[0] < 1.0 || rect.size[1] < 1.0 {
            return;
        }
        pass.set_viewport(
            rect.origin[0],
            rect.origin[1],
            rect.size[0],
            rect.size[1],
            0.0,
            1.0,
        );
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_scale_centres_with_border() {
        // 320x180 in 1920x1080 is exactly 6x.
        let r = layout([1920, 1080], [320, 180], ScaleMode::Integer);
        assert_eq!(r.origin, [0.0, 0.0]);
        assert_eq!(r.size, [1920.0, 1080.0]);
        // 1000x700: 3x (960x540), centred.
        let r = layout([1000, 700], [320, 180], ScaleMode::Integer);
        assert_eq!(r.size, [960.0, 540.0]);
        assert_eq!(r.origin, [20.0, 80.0]);
    }

    #[test]
    fn integer_falls_back_to_fit_when_window_is_smaller() {
        let r = layout([160, 90], [320, 180], ScaleMode::Integer);
        assert_eq!(r.size, [160.0, 90.0]);
    }

    #[test]
    fn fit_is_fractional() {
        // 800x600 on 1920x1080: 1.8x -> 1440x1080, pillarboxed.
        let r = layout([1920, 1080], [800, 600], ScaleMode::Fit);
        assert_eq!(r.size, [1440.0, 1080.0]);
        assert_eq!(r.origin, [240.0, 0.0]);
        assert_eq!(
            layout([1920, 1080], [800, 600], ScaleMode::Integer).size,
            [800.0, 600.0]
        );
    }

    #[test]
    fn stretch_fills_window() {
        let r = layout([1000, 700], [320, 180], ScaleMode::Stretch);
        assert_eq!(r.origin, [0.0, 0.0]);
        assert_eq!(r.size, [1000.0, 700.0]);
    }

    #[test]
    fn window_to_canvas_inverts_layout() {
        let canvas = [320, 180];
        let r = layout([1000, 700], canvas, ScaleMode::Integer);
        assert_eq!(r.window_to_canvas(canvas, [20.0, 80.0]), [0.0, 0.0]);
        assert_eq!(r.window_to_canvas(canvas, [980.0, 620.0]), [320.0, 180.0]);
        assert_eq!(r.window_to_canvas(canvas, [23.0, 83.0]), [1.0, 1.0]);
        assert_eq!(r.canvas_per_window(canvas), [1.0 / 3.0, 1.0 / 3.0]);
    }
}
