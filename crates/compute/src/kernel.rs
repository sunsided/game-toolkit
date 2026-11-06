/// One resource a [`Kernel`] binds, in `@group(0)`. The position in the list given to
/// [`KernelBuilder::bindings`] is the `@binding(n)` index in the shader.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Binding {
    /// `var<uniform> u: T;`
    Uniform,
    /// `var<storage, read> s: array<T>;`
    StorageRead,
    /// `var<storage, read_write> s: array<T>;` (also for `atomic<u32>` counters).
    StorageReadWrite,
    /// `var t: texture_storage_2d<format, write>;`, e.g. the output of `StorageTexture` with
    /// `StorageTexture::FORMAT` (feature `gfx`).
    StorageTextureWrite(wgpu::TextureFormat),
    /// `var t: texture_2d<f32>;`, read with `textureLoad` (no sampler).
    Texture,
}

impl Binding {
    fn ty(self) -> wgpu::BindingType {
        let buffer = |ty| wgpu::BindingType::Buffer {
            ty,
            has_dynamic_offset: false,
            min_binding_size: None,
        };
        match self {
            Self::Uniform => buffer(wgpu::BufferBindingType::Uniform),
            Self::StorageRead => buffer(wgpu::BufferBindingType::Storage { read_only: true }),
            Self::StorageReadWrite => buffer(wgpu::BufferBindingType::Storage { read_only: false }),
            Self::StorageTextureWrite(format) => wgpu::BindingType::StorageTexture {
                access: wgpu::StorageTextureAccess::WriteOnly,
                format,
                view_dimension: wgpu::TextureViewDimension::D2,
            },
            Self::Texture => wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
        }
    }
}

/// Number of workgroups of `group` threads needed to cover `items` threads: `ceil(items /
/// group)`. Zero items need zero workgroups.
///
/// # Panics
///
/// If `group` is zero.
pub const fn workgroups(items: u32, group: u32) -> u32 {
    assert!(group > 0, "workgroup size must be at least 1");
    items.div_ceil(group)
}

/// Builds a [`Kernel`]; start with [`Kernel::builder`].
#[derive(Clone, Debug)]
pub struct KernelBuilder<'a> {
    label: &'a str,
    source: &'a str,
    entry_point: Option<&'a str>,
    workgroup_size: [u32; 3],
    bindings: &'a [Binding],
}

impl<'a> KernelBuilder<'a> {
    /// The shader's entry point. Optional when the module has exactly one `@compute` function.
    pub fn entry_point(mut self, name: &'a str) -> Self {
        self.entry_point = Some(name);
        self
    }

    /// Threads per workgroup, `[x, y, z]`; default `[8, 8, 1]`. Used to turn a grid size into
    /// workgroup counts in [`Kernel::dispatch_for`], so it must match the shader's
    /// `@workgroup_size`. The kernel defines `WORKGROUP_SIZE_X`, `WORKGROUP_SIZE_Y` and
    /// `WORKGROUP_SIZE_Z` (`u32` constants) ahead of the source, so writing
    /// `@workgroup_size(WORKGROUP_SIZE_X, WORKGROUP_SIZE_Y)` keeps the two in step.
    ///
    /// # Panics
    ///
    /// [`Self::build`] panics if a component is zero.
    pub fn workgroup_size(mut self, size: [u32; 3]) -> Self {
        self.workgroup_size = size;
        self
    }

    /// The resources the shader declares in `@group(0)`, one per `@binding(n)` in order.
    pub fn bindings(mut self, bindings: &'a [Binding]) -> Self {
        self.bindings = bindings;
        self
    }

