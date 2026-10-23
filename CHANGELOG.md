# Changelog

## 0.1.1

- Reduce the optional GNOME autostart timer from 45 to 30 seconds; the existing crash-recovery guard and `Restart=no` behavior remain unchanged.
- Let the widget event loop sleep until its next clock, sensor, or startup-guard deadline instead of waking every second unnecessarily.
- Reuse the X11 upload buffer during RGBA-to-BGRA conversion to avoid incremental appends for each pixel.
- Update version metadata for the widget and GTK4 Settings, with a version-aware changelog test.
- Preserve visuals, configuration schema, multi-monitor behavior, Save & Apply, and the opt-in startup policy.

## 0.1.0

- Rebranded the desktop widget and its settings application as zDesk.
- Reset the release version to 0.1.0 for both Rust packages.
- Renamed binaries, user configuration paths, systemd units, desktop launcher, and startup guard paths to zdesk.
- Retained clock, weekday, greeting, CPU/temperature/RAM rings, multi-monitor support, settings, and safe delayed startup.
- Preserved previous development notes in `docs/LEGACY-CHANGELOG.md`.
- Stop on unexpected XWayland desktop-window destruction rather than risk a window recreation loop.
- Disable previously enabled zDesk startup units during installation; automatic startup remains opt-in.
