use crate::clock;
use crate::config::{Anchor, Settings, Window as WindowConfig};
use crate::lite_font::{LiteFont, PreparedText};
use crate::metrics;
use crate::monitor::{self, Rect};
use crate::widgets;
use crate::AppResult;
use std::os::fd::AsRawFd;
use std::time::{Duration, Instant};
use tiny_skia::{Color, Pixmap};
use x11rb::connection::Connection;
use x11rb::protocol::randr::{ConnectionExt as RandrConnectionExt, NotifyMask};
use x11rb::protocol::render::{ConnectionExt as RenderConnectionExt, PictType};
use x11rb::protocol::shape::{ConnectionExt as ShapeConnectionExt, SK, SO};
use x11rb::protocol::xproto::ConnectionExt as XConnectionExt;
use x11rb::protocol::xproto::{
    AtomEnum, ClipOrdering, ColormapAlloc, ConfigureWindowAux, CreateGCAux, CreateWindowAux,
    EventMask, ImageFormat, ImageOrder, PropMode, Screen, VisualClass, WindowClass,
};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as WrapperConnectionExt;

const STRIP_ROWS: usize = 32;

struct DesktopWindow {
    window: u32,
    gc: u32,
    width: u16,
    height: u16,
    strip: Pixmap,
    wire: Vec<u8>,
    dirty: bool,
}

struct MonitorWindows {
    monitor: Rect,
    main: DesktopWindow,
    rings: Option<DesktopWindow>,
}

impl MonitorWindows {
    fn create(
        conn: &RustConnection,
        screen: &Screen,
        visual: u32,
        colormap: u32,
        monitor: Rect,
        settings: &Settings,
    ) -> AppResult<Self> {
        let main = DesktopWindow::create(
            conn,
            screen,
            visual,
            colormap,
            monitor,
            &settings.window,
            b"zDesk",
        )?;
        let rings = settings
            .rings_window
            .as_ref()
            .map(|config| {
                DesktopWindow::create(
                    conn,
                    screen,
                    visual,
                    colormap,
                    monitor,
                    config,
                    b"zDesk Rings",
                )
            })
            .transpose()?;
        conn.map_window(main.window)?.check()?;
        if let Some(window) = &rings {
            conn.map_window(window.window)?.check()?;
        }
        Ok(Self {
            monitor,
            main,
            rings,
        })
    }

    fn reposition(
        &mut self,
        conn: &RustConnection,
        settings: &Settings,
        monitor: Rect,
    ) -> AppResult<()> {
        self.main.reposition(conn, &settings.window, monitor)?;
        if let (Some(window), Some(config)) = (&self.rings, &settings.rings_window) {
            window.reposition(conn, config, monitor)?;
        }
        self.monitor = monitor;
        Ok(())
    }

    fn contains(&self, window: u32) -> bool {
        self.main.window == window
            || self
                .rings
                .as_ref()
                .is_some_and(|rings| rings.window == window)
    }

    fn destroy(self, conn: &RustConnection, except: Option<u32>) {
        // DestroyNotify for windows deliberately retired on a monitor change
        // arrives later. By then the pair is gone from our live list.
        for window in [Some(self.main.window), self.rings.map(|rings| rings.window)]
            .into_iter()
            .flatten()
        {
            if Some(window) != except {
                if let Ok(cookie) = conn.destroy_window(window) {
                    let _ = cookie.check();
                }
            }
        }
    }
}

fn reconcile_monitors(
    conn: &RustConnection,
    screen: &Screen,
    visual: u32,
    colormap: u32,
    settings: &Settings,
    windows: &mut Vec<MonitorWindows>,
    monitors: &[Rect],
) -> AppResult<()> {
    while windows.len() > monitors.len() {
        windows.pop().expect("length checked").destroy(conn, None);
    }
    for (index, &monitor) in monitors.iter().enumerate() {
        if let Some(pair) = windows.get_mut(index) {
            if pair.monitor != monitor {
                pair.reposition(conn, settings, monitor)?;
            }
        } else {
            windows.push(MonitorWindows::create(
                conn, screen, visual, colormap, monitor, settings,
            )?);
        }
    }
    conn.flush()?;
    Ok(())
}

