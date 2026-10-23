#!/usr/bin/env bash
set -euo pipefail

project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_dir"

if ! command -v cargo >/dev/null 2>&1; then
  echo "Cargo is not installed." >&2
  exit 1
fi
if ! pkg-config --exists gtk4 libadwaita-1; then
  echo "GTK4 and libadwaita are unavailable. Install libgtk-4-dev and libadwaita-1-dev." >&2
  exit 1
fi

# The desktop widget and settings app are separate binaries. Replacing the
# widget binary does not interrupt the currently running desktop process.
cargo build --release --locked
cargo build --manifest-path settings/Cargo.toml --release --locked

widget="$HOME/.local/bin/zdesk"
settings="$HOME/.local/bin/zdesk-settings"
launcher="$HOME/.local/share/applications/id.lamayuda.zDesk.Settings.desktop"
legacy_launcher="$HOME/.local/share/applications/zdesk-settings.desktop"

install -D -m 0755 target/release/zdesk "${widget}.new"
mv -f -- "${widget}.new" "$widget"
install -D -m 0755 settings/target/release/zdesk-settings "${settings}.new"
mv -f -- "${settings}.new" "$settings"

mkdir -p -- "$(dirname -- "$launcher")"
cat > "${launcher}.new" <<EOF
[Desktop Entry]
Type=Application
Name=zDesk Settings
Name[id]=zDesk Settings
GenericName=Desktop widget settings
GenericName[id]=Desktop widget settings
Comment=Adjust clock, greeting and sensor widgets
Comment[id]=Adjust clock, greeting and sensor widgets
Exec=$settings
Icon=preferences-system
Terminal=false
Categories=Settings;
Keywords=zDesk;Clock;Desktop;Widget;
Keywords[id]=zDesk;Clock;Desktop;Widget;
StartupNotify=true
EOF
chmod 0644 "${launcher}.new"
mv -f -- "${launcher}.new" "$launcher"
if [[ -f "$legacy_launcher" ]] && grep -Fqx "Exec=$settings" "$legacy_launcher"; then
  rm -f -- "$legacy_launcher"
fi

echo "Settings installed: $settings"
echo "Open 'zDesk Settings' from the GNOME application menu."
echo "The zDesk service and timer are unchanged; the settings app does not start automatically."
