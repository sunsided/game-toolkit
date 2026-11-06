//! Full-screen post-processing after the scene is drawn: effects that read the finished frame
//! as a texture and write a new one.

use std::any::Any;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::canvas::{Blit, CanvasRect, RenderTexture};

/// Where in the frame a post effect runs.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum PostStage {
    /// On the canvas, at the virtual resolution, before it is scaled onto the window (palette
    /// tricks, dithering, anything that should stay on the pixel grid). Without a virtual
    /// resolution the canvas is the window, so this runs at window resolution, before the
    /// [`Window`](PostStage::Window) stage.
    Canvas,
    /// At window resolution, after the canvas has been scaled onto the window, border
    /// included (scanlines, shadow masks, curvature, bloom).
    Window,
}

/// Handle to an effect added with [`Graphics::add_post_effect`](crate::Graphics::add_post_effect).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PostEffectId(u64);

/// One full-screen effect. Implement it to record arbitrary passes; for the common case of a
/// single WGSL fragment shader, use [`ShaderEffect`].
pub trait PostEffect: 'static {
    /// Read [`PostPass::input`] and write every pixel of [`PostPass::output`]. Both are
    /// [`PostPass::size`] pixels in [`PostPass::format`].
    fn apply(&mut self, pass: &mut PostPass<'_>);

    /// Return `true` to get [`PostPass::history`]: the stage's final output from the
    /// previous frame, for afterglow and other temporal effects.
    fn uses_history(&self) -> bool {
        false
    }
}

/// Everything a [`PostEffect`] gets for one run.
pub struct PostPass<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub encoder: &'a mut wgpu::CommandEncoder,
    /// The image so far: the scene, or the previous effect's output.
    pub input: &'a wgpu::TextureView,
    /// Where to write; the same size and format as `input`.
    pub output: &'a wgpu::TextureView,
    /// The stage's final output from the previous frame, when an effect of the stage asked
    /// for it via [`PostEffect::uses_history`]; black on the first frame and after a resize.
    pub history: Option<&'a wgpu::TextureView>,
    pub format: wgpu::TextureFormat,
    pub stage: PostStage,
    /// Width and height of `input` and `output`.
    pub size: [u32; 2],
    pub globals: PostGlobals,
}

/// Frame values passed to every effect, and to [`ShaderEffect`] shaders as the `post`
/// uniform.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct PostGlobals {
    /// Size of the image the effect works on.
    pub size: [f32; 2],
    /// Size of the canvas (the virtual resolution, or the window without one).
    pub canvas_size: [f32; 2],
    /// Size of the window.
    pub window_size: [f32; 2],
    /// Where the canvas lands in the window, top-left and size in window pixels. In the
    /// [`PostStage::Window`] stage, use it to lock effects to canvas rows (scanlines).
    pub canvas_origin: [f32; 2],
    pub canvas_extent: [f32; 2],
    /// Seconds since the renderer started.
    pub time: f32,
    /// Frames since the renderer started.
    pub frame: u32,
}

