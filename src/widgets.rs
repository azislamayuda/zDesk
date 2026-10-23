use crate::clock::LocalClock;
use crate::config::{Ring, Rings, Settings};
use crate::lite_font::{LiteFont, PreparedText};
use crate::metrics::Metrics;
use crate::shapes;
use crate::AppResult;
use tiny_skia::{Color, Pixmap};

pub struct Fonts {
    weekday: Option<LiteFont>,
    greeting: Option<LiteFont>,
    rings: Option<LiteFont>,
}

impl Fonts {
    pub fn load(settings: &Settings) -> AppResult<Self> {
        let fonts = Self {
            weekday: settings
                .weekday
                .as_ref()
                .map(|w| LiteFont::load(&w.label_font_file))
                .transpose()?,
            greeting: settings
                .greeting
                .as_ref()
                .map(|g| LiteFont::load(&g.font_file))
                .transpose()?,
            rings: settings
                .rings
                .as_ref()
                .map(|r| LiteFont::load(&r.font_file))
                .transpose()?,
        };

        if let (Some(style), Some(font)) = (&settings.weekday, &fonts.weekday) {
            for (index, label) in style.labels.iter().enumerate() {
                font.validate_characters(label, &format!("weekday label {}", index + 1))?;
            }
        }
        if let (Some(style), Some(font)) = (&settings.greeting, &fonts.greeting) {
            for (name, value) in [
                ("greeting morning_text", &style.morning_text),
                ("greeting afternoon_text", &style.afternoon_text),
                ("greeting evening_text", &style.evening_text),
                ("greeting night_text", &style.night_text),
                ("greeting name", &style.name),
                ("greeting suffix", &style.suffix),
            ] {
                font.validate_characters(value, name)?;
            }
        }
        if let (Some(style), Some(font)) = (&settings.rings, &fonts.rings) {
            for (name, ring) in [
                ("rings.cpu", &style.cpu),
                ("rings.temperature", &style.temperature),
                ("rings.ram", &style.ram),
            ] {
                font.validate_characters(&ring.label, &format!("{name} label"))?;
                font.validate_characters(&ring.unit, &format!("{name} unit"))?;
            }
            font.validate_characters("0123456789-", "rings dynamic values")?;
        }

        Ok(fonts)
    }

    pub fn prepare_main(&self, settings: &Settings, local: LocalClock) -> AppResult<MainScene> {
        let mut scene = MainScene {
            texts: Vec::new(),
            dots: Vec::new(),
        };
        if let (Some(style), Some(font)) = (&settings.weekday, &self.weekday) {
            let active_index = (local.weekday + 6) % 7;
            for (index, label) in style.labels.iter().enumerate() {
                scene.texts.push(font.prepare_text(
                    label,
                    style.label_font_size,
                    style.center_x[index],
                    style.label_center_y,
                    style.label_rgba,
                )?);
                let active = index == active_index;
                let color = if !active {
                    style.inactive_rgba
                } else if index <= 2 {
                    style.active_mon_wed_rgba
                } else if index <= 5 {
                    style.active_thu_sat_rgba
                } else {
                    style.active_sun_rgba
                };
                scene.dots.push(Dot {
                    x: style.center_x[index] as f32,
                    y: style.dot_center_y as f32,
                    radius: style.dot_radius,
                    stroke_width: style.dot_stroke_width,
                    color,
                    active,
                });
            }
        }
        if let (Some(style), Some(font)) = (&settings.greeting, &self.greeting) {
            let hour = local.hour as u8;
            let prefix = if hour >= style.night_start || hour < style.morning_start {
                &style.night_text
            } else if hour >= style.evening_start {
                &style.evening_text
            } else if hour >= style.afternoon_start {
                &style.afternoon_text
            } else {
                &style.morning_text
            };
            let mut parts = Vec::with_capacity(3);
            let mut pen_x = 0.0_f32;
            for (value, color) in [
                (prefix.as_str(), style.rgba),
                (style.name.as_str(), style.name_rgba),
                (style.suffix.as_str(), style.rgba),
            ] {
                if value.is_empty() {
                    continue;
                }
                let text =
                    font.prepare_text_at(value, style.font_size, pen_x.round() as i32, 0, color)?;
                pen_x += text.advance_width();
                parts.push(text);
            }
            let mut bounds = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
            for part in &parts {
                let (left, top, right, bottom) = part.visual_bounds();
                bounds.0 = bounds.0.min(left);
                bounds.1 = bounds.1.min(top);
                bounds.2 = bounds.2.max(right);
                bounds.3 = bounds.3.max(bottom);
            }
            let dx = style.center_x - (bounds.0 + bounds.2) / 2;
            let dy = style.center_y - (bounds.1 + bounds.3) / 2;
            for mut part in parts {
                part.translate(dx, dy);
                scene.texts.push(part);
            }
        }
        Ok(scene)
    }