    /// Compile the WGSL and create the pipeline. Shader errors surface as wgpu validation
    /// errors (a panic by default, with the WGSL diagnostic); lines after your global
    /// directives (`enable`, ...) are one higher than in your source because of the injected
    /// `WORKGROUP_SIZE_*` line.
    pub fn build(self, device: &wgpu::Device) -> Kernel {
        let [x, y, z] = self.workgroup_size;
        assert!(
            x > 0 && y > 0 && z > 0,
            "workgroup size {:?} has a zero component",
            self.workgroup_size
        );
        let source = with_workgroup_prelude(self.source, self.workgroup_size);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(self.label),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let entries: Vec<_> = self
            .bindings
            .iter()
            .enumerate()
            .map(|(i, b)| wgpu::BindGroupLayoutEntry {
                binding: i as u32,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: b.ty(),
                count: None,
            })
            .collect();
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(self.label),
            entries: &entries,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(self.label),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(self.label),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: self.entry_point,
            compilation_options: Default::default(),
            cache: None,
        });
        Kernel {
            label: self.label.to_owned(),
            pipeline,
            layout,
            workgroup_size: self.workgroup_size,
        }
    }
}

/// The constants injected ahead of a kernel's source, on one line.
fn workgroup_prelude([x, y, z]: [u32; 3]) -> String {
    format!(
        "const WORKGROUP_SIZE_X: u32 = {x}u; const WORKGROUP_SIZE_Y: u32 = {y}u; \
         const WORKGROUP_SIZE_Z: u32 = {z}u;\n"
    )
}

/// `source` with the `WORKGROUP_SIZE_*` line inserted after any leading global directives
/// (`enable`, `requires`, `diagnostic`), which WGSL requires before every declaration.
/// Blank and `//` comment lines among the directives stay with them.
fn with_workgroup_prelude(source: &str, size: [u32; 3]) -> String {
    let mut split = 0;
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        offset += line.len();
        let t = line.trim();
        let is_directive = ["enable", "requires", "diagnostic"].iter().any(|d| {
            t.strip_prefix(d)
                .is_some_and(|r| r.starts_with([' ', '\t', '(']))
        });
        if is_directive {
            split = offset;
        } else if !(t.is_empty() || t.starts_with("//")) {
            break;
        }
    }
    let (head, tail) = source.split_at(split);
    let mut out = String::with_capacity(source.len() + 96);
    out.push_str(head);
    if !head.is_empty() && !head.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&workgroup_prelude(size));
    out.push_str(tail);
    out
}

/// A compute shader with its pipeline and `@group(0)` layout, ready to dispatch into a frame's
/// encoder.
///
/// ```no_run
/// # use game_toolkit_compute::{Binding, Kernel};
/// # use game_toolkit_compute::wgpu;
/// # fn demo(device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, buf: &wgpu::Buffer) {
/// let kernel = Kernel::builder(
///     "double",
///     "@group(0) @binding(0) var<storage, read_write> v: array<f32>;
///      @compute @workgroup_size(WORKGROUP_SIZE_X)
///      fn main(@builtin(global_invocation_id) id: vec3<u32>) {
///          if id.x < arrayLength(&v) { v[id.x] *= 2.0; }
///      }",
/// )
/// .workgroup_size([64, 1, 1])
/// .bindings(&[Binding::StorageReadWrite])
/// .build(device);
/// let bind_group = kernel.bind_group(device, &[buf.as_entire_binding()]);
/// kernel.dispatch_for(encoder, &bind_group, 1000, 1);
/// # }
/// ```
#[derive(Debug)]
pub struct Kernel {
    label: String,
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    workgroup_size: [u32; 3],
}

impl Kernel {
    /// Start building a kernel from WGSL `source`. `label` names it in GPU debuggers and
    /// error messages.
    pub fn builder<'a>(label: &'a str, source: &'a str) -> KernelBuilder<'a> {
        KernelBuilder {
            label,
            source,
            entry_point: None,
            workgroup_size: [8, 8, 1],
            bindings: &[],
        }
    }

    /// The `@group(0)` layout, for building bind groups by hand.
    pub fn layout(&self) -> &wgpu::BindGroupLayout {
        &self.layout
    }

    /// The compute pipeline, for recording into your own `wgpu::ComputePass`.
    pub fn pipeline(&self) -> &wgpu::ComputePipeline {
        &self.pipeline
    }

