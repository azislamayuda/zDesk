# zDesk

**Lightweight desktop widgets for Ubuntu GNOME, written in Rust.**

zDesk is a Conky-inspired desktop widget application that displays a digital clock, weekday indicator, personalized greeting, and circular CPU, temperature, and RAM indicators. The widget runs as a small native Rust process using X11/XWayland. A separate GTK4/libadwaita application provides graphical settings.

**Current version:** `v0.1.1`  
**Author:** Azis Lamayuda  
**License:** [MIT](LICENSE)

> **Compatibility:** Developed and tested on Ubuntu GNOME with a Wayland session and XWayland. Other desktop environments and compositors have not been verified. zDesk requires a compatible 32-bit ARGB X11 visual and related X11 extensions.

## Features

- Digital clock with customizable font, size, color, and position.
- Weekday labels with an active-day indicator.
- Time-dependent greetings with customizable text and name.
- Circular CPU usage, CPU temperature, and RAM usage widgets.
- Transparent, click-through desktop windows.
- Support for multiple monitors via XRandR, using one widget process.
- Lightweight drawing without loading GTK into the widget process.
- Separate **zDesk Settings** GUI built with GTK4 and libadwaita.
- TOML configuration with validation and **Save & Apply**.
- Optional delayed autostart using a systemd user timer and startup guard.

## Requirements

- Linux desktop running **Ubuntu GNOME** with **XWayland** available.
- Rust and Cargo to build the project.
- For the Settings GUI: `pkg-config`, GTK4 development files, and **libadwaita 1.9 or newer** development files.
- TTF/OTF fonts referenced by the configuration file.

On a compatible Ubuntu version, install the system build dependencies with:

```bash
sudo apt update
sudo apt install build-essential pkg-config libgtk-4-dev libadwaita-1-dev
```

Install or update Rust/Cargo separately if they are not already available. Verify the installed development libraries with:

```bash
rustc --version
cargo --version
pkg-config --modversion gtk4
pkg-config --modversion libadwaita-1
```

**Fonts are not bundled.** The example configuration references Monoton, Bebas Neue, Roboto, and DejaVu Sans Mono font files. Install the necessary fonts or edit their paths in `config/default.toml` / `~/.config/zdesk/config.toml` before starting zDesk. The example greeting name should also be personalized.

## Installation

From the root of the cloned `zDesk` repository:

```bash
# Build and install the widget, configuration and systemd user units
bash scripts/install.sh

# Build and install the separate Settings application and GNOME launcher
bash scripts/install-settings.sh
```

**Do not use `sudo` with these installer scripts.** They install into the current user's home directory:

| Installed component | Location |
| --- | --- |
| Widget binary | `~/.local/bin/zdesk` |
| Settings binary | `~/.local/bin/zdesk-settings` |
| User configuration | `~/.config/zdesk/config.toml` |
| Example configuration | `~/.config/zdesk/example.toml` |
| Service | `~/.config/systemd/user/zdesk.service` |
| Timer | `~/.config/systemd/user/zdesk.timer` |
| Startup guard | `~/.local/lib/zdesk/login-guard.sh` |
| GNOME Settings launcher | `~/.local/share/applications/id.lamayuda.zDesk.Settings.desktop` |

The widget installer preserves an existing edited configuration and **disables zDesk autostart during installation**. It does not remove an older DeskLamayuda installation automatically. Check for older user services before enabling zDesk's timer so that two versions do not run simultaneously.

### Verify the installation

```bash
zdesk --version
zdesk --check-config ~/.config/zdesk/config.toml
```

Expected version: `zDesk 0.1.1`. Configuration validation also checks referenced font files and does not open a desktop window.

## Running zDesk safely

When first installing or migrating, keep automatic startup disabled and test the service **after the GNOME desktop is fully loaded**:

```bash
systemctl --user start zdesk.service
systemctl --user --no-pager status zdesk.service
```

To stop the widget:

```bash
systemctl --user stop zdesk.service
```

To run a build directly without installing it, first ensure `~/.config/zdesk/config.toml` exists, then run:

```bash
cargo build --release --locked
./target/release/zdesk
```

`zdesk` does **not** accept a positional configuration path. Use `--check-config /path/to/file.toml` to validate a particular file.

## zDesk Settings

Open **zDesk Settings** from the GNOME application menu, or run:

```bash
zdesk-settings
```

The editor provides settings for the clock, weekday labels, greeting, positions, ring layout, colors, and sensor refresh interval. Click **Save & Apply** to validate and save edits. If the widget service is active, Settings requests a service restart to apply the updated configuration. The settings file's unrelated values and comments are preserved, and a backup is created when saving changes.

The Settings application runs only when opened; GTK4/libadwaita is not loaded by the widget itself. More advanced options can be changed directly in the TOML file.

## Optional autostart and GNOME safety