/// Internal: lets `Graphics::post_effect_mut` downcast boxed effects.
pub(crate) trait AnyPostEffect: PostEffect {
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T: PostEffect> AnyPostEffect for T {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

struct Entry {
    id: PostEffectId,
    enabled: bool,
    effect: Box<dyn AnyPostEffect>,
}

/// Ping-pong targets (and the history copy) for one stage, at that stage's size.
struct Targets {
    ping: [RenderTexture; 2],
    history: Option<RenderTexture>,
}

#[derive(Default)]
struct Stage {
    effects: Vec<Entry>,
    targets: Option<Targets>,
}

/// Which stage target holds a stage's result.
#[derive(Copy, Clone)]
pub(crate) enum StageOutput {
    Ping(usize),
}

impl Stage {
    fn active(&self) -> bool {
        self.effects.iter().any(|e| e.enabled)
    }

    fn wants_history(&self) -> bool {
        self.effects
            .iter()
            .any(|e| e.enabled && e.effect.uses_history())
    }

    fn ensure_targets(
        &mut self,
        device: &wgpu::Device,
        blit: &Blit,
        format: wgpu::TextureFormat,
        size: [u32; 2],
        label: &str,
    ) {
        let history = self.wants_history();
        let fits = self
            .targets
            .as_ref()
            .is_some_and(|t| t.ping[0].size == size && t.history.is_some() == history);
        if fits {
            return;
        }
        let make = |l: &str| RenderTexture::new(device, blit, format, size, l);
        self.targets = Some(Targets {
            ping: [make(label), make(label)],
            // Fresh textures are zeroed: black history on the first frame.
            history: history.then(|| make(label)),
        });
    }
}

/// Both stages' effects.
#[derive(Default)]
pub(crate) struct PostChain {
    stages: [Stage; 2],
    next_id: u64,
}

pub(crate) struct StageRun<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub blit: &'a Blit,
    pub format: wgpu::TextureFormat,
    pub globals: PostGlobals,
}

impl PostChain {
    pub fn is_empty(&self) -> bool {
        self.stages.iter().all(|s| s.effects.is_empty())
    }

    pub fn active(&self, stage: PostStage) -> bool {
        self.stages[stage as usize].active()
    }

    pub fn add(&mut self, stage: PostStage, effect: Box<dyn AnyPostEffect>) -> PostEffectId {
        self.next_id += 1;
        let id = PostEffectId(self.next_id);
        self.stages[stage as usize].effects.push(Entry {
            id,
            enabled: true,
            effect,
        });
        id
    }

    fn find(&mut self, id: PostEffectId) -> Option<(usize, usize)> {
        self.stages.iter().enumerate().find_map(|(s, stage)| {
            stage
                .effects
                .iter()
                .position(|e| e.id == id)
                .map(|i| (s, i))
        })
    }

    pub fn remove(&mut self, id: PostEffectId) -> bool {
        match self.find(id) {
            Some((s, i)) => {
                self.stages[s].effects.remove(i);
                true
            }
            None => false,
        }
    }

    pub fn set_enabled(&mut self, id: PostEffectId, enabled: bool) -> bool {
        match self.find(id) {
            Some((s, i)) => {
                self.stages[s].effects[i].enabled = enabled;
                true
            }
            None => false,
        }
    }

    pub fn is_enabled(&mut self, id: PostEffectId) -> Option<bool> {
        self.find(id)
            .map(|(s, i)| self.stages[s].effects[i].enabled)
    }

    pub fn get_mut(&mut self, id: PostEffectId) -> Option<&mut dyn Any> {
        let (s, i) = self.find(id)?;
        Some(self.stages[s].effects[i].effect.as_any_mut())
    }

    /// Run `stage`'s enabled effects on `input` (`size` pixels). Returns the target holding
    /// the result, or `None` when the stage has nothing enabled.
    pub fn run(
        &mut self,
        stage: PostStage,
        run: &StageRun<'_>,
        encoder: &mut wgpu::CommandEncoder,
        input: &wgpu::TextureView,
        size: [u32; 2],
    ) -> Option<StageOutput> {
        let st = &mut self.stages[stage as usize];
        if !st.active() {
            return None;
        }
        let label = match stage {
            PostStage::Canvas => "post.canvas",
            PostStage::Window => "post.window",
        };
        st.ensure_targets(run.device, run.blit, run.format, size, label);
        let targets = st.targets.as_ref()?;
        let mut globals = run.globals;
        globals.size = [size[0] as f32, size[1] as f32];
        let mut current: Option<usize> = None;
        for entry in st.effects.iter_mut().filter(|e| e.enabled) {
            let out = current.map_or(0, |c| 1 - c);
            let input = current.map_or(input, |c| &targets.ping[c].view);
            let mut pass = PostPass {
                device: run.device,
                queue: run.queue,
                encoder,
                input,
                output: &targets.ping[out].view,
                history: targets.history.as_ref().map(|h| &h.view),
                format: run.format,
                stage,
                size,
                globals,
            };
            entry.effect.apply(&mut pass);
            current = Some(out);
        }
        let out = current?;
        if let Some(history) = &targets.history {
            encoder.copy_texture_to_texture(
                targets.ping[out].texture.as_image_copy(),
                history.texture.as_image_copy(),
                history.texture.size(),
            );
        }
        Some(StageOutput::Ping(out))
    }

