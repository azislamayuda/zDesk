use crate::AppResult;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub window: Window,
    pub clock: Clock,
    // Absent in an installed v0.1 config: preserve its clock-only behavior.
    pub weekday: Option<Weekday>,
    pub greeting: Option<Greeting>,
    pub rings_window: Option<Window>,
    pub rings: Option<Rings>,
    #[serde(skip)]
    pub font_path: PathBuf,
    #[serde(skip)]
    pub config_path: PathBuf,
    #[serde(skip)]
    pub rgba: [u8; 4],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Window {
    pub width: u16,
    pub height: u16,
    pub anchor: Anchor,
    pub offset_x: i16,
    pub offset_y: i16,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Anchor {
    Center,
    TopLeft,
    BottomCenter,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Clock {
    pub font_path: String,
    pub font_size: f32,
    pub color: String,
    pub center_x: i32,
    pub center_y: i32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Weekday {
    // Monday through Sunday, matching the Conky design.
    pub labels: [String; 7],
    pub center_x: [i32; 7],
    pub label_center_y: i32,
    pub label_font_path: String,
    pub label_font_size: f32,
    pub label_color: String,
    pub dot_center_y: i32,
    pub dot_radius: f32,
    pub dot_stroke_width: f32,
    pub inactive_color: String,
    pub active_color_mon_wed: String,
    pub active_color_thu_sat: String,
    pub active_color_sun: String,
    #[serde(skip)]
    pub label_font_file: PathBuf,
    #[serde(skip)]
    pub label_rgba: [u8; 4],
    #[serde(skip)]
    pub inactive_rgba: [u8; 4],
    #[serde(skip)]
    pub active_mon_wed_rgba: [u8; 4],
    #[serde(skip)]
    pub active_thu_sat_rgba: [u8; 4],
    #[serde(skip)]
    pub active_sun_rgba: [u8; 4],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Greeting {
    pub font_path: String,
    pub font_size: f32,
    pub center_x: i32,
    pub center_y: i32,
    pub morning_start: u8,
    pub afternoon_start: u8,
    pub evening_start: u8,
    pub night_start: u8,
    pub morning_text: String,
    pub afternoon_text: String,
    pub evening_text: String,
    pub night_text: String,
    pub name: String,
    pub suffix: String,
    pub color: String,
    pub name_color: String,
    #[serde(skip)]
    pub font_file: PathBuf,
    #[serde(skip)]
    pub rgba: [u8; 4],
    #[serde(skip)]
    pub name_rgba: [u8; 4],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rings {
    pub refresh_ms: u64,
    pub font_path: String,
    pub label_font_size: f32,
    pub value_font_size: f32,
    pub background_color: String,
    pub label_color: String,
    pub value_color: String,
    pub radius: f32,
    pub thickness: f32,
    pub start_angle_degrees: f32,
    pub label_offset_y: i32,
    pub value_offset_y: i32,
    pub temperature_zone_types: Vec<String>,
    pub cpu: Ring,
    pub temperature: Ring,
    pub ram: Ring,
    #[serde(skip)]
    pub font_file: PathBuf,
    #[serde(skip)]
    pub background_rgba: [u8; 4],
    #[serde(skip)]
    pub label_rgba: [u8; 4],
    #[serde(skip)]
    pub value_rgba: [u8; 4],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ring {
    pub center_x: i32,
    pub center_y: i32,
    pub label: String,
    pub unit: String,
    pub max_value: f32,
    pub foreground_color: String,
    #[serde(skip)]
    pub foreground_rgba: [u8; 4],
}

impl Settings {
    pub fn load() -> AppResult<Self> {
        let home = std::env::var_os("HOME").ok_or("HOME is unset")?;
        let home = PathBuf::from(home);
        let path = home.join(".config/zdesk/config.toml");
        Self::load_from_path(&path)
    }

    pub fn load_from_path(path: &Path) -> AppResult<Self> {
        let home = std::env::var_os("HOME").ok_or("HOME is unset")?;
        let home = PathBuf::from(home);
        let source = std::fs::read_to_string(&path).map_err(|error| {
            format!(
                "cannot read {}: {error}; install config/default.toml first",
                path.display()
            )
        })?;
        let mut settings: Self = toml::from_str(&source)
            .map_err(|error| format!("invalid {}: {error}", path.display()))?;
        settings.validate()?;

        settings.font_path = expand_home(&home, &settings.clock.font_path, "clock font_path")?;
        settings.rgba = parse_color(&settings.clock.color, "clock color")?;
        if let Some(weekday) = &mut settings.weekday {
            weekday.label_font_file =
                expand_home(&home, &weekday.label_font_path, "weekday label_font_path")?;
            weekday.label_rgba = parse_color(&weekday.label_color, "weekday label_color")?;
            weekday.inactive_rgba = parse_color(&weekday.inactive_color, "weekday inactive_color")?;
            weekday.active_mon_wed_rgba = parse_color(
                &weekday.active_color_mon_wed,
                "weekday active_color_mon_wed",
            )?;
            weekday.active_thu_sat_rgba = parse_color(
                &weekday.active_color_thu_sat,
                "weekday active_color_thu_sat",
            )?;
            weekday.active_sun_rgba =
                parse_color(&weekday.active_color_sun, "weekday active_color_sun")?;
            check_file(&weekday.label_font_file, "weekday label_font_path")?;
        }
        if let Some(greeting) = &mut settings.greeting {
            greeting.font_file = expand_home(&home, &greeting.font_path, "greeting font_path")?;
            greeting.rgba = parse_color(&greeting.color, "greeting color")?;
            greeting.name_rgba = parse_color(&greeting.name_color, "greeting name_color")?;
            check_file(&greeting.font_file, "greeting font_path")?;
        }
        if let Some(rings) = &mut settings.rings {
            rings.font_file = expand_home(&home, &rings.font_path, "rings font_path")?;
            rings.background_rgba = parse_color(&rings.background_color, "rings background_color")?;
            rings.label_rgba = parse_color(&rings.label_color, "rings label_color")?;
            rings.value_rgba = parse_color(&rings.value_color, "rings value_color")?;
            for (name, ring) in [
                ("cpu", &mut rings.cpu),
                ("temperature", &mut rings.temperature),
                ("ram", &mut rings.ram),
            ] {
                ring.foreground_rgba = parse_color(
                    &ring.foreground_color,
                    &format!("rings.{name} foreground_color"),
                )?;
            }
            check_file(&rings.font_file, "rings font_path")?;
        }
        settings.config_path = path.to_path_buf();
        Ok(settings)
    }

    fn validate(&self) -> AppResult<()> {
        validate_window(&self.window, "window")?;
        validate_size(self.clock.font_size, "clock font_size")?;
        validate_point(
            self.clock.center_x,
            self.clock.center_y,
            &self.window,
            "clock center_x/center_y",
        )?;
        if let Some(weekday) = &self.weekday {
            validate_size(weekday.label_font_size, "weekday label_font_size")?;
            for (index, x) in weekday.center_x.iter().enumerate() {
                validate_point(
                    *x,
                    weekday.label_center_y,
                    &self.window,
                    &format!("weekday label position {index}"),
                )?;
                validate_point(
                    *x,
                    weekday.dot_center_y,
                    &self.window,
                    &format!("weekday dot position {index}"),
                )?;
            }
            if weekday
                .labels
                .iter()
                .any(|label| label.is_empty() || label.chars().count() > 8)
            {
                return Err("weekday labels must each contain 1..=8 characters".into());
            }
            if !weekday.dot_radius.is_finite() || !(1.0..=32.0).contains(&weekday.dot_radius) {
                return Err("weekday dot_radius must be 1.0..=32.0".into());
            }
            if !weekday.dot_stroke_width.is_finite()
                || !(0.25..=8.0).contains(&weekday.dot_stroke_width)
            {
                return Err("weekday dot_stroke_width must be 0.25..=8.0".into());
            }
        }
        if let Some(greeting) = &self.greeting {
            validate_size(greeting.font_size, "greeting font_size")?;
            validate_point(
                greeting.center_x,
                greeting.center_y,
                &self.window,
                "greeting center",
            )?;
            if !(greeting.morning_start < greeting.afternoon_start
                && greeting.afternoon_start < greeting.evening_start
                && greeting.evening_start < greeting.night_start
                && greeting.night_start < 24)
            {
                return Err("greeting time starts must increase from 0 through 23".into());
            }
            if [
                &greeting.morning_text,
                &greeting.afternoon_text,
                &greeting.evening_text,
                &greeting.night_text,
                &greeting.name,
            ]
            .iter()
            .any(|text| text.is_empty())
            {
                return Err("greeting texts and name must be nonempty".into());
            }
        }
        match (&self.rings_window, &self.rings) {
            (None, None) => {}
            (Some(window), Some(rings)) => {
                validate_window(window, "rings_window")?;
                validate_size(rings.label_font_size, "rings label_font_size")?;
                validate_size(rings.value_font_size, "rings value_font_size")?;
                if !(250..=60_000).contains(&rings.refresh_ms) {
                    return Err("rings refresh_ms must be 250..=60000".into());
                }
                if !rings.radius.is_finite() || !(5.0..=200.0).contains(&rings.radius) {
                    return Err("rings radius must be 5.0..=200.0".into());
                }
                if !rings.thickness.is_finite() || !(1.0..=32.0).contains(&rings.thickness) {
                    return Err("rings thickness must be 1.0..=32.0".into());
                }
                if !rings.start_angle_degrees.is_finite()
                    || !(-360.0..=360.0).contains(&rings.start_angle_degrees)
                {
                    return Err("rings start_angle_degrees must be -360.0..=360.0".into());
                }
                if rings.temperature_zone_types.is_empty()
                    || rings.temperature_zone_types.iter().any(String::is_empty)
                {
                    return Err("rings temperature_zone_types must be nonempty".into());
                }
                for (name, ring) in [
                    ("cpu", &rings.cpu),
                    ("temperature", &rings.temperature),
                    ("ram", &rings.ram),
                ] {
                    validate_point(
                        ring.center_x,
                        ring.center_y,
                        window,
                        &format!("rings.{name} center"),
                    )?;
                    let reach = (rings.radius + rings.thickness / 2.0).ceil() as i32;
                    if ring.center_x - reach < 0
                        || ring.center_y - reach < 0
                        || ring.center_x + reach >= i32::from(window.width)
                        || ring.center_y + reach >= i32::from(window.height)
                    {
                        return Err(
                            format!("rings.{name} circle extends beyond rings_window").into()
                        );
                    }
                    if ring.label.is_empty() || ring.unit.is_empty() {
                        return Err(format!("rings.{name} label and unit must be nonempty").into());
                    }
                    if !ring.max_value.is_finite() || ring.max_value <= 0.0 {
                        return Err(format!("rings.{name} max_value must be positive").into());
                    }
                }
            }
            _ => return Err("rings_window and rings must be configured together".into()),
        }
        Ok(())
    }
}

fn validate_window(window: &Window, name: &str) -> AppResult<()> {
    if !(64..=2048).contains(&window.width) || !(64..=2048).contains(&window.height) {
        return Err(format!("{name} width and height must be 64..=2048").into());
    }
    Ok(())
}

fn validate_size(size: f32, name: &str) -> AppResult<()> {
    if !size.is_finite() || !(4.0..=256.0).contains(&size) {
        return Err(format!("{name} must be 4.0..=256.0").into());
    }
    Ok(())
}

fn validate_point(x: i32, y: i32, window: &Window, name: &str) -> AppResult<()> {
    if !(0..i32::from(window.width)).contains(&x) || !(0..i32::from(window.height)).contains(&y) {
        return Err(format!("{name} must be inside its window").into());
    }
    Ok(())
}

fn expand_home(home: &Path, value: &str, field: &str) -> AppResult<PathBuf> {
    if let Some(rest) = value.strip_prefix("~/") {
        return Ok(home.join(rest));
    }
    let path = PathBuf::from(value);
    if path.is_absolute() {
        Ok(path)
    } else {
        Err(format!("{field} must be absolute or start with ~/").into())
    }
}

fn check_file(path: &Path, field: &str) -> AppResult<()> {
    if !path.is_file() {
        return Err(format!("{field} is not a font file: {}", path.display()).into());
    }
    Ok(())
}

fn parse_color(value: &str, field: &str) -> AppResult<[u8; 4]> {
    let hex = value
        .strip_prefix('#')
        .ok_or_else(|| format!("{field} must start with #"))?;
    if (hex.len() != 6 && hex.len() != 8) || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!("{field} must be #RRGGBB or #RRGGBBAA").into());
    }
    let component = |start| u8::from_str_radix(&hex[start..start + 2], 16);
    Ok([
        component(0)?,
        component(2)?,
        component(4)?,
        if hex.len() == 8 { component(6)? } else { 255 },
    ])
}
