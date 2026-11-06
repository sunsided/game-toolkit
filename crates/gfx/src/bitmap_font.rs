//! Pixel fonts drawn from a glyph sheet through the sprite batcher: whole-pixel positions,
//! whole-number scales, no anti-aliasing.

use std::collections::HashMap;

use anyhow::{Context as _, Result, bail};

use crate::graphics::Graphics;
use crate::texture::TextureId;

/// Where one glyph sits in the font's texture and how it is placed.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Glyph {
    /// Top-left corner of the glyph in the texture, in pixels.
    pub src_pos: [u32; 2],
    /// Size of the glyph in the texture, in pixels.
    pub src_size: [u32; 2],
    /// Offset from the pen position (top of the line) to the glyph's top-left corner.
    pub offset: [i32; 2],
    /// How far the pen moves after this glyph.
    pub advance: i32,
}

/// How glyphs are laid out in a sheet for [`BitmapFont::from_grid`].
#[derive(Clone, Debug)]
pub struct GridLayout {
    /// Size of one cell in pixels; glyphs are read left to right, top to bottom.
    pub cell: [u32; 2],
    /// The characters in sheet order, one per cell, e.g. every printable ASCII character from
    /// `' '` to `'~'`. Cells past the end are ignored.
    pub chars: String,
    /// `false`: every glyph advances by the cell width (monospaced). `true`: each glyph is
    /// trimmed to its non-transparent columns and advances by that width plus
    /// [`GridLayout::spacing`]; fully transparent cells (space) keep half a cell.
    pub proportional: bool,
    /// Extra pixels between glyphs in proportional mode.
    pub spacing: u32,
}

impl GridLayout {
    /// Printable ASCII (`' '` to `'~'`) in cells of `width` × `height`, monospaced.
    pub fn ascii(width: u32, height: u32) -> Self {
        Self {
            cell: [width, height],
            chars: (' '..='~').collect(),
            proportional: false,
            spacing: 1,
        }
    }
}

/// A pixel font: glyphs from a sheet texture, drawn at their native size or whole multiples,
/// on the pixel grid, through the sprite batcher (so text sorts with sprites by layer).
///
/// Get one from [`BitmapFont::from_grid`] (a glyph sheet), [`BitmapFont::from_bmfont`] (an
/// AngelCode BMFont `.fnt` text file plus its page image), or use the built-in 5 × 7 font
/// through [`Painter::pixel_text`](crate::Painter::pixel_text) /
/// [`Graphics::builtin_font`]. Draw with [`Painter::bitmap_text`](crate::Painter::bitmap_text).
#[derive(Clone, Debug)]
pub struct BitmapFont {
    texture: TextureId,
    texture_size: [u32; 2],
    glyphs: HashMap<char, Glyph>,
    kerning: HashMap<(char, char), i32>,
    line_height: u32,
    fallback: Option<char>,
}

impl BitmapFont {
    /// Build a font from glyph metrics over an already-loaded texture.
    pub fn new(
        texture: TextureId,
        texture_size: [u32; 2],
        line_height: u32,
        glyphs: HashMap<char, Glyph>,
    ) -> Self {
        let fallback = glyphs.contains_key(&'?').then_some('?');
        Self {
            texture,
            texture_size,
            glyphs,
            kerning: HashMap::new(),
            line_height,
            fallback,
        }
    }

    /// Load a glyph sheet (PNG bytes) laid out as a grid. Glyph pixels are drawn with their
    /// own colour times the tint, so draw the sheet in white to colour text freely.
    pub fn from_grid(gfx: &mut Graphics, png: &[u8], layout: &GridLayout) -> Result<Self> {
        let img = image::load_from_memory(png)
            .context("decode glyph sheet")?
            .to_rgba8();
        let (w, h) = img.dimensions();
        let texture = gfx.create_texture_rgba(w, h, img.as_raw(), Some("bitmap_font"));
        Ok(Self::from_grid_rgba(texture, [w, h], img.as_raw(), layout))
    }