    /// The texture a stage's result is in.
    pub fn output(&self, stage: PostStage, out: StageOutput) -> Option<&RenderTexture> {
        let targets = self.stages[stage as usize].targets.as_ref()?;
        match out {
            StageOutput::Ping(i) => targets.ping.get(i),
        }
    }
}

/// The bindings, vertex shader and `VsOut` every [`ShaderEffect`] shader gets.
pub const SHADER_PRELUDE: &str = include_str!("post_prelude.wgsl");

/// A post effect made of one WGSL fragment shader, run as a full-screen pass.
///
/// The toolkit prepends [`SHADER_PRELUDE`], which declares:
///
/// - `input_tex: texture_2d<f32>` - the image so far, sampled with `nearest_sampler` or
///   `linear_sampler`;
/// - `history_tex: texture_2d<f32>` - the stage's previous output (see
///   [`ShaderEffect::with_history`]; otherwise the input again);
/// - `post: PostGlobals` - the uniform with `size`, `canvas_size`, `window_size`,
///   `canvas_origin`, `canvas_extent`, `time` and `frame` (see [`PostGlobals`]);
/// - `struct VsOut { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> }` and the
///   vertex shader.
///
/// Your source must define `@fragment fn fs_main(in: VsOut) -> @location(0) vec4<f32>`. With
/// [`ShaderEffect::with_params`], it must also declare its parameter block as
/// `@group(1) @binding(0) var<uniform> params: YourParams;` matching the Rust struct you pass
/// to [`ShaderEffect::set_params`]. Colours are linear (textures are sRGB, decoded on sample).
///
/// ```ignore
/// let invert = ShaderEffect::new(&ctx.gfx, "invert", r#"
///     @fragment fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
///         let c = textureSample(input_tex, nearest_sampler, in.uv);
///         return vec4<f32>(1.0 - c.rgb, c.a);
///     }
/// "#);
/// ctx.gfx.add_post_effect(PostStage::Canvas, invert);
/// ```
pub struct ShaderEffect {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    globals: wgpu::Buffer,
    samplers: [wgpu::Sampler; 2],
    params: Option<Params>,
    history: bool,
}

struct Params {
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    bytes: Vec<u8>,
    dirty: bool,
}

impl ShaderEffect {
    /// Compile `wgsl` (see the type docs for what it must define) for `gfx`'s frame format.
    /// Shader errors surface as wgpu validation errors, like any other pipeline.
    pub fn new(gfx: &crate::Graphics, label: &str, wgsl: &str) -> Self {
        Self::build(gfx.device(), gfx.surface_format(), label, wgsl, None)
    }

    /// Like [`ShaderEffect::new`], with a uniform parameter block initialised to `params`
    /// at `@group(1) @binding(0)`. Mind WGSL's uniform layout rules: pad the Rust struct to
    /// a multiple of 16 bytes and align `vec3`/`vec4` members to 16.
    pub fn with_params<T: Pod>(gfx: &crate::Graphics, label: &str, wgsl: &str, params: &T) -> Self {
        Self::build(
            gfx.device(),
            gfx.surface_format(),
            label,
            wgsl,
            Some(bytemuck::bytes_of(params)),
        )
    }

