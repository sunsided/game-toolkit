use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TextureId(pub u32);

pub struct Texture {
    pub size: [u32; 2],
    pub(crate) bind_group: wgpu::BindGroup,
    #[allow(dead_code)]
    pub(crate) texture: wgpu::Texture,
    #[allow(dead_code)]
    pub(crate) view: wgpu::TextureView,
}

pub(crate) struct TextureRegistry {
    map: HashMap<TextureId, Texture>,
    next: u32,
    pub(crate) layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
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

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sprite.sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let mut me = Self {
            map: HashMap::new(),
            next: 1,
            layout,
            sampler,
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
        let new_id = self.create_from_rgba(device, queue, width, height, rgba, label);
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
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label,
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        let id = TextureId(self.next);
        self.next += 1;
        self.map.insert(
            id,
            Texture {
                size: [width, height],
                bind_group,
                texture,
                view,
            },
        );
        id
    }
}