    pub fn prepare_rings(
        &self,
        settings: &Settings,
        metrics: &Metrics,
    ) -> AppResult<Option<RingScene>> {
        let (Some(style), Some(font)) = (&settings.rings, &self.rings) else {
            return Ok(None);
        };
        let visuals = [
            RingVisual::new(&style.cpu, style, font, Some(metrics.cpu_percent))?,
            RingVisual::new(&style.temperature, style, font, metrics.temp_celsius)?,
            RingVisual::new(&style.ram, style, font, Some(metrics.ram_percent))?,
        ];
        Ok(Some(RingScene {
            visuals,
            background: style.background_rgba,
            radius: style.radius,
            thickness: style.thickness,
            start_degrees: style.start_angle_degrees,
        }))
    }
}

struct Dot {
    x: f32,
    y: f32,
    radius: f32,
    stroke_width: f32,
    color: [u8; 4],
    active: bool,
}

pub struct MainScene {
    texts: Vec<PreparedText>,
    dots: Vec<Dot>,
}

impl MainScene {
    pub fn draw_strip(&self, strip: &mut Pixmap, strip_y: u32) {
        for dot in &self.dots {
            shapes::dot(
                strip,
                strip_y as i32,
                dot.x,
                dot.y,
                dot.radius,
                dot.stroke_width,
                dot.color,
                dot.active,
            );
        }
        for text in &self.texts {
            text.draw_strip(strip, strip_y);
        }
    }
}

struct RingVisual {
    x: f32,
    y: f32,
    fraction: f32,
    foreground: [u8; 4],
    label: PreparedText,
    value: PreparedText,
}

impl RingVisual {
    fn new(ring: &Ring, style: &Rings, font: &LiteFont, value: Option<f32>) -> AppResult<Self> {
        let display = match value {
            Some(value) => format!("{value:.0}{}", ring.unit),
            None => format!("--{}", ring.unit),
        };
        Ok(Self {
            x: ring.center_x as f32,
            y: ring.center_y as f32,
            fraction: value.map_or(0.0, |value| (value / ring.max_value).clamp(0.0, 1.0)),
            foreground: ring.foreground_rgba,
            label: font.prepare_text(
                &ring.label,
                style.label_font_size,
                ring.center_x,
                ring.center_y + style.label_offset_y,
                style.label_rgba,
            )?,
            value: font.prepare_text(
                &display,
                style.value_font_size,
                ring.center_x,
                ring.center_y + style.value_offset_y,
                style.value_rgba,
            )?,
        })
    }
}

pub struct RingScene {
    visuals: [RingVisual; 3],
    background: [u8; 4],
    radius: f32,
    thickness: f32,
    start_degrees: f32,
}

impl RingScene {
    pub fn draw_strip(&self, strip: &mut Pixmap, strip_y: u32) {
        strip.fill(Color::TRANSPARENT);
        for ring in &self.visuals {
            shapes::ring(
                strip,
                strip_y as i32,
                ring.x,
                ring.y,
                self.radius,
                self.thickness,
                self.start_degrees,
                ring.fraction,
                self.background,
                ring.foreground,
            );
        }
        for ring in &self.visuals {
            ring.label.draw_strip(strip, strip_y);
            ring.value.draw_strip(strip, strip_y);
        }
    }
}
