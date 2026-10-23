#!/usr/bin/env bash
set -euo pipefail

project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_dir"

if ! command -v cargo >/dev/null 2>&1; then
  echo "Cargo is not installed. Install Rust/Cargo, then run this script again." >&2
  exit 1
fi

cargo build --release --locked

# Fail closed on updates: do not leave any previously enabled zDesk startup
# units running while replacing the binary or unit files. The user must
# explicitly opt in to automatic startup after testing this release.
for unit in zdesk.timer zdesk.service; do
  state="$(systemctl --user is-active "$unit" 2>/dev/null || true)"
  enabled="$(systemctl --user is-enabled "$unit" 2>/dev/null || true)"
  if [[ "$state" == active || "$state" == activating ]]; then
    systemctl --user stop "$unit"
  fi
  if [[ "$enabled" == enabled || "$enabled" == enabled-runtime ]]; then
    systemctl --user disable "$unit"
  fi
done

binary="$HOME/.local/bin/zdesk"
config="$HOME/.config/zdesk/config.toml"
service="$HOME/.config/systemd/user/zdesk.service"
timer="$HOME/.config/systemd/user/zdesk.timer"
guard="$HOME/.local/lib/zdesk/login-guard.sh"

install -D -m 0755 target/release/zdesk "${binary}.new"
mv -f -- "${binary}.new" "$binary"
if [[ ! -e "$config" ]]; then
  install -D -m 0644 config/default.toml "$config"
elif cmp -s "$config" config/v0.1.toml; then
  cp -p -- "$config" "${config}.v0.1.bak"
  install -D -m 0644 config/default.toml "$config"
  echo "Default v0.1 configuration backed up and updated: $config"
else
  echo "Existing configuration preserved: $config"
fi
example="$HOME/.config/zdesk/example.toml"
if [[ ! -e "$example" ]]; then
  install -D -m 0644 config/default.toml "$example"
fi
# Preserve a backup of any older unit before replacing it.
if [[ -e "$service" ]]; then
  if ! cmp -s "$service" systemd/zdesk.service; then
    if [[ ! -e "${service}.pre-safe-start.bak" ]]; then
      cp -p -- "$service" "${service}.pre-safe-start.bak"
    fi
  fi
fi
install -D -m 0644 systemd/zdesk.service "$service"
if [[ -e "$timer" ]] && ! cmp -s "$timer" systemd/zdesk.timer; then
  if [[ ! -e "${timer}.pre-safe-start.bak" ]]; then
    cp -p -- "$timer" "${timer}.pre-safe-start.bak"
  fi
fi
install -D -m 0644 systemd/zdesk.timer "$timer"
install -D -m 0755 scripts/login-guard.sh "$guard"
systemctl --user daemon-reload
"$binary" --check-config

echo "Installed: $binary"
echo "Service ready: $service"
echo "Safe startup timer ready: $timer"
echo "All zDesk autostart units are disabled. Enable only after manual GNOME/XWayland testing. Conky was not changed."