impl DesktopWindow {
    fn create(
        conn: &RustConnection,
        screen: &Screen,
        visual: u32,
        colormap: u32,
        monitor: Rect,
        config: &WindowConfig,
        title: &[u8],
    ) -> AppResult<Self> {
        let (x, y) = position(config, monitor)?;
        let window = conn.generate_id()?;
        conn.create_window(
            32,
            window,
            screen.root,
            x,
            y,
            config.width,
            config.height,
            0,
            WindowClass::INPUT_OUTPUT,
            visual,
            &CreateWindowAux::new()
                .background_pixel(0)
                .border_pixel(0)
                .colormap(colormap)
                .event_mask(EventMask::EXPOSURE | EventMask::STRUCTURE_NOTIFY),
        )?
        .check()?;

        // Set desktop hints before GNOME manages either window.
        let window_type = atom(conn, b"_NET_WM_WINDOW_TYPE")?;
        let desktop_type = atom(conn, b"_NET_WM_WINDOW_TYPE_DESKTOP")?;
        conn.change_property32(
            PropMode::REPLACE,
            window,
            window_type,
            AtomEnum::ATOM,
            &[desktop_type],
        )?
        .check()?;
        conn.change_property32(
            PropMode::REPLACE,
            window,
            atom(conn, b"_NET_WM_DESKTOP")?,
            AtomEnum::CARDINAL,
            &[u32::MAX],
        )?
        .check()?;
        conn.change_property32(
            PropMode::REPLACE,
            window,
            atom(conn, b"_NET_WM_USER_TIME")?,
            AtomEnum::CARDINAL,
            &[0],
        )?
        .check()?;
        conn.change_property8(
            PropMode::REPLACE,
            window,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            b"zdesk\0zDesk\0",
        )?
        .check()?;
        conn.change_property8(
            PropMode::REPLACE,
            window,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            title,
        )?
        .check()?;

        // An empty input region lets clicks pass through both windows.
        conn.shape_rectangles(
            SO::SET,
            SK::INPUT,
            ClipOrdering::UNSORTED,
            window,
            0,
            0,
            &[],
        )?
        .check()?;

        let gc = conn.generate_id()?;
        conn.create_gc(gc, window, &CreateGCAux::new())?.check()?;
        let strip = new_strip(conn, config.width, config.height)?;
        let wire = Vec::with_capacity(strip.data().len());
        Ok(Self {
            window,
            gc,
            width: config.width,
            height: config.height,
            strip,
            wire,
            dirty: true,
        })
    }

    fn resize(&mut self, conn: &RustConnection, width: u16, height: u16) -> AppResult<()> {
        if width != self.width || height != self.height {
            let strip = new_strip(conn, width, height)?;
            self.width = width;
            self.height = height;
            self.wire = Vec::with_capacity(strip.data().len());
            self.strip = strip;
            self.dirty = true;
        }
        Ok(())
    }

    fn reposition(
        &self,
        conn: &RustConnection,
        config: &WindowConfig,
        monitor: Rect,
    ) -> AppResult<()> {
        let (x, y) = position(config, monitor)?;
        conn.configure_window(
            self.window,
            &ConfigureWindowAux::new().x(i32::from(x)).y(i32::from(y)),
        )?
        .check()?;
        Ok(())
    }

    fn upload<F>(&mut self, conn: &RustConnection, paint: F) -> AppResult<()>
    where
        F: FnMut(&mut Pixmap, u32),
    {
        if self.dirty {
            upload(
                conn,
                self.window,
                self.gc,
                &mut self.strip,
                self.height,
                &mut self.wire,
                paint,
            )?;
            self.dirty = false;
        }
        Ok(())
    }
}