    /// [`BitmapFont::from_grid`] over RGBA pixels already uploaded as `texture`.
    ///
    /// # Panics
    ///
    /// If `rgba` holds fewer than `size[0] * size[1] * 4` bytes.
    pub fn from_grid_rgba(
        texture: TextureId,
        size: [u32; 2],
        rgba: &[u8],
        layout: &GridLayout,
    ) -> Self {
        let needed = size[0] as usize * size[1] as usize * 4;
        assert!(
            rgba.len() >= needed,
            "from_grid_rgba: {} bytes of RGBA for a {}x{} sheet, need {needed}",
            rgba.len(),
            size[0],
            size[1]
        );
        let [cw, ch] = [layout.cell[0].max(1), layout.cell[1].max(1)];
        let cols = size[0] / cw;
        let rows = size[1] / ch;
        let alpha = |x: u32, y: u32| rgba[((y * size[0] + x) * 4 + 3) as usize];
        let mut glyphs = HashMap::new();
        for (i, c) in layout.chars.chars().enumerate() {
            let i = i as u32;
            if i >= cols * rows {
                break;
            }
            let x0 = (i % cols) * cw;
            let y0 = (i / cols) * ch;
            let glyph = if layout.proportional {
                let used: Vec<u32> = (0..cw)
                    .filter(|&dx| (0..ch).any(|dy| alpha(x0 + dx, y0 + dy) > 0))
                    .collect();
                match (used.first(), used.last()) {
                    (Some(&l), Some(&r)) => Glyph {
                        src_pos: [x0 + l, y0],
                        src_size: [r - l + 1, ch],
                        offset: [0, 0],
                        advance: (r - l + 1 + layout.spacing) as i32,
                    },
                    _ => Glyph {
                        src_pos: [x0, y0],
                        src_size: [0, 0],
                        offset: [0, 0],
                        advance: (cw / 2).max(1) as i32,
                    },
                }
            } else {
                Glyph {
                    src_pos: [x0, y0],
                    src_size: [cw, ch],
                    offset: [0, 0],
                    advance: cw as i32,
                }
            };
            glyphs.insert(c, glyph);
        }
        Self::new(texture, size, ch, glyphs)
    }

    /// Load an AngelCode BMFont in its text format (`.fnt`) with a single page, given the
    /// page image's PNG bytes. Kerning pairs are applied.
    pub fn from_bmfont(gfx: &mut Graphics, fnt: &str, page_png: &[u8]) -> Result<Self> {
        let img = image::load_from_memory(page_png)
            .context("decode BMFont page")?
            .to_rgba8();
        let (w, h) = img.dimensions();
        let texture = gfx.create_texture_rgba(w, h, img.as_raw(), Some("bmfont"));
        Self::parse_bmfont(texture, [w, h], fnt)
    }

    /// The `file` of page 0 named in a BMFont text file, to find its image next to it.
    pub fn bmfont_page_file(fnt: &str) -> Option<String> {
        fnt.lines()
            .filter_map(parse_line)
            .find(|(tag, kv)| *tag == "page" && kv.get("id").is_none_or(|id| id == "0"))
            .and_then(|(_, kv)| kv.get("file").cloned())
    }

    /// Parse BMFont text metrics over an uploaded page texture.
    pub fn parse_bmfont(texture: TextureId, texture_size: [u32; 2], fnt: &str) -> Result<Self> {
        let mut line_height = None;
        let mut glyphs = HashMap::new();
        let mut kerning = HashMap::new();
        for (tag, kv) in fnt.lines().filter_map(parse_line) {
            let num = |k: &str| -> Result<i32> {
                kv.get(k)
                    .with_context(|| format!("BMFont {tag}: missing {k}"))?
                    .parse()
                    .with_context(|| format!("BMFont {tag}: bad {k}"))
            };
            match tag {
                "common" => {
                    line_height = Some(num("lineHeight")?.max(1) as u32);
                    if kv.get("pages").is_some_and(|p| p != "1") {
                        bail!("BMFont with more than one page is not supported");
                    }
                }
                "char" => {
                    let Some(c) = char::from_u32(num("id")? as u32) else {
                        continue;
                    };
                    glyphs.insert(
                        c,
                        Glyph {
                            src_pos: [num("x")?.max(0) as u32, num("y")?.max(0) as u32],
                            src_size: [num("width")?.max(0) as u32, num("height")?.max(0) as u32],
                            offset: [num("xoffset")?, num("yoffset")?],
                            advance: num("xadvance")?,
                        },
                    );
                }
                "kerning" => {
                    let (Some(a), Some(b)) = (
                        char::from_u32(num("first")? as u32),
                        char::from_u32(num("second")? as u32),
                    ) else {
                        continue;
                    };
                    kerning.insert((a, b), num("amount")?);
                }
                _ => {}
            }
        }
        let line_height = line_height.context("BMFont: missing `common` line")?;
        let mut font = Self::new(texture, texture_size, line_height, glyphs);
        font.kerning = kerning;
        Ok(font)
    }