    /// Threads per workgroup, as given to [`KernelBuilder::workgroup_size`].
    pub fn workgroup_size(&self) -> [u32; 3] {
        self.workgroup_size
    }

    /// Workgroup counts that cover a `width x height x depth` grid of threads (one per cell),
    /// rounding up. The shader must skip the threads past the edge.
    pub fn workgroups_for(&self, width: u32, height: u32, depth: u32) -> [u32; 3] {
        let [x, y, z] = self.workgroup_size;
        [
            workgroups(width, x),
            workgroups(height, y),
            workgroups(depth, z),
        ]
    }

    /// A bind group for this kernel's layout: `resources[n]` goes to `@binding(n)`.
    pub fn bind_group(
        &self,
        device: &wgpu::Device,
        resources: &[wgpu::BindingResource<'_>],
    ) -> wgpu::BindGroup {
        let entries: Vec<_> = resources
            .iter()
            .enumerate()
            .map(|(i, resource)| wgpu::BindGroupEntry {
                binding: i as u32,
                resource: resource.clone(),
            })
            .collect();
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&self.label),
            layout: &self.layout,
            entries: &entries,
        })
    }

    /// Record one dispatch of exactly `workgroups` (`[x, y, z]`) into `encoder`, in its own
    /// compute pass.
    pub fn dispatch(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        bind_group: &wgpu::BindGroup,
        workgroups: [u32; 3],
    ) {
        let [x, y, z] = workgroups;
        if x == 0 || y == 0 || z == 0 {
            return;
        }
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(&self.label),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.dispatch_workgroups(x, y, z);
    }

    /// Record a dispatch with one thread per cell of a `width x height` grid (enough
    /// workgroups to cover it, see [`Self::workgroups_for`]).
    pub fn dispatch_for(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        bind_group: &wgpu::BindGroup,
        width: u32,
        height: u32,
    ) {
        self.dispatch(encoder, bind_group, self.workgroups_for(width, height, 1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workgroups_round_up() {
        assert_eq!(workgroups(0, 8), 0);
        assert_eq!(workgroups(1, 8), 1);
        assert_eq!(workgroups(8, 8), 1);
        assert_eq!(workgroups(9, 8), 2);
        assert_eq!(workgroups(256, 64), 4);
        assert_eq!(workgroups(u32::MAX, 1), u32::MAX);
        assert_eq!(workgroups(u32::MAX, 2), u32::MAX / 2 + 1);
    }

    #[test]
    #[should_panic(expected = "at least 1")]
    fn zero_workgroup_size_panics() {
        let _ = workgroups(4, 0);
    }

    #[test]
    fn prelude_goes_after_global_directives() {
        let src = "// header\nenable f16;\ndiagnostic(off, derivative_uniformity);\n\n@compute fn main() {}\n";
        let out = with_workgroup_prelude(src, [8, 8, 1]);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[1], "enable f16;");
        assert_eq!(lines[2], "diagnostic(off, derivative_uniformity);");
        assert!(lines[3].starts_with("const WORKGROUP_SIZE_X"));
        assert!(out.ends_with("@compute fn main() {}\n"));
        // Without directives the line goes first.
        let plain = with_workgroup_prelude("@compute fn main() {}", [1, 1, 1]);
        assert!(plain.starts_with("const WORKGROUP_SIZE_X"));
        // An identifier merely starting with a directive name is not one.
        let ident = with_workgroup_prelude("enabled_flag();", [1, 1, 1]);
        assert!(ident.starts_with("const WORKGROUP_SIZE_X"));
    }

    #[test]
    fn prelude_is_one_line_with_all_three_constants() {
        let p = workgroup_prelude([16, 4, 1]);
        assert_eq!(p.lines().count(), 1);
        assert!(p.ends_with('\n'));
        assert!(p.contains("WORKGROUP_SIZE_X: u32 = 16u"));
        assert!(p.contains("WORKGROUP_SIZE_Y: u32 = 4u"));
        assert!(p.contains("WORKGROUP_SIZE_Z: u32 = 1u"));
    }
}