pub fn run(
    settings: &Settings,
    clock_font: &LiteFont,
    widget_fonts: &widgets::Fonts,
) -> AppResult<()> {
    let (conn, screen_number) = x11rb::connect(None)
        .map_err(|error| format!("cannot connect to XWayland via DISPLAY: {error}"))?;
    let setup = conn.setup();
    let screen = &setup.roots[screen_number];
    let format = setup
        .pixmap_formats
        .iter()
        .find(|format| format.depth == 32);
    if setup.image_byte_order != ImageOrder::LSB_FIRST
        || !format.is_some_and(|format| format.bits_per_pixel == 32 && format.scanline_pad == 32)
    {
        return Err("this MVP requires Ubuntu's little-endian 32-bit XWayland image format".into());
    }
    let visual = find_argb_visual(&conn, screen_number)?;
    if conn
        .query_extension(b"RANDR")
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .is_some_and(|reply| reply.present)
    {
        // Startup centering works even if this older server cannot subscribe
        // to monitor changes while the application is running.
        if let Ok(cookie) = conn.randr_select_input(
            screen.root,
            NotifyMask::SCREEN_CHANGE
                | NotifyMask::CRTC_CHANGE
                | NotifyMask::OUTPUT_CHANGE
                | NotifyMask::RESOURCE_CHANGE,
        ) {
            let _ = cookie.check();
        }
    }
    let shape_version = conn.shape_query_version()?.reply()?;
    if shape_version.major_version < 1
        || (shape_version.major_version == 1 && shape_version.minor_version < 1)
    {
        return Err("XWayland SHAPE input regions are unavailable".into());
    }

    // Subscribe before the first read so a hotplug during startup is not lost.
    let monitors = monitor::query_all(&conn, screen_number)?;

    let colormap = conn.generate_id()?;
    conn.create_colormap(ColormapAlloc::NONE, colormap, screen.root, visual)?
        .check()?;
    let mut windows = Vec::with_capacity(monitors.len());
    reconcile_monitors(
        &conn,
        screen,
        visual,
        colormap,
        settings,
        &mut windows,
        &monitors,
    )?;

    let mut last_clock = clock::local_clock()?;
    let mut clock_text = prepare_clock(clock_font, settings, last_clock.hour, last_clock.minute)?;
    let mut main_scene = widget_fonts.prepare_main(settings, last_clock)?;

    let mut sampler = settings
        .rings
        .as_ref()
        .map(|style| metrics::Sampler::new_with_preference(&style.temperature_zone_types))
        .transpose()?;
    let mut ring_scene = if let Some(sampler) = sampler.as_mut() {
        widget_fonts.prepare_rings(settings, &sampler.sample()?)?
    } else {
        None
    };
    let ring_refresh = settings
        .rings
        .as_ref()
        .map(|style| Duration::from_millis(style.refresh_ms));
    let mut next_ring_refresh = ring_refresh.map(|interval| Instant::now() + interval);
    let mut settle_at = Some(Instant::now() + Duration::from_secs(60));
    let mut settle_error_reported = false;

    loop {
        let mut monitor_changed = false;
        while let Some(event) = conn.poll_for_event()? {
            match event {
                Event::RandrNotify(_) | Event::RandrScreenChangeNotify(_) => {
                    monitor_changed = true;
                }
                Event::Expose(expose) if expose.count == 0 => {
                    for pair in &mut windows {
                        if expose.window == pair.main.window {
                            pair.main.dirty = true;
                        } else if let Some(window) = pair.rings.as_mut() {
                            if expose.window == window.window {
                                window.dirty = true;
                            }
                        }
                    }
                }
                Event::ConfigureNotify(configure) => {
                    for pair in &mut windows {
                        if configure.window == pair.main.window {
                            pair.main.resize(&conn, configure.width, configure.height)?;
                        } else if let Some(window) = pair.rings.as_mut() {
                            if configure.window == window.window {
                                window.resize(&conn, configure.width, configure.height)?;
                            }
                        }
                    }
                }
                Event::DestroyNotify(destroy) => {
                    // If the compositor unexpectedly removes a live desktop window,
                    // exit instead of repeatedly recreating XWayland windows.
                    // Pairs retired during monitor changes are no longer in this list.
                    if windows.iter().any(|pair| pair.contains(destroy.window)) {
                        return Err(
                            "XWayland unexpectedly destroyed a zDesk window; stopping to avoid a recreation loop"
                                .into(),
                        );
                    }
                }
                _ => {}
            }
        }

        if monitor_changed {
            let current_monitors = monitor::query_all(&conn, screen_number)?;
            reconcile_monitors(
                &conn,
                screen,
                visual,
                colormap,
                settings,
                &mut windows,
                &current_monitors,
            )?;
        }

        let current = clock::local_clock()?;
        if current != last_clock {
            last_clock = current;
            clock_text = prepare_clock(clock_font, settings, current.hour, current.minute)?;
            main_scene = widget_fonts.prepare_main(settings, current)?;
            for pair in &mut windows {
                pair.main.dirty = true;
            }
        }

        if let (Some(due), Some(interval)) = (next_ring_refresh, ring_refresh) {
            if Instant::now() >= due {
                if let Some(sampler) = sampler.as_mut() {
                    let readings = sampler.sample()?;
                    if let Some(scene) = ring_scene.as_mut() {
                        widget_fonts.update_rings(settings, scene, &readings)?;
                    }
                    for pair in &mut windows {
                        if let Some(window) = pair.rings.as_mut() {
                            window.dirty = true;
                        }
                    }
                }
                next_ring_refresh = Some(Instant::now() + interval);
            }
        }

        if settle_at.is_some_and(|due| Instant::now() >= due) {
            match crate::startup_guard::clear_stable_marker() {
                Ok(cleared) => {
                    if cleared {
                        eprintln!("zdesk: startup stable for 60s; recovery marker cleared");
                    }
                    settle_at = None;
                }
                Err(error) => {
                    if !settle_error_reported {
                        eprintln!("zdesk: could not clear startup marker: {error}");
                        settle_error_reported = true;
                    }
                    settle_at = Some(Instant::now() + Duration::from_secs(10));
                }
            }
        }

        for pair in &mut windows {
            pair.main.upload(&conn, |strip, y| {
                strip.fill(Color::TRANSPARENT);
                clock_text.draw_strip(strip, y);
                main_scene.draw_strip(strip, y);
            })?;
            if let Some(window) = pair.rings.as_mut() {
                window.upload(&conn, |strip, y| {
                    if let Some(scene) = &ring_scene {
                        scene.draw_strip(strip, y);
                    } else {
                        strip.fill(Color::TRANSPARENT);
                    }
                })?;
            }
        }

        // Sleep until the next visible update or startup-guard check. X11
        // events still wake poll immediately, including monitor hotplug.
        let mut timeout = clock::milliseconds_to_next_minute()?;
        for deadline in [next_ring_refresh, settle_at].into_iter().flatten() {
            let remaining = deadline
                .saturating_duration_since(Instant::now())
                .as_millis();
            timeout = timeout.min(remaining.min(i32::MAX as u128) as i32);
        }
        let mut descriptor = libc::pollfd {
            fd: conn.stream().as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let result = unsafe { libc::poll(&mut descriptor, 1, timeout) };
        if result < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() != std::io::ErrorKind::Interrupted {
                return Err(error.into());
            }
        } else if descriptor.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            return Err("XWayland connection closed".into());
        }
    }
}

