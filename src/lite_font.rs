//! Small glyph rasterizer for the labels and values outside the large clock.
//!
//! `ttf-parser` keeps the font in its original compact form. Only glyphs
//! actually used by a widget are outlined and cached as small alpha masks.

use crate::AppResult;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Transform};
use ttf_parser::{Face, GlyphId, OutlineBuilder};

struct GlyphMask {
    x: i32,
    y: i32,
    width: usize,
    height: usize,
    coverage: Vec<u8>,
    advance: f32,
}

struct PlacedGlyph {
    mask: Rc<GlyphMask>,
    x: i32,
    y: i32,
}

/// Owns one font file and lazily caches glyph masks for each requested size.
pub struct LiteFont {
    bytes: Vec<u8>,
    cache: RefCell<HashMap<(char, u32), Rc<GlyphMask>>>,
}

/// Text positioned in desktop-window coordinates.
pub struct PreparedText {
    glyphs: Vec<PlacedGlyph>,
    shift_x: i32,
    shift_y: i32,
    color: [u8; 4],
    bounds: (i32, i32, i32, i32),
    advance_width: f32,
}

impl LiteFont {
    pub fn load(path: &Path) -> AppResult<Self> {
        let bytes = std::fs::read(path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        Face::parse(&bytes, 0)
            .map_err(|error| format!("cannot parse {}: {error:?}", path.display()))?;
        Ok(Self {
            bytes,
            cache: RefCell::new(HashMap::new()),
        })
    }

    /// Check every configured character before the application starts rendering.
    pub fn validate_characters(&self, value: &str, context: &str) -> AppResult<()> {
        let face = Face::parse(&self.bytes, 0)
            .map_err(|error| format!("cannot parse {context} font: {error:?}"))?;
        for character in value.chars() {
            if face.glyph_index(character).is_none() {
                return Err(format!("{context} font lacks character {character:?}").into());
            }
        }
        Ok(())
    }

    /// Center a run by its visible pixels, matching the existing clock layout.
    pub fn prepare_text(
        &self,
        label: &str,
        size: f32,
        center_x: i32,
        center_y: i32,
        rgba: [u8; 4],
    ) -> AppResult<PreparedText> {
        let mut text = self.prepare_text_at(label, size, 0, 0, rgba)?;
        let (left, top, right, bottom) = text.bounds;
        text.shift_x = center_x - (left + right) / 2;
        text.shift_y = center_y - (top + bottom) / 2;
        Ok(text)
    }

    /// Position a run by its pen origin and baseline. Adjacent runs can use
    /// `advance_width()` to share one baseline while changing color.
    pub fn prepare_text_at(
        &self,
        label: &str,
        size: f32,
        origin_x: i32,
        baseline_y: i32,
        rgba: [u8; 4],
    ) -> AppResult<PreparedText> {
        if !size.is_finite() || size <= 0.0 {
            return Err("text font size must be a positive finite number".into());
        }

        let face = Face::parse(&self.bytes, 0)
            .map_err(|error| format!("cannot parse widget font: {error:?}"))?;
        let scale = size / f32::from(face.units_per_em());
        let mut cache = self.cache.borrow_mut();
        let mut glyphs = Vec::with_capacity(label.chars().count());
        let mut pen_x = 0.0_f32;
        let mut bounds = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);

        for character in label.chars() {
            let mask = if let Some(mask) = cache.get(&(character, size.to_bits())) {
                Rc::clone(mask)
            } else {
                let id = face
                    .glyph_index(character)
                    .ok_or_else(|| format!("widget font lacks character {character:?}"))?;
                let mask = Rc::new(rasterize(&face, id, scale)?);
                cache.insert((character, size.to_bits()), Rc::clone(&mask));
                mask
            };

            let x = pen_x.round() as i32 + mask.x;
            let y = mask.y;
            if mask.width > 0 && mask.height > 0 {
                bounds.0 = bounds.0.min(x);
                bounds.1 = bounds.1.min(y);
                bounds.2 = bounds.2.max(x + mask.width as i32);
                bounds.3 = bounds.3.max(y + mask.height as i32);
            }
            pen_x += mask.advance;
            glyphs.push(PlacedGlyph { mask, x, y });
        }
        if bounds.0 == i32::MAX {
            return Err("widget font produced no visible glyphs".into());
        }
        Ok(PreparedText {
            glyphs,
            shift_x: origin_x,
            shift_y: baseline_y,
            color: rgba,
            bounds,
            advance_width: pen_x,
        })
    }
}

