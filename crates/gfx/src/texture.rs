use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TextureId(pub u32);

/// What a texture shows for UVs outside `0..1`. Set per texture with
/// [`crate::Graphics::set_texture_wrap`].
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextureWrap {
    /// Repeat the edge texels (default). Right for sprites and atlas regions, which sample up
    /// to their edges and would bleed in the opposite edge with wrapping.
    #[default]
    Clamp,
    /// Tile the texture: a UV of `2.5` samples the same texel as `0.5`.
    Repeat,
    /// Tile, mirroring every other repetition so the seams match.
    MirrorRepeat,
}

impl TextureWrap {
    const ALL: [Self; 3] = [Self::Clamp, Self::Repeat, Self::MirrorRepeat];

    fn address_mode(self) -> wgpu::AddressMode {
        match self {
            Self::Clamp => wgpu::AddressMode::ClampToEdge,
            Self::Repeat => wgpu::AddressMode::Repeat,
            Self::MirrorRepeat => wgpu::AddressMode::MirrorRepeat,
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

pub struct Texture {
    pub size: [u32; 2],
    pub(crate) bind_group: wgpu::BindGroup,
    #[allow(dead_code)]
    pub(crate) texture: wgpu::Texture,
    pub(crate) view: wgpu::TextureView,
    pub(crate) wrap: TextureWrap,
}

pub(crate) struct TextureRegistry {
    map: HashMap<TextureId, Texture>,
    next: u32,
    pub(crate) layout: wgpu::BindGroupLayout,
    /// One sampler per [`TextureWrap`], indexed by [`TextureWrap::index`].
    samplers: [wgpu::Sampler; 3],
    white: TextureId,
}

impl TextureRegistry {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sprite.texture_bgl"),
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

        let samplers = TextureWrap::ALL.map(|wrap| {
            let mode = wrap.address_mode();
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("sprite.sampler"),
                address_mode_u: mode,
                address_mode_v: mode,
                address_mode_w: mode,
                mag_filter: wgpu::FilterMode::Nearest,
                min_filter: wgpu::FilterMode::Nearest,
                mipmap_filter: wgpu::MipmapFilterMode::Nearest,
                ..Default::default()
            })
        });

        let mut me = Self {
            map: HashMap::new(),
            next: 1,
            layout,
            samplers,
            white: TextureId(0),
        };
        me.white = me.create_from_rgba(device, queue, 1, 1, &[255, 255, 255, 255], Some("white"));
        me
    }

    pub fn white(&self) -> TextureId {
        self.white
    }

    pub fn bind_group(&self, id: TextureId) -> &wgpu::BindGroup {
        &self
            .map
            .get(&id)
            .unwrap_or_else(|| self.map.get(&self.white).expect("white texture"))
            .bind_group
    }

    pub fn get(&self, id: TextureId) -> Option<&Texture> {
        self.map.get(&id)
    }

    /// Switch `id` to `wrap` by rebuilding its bind group with the matching sampler. Returns
    /// `false` for an unknown id.
    pub fn set_wrap(&mut self, device: &wgpu::Device, id: TextureId, wrap: TextureWrap) -> bool {
        let Some(tex) = self.map.get(&id) else {
            return false;
        };
        if tex.wrap != wrap {
            let bind_group = self.create_bind_group(device, &tex.view, wrap, None);
            let tex = self.map.get_mut(&id).expect("checked above");
            tex.bind_group = bind_group;
            tex.wrap = wrap;
        }
        true
    }

    fn create_bind_group(
        &self,
        device: &wgpu::Device,
        view: &wgpu::TextureView,
        wrap: TextureWrap,
        label: Option<&str>,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label,
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.samplers[wrap.index()]),
                },
            ],
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_file(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        path: impl AsRef<Path>,
    ) -> Result<TextureId> {
        let path = path.as_ref();
        let img = image::open(path).with_context(|| format!("loading {}", path.display()))?;
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        Ok(self.create_from_rgba(
            device,
            queue,
            w,
            h,
            rgba.as_raw(),
            path.file_name().and_then(|s| s.to_str()),
        ))
    }

    /// Decode an encoded image (PNG) held in memory and upload it.
    pub fn load_bytes(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bytes: &[u8],
        label: Option<&str>,
    ) -> Result<TextureId> {
        let img = image::load_from_memory(bytes)
            .with_context(|| format!("decoding {}", label.unwrap_or("image")))?;
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        Ok(self.create_from_rgba(device, queue, w, h, rgba.as_raw(), label))
    }

    /// Replace the contents of `id` with a freshly decoded copy of `path`. Keeps the same
    /// `TextureId` so anything already holding it stays valid.
    pub fn reload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: TextureId,
        path: impl AsRef<Path>,
    ) -> Result<()> {
        let path = path.as_ref();
        let img = image::open(path).with_context(|| format!("reloading {}", path.display()))?;
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        self.replace(
            device,
            queue,
            id,
            w,
            h,
            rgba.as_raw(),
            path.file_name().and_then(|s| s.to_str()),
        );
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn replace(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: TextureId,
        width: u32,
        height: u32,
        rgba: &[u8],
        label: Option<&str>,
    ) {
        let wrap = self.map.get(&id).map(|t| t.wrap).unwrap_or_default();
        let new_id = self.create_from_rgba(device, queue, width, height, rgba, label);
        // Keep the wrap mode across hot-reloads.
        self.set_wrap(device, new_id, wrap);
        if let Some(new_tex) = self.map.remove(&new_id) {
            self.map.insert(id, new_tex);
        }
    }

    pub fn create_from_rgba(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        rgba: &[u8],
        label: Option<&str>,
    ) -> TextureId {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            size,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.create_bind_group(device, &view, TextureWrap::Clamp, label);

        let id = TextureId(self.next);
        self.next += 1;
        self.map.insert(
            id,
            Texture {
                size: [width, height],
                bind_group,
                texture,
                view,
                wrap: TextureWrap::Clamp,
            },
        );
        id
    }
}