> **Important:** An earlier iteration of this project was associated with a GNOME/XWayland login-loop incident. Delayed startup and the guard reduce the risk of repeated automatic launches, but **cannot guarantee that the compositor will never fail**.

The installed `zdesk.service` does not restart automatically (`Restart=no`). The optional `zdesk.timer` starts the service approximately 30 seconds after the timer activates. A startup marker is cleared after 60 seconds of stable runtime or a normal stop; if startup fails unexpectedly, the guard can prevent subsequent automatic attempts.

In v0.1.1 the default timer delay is shortened from 45 to **30 seconds**. It is measured from timer activation, not from entering the GNOME password. Shorter delays have not been validated on every GNOME/XWayland version; start with autostart disabled after upgrading and verify a full logout/login cycle. The installer deliberately disables an existing zDesk timer until you opt in again. To restore the previously tested 45-second delay, edit `~/.config/systemd/user/zdesk.timer` and set `OnActiveSec=45s`, then run `systemctl --user daemon-reload` before re-enabling the timer. Avoid an immediate 0-second start on systems with a history of login loops.

Only after successful manual tests and a normal logout/login test, enable autostart:

```bash
systemctl --user enable zdesk.timer
```

The timer will be activated by the graphical user session; it is **not necessary to enable `zdesk.service` directly**. To disable autostart and stop the widget:

```bash
systemctl --user disable --now zdesk.timer
systemctl --user stop zdesk.service
```

### Recovery if GNOME cannot open normally

If a future login is affected, try `Ctrl + Alt + F3` to access a text console (TTY), sign in, then disable the user timer:

```bash
systemctl --user disable --now zdesk.timer
systemctl --user stop zdesk.service
```

If the user systemd manager is unavailable from TTY, remove the autostart symlink and try logging in again:

```bash
rm -f ~/.config/systemd/user/graphical-session.target.wants/zdesk.timer
```

These commands only target zDesk's autostart and do not modify GNOME Shell or GDM. A different GNOME failure may require separate diagnosis.

If a guarded startup was skipped, inspect the logs and reset the marker **only after** the desktop is stable:

```bash
journalctl --user -u zdesk.service -n 50 --no-pager
~/.local/lib/zdesk/login-guard.sh reset
```

## Building and testing

```bash
# Desktop widget
cargo fmt --all -- --check
cargo test --locked
cargo build --release --locked

# Separate Settings app
cargo fmt --manifest-path settings/Cargo.toml --all -- --check
cargo test --manifest-path settings/Cargo.toml --locked
cargo build --manifest-path settings/Cargo.toml --release --locked
```

For a one-minute resource measurement while the widget is running:

```bash
bash scripts/benchmark.sh 60
```

The benchmark script reads CPU and memory statistics from `/proc`; it can also report Conky statistics when Conky is running. Results depend on the machine, monitor arrangement, and workload. One initial Ubuntu desktop test of zDesk v0.1.0 measured approximately **4.8 MiB RSS** and **0.1–0.2% CPU** in short samples; this is not a long-term performance guarantee.

## Troubleshooting

| Symptom | What to check |
| --- | --- |
| Missing font / invalid config | Run `zdesk --check-config ~/.config/zdesk/config.toml` and correct the referenced font paths. |
| Widget does not start through systemd | Check `systemctl --user status zdesk.service` and `journalctl --user -u zdesk.service -n 50 --no-pager`. |
| XWayland environment unavailable | Compare `DISPLAY`, `WAYLAND_DISPLAY`, and `XAUTHORITY` with `systemctl --user show-environment`. |
| Window placement differs | Inspect `xrandr --listmonitors`; GNOME/Mutter ultimately controls XWayland window placement. |
| Settings changes are not visible | Check whether the service is active, then use **Save & Apply** or restart it manually. |
| Startup guard skips launch | Review the journal; reset the guard only after GNOME is stable. |

## Repository layout

```text
zDesk/
├── Cargo.toml
├── Cargo.lock
├── README.md
├── LICENSE
├── CHANGELOG.md
├── config/
│   ├── default.toml
│   └── v0.1.toml
├── src/                 # Native Rust desktop widget
├── settings/            # GTK4/libadwaita Settings app (separate Cargo package)
├── scripts/             # Installers, login guard, benchmark
├── systemd/             # User service and delayed timer
└── docs/                # Historical development notes
```

## Limitations

- Designed primarily for Ubuntu GNOME with XWayland; other environments are not verified.
- Configuration changes require a service restart; there is no live config reload.
- Advanced configuration options are edited directly in TOML.
- GNOME/Mutter controls final placement of XWayland desktop-type windows.

## License

zDesk is released under the **MIT License**. See [LICENSE](LICENSE). Third-party Rust crates, GTK libraries, and font files remain subject to their respective licenses.

Copyright © 2026 Azis Lamayuda.
