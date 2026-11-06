use game_toolkit_gfx::{Graphics, TextureId};

/// An RGBA texture a compute shader writes and the [`Painter`](game_toolkit_gfx::Painter)
/// draws: the usual way to show a GPU simulation without reading it back.
///
/// Bind [`Self::binding`] as [`Binding::StorageTextureWrite`](crate::Binding::StorageTextureWrite)`(`[`StorageTexture::FORMAT`]`)`
/// and write it with `textureStore` from a
/// `var out: texture_storage_2d<rgba8unorm, write>;`; draw [`Self::id`] like any texture
/// (`p.sprite(tex.id(), pos, size)`). Values written are sRGB-encoded, like the pixels of a
/// PNG: `vec4(1.0, 0.5, 0.0, 1.0)` looks like `#ff8000`.
#[derive(Debug)]
pub struct StorageTexture {
    id: TextureId,
    storage_view: wgpu::TextureView,
    size: [u32; 2],
}

impl StorageTexture {
    /// The storage format, as named in WGSL (`rgba8unorm`). Allowed for write-only storage
    /// textures on every WebGPU device.
    pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

    /// Create a `width x height` texture (initially transparent black) and register it with
    /// `gfx` for drawing.
    pub fn new(gfx: &mut Graphics, width: u32, height: u32, label: &str) -> Self {
        let texture = gfx.device().create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::FORMAT,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST,
            // Sampled through an sRGB view so stored values are read as sRGB, like PNGs.
            view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
        });
        let storage_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some(label),
            format: Some(Self::FORMAT),
            usage: Some(wgpu::TextureUsages::STORAGE_BINDING),
            ..Default::default()
        });
        let sampled_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some(label),
            format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
            // sRGB formats can't be storage-bound; this view is only sampled.
            usage: Some(wgpu::TextureUsages::TEXTURE_BINDING),
            ..Default::default()
        });
        let id = gfx.register_texture(texture, sampled_view, Some(label));
        Self {
            id,
            storage_view,
            size: [width, height],
        }
    }

    /// The handle to draw with.
    pub fn id(&self) -> TextureId {
        self.id
    }

    /// Size in pixels, `[width, height]`.
    pub fn size(&self) -> [u32; 2] {
        self.size
    }

    /// The write-only storage view, for a kernel's bind group.
    pub fn binding(&self) -> wgpu::BindingResource<'_> {
        wgpu::BindingResource::TextureView(&self.storage_view)
    }
}
