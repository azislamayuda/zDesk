# Previous development history

These entries document technical milestones of the codebase before its rebranding and version reset to **zDesk 0.1.0**. The earlier version numbers are retained for historical reference; the product name has been normalized to zDesk throughout this repository.

# Changelog

## 0.4.3

- Fixed startup being blocked after a normal shutdown soon after Save & Apply.
- Moved the 60-second stability check into the widget process and cleared the current boot's marker when the service stops cleanly; unexpected failures still retain it.
- Made Save & Apply wait for the restart result and verify the service stays active briefly before reporting success.
- Kept the running timer-backed service in place when upgrading with the installer.

## 0.4.2

- Changed the settings window, launcher, installer messages, and documentation to English.
- Added a Changelog item directly below About zDesk in the application menu.

## 0.4.1

- Moved Save & Apply to the bottom-right corner of the settings window.
- Added an About dialog showing Azis Lamayuda as the creator and the application version.
- Improved the settings application menu and desktop launcher metadata.

## 0.4.0

- Added a separate GTK4/libadwaita settings window; the desktop widget remains toolkit-free.
- Added editable layout, font, color, greeting, and sensor settings.
- Added validated TOML saves with a backup and an apply request when the widget is running.
- Added a GNOME application launcher for the settings window.

## 0.3.1

- Added a delayed systemd user timer and login guard for safer startup after login.
- Stopped automatic retries after a startup failure and preserved a backup of the previous service.

## 0.3

- Added a clock and sensor rings to every active monitor in one process.
- Shared sensor readings across monitors and updated widget placement when the monitor layout changed.

## 0.2.1

- Centered widgets relative to the primary monitor using RandR.
- Repositioned widget windows after monitor layout changes.

## 0.2

- Added weekday labels, a greeting, and CPU, temperature, and RAM rings.
- Read sensors directly from Linux and drew the rings in a second transparent window.
- Rendered in small strips to keep memory use low.

## 0.1

- Introduced the lightweight XWayland desktop clock with a transparent window.
- Displayed HH:MM using styles and placement from a TOML configuration file.
