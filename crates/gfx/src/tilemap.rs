use crate::painter::Painter;
use crate::sprite::{BlendMode, SpriteInstance};
use crate::texture::TextureId;

/// Grid tilemap backed by an atlas. `tiles` indexes into the atlas left-to-right,
/// top-to-bottom; index `0` is treated as empty and skipped.
///
/// The map is split into square chunks (default 32x32 tiles). Each chunk caches its sprite
/// instances and only rebuilds them when a tile in it changes (or the origin moves). On
/// `draw`, chunks whose world-space AABB does not intersect the camera's visible rectangle
/// are skipped, so a large map only touches the tiles actually on screen.
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
    chunk_size: u32,
    chunk_cols: u32,
    chunks: Vec<Chunk>,
    last_origin: [f32; 2],
}

/// Cached sprite instances for one chunk, rebuilt only when `dirty`.
struct Chunk {
    instances: Vec<SpriteInstance>,
    dirty: bool,
}

impl Tilemap {
    pub fn new(
        atlas: TextureId,
        atlas_size: [u32; 2],
        tile_size: [u32; 2],
        width: u32,
        height: u32,
    ) -> Self {
        Self::with_chunk_size(atlas, atlas_size, tile_size, width, height, 32)
    }

    /// Like [`Tilemap::new`] but with an explicit chunk size (in tiles).
    pub fn with_chunk_size(
        atlas: TextureId,
        atlas_size: [u32; 2],
        tile_size: [u32; 2],
        width: u32,
        height: u32,
        chunk_size: u32,
    ) -> Self {
        let chunk_size = chunk_size.max(1);
        let chunk_cols = width.div_ceil(chunk_size);
        let chunk_rows = height.div_ceil(chunk_size);
        let chunks = (0..chunk_cols * chunk_rows)
            .map(|_| Chunk {
                instances: Vec::new(),
                dirty: true,
            })
            .collect();
        Self {
            atlas,
            atlas_size,
            tile_size,
            width,
            height,
            origin: [0.0, 0.0],
            tiles: vec![0; (width * height) as usize],
            layer: 0,
            chunk_size,
            chunk_cols,
            chunks,
            last_origin: [0.0, 0.0],
        }
    }

    pub fn set(&mut self, x: u32, y: u32, tile: u16) {
        if x < self.width && y < self.height {
            self.tiles[(y * self.width + x) as usize] = tile;
            let ci = (y / self.chunk_size) * self.chunk_cols + (x / self.chunk_size);
            self.chunks[ci as usize].dirty = true;
        }
    }

    pub fn get(&self, x: u32, y: u32) -> u16 {
        if x < self.width && y < self.height {
            self.tiles[(y * self.width + x) as usize]
        } else {
            0
        }
    }

