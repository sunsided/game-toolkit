use crate::painter::Painter;
use crate::sprite::{BlendMode, SpriteInstance};
use crate::texture::TextureId;

/// Simple grid tilemap backed by an atlas. `tiles` indexes into the atlas left-to-right,
/// top-to-bottom; index `0` is treated as empty and skipped.
///
/// Rendering walks the visible bounds each frame and emits one [`SpriteInstance`] per
/// non-empty cell into the sprite batcher. For jam-scale maps (<= ~10k visible tiles)
/// this is fine; chunked rebuild becomes worthwhile only for very large maps.
pub struct Tilemap {
    pub atlas: TextureId,
    pub atlas_size: [u32; 2],
    pub tile_size: [u32; 2],
    pub width: u32,
    pub height: u32,
    /// Origin in world space (top-left corner of cell (0,0)).
    pub origin: [f32; 2],
    pub tiles: Vec<u16>,
    pub layer: i16,
}

impl Tilemap {
    pub fn new(
        atlas: TextureId,
        atlas_size: [u32; 2],
        tile_size: [u32; 2],
        width: u32,
        height: u32,
    ) -> Self {
        Self {
            atlas,
            atlas_size,
            tile_size,
            width,
            height,
            origin: [0.0, 0.0],
            tiles: vec![0; (width * height) as usize],
            layer: 0,
        }
    }

    pub fn set(&mut self, x: u32, y: u32, tile: u16) {
        if x < self.width && y < self.height {
            self.tiles[(y * self.width + x) as usize] = tile;
        }
    }

    pub fn get(&self, x: u32, y: u32) -> u16 {
        if x < self.width && y < self.height {
            self.tiles[(y * self.width + x) as usize]
        } else {
            0
        }
    }

    fn tiles_per_row(&self) -> u32 {
        (self.atlas_size[0] / self.tile_size[0]).max(1)
    }

    /// Atlas UV rect for a 1-based tile index.
    fn uv_for(&self, tile: u16) -> ([f32; 2], [f32; 2]) {
        let idx = (tile - 1) as u32;
        let cols = self.tiles_per_row();
        let cx = idx % cols;
        let cy = idx / cols;
        let u0 = (cx * self.tile_size[0]) as f32 / self.atlas_size[0] as f32;
        let v0 = (cy * self.tile_size[1]) as f32 / self.atlas_size[1] as f32;
        let u1 = ((cx + 1) * self.tile_size[0]) as f32 / self.atlas_size[0] as f32;
        let v1 = ((cy + 1) * self.tile_size[1]) as f32 / self.atlas_size[1] as f32;
        ([u0, v0], [u1, v1])
    }

    /// Emit all non-empty cells as sprite instances on the given painter.
    pub fn draw(&self, painter: &mut Painter) {
        let tw = self.tile_size[0] as f32;
        let th = self.tile_size[1] as f32;
        for y in 0..self.height {
            for x in 0..self.width {
                let t = self.tiles[(y * self.width + x) as usize];
                if t == 0 {
                    continue;
                }
                let (uv_min, uv_max) = self.uv_for(t);
                let pos = [
                    self.origin[0] + x as f32 * tw,
                    self.origin[1] + y as f32 * th,
                ];
                let inst = SpriteInstance::at(pos, [tw, th]).with_uv(uv_min, uv_max);
                painter.sprite_ex(self.atlas, inst, self.layer, BlendMode::Alpha);
            }
        }
    }
}