fn prepare_clock(
    font: &LiteFont,
    settings: &Settings,
    hour: i32,
    minute: i32,
) -> AppResult<PreparedText> {
    font.prepare_text(
        &format!("{hour:02}:{minute:02}"),
        settings.clock.font_size,
        settings.clock.center_x,
        settings.clock.center_y,
        settings.rgba,
    )
}

fn find_argb_visual(conn: &RustConnection, screen_number: usize) -> AppResult<u32> {
    let screen = &conn.setup().roots[screen_number];
    let formats = conn.render_query_pict_formats()?.reply()?;
    let pict_screen = formats
        .screens
        .get(screen_number)
        .ok_or("XRender screen is missing")?;
    let depth = screen
        .allowed_depths
        .iter()
        .find(|depth| depth.depth == 32)
        .ok_or("XWayland has no 32-bit visual")?;

    for pict_depth in pict_screen.depths.iter().filter(|depth| depth.depth == 32) {
        for pict_visual in &pict_depth.visuals {
            let Some(info) = formats
                .formats
                .iter()
                .find(|info| info.id == pict_visual.format)
            else {
                continue;
            };
            let direct = info.direct;
            if info.type_ != PictType::DIRECT
                || info.depth != 32
                || direct.red_shift != 16
                || direct.green_shift != 8
                || direct.blue_shift != 0
                || direct.alpha_shift != 24
                || direct.red_mask != 255
                || direct.green_mask != 255
                || direct.blue_mask != 255
                || direct.alpha_mask != 255
            {
                continue;
            }
            if depth.visuals.iter().any(|visual| {
                visual.visual_id == pict_visual.visual
                    && visual.class == VisualClass::TRUE_COLOR
                    && visual.red_mask == 0x00ff_0000
                    && visual.green_mask == 0x0000_ff00
                    && visual.blue_mask == 0x0000_00ff
            }) {
                return Ok(pict_visual.visual);
            }
        }
    }
    Err("XWayland has no premultiplied ARGB32 visual with alpha".into())
}

