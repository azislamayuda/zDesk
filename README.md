# zDesk 0.1.0

zDesk is a lightweight desktop widget for Ubuntu GNOME Wayland through XWayland. One Rust process creates two transparent ARGB windows on each active monitor: a clock, weekday, and greeting above, with CPU, temperature, and RAM rings below. The windows use the *desktop* type and pass clicks through. The widget process uses no GTK, Pango, Lua, or shell commands. It draws in strips no taller than 32 pixels to avoid keeping a large framebuffer in memory.

The clock and weekday update when the minute changes. Sensors are read directly from `/proc` and `/sys` at the configured interval, and one reading is shared across monitors. Monitor geometry comes from RandR, so the windows follow layout changes. On GNOME Wayland, Mutter ultimately decides where XWayland windows appear; check placement visually. If Mutter destroys a live zDesk window unexpectedly, zDesk exits instead of automatically recreating the window in a loop. Measure RSS and CPU in the same desktop session after each change.

This release begins the zDesk version series at **v0.1.0**. It is a rebrand
and version reset of the existing implementation, not a return to the older
clock-only feature set. Prior development notes are preserved in
[`docs/LEGACY-CHANGELOG.md`](docs/LEGACY-CHANGELOG.md). Existing installations
under the previous binary/service name are **not** removed automatically.
Check the old user services before enabling the new startup timer so both
widgets do not run simultaneously.

## Project layout

```text
zDesk/
├── Cargo.toml                     # desktop widget
├── settings/                      # separate settings application
│   ├── Cargo.toml
│   └── src/{main,store,changelog}.rs
├── config/{default,v0.1}.toml
├── CHANGELOG.md
├── src/{main,config,clock,lite_font,metrics,monitor,shapes,startup_guard,widgets,x11}.rs
├── systemd/{zdesk.service,zdesk.timer}
└── scripts/{install,install-settings,benchmark,login-guard}.sh
```

`config/default.toml` defines the text, fonts, colors, sizes, positions, ring geometry, sensor interval, and temperature source. Its initial layout follows the user's Conky setup. Only glyphs that appear on screen are processed, keeping memory use low. Config changes take effect after the service restarts; live reload is not available. The separate settings application uses GTK4/libadwaita only while its window is open. The desktop widget does not load that toolkit.

## Build and install

Install Rust and Cargo on Ubuntu if needed:

```bash
sudo apt update
sudo apt install rustc cargo
```

From the `zDesk` directory, which contains `scripts`, run:

```bash
bash scripts/install.sh
```

If the terminal is already in `zDesk/scripts`, run:

```bash
bash install.sh
```

The script builds a release binary and installs it at `~/.local/bin/zdesk`. It preserves the user's edited config; a complete example is available at `~/.config/zdesk/example.toml`. The installer disables zDesk autostart, backs up a differing service unit as `zdesk.service.pre-safe-start.bak`, and installs the safe service and timer. On existing installations, it stops active zDesk units and disables any enabled zDesk service/timer before replacing them. It does not enable the timer automatically or modify Conky's configuration.

To apply a config change after zDesk starts:

```bash
systemctl --user restart zdesk.service
```

To build the widget without installing a service:

```bash
cargo build --release
```

Make sure the configured TTF/OTF font files exist, or change their paths. `--check-config` does not need an XWayland connection.

To test the widget manually without installing systemd units, ensure
`~/.config/zdesk/config.toml` exists (copy it from `config/default.toml` if needed),
then run `./target/release/zdesk` from the repository. The widget takes no
positional config path; `--check-config /path/to/file.toml` only validates that
file and does not start the widget.

## Settings window in v0.1.0

To add the settings application to an existing safe installation, run this from `zDesk`:

```bash
bash scripts/install-settings.sh
```

This builds and installs both binaries and adds `~/.local/share/applications/id.lamayuda.zDesk.Settings.desktop`. Open **zDesk Settings** from the GNOME application menu, or run `~/.local/bin/zdesk-settings`. The launcher opens the editor; the desktop widget does not autostart unless you have separately enabled its timer after testing. Building the editor requires `pkg-config`, `libgtk-4-dev`, and `libadwaita-1-dev`:

```bash
sudo apt install pkg-config libgtk-4-dev libadwaita-1-dev
```

The window controls font sizes, clock and greeting positions, ring positions, greeting name, colors, and the available sensor refresh interval. **Save & Apply** is at the bottom right. The top-right menu has **About zDesk**, showing creator **Azis Lamayuda** and the application version, followed by **Changelog**, which lists changes across all released versions.

**Save & Apply** validates the edits, creates a backup, and saves the TOML file. If the widget service is active, the editor waits for its restart and confirms it stays active briefly before showing “Changes applied”. Click the button again to retry applying saved settings if needed, even when no fields have changed. If the service is inactive, the changes take effect the next time it starts. If it is still starting, wait until it becomes active and click the button again. Other TOML values and comments are preserved.

The settings installer leaves the running service and timer as they are. The settings window does not start automatically at login. To validate another TOML file without opening the widget:

```bash
~/.local/bin/zdesk --check-config /path/to/config.toml
```

The editor selects the Cairo renderer in this session to reduce memory while open. In one local measurement, it used about **125–128 MiB RSS / 43–46 MiB PSS**, while the running desktop widget used about **5 MiB RSS**. Closing the editor releases its memory. Those editor figures are separate from the widget's memory target.

## Multiple monitors