    /// Mark every chunk dirty so the next [`Tilemap::draw`] rebuilds all instances. Call this
    /// after mutating `tiles`, `atlas_size`, or `tile_size` directly (bypassing [`set`]).
    ///
    /// [`set`]: Tilemap::set
    pub fn mark_all_dirty(&mut self) {
        for c in &mut self.chunks {
            c.dirty = true;
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

    /// Tile cell range `(x0, y0, x1, y1)` (half-open) covered by chunk `ci`.
    fn chunk_cells(&self, ci: usize) -> (u32, u32, u32, u32) {
        let ccx = ci as u32 % self.chunk_cols;
        let ccy = ci as u32 / self.chunk_cols;
        let x0 = ccx * self.chunk_size;
        let y0 = ccy * self.chunk_size;
        (
            x0,
            y0,
            (x0 + self.chunk_size).min(self.width),
            (y0 + self.chunk_size).min(self.height),
        )
    }

    /// World-space AABB `(min, max)` of chunk `ci`.
    fn chunk_aabb(&self, ci: usize) -> ([f32; 2], [f32; 2]) {
        let (x0, y0, x1, y1) = self.chunk_cells(ci);
        let tw = self.tile_size[0] as f32;
        let th = self.tile_size[1] as f32;
        (
            [
                self.origin[0] + x0 as f32 * tw,
                self.origin[1] + y0 as f32 * th,
            ],
            [
                self.origin[0] + x1 as f32 * tw,
                self.origin[1] + y1 as f32 * th,
            ],
        )
    }

    fn build_chunk(&self, ci: usize) -> Vec<SpriteInstance> {
        let (x0, y0, x1, y1) = self.chunk_cells(ci);
        let tw = self.tile_size[0] as f32;
        let th = self.tile_size[1] as f32;
        let mut instances = Vec::new();
        for y in y0..y1 {
            for x in x0..x1 {
                let t = self.tiles[(y * self.width + x) as usize];
                if t == 0 {
                    continue;
                }
                let (uv_min, uv_max) = self.uv_for(t);
                let pos = [
                    self.origin[0] + x as f32 * tw,
                    self.origin[1] + y as f32 * th,
                ];
                instances.push(SpriteInstance::at(pos, [tw, th]).with_uv(uv_min, uv_max));
            }
        }
        instances
    }

    /// Rebuild dirty chunks, then emit the instances of every chunk that overlaps the
    /// camera's visible rectangle into the sprite batcher.
    pub fn draw(&mut self, painter: &mut Painter) {
        // Moving the origin shifts every tile's world position, so all caches go stale.
        if self.origin != self.last_origin {
            self.mark_all_dirty();
            self.last_origin = self.origin;
        }
        for ci in 0..self.chunks.len() {
            if self.chunks[ci].dirty {
                self.chunks[ci].instances = self.build_chunk(ci);
                self.chunks[ci].dirty = false;
            }
        }

        let (vmin, vmax) = painter.visible_rect();
        for ci in 0..self.chunks.len() {
            let (cmin, cmax) = self.chunk_aabb(ci);
            if !rects_intersect(cmin, cmax, vmin, vmax) {
                continue;
            }
            for inst in &self.chunks[ci].instances {
                painter.sprite_ex(self.atlas, *inst, self.layer, BlendMode::Alpha);
            }
        }
    }
}

/// True if the axis-aligned rectangles `a` and `b` overlap.
fn rects_intersect(amin: [f32; 2], amax: [f32; 2], bmin: [f32; 2], bmax: [f32; 2]) -> bool {
    amin[0] < bmax[0] && amax[0] > bmin[0] && amin[1] < bmax[1] && amax[1] > bmin[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> Tilemap {
        // 256x256 tiles, 16px each, default 32-tile chunks -> 8x8 = 64 chunks.
        Tilemap::new(TextureId(0), [64, 64], [16, 16], 256, 256)
    }

    #[test]
    fn chunk_grid_dimensions() {
        let m = map();
        assert_eq!(m.chunk_cols, 8);
        assert_eq!(m.chunks.len(), 64); // 8x8 chunks
    }

    #[test]
    fn set_marks_only_the_owning_chunk() {
        let mut m = map();
        m.mark_all_dirty();
        for c in &mut m.chunks {
            c.dirty = false;
        }
        m.set(40, 70, 1); // chunk col 1 (40/32), row 2 (70/32) -> index 2*8 + 1 = 17
        assert!(m.chunks[17].dirty);
        assert_eq!(m.chunks.iter().filter(|c| c.dirty).count(), 1);
    }

    #[test]
    fn culling_skips_offscreen_chunks() {
        let m = map();
        // A viewport covering only the top-left ~2 tiles intersects exactly one chunk.
        let (vmin, vmax) = ([0.0, 0.0], [32.0, 32.0]);
        let visible = (0..m.chunks.len())
            .filter(|&ci| {
                let (cmin, cmax) = m.chunk_aabb(ci);
                rects_intersect(cmin, cmax, vmin, vmax)
            })
            .count();
        assert_eq!(visible, 1);
    }

    #[test]
    fn empty_tiles_emit_no_instances() {
        let mut m = map();
        m.set(3, 3, 5);
        let insts = m.build_chunk(0);
        assert_eq!(insts.len(), 1);
    }
}
