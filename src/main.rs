mod clock;
mod config;
mod lite_font;
mod metrics;
mod monitor;
mod shapes;
mod startup_guard;
mod widgets;
mod x11;

use std::error::Error;
use std::path::PathBuf;

type AppResult<T> = Result<T, Box<dyn Error>>;

fn main() {
    if let Err(error) = run() {
        eprintln!("zdesk: {error}");
        std::process::exit(1);
    }
}

fn run() -> AppResult<()> {
    let mut args = std::env::args().skip(1);
    let (check_only, check_path) = match args.next().as_deref() {
        None => (false, None),
        Some("--check-config") => (true, args.next().map(PathBuf::from)),
        Some("--version") => {
            if args.next().is_some() {
                return Err("usage: zdesk [--check-config [path]|--version]".into());
            }
            println!("zDesk {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        _ => return Err("usage: zdesk [--check-config [path]|--version]".into()),
    };
    if args.next().is_some() {
        return Err("usage: zdesk [--check-config [path]|--version]".into());
    }

    let settings = match check_path {
        Some(path) => config::Settings::load_from_path(&path)?,
        None => config::Settings::load()?,
    };
    let font = lite_font::LiteFont::load(&settings.font_path)?;
    // Check and cache every character that a clock can show before starting X11.
    let _ = font.prepare_text(
        "0123456789:",
        settings.clock.font_size,
        settings.clock.center_x,
        settings.clock.center_y,
        settings.rgba,
    )?;
    let widget_fonts = widgets::Fonts::load(&settings)?;

    if check_only {
        println!(
            "configuration and font OK: {}",
            settings.config_path.display()
        );
        return Ok(());
    }

    x11::run(&settings, &font, &widget_fonts)
}
