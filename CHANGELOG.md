# Changelog

## 0.1.0

- Rebranded the desktop widget and its settings application as zDesk.
- Reset the release version to 0.1.0 for both Rust packages.
- Renamed binaries, user configuration paths, systemd units, desktop launcher, and startup guard paths to zdesk.
- Retained clock, weekday, greeting, CPU/temperature/RAM rings, multi-monitor support, settings, and safe delayed startup.
- Preserved previous development notes in `docs/LEGACY-CHANGELOG.md`.
- Stop on unexpected XWayland desktop-window destruction rather than risk a window recreation loop.
- Disable previously enabled zDesk startup units during installation; automatic startup remains opt-in.