impl PreparedText {
    pub fn translate(&mut self, dx: i32, dy: i32) {
        self.shift_x += dx;
        self.shift_y += dy;
    }

    pub fn advance_width(&self) -> f32 {
        self.advance_width
    }

    pub fn visual_bounds(&self) -> (i32, i32, i32, i32) {
        let (left, top, right, bottom) = self.bounds;
        (
            left + self.shift_x,
            top + self.shift_y,
            right + self.shift_x,
            bottom + self.shift_y,
        )
    }

    /// Blend into a reusable strip whose first row is at `top` in the window.
    /// The caller clears the strip once before drawing all scene elements.
    pub fn draw_strip(&self, strip: &mut Pixmap, top: u32) {
        let strip_width = i64::from(strip.width());
        let strip_height = i64::from(strip.height());
        let pixels = strip.data_mut();
        for placed in &self.glyphs {
            let mask = &placed.mask;
            let glyph_x = i64::from(placed.x) + i64::from(self.shift_x);
            let glyph_y = i64::from(placed.y) + i64::from(self.shift_y) - i64::from(top);
            let mask_width = mask.width as i64;
            let mask_height = mask.height as i64;
            let start_x = (-glyph_x).clamp(0, mask_width) as usize;
            let end_x = (strip_width - glyph_x).clamp(0, mask_width) as usize;
            let start_y = (-glyph_y).clamp(0, mask_height) as usize;
            let end_y = (strip_height - glyph_y).clamp(0, mask_height) as usize;
            for y in start_y..end_y {
                let target_y = glyph_y + y as i64;
                for x in start_x..end_x {
                    let coverage = u32::from(mask.coverage[y * mask.width + x]);
                    let alpha = (coverage * u32::from(self.color[3]) + 127) / 255;
                    if alpha == 0 {
                        continue;
                    }
                    let target_x = glyph_x + x as i64;
                    let offset = ((target_y * strip_width + target_x) * 4) as usize;
                    let remaining = 255 - alpha;
                    for channel in 0..3 {
                        let source = (u32::from(self.color[channel]) * alpha + 127) / 255;
                        let destination = u32::from(pixels[offset + channel]);
                        pixels[offset + channel] =
                            (source + (destination * remaining + 127) / 255) as u8;
                    }
                    let destination_alpha = u32::from(pixels[offset + 3]);
                    pixels[offset + 3] =
                        (alpha + (destination_alpha * remaining + 127) / 255) as u8;
                }
            }
        }
    }
}

fn rasterize(face: &Face<'_>, id: GlyphId, scale: f32) -> AppResult<GlyphMask> {
    let advance = f32::from(face.glyph_hor_advance(id).unwrap_or(0)) * scale;
    let Some(box_) = face.glyph_bounding_box(id) else {
        return Ok(GlyphMask {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            coverage: Vec::new(),
            advance,
        });
    };
    let left = (f32::from(box_.x_min) * scale).floor();
    let top = (-f32::from(box_.y_max) * scale).floor();
    let right = (f32::from(box_.x_max) * scale).ceil();
    let bottom = (-f32::from(box_.y_min) * scale).ceil();
    let width = (right - left) as u32;
    let height = (bottom - top) as u32;
    if width == 0 || height == 0 {
        return Ok(GlyphMask {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            coverage: Vec::new(),
            advance,
        });
    }
    let mut pixmap = Pixmap::new(width, height).ok_or("widget glyph bitmap is too large")?;
    let mut outline = OutlinePath {
        path: PathBuilder::new(),
        scale,
        left,
        top,
    };
    face.outline_glyph(id, &mut outline)
        .ok_or("widget font glyph has no vector outline")?;
    let path = outline
        .path
        .finish()
        .ok_or("widget font glyph has an empty outline")?;
    let mut paint = Paint::default();
    paint.set_color_rgba8(255, 255, 255, 255);
    paint.anti_alias = true;
    pixmap.fill_path(
        &path,
        &paint,
        FillRule::Winding,
        Transform::identity(),
        None,
    );
    let coverage = pixmap
        .data()
        .chunks_exact(4)
        .map(|pixel| pixel[3])
        .collect();
    Ok(GlyphMask {
        x: left as i32,
        y: top as i32,
        width: width as usize,
        height: height as usize,
        coverage,
        advance,
    })
}