    /// The built-in 5 × 7 pixel font (6 × 8 cells, printable ASCII, monospaced), uploaded to
    /// `gfx`. [`Graphics::builtin_font`] keeps one around; use this for a separate copy.
    pub fn builtin(gfx: &mut Graphics) -> Self {
        let (size, rgba) = builtin_sheet();
        let texture = gfx.create_texture_rgba(size[0], size[1], &rgba, Some("builtin_font"));
        Self::from_grid_rgba(texture, size, &rgba, &GridLayout::ascii(6, 8))
    }

    /// Replace characters the font lacks with `c` (default `'?'` if the font has it); `None`
    /// skips them.
    pub fn set_fallback(&mut self, c: Option<char>) {
        self.fallback = c;
    }

    pub fn texture(&self) -> TextureId {
        self.texture
    }

    pub fn texture_size(&self) -> [u32; 2] {
        self.texture_size
    }

    /// Height of one line at scale 1, in pixels.
    pub fn line_height(&self) -> u32 {
        self.line_height
    }

    pub fn glyph(&self, c: char) -> Option<&Glyph> {
        self.glyphs.get(&c)
    }

    /// Size `[width, height]` of `s` drawn at `scale`, in whole pixels: the widest line, and
    /// the height of all lines (`\n` or `\r\n` starts a new one). The width includes glyphs
    /// that overhang left of the start position (BMFont glyphs with a negative `xoffset`).
    /// Saturates instead of overflowing for absurd scales.
    pub fn measure(&self, s: &str, scale: u32) -> [u32; 2] {
        let mut width = 0i64;
        let mut lines = 0u32;
        for line in text_lines(s) {
            lines = lines.saturating_add(1);
            let mut pen = 0i64;
            let mut left = 0i64;
            let mut right = 0i64;
            self.layout_line(line, |g, x| {
                let x0 = i64::from(x) + i64::from(g.offset[0]);
                left = left.min(x0);
                right = right.max(x0 + i64::from(g.src_size[0]));
                pen = i64::from(x) + i64::from(g.advance);
            });
            width = width.max(pen.max(right) - left);
        }
        let scale = scale.max(1);
        let width = u32::try_from(width.max(0)).unwrap_or(u32::MAX);
        [
            width.saturating_mul(scale),
            lines.saturating_mul(self.line_height).saturating_mul(scale),
        ]
    }

    /// Call `f(glyph, pen_x)` for each glyph of one line at scale 1.
    pub(crate) fn layout_line(&self, line: &str, mut f: impl FnMut(&Glyph, i32)) {
        let mut pen = 0;
        let mut prev: Option<char> = None;
        for c in line.chars() {
            let Some((c, g)) = self.glyphs.get(&c).map(|g| (c, g)).or_else(|| {
                self.fallback
                    .and_then(|f| self.glyphs.get(&f).map(|g| (f, g)))
            }) else {
                continue;
            };
            if let Some(p) = prev {
                pen += self.kerning.get(&(p, c)).copied().unwrap_or(0);
            }
            f(g, pen);
            pen += g.advance;
            prev = Some(c);
        }
    }
}

/// The lines of `s` as text is drawn: split at `\n`, with a trailing `\r` (Windows line
/// endings) dropped so it doesn't render as a glyph.
pub(crate) fn text_lines(s: &str) -> impl Iterator<Item = &str> {
    s.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l))
}