fn position(window: &WindowConfig, monitor: Rect) -> AppResult<(i16, i16)> {
    let (base_x, base_y) = match window.anchor {
        Anchor::Center => (
            i32::from(monitor.x) + (i32::from(monitor.width) - i32::from(window.width)) / 2,
            i32::from(monitor.y) + (i32::from(monitor.height) - i32::from(window.height)) / 2,
        ),
        Anchor::TopLeft => (i32::from(monitor.x), i32::from(monitor.y)),
        Anchor::BottomCenter => (
            i32::from(monitor.x) + (i32::from(monitor.width) - i32::from(window.width)) / 2,
            i32::from(monitor.y) + i32::from(monitor.height) - i32::from(window.height),
        ),
    };
    let x = (base_x + i32::from(window.offset_x)).try_into()?;
    let y = (base_y + i32::from(window.offset_y)).try_into()?;
    Ok((x, y))
}

fn atom(conn: &RustConnection, name: &[u8]) -> AppResult<u32> {
    Ok(conn.intern_atom(false, name)?.reply()?.atom)
}

fn new_strip(conn: &RustConnection, width: u16, height: u16) -> AppResult<Pixmap> {
    if width == 0 || height == 0 {
        return Err("compositor configured a zero-size window".into());
    }
    let stride = usize::from(width) * 4;
    // Core X11 requests may be limited to 65535 four-byte units. Leave ample
    // space for the PutImage header and cap the reusable client buffer at 32 rows.
    let maximum = usize::from(conn.setup().maximum_request_length) * 4;
    let rows_per_request = (maximum.saturating_sub(64) / stride)
        .min(STRIP_ROWS)
        .min(usize::from(height));
    if rows_per_request == 0 {
        return Err("window is too wide for XWayland PutImage".into());
    }
    Pixmap::new(u32::from(width), rows_per_request as u32)
        .ok_or_else(|| "could not allocate the clock strip".into())
}

fn upload<F>(
    conn: &RustConnection,
    window: u32,
    gc: u32,
    strip: &mut Pixmap,
    height: u16,
    wire: &mut Vec<u8>,
    mut paint: F,
) -> AppResult<()>
where
    F: FnMut(&mut Pixmap, u32),
{
    let stride = strip.width() as usize * 4;
    let height = usize::from(height);
    let rows_per_request = strip.height() as usize;
    let mut y = 0;
    while y < height {
        let rows = rows_per_request.min(height - y);
        paint(strip, y as u32);
        // Reuse the wire allocation instead of extending it once per pixel.
        wire.resize(rows * stride, 0);
        for (rgba, bgra) in strip.data()[..rows * stride]
            .chunks_exact(4)
            .zip(wire.chunks_exact_mut(4))
        {
            // tiny-skia stores premultiplied RGBA; little-endian ARGB32 is BGRA.
            bgra.copy_from_slice(&[rgba[2], rgba[1], rgba[0], rgba[3]]);
        }
        conn.put_image(
            ImageFormat::Z_PIXMAP,
            window,
            gc,
            strip.width() as u16,
            rows as u16,
            0,
            y as i16,
            0,
            32,
            &wire,
        )?
        .check()?;
        y += rows;
    }
    conn.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centers_each_window_with_offsets_relative_to_its_monitor() {
        let primary = Rect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let hdmi = Rect {
            x: 1920,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let main = WindowConfig {
            width: 620,
            height: 650,
            anchor: Anchor::Center,
            offset_x: 20,
            offset_y: -10,
        };
        assert_eq!(position(&main, primary).unwrap(), (670, 205));
        assert_eq!(position(&main, hdmi).unwrap(), (2590, 205));

        let rings = WindowConfig {
            width: 420,
            height: 115,
            anchor: Anchor::BottomCenter,
            offset_x: 0,
            offset_y: -30,
        };
        assert_eq!(position(&rings, primary).unwrap(), (750, 935));
        assert_eq!(position(&rings, hdmi).unwrap(), (2670, 935));
    }
}
