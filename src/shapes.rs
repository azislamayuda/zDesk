use tiny_skia::Pixmap;

// Pixel coverage uses global window coordinates, so drawing the same shape in
// eight-row strips gives the same pixels as drawing it in one full framebuffer.
fn blend(pixel: &mut [u8], rgba: [u8; 4], coverage: f32) {
    let alpha = (f32::from(rgba[3]) * coverage.clamp(0.0, 1.0)).round() as u32;
    if alpha == 0 {
        return;
    }
    let remaining = 255 - alpha;
    for channel in 0..3 {
        let source = (u32::from(rgba[channel]) * alpha + 127) / 255;
        pixel[channel] = (source + (u32::from(pixel[channel]) * remaining + 127) / 255) as u8;
    }
    pixel[3] = (alpha + (u32::from(pixel[3]) * remaining + 127) / 255) as u8;
}

pub fn dot(
    strip: &mut Pixmap,
    strip_y: i32,
    x: f32,
    y: f32,
    radius: f32,
    outline_width: f32,
    rgba: [u8; 4],
    active: bool,
) {
    let width = strip.width() as i32;
    let height = strip.height() as i32;
    let reach = radius + outline_width / 2.0 + 1.0;
    let x_start = ((x - reach).floor() as i32).clamp(0, width);
    let x_end = ((x + reach).ceil() as i32).clamp(0, width);
    let y_start = ((y - reach).floor() as i32 - strip_y).clamp(0, height);
    let y_end = ((y + reach).ceil() as i32 - strip_y).clamp(0, height);
    let pixels = strip.data_mut();
    for row in y_start..y_end {
        let dy = strip_y as f32 + row as f32 + 0.5 - y;
        for col in x_start..x_end {
            let dx = col as f32 + 0.5 - x;
            let distance = dx.hypot(dy);
            let coverage = if active {
                radius + 0.5 - distance
            } else {
                outline_width / 2.0 + 0.5 - (distance - radius).abs()
            };
            if coverage > 0.0 {
                let offset = ((row * width + col) * 4) as usize;
                blend(&mut pixels[offset..offset + 4], rgba, coverage);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn ring(
    strip: &mut Pixmap,
    strip_y: i32,
    center_x: f32,
    center_y: f32,
    radius: f32,
    thickness: f32,
    start_degrees: f32,
    fraction: f32,
    background: [u8; 4],
    foreground: [u8; 4],
) {
    let width = strip.width() as i32;
    let height = strip.height() as i32;
    let half = thickness / 2.0;
    let reach = radius + half + 1.0;
    let x_start = ((center_x - reach).floor() as i32).clamp(0, width);
    let x_end = ((center_x + reach).ceil() as i32).clamp(0, width);
    let y_start = ((center_y - reach).floor() as i32 - strip_y).clamp(0, height);
    let y_end = ((center_y + reach).ceil() as i32 - strip_y).clamp(0, height);
    let fraction = fraction.clamp(0.0, 1.0);
    let sweep = fraction * 360.0;
    let start_radians = start_degrees.to_radians();
    let end_radians = (start_degrees + sweep).to_radians();
    let start = (
        center_x + radius * start_radians.cos(),
        center_y + radius * start_radians.sin(),
    );
    let end = (
        center_x + radius * end_radians.cos(),
        center_y + radius * end_radians.sin(),
    );
    let pixels = strip.data_mut();
    for row in y_start..y_end {
        let py = strip_y as f32 + row as f32 + 0.5;
        let dy = py - center_y;
        for col in x_start..x_end {
            let px = col as f32 + 0.5;
            let dx = px - center_x;
            let distance = dx.hypot(dy);
            let coverage = (half + 0.5 - (distance - radius).abs()).clamp(0.0, 1.0);
            if coverage == 0.0 {
                continue;
            }
            let offset = ((row * width + col) * 4) as usize;
            let pixel = &mut pixels[offset..offset + 4];
            blend(pixel, background, coverage);
            if fraction == 0.0 {
                continue;
            }
            let in_arc = fraction == 1.0
                || (dy.atan2(dx).to_degrees() - start_degrees).rem_euclid(360.0) <= sweep;
            let cap_coverage = if fraction == 1.0 {
                0.0
            } else {
                (half + 0.5 - (px - start.0).hypot(py - start.1))
                    .max(half + 0.5 - (px - end.0).hypot(py - end.1))
                    .clamp(0.0, 1.0)
            };
            blend(
                pixel,
                foreground,
                if in_arc { coverage } else { cap_coverage },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circles_and_arcs_match_across_strip_boundaries() {
        let draw = |pixmap: &mut Pixmap, y: i32| {
            ring(
                pixmap,
                y,
                50.0,
                32.0,
                23.0,
                5.0,
                -90.0,
                0.37,
                [40, 30, 35, 200],
                [255, 100, 80, 255],
            );
            dot(pixmap, y, 50.0, 7.5, 5.0, 1.0, [255, 240, 230, 255], true);
        };

        let mut full = Pixmap::new(100, 64).unwrap();
        draw(&mut full, 0);
        let mut joined = Vec::new();
        for y in (0..64).step_by(8) {
            let mut strip = Pixmap::new(100, 8).unwrap();
            draw(&mut strip, y);
            joined.extend_from_slice(strip.data());
        }
        let mismatch = full
            .data()
            .iter()
            .zip(&joined)
            .position(|(left, right)| left != right);
        assert!(mismatch.is_none(), "first mismatched byte: {mismatch:?}");
    }
}