/// One `key=value` line of a BMFont text file: the tag and its attributes.
fn parse_line(line: &str) -> Option<(&str, HashMap<String, String>)> {
    let line = line.trim();
    let (tag, mut rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    if tag.is_empty() {
        return None;
    }
    let mut kv = HashMap::new();
    loop {
        rest = rest.trim_start();
        let Some((key, after)) = rest.split_once('=') else {
            break;
        };
        let (value, next) = if let Some(quoted) = after.strip_prefix('"') {
            match quoted.split_once('"') {
                Some((v, n)) => (v, n),
                None => (quoted, ""),
            }
        } else {
            after.split_once(char::is_whitespace).unwrap_or((after, ""))
        };
        kv.insert(key.trim().to_owned(), value.to_owned());
        rest = next;
    }
    Some((tag, kv))
}

/// 5 × 7 glyphs for `' '..='~'`, one byte per row, top to bottom, bit 4 the leftmost pixel.
#[rustfmt::skip]
const BUILTIN_GLYPHS: [[u8; 7]; 95] = [
    [0x00,0x00,0x00,0x00,0x00,0x00,0x00], // ' '
    [0x04,0x04,0x04,0x04,0x04,0x00,0x04], // !
    [0x0A,0x0A,0x00,0x00,0x00,0x00,0x00], // "
    [0x0A,0x0A,0x1F,0x0A,0x1F,0x0A,0x0A], // #
    [0x04,0x0F,0x14,0x0E,0x05,0x1E,0x04], // $
    [0x19,0x19,0x02,0x04,0x08,0x13,0x13], // %
    [0x08,0x14,0x14,0x08,0x15,0x12,0x0D], // &
    [0x04,0x04,0x00,0x00,0x00,0x00,0x00], // '
    [0x02,0x04,0x08,0x08,0x08,0x04,0x02], // (
    [0x08,0x04,0x02,0x02,0x02,0x04,0x08], // )
    [0x00,0x04,0x15,0x0E,0x15,0x04,0x00], // *
    [0x00,0x04,0x04,0x1F,0x04,0x04,0x00], // +
    [0x00,0x00,0x00,0x00,0x06,0x04,0x08], // ,
    [0x00,0x00,0x00,0x1F,0x00,0x00,0x00], // -
    [0x00,0x00,0x00,0x00,0x00,0x06,0x06], // .
    [0x01,0x01,0x02,0x04,0x08,0x10,0x10], // /
    [0x0E,0x11,0x13,0x15,0x19,0x11,0x0E], // 0
    [0x04,0x0C,0x04,0x04,0x04,0x04,0x0E], // 1
    [0x0E,0x11,0x01,0x06,0x08,0x10,0x1F], // 2
    [0x1F,0x01,0x02,0x06,0x01,0x11,0x0E], // 3
    [0x02,0x06,0x0A,0x12,0x1F,0x02,0x02], // 4
    [0x1F,0x10,0x1E,0x01,0x01,0x11,0x0E], // 5
    [0x06,0x08,0x10,0x1E,0x11,0x11,0x0E], // 6
    [0x1F,0x01,0x02,0x04,0x08,0x08,0x08], // 7
    [0x0E,0x11,0x11,0x0E,0x11,0x11,0x0E], // 8
    [0x0E,0x11,0x11,0x0F,0x01,0x02,0x0C], // 9
    [0x00,0x06,0x06,0x00,0x06,0x06,0x00], // :
    [0x00,0x06,0x06,0x00,0x06,0x04,0x08], // ;
    [0x02,0x04,0x08,0x10,0x08,0x04,0x02], // <
    [0x00,0x00,0x1F,0x00,0x1F,0x00,0x00], // =
    [0x08,0x04,0x02,0x01,0x02,0x04,0x08], // >
    [0x0E,0x11,0x01,0x02,0x04,0x00,0x04], // ?
    [0x0E,0x11,0x17,0x15,0x17,0x10,0x0E], // @
    [0x0E,0x11,0x11,0x1F,0x11,0x11,0x11], // A
    [0x1E,0x11,0x11,0x1E,0x11,0x11,0x1E], // B
    [0x0E,0x11,0x10,0x10,0x10,0x11,0x0E], // C
    [0x1E,0x11,0x11,0x11,0x11,0x11,0x1E], // D
    [0x1F,0x10,0x10,0x1E,0x10,0x10,0x1F], // E
    [0x1F,0x10,0x10,0x1E,0x10,0x10,0x10], // F
    [0x0E,0x11,0x10,0x17,0x11,0x11,0x0F], // G
    [0x11,0x11,0x11,0x1F,0x11,0x11,0x11], // H
    [0x0E,0x04,0x04,0x04,0x04,0x04,0x0E], // I
    [0x07,0x02,0x02,0x02,0x02,0x12,0x0C], // J
    [0x11,0x12,0x14,0x18,0x14,0x12,0x11], // K
    [0x10,0x10,0x10,0x10,0x10,0x10,0x1F], // L
    [0x11,0x1B,0x15,0x15,0x11,0x11,0x11], // M
    [0x11,0x11,0x19,0x15,0x13,0x11,0x11], // N
    [0x0E,0x11,0x11,0x11,0x11,0x11,0x0E], // O
    [0x1E,0x11,0x11,0x1E,0x10,0x10,0x10], // P
    [0x0E,0x11,0x11,0x11,0x15,0x12,0x0D], // Q
    [0x1E,0x11,0x11,0x1E,0x14,0x12,0x11], // R
    [0x0F,0x10,0x10,0x0E,0x01,0x01,0x1E], // S
    [0x1F,0x04,0x04,0x04,0x04,0x04,0x04], // T
    [0x11,0x11,0x11,0x11,0x11,0x11,0x0E], // U
    [0x11,0x11,0x11,0x11,0x11,0x0A,0x04], // V
    [0x11,0x11,0x11,0x15,0x15,0x15,0x0A], // W
    [0x11,0x11,0x0A,0x04,0x0A,0x11,0x11], // X
    [0x11,0x11,0x0A,0x04,0x04,0x04,0x04], // Y
    [0x1F,0x01,0x02,0x04,0x08,0x10,0x1F], // Z
    [0x0E,0x08,0x08,0x08,0x08,0x08,0x0E], // [
    [0x10,0x10,0x08,0x04,0x02,0x01,0x01], // backslash
    [0x0E,0x02,0x02,0x02,0x02,0x02,0x0E], // ]
    [0x04,0x0A,0x11,0x00,0x00,0x00,0x00], // ^
    [0x00,0x00,0x00,0x00,0x00,0x00,0x1F], // _
    [0x08,0x04,0x00,0x00,0x00,0x00,0x00], // `
    [0x00,0x00,0x0E,0x01,0x0F,0x11,0x0F], // a
    [0x10,0x10,0x16,0x19,0x11,0x11,0x1E], // b
    [0x00,0x00,0x0E,0x10,0x10,0x11,0x0E], // c
    [0x01,0x01,0x0D,0x13,0x11,0x11,0x0F], // d
    [0x00,0x00,0x0E,0x11,0x1F,0x10,0x0E], // e
    [0x06,0x09,0x08,0x1C,0x08,0x08,0x08], // f
    [0x00,0x0F,0x11,0x11,0x0F,0x01,0x0E], // g
    [0x10,0x10,0x16,0x19,0x11,0x11,0x11], // h
    [0x04,0x00,0x0C,0x04,0x04,0x04,0x0E], // i
    [0x02,0x00,0x06,0x02,0x02,0x12,0x0C], // j
    [0x10,0x10,0x12,0x14,0x18,0x14,0x12], // k
    [0x0C,0x04,0x04,0x04,0x04,0x04,0x0E], // l
    [0x00,0x00,0x1A,0x15,0x15,0x11,0x11], // m
    [0x00,0x00,0x16,0x19,0x11,0x11,0x11], // n
    [0x00,0x00,0x0E,0x11,0x11,0x11,0x0E], // o
    [0x00,0x00,0x1E,0x11,0x1E,0x10,0x10], // p
    [0x00,0x00,0x0D,0x13,0x0F,0x01,0x01], // q
    [0x00,0x00,0x16,0x19,0x10,0x10,0x10], // r
    [0x00,0x00,0x0E,0x10,0x0E,0x01,0x1E], // s
    [0x08,0x08,0x1C,0x08,0x08,0x09,0x06], // t
    [0x00,0x00,0x11,0x11,0x11,0x13,0x0D], // u
    [0x00,0x00,0x11,0x11,0x11,0x0A,0x04], // v
    [0x00,0x00,0x11,0x11,0x15,0x15,0x0A], // w
    [0x00,0x00,0x11,0x0A,0x04,0x0A,0x11], // x
    [0x00,0x00,0x11,0x11,0x0F,0x01,0x0E], // y
    [0x00,0x00,0x1F,0x02,0x04,0x08,0x1F], // z
    [0x02,0x04,0x04,0x08,0x04,0x04,0x02], // {
    [0x04,0x04,0x04,0x04,0x04,0x04,0x04], // |
    [0x08,0x04,0x04,0x02,0x04,0x04,0x08], // }
    [0x00,0x00,0x08,0x15,0x02,0x00,0x00], // ~
];

/// The built-in glyphs as a 96 × 48 RGBA sheet (16 × 6 cells of 6 × 8), white on transparent.
fn builtin_sheet() -> ([u32; 2], Vec<u8>) {
    const COLS: u32 = 16;
    let size = [COLS * 6, 6 * 8];
    let mut rgba = vec![0u8; (size[0] * size[1] * 4) as usize];
    for (i, rows) in BUILTIN_GLYPHS.iter().enumerate() {
        let (cx, cy) = ((i as u32 % COLS) * 6, (i as u32 / COLS) * 8);
        for (dy, bits) in rows.iter().enumerate() {
            for dx in 0..5u32 {
                if bits & (0x10 >> dx) != 0 {
                    let o = (((cy + dy as u32) * size[0] + cx + dx) * 4) as usize;
                    rgba[o..o + 4].copy_from_slice(&[255; 4]);
                }
            }
        }
    }
    (size, rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn builtin() -> BitmapFont {
        let (size, rgba) = builtin_sheet();
        BitmapFont::from_grid_rgba(TextureId(0), size, &rgba, &GridLayout::ascii(6, 8))
    }

    #[test]
    fn builtin_measures_whole_pixels() {
        let f = builtin();
        assert_eq!(f.line_height(), 8);
        assert_eq!(f.measure("Hi!", 1), [18, 8]);
        assert_eq!(f.measure("Hi!", 3), [54, 24]);
        assert_eq!(f.measure("ab\nabcd", 2), [48, 32]);
        // Unknown characters fall back to '?'.
        assert_eq!(f.measure("é", 1), [6, 8]);
    }

    #[test]
    fn proportional_grid_trims_glyphs() {
        let (size, rgba) = builtin_sheet();
        let layout = GridLayout {
            proportional: true,
            ..GridLayout::ascii(6, 8)
        };
        let f = BitmapFont::from_grid_rgba(TextureId(0), size, &rgba, &layout);
        // 'i' is 3 pixels wide (0x0C/0x04/0x0E), plus 1 spacing.
        assert_eq!(f.glyph('i').map(|g| g.advance), Some(4));
        assert_eq!(f.glyph('M').map(|g| g.advance), Some(6));
        assert_eq!(f.glyph(' ').map(|g| g.advance), Some(3));
    }

    #[test]
    fn bmfont_text_parses() {
        let fnt = r#"info face="Tiny" size=8
common lineHeight=10 base=8 scaleW=64 scaleH=64 pages=1
page id=0 file="tiny_0.png"
chars count=2
char id=65 x=0 y=0 width=5 height=7 xoffset=0 yoffset=1 xadvance=6 page=0
char id=86 x=6 y=0 width=5 height=7 xoffset=0 yoffset=1 xadvance=6 page=0
kernings count=1
kerning first=65 second=86 amount=-1
"#;
        assert_eq!(
            BitmapFont::bmfont_page_file(fnt).as_deref(),
            Some("tiny_0.png")
        );
        let f = BitmapFont::parse_bmfont(TextureId(0), [64, 64], fnt).unwrap();
        assert_eq!(f.line_height(), 10);
        assert_eq!(
            f.glyph('A'),
            Some(&Glyph {
                src_pos: [0, 0],
                src_size: [5, 7],
                offset: [0, 1],
                advance: 6
            })
        );
        // Kerning pulls V one pixel closer.
        assert_eq!(f.measure("AV", 1), [11, 10]);
    }

    #[test]
    fn measure_counts_left_overhang_and_crlf() {
        let fnt = "common lineHeight=8 pages=1
char id=106 x=0 y=0 width=4 height=8 xoffset=-2 yoffset=0 xadvance=3
";
        let f = BitmapFont::parse_bmfont(TextureId(0), [16, 16], fnt).unwrap();
        // 'j' starts 2 px left of the pen and ends at 2: 4 px wide, pen ends at 3 -> 5.
        assert_eq!(f.measure("j", 1), [5, 8]);
        let b = builtin();
        assert_eq!(b.measure("ab\r\ncd", 1), b.measure("ab\ncd", 1));
        assert_eq!(b.measure("W", u32::MAX)[0], u32::MAX);
    }

    #[test]
    #[should_panic(expected = "need")]
    fn short_rgba_is_rejected() {
        let _ = BitmapFont::from_grid_rgba(TextureId(0), [6, 8], &[0; 4], &GridLayout::ascii(6, 8));
    }
}
