#!/bin/sh
# ts6rpc installer for Linux — Discord Rich Presence for TeamSpeak 6
#
#   Install / update:  curl -fsSL https://raw.githubusercontent.com/wildesPepega/ts6rpc/main/install.sh | sh
#   Without autostart: curl -fsSL https://raw.githubusercontent.com/wildesPepega/ts6rpc/main/install.sh | sh -s -- --no-autostart
#   Uninstall:         curl -fsSL https://raw.githubusercontent.com/wildesPepega/ts6rpc/main/install.sh | sh -s -- --uninstall
#
# Installs into your home directory only (no root needed):
#   ~/.local/bin/ts6rpc, a menu entry, an icon and an autostart entry.
set -eu

REPO="wildesPepega/ts6rpc"
VERSION="${TS6RPC_VERSION:-latest}"

BIN_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}"
BIN="$BIN_DIR/ts6rpc"
APP_ENTRY="$DATA_DIR/applications/ts6rpc.desktop"
AUTOSTART_ENTRY="$CONFIG_DIR/autostart/ts6rpc.desktop"
ICON="$DATA_DIR/icons/hicolor/256x256/apps/ts6rpc.png"

info() { printf '\033[1;34m::\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m!!\033[0m %s\n' "$*" >&2; }
die() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

download() { # url dest
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL --retry 3 -o "$2" "$1"
    elif command -v wget >/dev/null 2>&1; then
        wget -q -O "$2" "$1"
    else
        die "curl or wget is required"
    fi
}

stop_running() {
    if pkill -x ts6rpc 2>/dev/null; then
        info "Stopping the running ts6rpc"
        sleep 1
    fi
}

uninstall() {
    stop_running
    rm -f "$BIN" "$APP_ENTRY" "$AUTOSTART_ENTRY" "$ICON"
    command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q "$DATA_DIR/applications" 2>/dev/null || true
    info "ts6rpc uninstalled. Your settings are kept in $CONFIG_DIR/ts6rpc (delete it to remove them too)."
}

desktop_entry() {
    cat <<EOF
[Desktop Entry]
Type=Application
Name=ts6rpc
GenericName=TeamSpeak Discord Rich Presence
Comment=Discord Rich Presence for TeamSpeak 6
Exec="$BIN"
Icon=ts6rpc
Terminal=false
Categories=Network;Chat;
X-GNOME-Autostart-enabled=true
EOF
}

AUTOSTART=1
for arg in "$@"; do
    case "$arg" in
        --uninstall) uninstall; exit 0 ;;
        --no-autostart) AUTOSTART=0 ;;
        -h|--help) sed -n '2,10p' "$0" 2>/dev/null || true; exit 0 ;;
        *) die "unknown option: $arg" ;;
    esac
done

[ "$(uname -s)" = "Linux" ] || die "this installer is for Linux. On Windows use install.ps1 (see the README)."

case "$(uname -m)" in
    x86_64|amd64) ARCH="x86_64" ;;
    aarch64|arm64) ARCH="aarch64" ;;
    *) die "unsupported CPU architecture: $(uname -m) (supported: x86_64, aarch64)" ;;
esac

if [ "$VERSION" = "latest" ]; then
    BASE="https://github.com/$REPO/releases/latest/download"
else
    BASE="https://github.com/$REPO/releases/download/$VERSION"
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT INT TERM

info "Downloading ts6rpc ($VERSION, linux-$ARCH)"
download "$BASE/ts6rpc-linux-$ARCH" "$TMP/ts6rpc" || die "download failed"
chmod +x "$TMP/ts6rpc"
"$TMP/ts6rpc" --version >/dev/null 2>&1 || die "the downloaded binary does not run on this system"
download "https://raw.githubusercontent.com/$REPO/main/assets/icon.png" "$TMP/icon.png" || warn "could not download the icon"

stop_running
mkdir -p "$BIN_DIR" "$DATA_DIR/applications"
mv -f "$TMP/ts6rpc" "$BIN"
if [ -s "$TMP/icon.png" ]; then
    mkdir -p "$(dirname "$ICON")"
    mv -f "$TMP/icon.png" "$ICON"
fi
desktop_entry > "$APP_ENTRY"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q "$DATA_DIR/applications" 2>/dev/null || true
info "Installed $("$BIN" --version) to $BIN"

if [ "$AUTOSTART" = 1 ]; then
    mkdir -p "$(dirname "$AUTOSTART_ENTRY")"
    desktop_entry > "$AUTOSTART_ENTRY"
    info "Starts automatically when you log in (switch off in the tray menu: \"Start on login\")"
fi

if [ -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]; then
    if command -v setsid >/dev/null 2>&1; then
        setsid "$BIN" >/dev/null 2>&1 </dev/null &
    else
        nohup "$BIN" >/dev/null 2>&1 </dev/null &
    fi
    info "ts6rpc is running. Look for its icon in your system tray."
else
    info "Start it from your app menu or by running: $BIN"
fi

case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) warn "$BIN_DIR is not in your PATH (only matters if you want to run 'ts6rpc' from a terminal)" ;;
esac

cat <<EOF

On the first start, TeamSpeak asks whether to allow "Discord Rich Presence". Click allow.
Settings: $CONFIG_DIR/ts6rpc/config.json
GNOME needs the "AppIndicator and KStatusNotifierItem Support" extension for the tray icon.
EOF