zDesk uses one process to show one clock and one set of rings on each monitor detected by XWayland. RandR supplies the geometry for each monitor; the config's `anchor`, `offset_x`, and `offset_y` apply relative to that monitor. Changing a font size does not move the window. When the monitor layout changes, the application updates its windows without a second instance.

To inspect the layout and windows from a terminal in the desktop session:

```bash
xrandr --listmonitors
xwininfo -root -tree | grep zDesk
```

With eDP-1 at `1920x1080+0+0` and HDMI-1 at `1920x1080+1920+0`, the user's config at the time of measurement placed clocks at `+650+215` and `+2570+215`, and rings at `+750+935` and `+2670+935`.

## Safe startup after login (opt-in, not recommended during initial testing)

On October 16, 2026, version 0.3 starting at the beginning of a GNOME session was associated with an XWayland crash followed by a GNOME Shell crash, returning the user to the login screen. The older unit also retried every three seconds. The optional timer waits 45 seconds after timer activation and does not retry on failure. This delay does not guarantee GNOME Shell or XWayland stability. Keep zDesk autostart disabled during first-time migration and testing. It records a startup marker that the widget clears after 60 seconds of stable runtime. A normal service stop also clears the current boot's marker, so shutting down shortly after Save & Apply does not block the next login. An unexpected failure keeps the marker; while it exists, later automatic starts are skipped, including across reboots.

After the GNOME desktop is open and stable, test the service manually first:

```bash
systemctl --user start zdesk.service
systemctl --user status zdesk.service
xwininfo -root -tree | grep zDesk
```

The `start` command returns when the process starts. Test multiple desktop sessions before considering automatic startup. The following command is **optional** and should **not** be used during initial migration:

```bash
systemctl --user enable zdesk.timer
```

Do not enable `zdesk.service` directly. To stop automatic starts:

```bash
systemctl --user disable --now zdesk.timer
systemctl --user stop zdesk.service
```

If the failure marker is present and you want to retry after the desktop stabilizes:

```bash
~/.local/lib/zdesk/login-guard.sh reset
systemctl --user start zdesk.service
```

The historical logs did not identify exactly why the early XWayland connection crashed. A prior installation reportedly passed shutdown/restart testing with its timer; this is not a guarantee of compatibility for zDesk v0.1.0. Keep autostart off until manual runtime and reboot tests pass. The startup guard avoids leaving a marker after a normal shutdown during the first minute following Save & Apply.

## Comparing with Conky (optional)

Run the following **in a terminal in the user's GNOME session**, not a remote terminal without display access:

```bash
systemctl --user status conky.service
systemctl --user show-environment | grep -E '^(DISPLAY|XAUTHORITY)='
```

If the session's `DISPLAY` or `XAUTHORITY` is missing from the user systemd environment, import it from a GNOME terminal before starting zDesk:

```bash
systemctl --user import-environment DISPLAY XAUTHORITY
systemctl --user start zdesk.service
```

When Conky runs for a comparison, its clock, greeting, and rings overlap zDesk because the positions are similar. For a visual check, temporarily change `window.offset_x` or `rings_window.offset_x` in the config, then restart only the zDesk service. If the widget does not appear or is on the wrong layer, inspect `journalctl --user -u zdesk.service -n 50 --no-pager`. Keep the Conky configuration intact.

## Checking RSS and CPU

After zDesk has been idle for a few minutes:

```bash
bash scripts/benchmark.sh 60
```

The script reads RSS, PSS, and CPU time differences from `/proc` for 60 seconds for zDesk and for Conky if it is running. CPU% treats one core as 100%; it is an average over the interval, not the process's lifetime. Summing the RSS of two Conky processes can count shared memory twice; PSS apportions it. Repeat the measurement under the same idle desktop conditions.

Historical 60-second measurement on one monitor, October 16, 2026, with the pre-rebrand widget (v0.2) and Conky running together:

| Application | RSS | PSS | CPU, one core |
| --- | ---: | ---: | ---: |
| Pre-rebrand widget v0.2 | 5,036 KiB | 2,375 KiB | 0.200% |
| Two Conky processes | 71,204 KiB | 17,147 KiB | 0.316% |

This is one historical sample, not a benchmark of the zDesk v0.1.0 release. The previous implementation met the original RSS target, while CPU was exactly 0.200%, rather than below the original `< 0.2%` target.

The pre-rebrand v0.3 build measured over 60 seconds on two monitors, with Conky stopped: **5,300 KiB RSS**, **2,635 KiB PSS**, and **0.233% CPU** of one core. This does not compare Conky in the same session.

## Current limits

Version 0.1.0 shows the weekday, greeting, and three sensor rings on every active monitor. The editor covers commonly changed settings; advanced options can still be edited directly in TOML. Automatic config reload and other compositors are not supported. `anchor = "center"` is calculated for each monitor if XWayland provides RandR 1.5; otherwise the app uses one X11 root geometry. XWayland must provide an ARGB32 visual and the XRender/SHAPE extensions. If Mutter places desktop-type windows unexpectedly, GNOME-specific adjustments may be needed.

## Version and name verification

```bash
cargo run -- --version
# zDesk 0.1.0

cargo test --locked
cargo test --manifest-path settings/Cargo.toml --locked
```

The widget binary and user service use the lowercase Linux identifiers
`zdesk` and `zdesk.service`, while the visible application name is **zDesk**.
The optional settings binary is `zdesk-settings`.
