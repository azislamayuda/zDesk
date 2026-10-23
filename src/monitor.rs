//! Find each physical display for the desktop widgets. XWayland's root screen
//! spans all monitors, so centering against it can put widgets between displays.

use crate::AppResult;
use x11rb::connection::Connection;
use x11rb::protocol::randr::{ConnectionExt as RandrConnectionExt, MonitorInfo};
use x11rb::protocol::xproto::ConnectionExt as XConnectionExt;
use x11rb::rust_connection::RustConnection;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i16,
    pub y: i16,
    pub width: u16,
    pub height: u16,
}

/// Returns every active monitor in X11 root coordinates. A stable ordering
/// allows the window manager to compare layouts after RandR hotplug events.
/// When RandR is unavailable, a single window can still cover the root.
pub fn query_all(conn: &RustConnection, screen_number: usize) -> AppResult<Vec<Rect>> {
    let (root_window, root) = root_rect(conn, screen_number)?;
    Ok(select_all(
        randr_monitors(conn, root_window).as_deref().unwrap_or(&[]),
        root,
    ))
}

fn root_rect(conn: &RustConnection, screen_number: usize) -> AppResult<(u32, Rect)> {
    let screen = conn
        .setup()
        .roots
        .get(screen_number)
        .ok_or("X11 screen number is not present in the server setup")?;
    let mut root = Rect {
        x: 0,
        y: 0,
        width: screen.width_in_pixels,
        height: screen.height_in_pixels,
    };
    // `setup().roots` is a connection snapshot. The root window can grow or
    // shrink after hotplug, so query its current geometry each time.
    if let Ok(cookie) = conn.get_geometry(screen.root) {
        if let Ok(geometry) = cookie.reply() {
            root.width = geometry.width;
            root.height = geometry.height;
        }
    }

    Ok((screen.root, root))
}

fn randr_monitors(conn: &RustConnection, root_window: u32) -> Option<Vec<MonitorInfo>> {
    // The extension and its version are optional on an X11 server. These
    // requests happen at startup or when the monitor layout changes, outside
    // the regular drawing path.
    let extension_present = match conn
        .query_extension(b"RANDR")
        .ok()
        .and_then(|cookie| cookie.reply().ok())
    {
        Some(reply) => reply.present,
        _ => return None,
    };
    if !extension_present {
        return None;
    }
    let version = match conn
        .randr_query_version(1, 5)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
    {
        Some(reply) => reply,
        None => return None,
    };
    if (version.major_version, version.minor_version) < (1, 5) {
        return None;
    }

    let monitors = match conn
        .randr_get_monitors(root_window, true)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
    {
        Some(reply) => reply,
        None => return None,
    };
    Some(monitors.monitors)
}

fn select_all(monitors: &[MonitorInfo], root: Rect) -> Vec<Rect> {
    let mut rectangles: Vec<_> = monitors
        .iter()
        .filter(|monitor| monitor.width > 0 && monitor.height > 0)
        .map(|monitor| Rect {
            x: monitor.x,
            y: monitor.y,
            width: monitor.width,
            height: monitor.height,
        })
        .collect();
    rectangles.sort_unstable_by_key(|rect| (rect.x, rect.y, rect.width, rect.height));
    rectangles.dedup();
    if rectangles.is_empty() {
        rectangles.push(root);
    }
    rectangles
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_disconnected_zero_sized_monitor() {
        let monitors = [
            MonitorInfo {
                primary: true,
                x: 0,
                y: 0,
                width: 0,
                height: 0,
                ..Default::default()
            },
            MonitorInfo {
                x: 1920,
                y: 0,
                width: 1920,
                height: 1080,
                ..Default::default()
            },
        ];
        let root = Rect {
            x: 0,
            y: 0,
            width: 3840,
            height: 1080,
        };
        assert_eq!(
            select_all(&monitors, root),
            vec![Rect {
                x: 1920,
                y: 0,
                width: 1920,
                height: 1080
            },]
        );
    }

    #[test]
    fn returns_both_monitors_in_stable_order_and_removes_duplicates() {
        let right = MonitorInfo {
            x: 1920,
            y: 0,
            width: 1920,
            height: 1080,
            ..Default::default()
        };
        let left = MonitorInfo {
            primary: true,
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            ..Default::default()
        };
        let root = Rect {
            x: 0,
            y: 0,
            width: 3840,
            height: 1080,
        };
        assert_eq!(
            select_all(&[right.clone(), left, right], root),
            vec![
                Rect {
                    x: 0,
                    y: 0,
                    width: 1920,
                    height: 1080
                },
                Rect {
                    x: 1920,
                    y: 0,
                    width: 1920,
                    height: 1080
                },
            ],
        );
    }

    #[test]
    fn falls_back_to_root_when_no_active_monitor_is_reported() {
        let root = Rect {
            x: 0,
            y: 0,
            width: 3840,
            height: 1080,
        };
        let inactive = MonitorInfo {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            ..Default::default()
        };
        assert_eq!(select_all(&[], root), vec![root]);
        assert_eq!(select_all(&[inactive], root), vec![root]);
    }
}
