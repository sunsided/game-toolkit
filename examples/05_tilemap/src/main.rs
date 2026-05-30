use game_toolkit::prelude::*;

struct App {
    map: Tilemap,
}

fn build_procedural_atlas(gfx: &mut Graphics) -> TextureId {
    // 4 tiles in a row: empty(skipped, idx 0), grass, sand, water, stone.
    let tile = 32u32;
    let cols = 4u32;
    let w = tile * cols;
    let h = tile;
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    let colors = [
        (96, 192, 96),   // grass
        (224, 200, 96),  // sand
        (64, 128, 224),  // water
        (160, 160, 168), // stone
    ];
    for (i, (r, g, b)) in colors.iter().enumerate() {
        for y in 0..tile {
            for x in 0..tile {
                let px = (i as u32 * tile + x) as usize;
                let py = y as usize;
                let idx = (py * w as usize + px) * 4;
                let edge = x < 1 || y < 1 || x >= tile - 1 || y >= tile - 1;
                let mul = if edge { 70 } else { 100 };
                rgba[idx] = (*r as u32 * mul / 100) as u8;
                rgba[idx + 1] = (*g as u32 * mul / 100) as u8;
                rgba[idx + 2] = (*b as u32 * mul / 100) as u8;
                rgba[idx + 3] = 255;
            }
        }
    }
    gfx.create_texture_rgba(w, h, &rgba, Some("tile-atlas"))
}

impl Game for App {
    fn init(ctx: &mut Context) -> Result<Self> {
        let atlas = build_procedural_atlas(&mut ctx.gfx);
        let (w_px, h_px) = ctx.gfx.size();
        let tw = 32u32;
        let cols = (w_px / tw).max(1);
        let rows = (h_px / tw).max(1);
        let mut map = Tilemap::new(atlas, [128, 32], [tw, tw], cols, rows);
        for y in 0..rows {
            for x in 0..cols {
                let r = ((x * 31 + y * 17 + 5) % 11) as u16;
                let tile = match r {
                    0..=5 => 1, // grass
                    6..=7 => 2, // sand
                    8 => 3,     // water
                    _ => 4,     // stone
                };
                map.set(x, y, tile);
            }
        }
        Ok(Self { map })
    }
    fn update(&mut self, ctx: &mut Context, _dt: f32) {
        if ctx.input.key_pressed(Key::Escape) {
            ctx.quit();
        }
    }
    fn render(&mut self, ctx: &mut Context, frame: &mut Frame) {
        let mut p = frame.painter(&mut ctx.gfx);
        p.clear([0.0, 0.0, 0.0, 1.0]);
        self.map.draw(&mut p);
        p.text([8.0, 8.0], "05_tilemap", 22.0, [1.0, 1.0, 1.0, 1.0]);
    }
}

fn main() -> Result<()> {
    env_logger::init();
    run::<App>(AppConfig {
        title: "05_tilemap".into(),
        width: 800,
        height: 608,
        ..Default::default()
    })
}