    pub(crate) fn build(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        label: &str,
        wgsl: &str,
        params: Option<&[u8]>,
    ) -> Self {
        let source = format!("{SHADER_PRELUDE}\n{wgsl}");
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(label),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let tex = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let sampler = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let uniform = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post.bgl"),
            entries: &[tex(0), sampler(1), sampler(2), uniform(3), tex(4)],
        });
        let params = params.map(|bytes| {
            let params_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("post.params.bgl"),
                entries: &[uniform(0)],
            });
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("post.params"),
                contents: bytes,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("post.params.bg"),
                layout: &params_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                }],
            });
            (
                params_layout,
                Params {
                    buffer,
                    bind_group,
                    bytes: bytes.to_vec(),
                    dirty: false,
                },
            )
        });
        let mut layouts = vec![Some(&layout)];
        if let Some((params_layout, _)) = &params {
            layouts.push(Some(params_layout));
        }
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(label),
            bind_group_layouts: &layouts,
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("post_vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
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
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("post.globals"),
            size: size_of::<PostGlobals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let make_sampler = |filter| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("post.sampler"),
                mag_filter: filter,
                min_filter: filter,
                ..Default::default()
            })
        };
        Self {
            pipeline,
            layout,
            globals,
            samplers: [
                make_sampler(wgpu::FilterMode::Nearest),
                make_sampler(wgpu::FilterMode::Linear),
            ],
            params: params.map(|(_, p)| p),
            history: false,
        }
    }

    /// Bind the stage's previous output as `history_tex` (otherwise it is the input again).
    #[must_use]
    pub fn with_history(mut self) -> Self {
        self.history = true;
        self
    }

    /// Replace the parameter block; uploaded before the next run. Ignored (with a warning)
    /// for an effect created without params or with a different size.
    pub fn set_params<T: Pod>(&mut self, params: &T) {
        let bytes = bytemuck::bytes_of(params);
        match &mut self.params {
            Some(p) if p.bytes.len() == bytes.len() => {
                p.bytes.copy_from_slice(bytes);
                p.dirty = true;
            }
            _ => log::warn!("ShaderEffect::set_params: no params block of that size"),
        }
    }
}

impl PostEffect for ShaderEffect {
    fn apply(&mut self, pass: &mut PostPass<'_>) {
        pass.queue
            .write_buffer(&self.globals, 0, bytemuck::bytes_of(&pass.globals));
        if let Some(p) = &mut self.params
            && p.dirty
        {
            pass.queue.write_buffer(&p.buffer, 0, &p.bytes);
            p.dirty = false;
        }
        let history = pass.history.filter(|_| self.history).unwrap_or(pass.input);
        let bind_group = pass.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("post.bg"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(pass.input),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.samplers[0]),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.samplers[1]),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.globals.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(history),
                },
            ],
        });
        let mut rp = pass.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("post.pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: pass.output,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });
        rp.set_pipeline(&self.pipeline);
        rp.set_bind_group(0, &bind_group, &[]);
        if let Some(p) = &self.params {
            rp.set_bind_group(1, &p.bind_group, &[]);
        }
        rp.draw(0..3, 0..1);
    }

    fn uses_history(&self) -> bool {
        self.history
    }
}

/// Fill in the parts of [`PostGlobals`] that don't depend on the stage.
pub(crate) fn globals(
    canvas_size: [u32; 2],
    window_size: [u32; 2],
    rect: CanvasRect,
    time: f32,
    frame: u32,
) -> PostGlobals {
    PostGlobals {
        size: [0.0; 2],
        canvas_size: [canvas_size[0] as f32, canvas_size[1] as f32],
        window_size: [window_size[0] as f32, window_size[1] as f32],
        canvas_origin: rect.origin,
        canvas_extent: rect.size,
        time,
        frame,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globals_are_uniform_sized() {
        // WGSL uniform structs round up to 16 bytes; keep the Rust side in step.
        assert_eq!(size_of::<PostGlobals>() % 16, 0);
    }
}