struct OutlinePath {
    path: PathBuilder,
    scale: f32,
    left: f32,
    top: f32,
}

impl OutlinePath {
    fn point(&self, x: f32, y: f32) -> (f32, f32) {
        (x * self.scale - self.left, -y * self.scale - self.top)
    }
}

impl OutlineBuilder for OutlinePath {
    fn move_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.point(x, y);
        self.path.move_to(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.point(x, y);
        self.path.line_to(x, y);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (x1, y1) = self.point(x1, y1);
        let (x, y) = self.point(x, y);
        self.path.quad_to(x1, y1, x, y);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (x1, y1) = self.point(x1, y1);
        let (x2, y2) = self.point(x2, y2);
        let (x, y) = self.point(x, y);
        self.path.cubic_to(x1, y1, x2, y2, x, y);
    }

    fn close(&mut self) {
        self.path.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_widget_fonts_load() {
        let home = std::env::var_os("HOME").unwrap();
        let home = Path::new(&home);
        for path in [
            Path::new("/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf").to_path_buf(),
            home.join(".local/share/fonts/BebasNeue-Regular.ttf"),
            home.join(".local/share/fonts/Roboto/static/Roboto-Regular.ttf"),
        ] {
            if path.exists() {
                let font = LiteFont::load(&path).unwrap();
                assert!(
                    font.prepare_text("42°C", 15.0, 50, 20, [255; 4]).is_ok(),
                    "{}",
                    path.display()
                );
            }
        }
    }

    #[test]
    fn strips_match_full_frame_for_utf8_text_and_alpha() {
        let path = Path::new("/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf");
        if !path.exists() {
            return;
        }
        let font = LiteFont::load(path).unwrap();
        let text = font
            .prepare_text("42°C", 19.0, 38, 12, [255, 132, 72, 210])
            .unwrap();
        let mut full = Pixmap::new(80, 24).unwrap();
        text.draw_strip(&mut full, 0);
        let mut first = Pixmap::new(80, 12).unwrap();
        text.draw_strip(&mut first, 0);
        let mut second = Pixmap::new(80, 12).unwrap();
        text.draw_strip(&mut second, 12);
        let mut joined = first.data().to_vec();
        joined.extend_from_slice(second.data());
        assert_eq!(full.data(), joined);
        assert!(full.data().chunks_exact(4).any(|pixel| pixel[3] > 0));
    }

    #[test]
    fn repeated_text_reuses_glyph_bitmaps() {
        let path = Path::new("/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf");
        if !path.exists() {
            return;
        }
        let font = LiteFont::load(path).unwrap();
        let _first = font.prepare_text("CPU 4%", 11.0, 40, 20, [255; 4]).unwrap();
        let cached = font.cache.borrow().len();
        let _second = font
            .prepare_text("CPU 44%", 11.0, 40, 20, [255; 4])
            .unwrap();
        assert_eq!(font.cache.borrow().len(), cached);
    }

    #[test]
    fn validation_rejects_missing_characters_before_rasterization() {
        let path = Path::new("/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf");
        if !path.exists() {
            return;
        }
        let font = LiteFont::load(path).unwrap();
        font.validate_characters("CPU 42%", "test label").unwrap();
        let error = font
            .validate_characters("CPU\u{10ffff}", "test label")
            .unwrap_err();
        assert!(error
            .to_string()
            .contains("test label font lacks character"));
    }
}
