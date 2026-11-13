#!/bin/sh
# Install LOQ Power from a local build.
#
# Usage:
#   ./packaging/install.sh            # install binaries, units and policy files
#   ./packaging/install.sh --uninstall
#
# This script never touches TLP. Disabling TLP is a separate, explicit step:
#
#   sudo loq-power takeover
#
# and can be undone with:
#
#   sudo loq-power restore-tlp

set -eu

PREFIX="${PREFIX:-/usr}"
DESTDIR="${DESTDIR:-}"
BINDIR="$DESTDIR$PREFIX/bin"
UNITDIR="$DESTDIR$PREFIX/lib/systemd/system"
DBUSDIR="$DESTDIR$PREFIX/share/dbus-1/system.d"
SERVICEDIR="$DESTDIR$PREFIX/share/dbus-1/system-services"
POLKITDIR="$DESTDIR$PREFIX/share/polkit-1/actions"
APPDIR="$DESTDIR$PREFIX/share/applications"
ICONDIR="$DESTDIR$PREFIX/share/icons"
CONFDIR="$DESTDIR/etc/loq-power"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

require_root() {
    if [ "$(id -u)" -ne 0 ] && [ -z "$DESTDIR" ]; then
        echo "run this script as root (or set DESTDIR for a staging install)" >&2
        exit 1
    fi
}

uninstall() {
    require_root
    rm -f "$BINDIR/loq-power" "$BINDIR/loq-power-helper" "$BINDIR/loq-powerd" "$BINDIR/loq-gui"
    rm -f "$UNITDIR/loq-powerd.service"
    rm -f "$DBUSDIR/org.loq.Power1.conf" "$SERVICEDIR/org.loq.Power1.service"
    rm -f "$POLKITDIR/org.loq.power.policy"
    rm -f "$APPDIR/org.loq.Power.desktop"
    rm -f "$ICONDIR/hicolor/128x128/apps/org.loq.Power.png"
    rm -f "$ICONDIR/hicolor/scalable/apps/org.loq.Power.svg"
    systemctl daemon-reload 2>/dev/null || true
    echo "LOQ Power removed. /etc/loq-power was left in place."
}

build_hint() {
    echo "build the release binaries first:" >&2
    echo "  cargo build --release -p loq-cli -p loq-helper -p loq-powerd" >&2
    echo "  cargo tauri build --release   # or: bun run build in ui/ then cargo build --release -p loq-gui" >&2
    exit 1
}

install_files() {
    require_root

    [ -x "$ROOT/target/release/loq-power" ] || build_hint
    [ -x "$ROOT/target/release/loq-power-helper" ] || build_hint
    [ -x "$ROOT/target/release/loq-powerd" ] || build_hint

    install -d "$BINDIR" "$UNITDIR" "$DBUSDIR" "$SERVICEDIR" "$POLKITDIR" "$APPDIR"
    install -d "$ICONDIR/hicolor/128x128/apps" "$ICONDIR/hicolor/scalable/apps"
    install -d "$CONFDIR"

    install -m755 "$ROOT/target/release/loq-power" "$BINDIR/loq-power"
    install -m755 "$ROOT/target/release/loq-power-helper" "$BINDIR/loq-power-helper"
    install -m755 "$ROOT/target/release/loq-powerd" "$BINDIR/loq-powerd"
    if [ -x "$ROOT/target/release/loq-gui" ]; then
        install -m755 "$ROOT/target/release/loq-gui" "$BINDIR/loq-gui"
    else
        echo "warning: loq-gui was not built; the desktop entry will not start" >&2
    fi
    if [ -x "$ROOT/target/release/loq-power-desktop" ]; then
        install -m755 "$ROOT/target/release/loq-power-desktop" "$BINDIR/loq-power-desktop"
    fi

    install -m644 "$ROOT/packaging/systemd/loq-powerd.service" "$UNITDIR/loq-powerd.service"
    install -m644 "$ROOT/packaging/dbus/org.loq.Power1.conf" "$DBUSDIR/org.loq.Power1.conf"
    install -m644 "$ROOT/packaging/dbus/org.loq.Power1.service" "$SERVICEDIR/org.loq.Power1.service"
    install -m644 "$ROOT/packaging/polkit/org.loq.power.policy" "$POLKITDIR/org.loq.power.policy"
    install -m644 "$ROOT/packaging/desktop/org.loq.Power.desktop" "$APPDIR/org.loq.Power.desktop"
    install -m644 "$ROOT/packaging/icons/hicolor/128x128/apps/org.loq.Power.png" \
        "$ICONDIR/hicolor/128x128/apps/org.loq.Power.png"
    install -m644 "$ROOT/packaging/icons/hicolor/scalable/apps/org.loq.Power.svg" \
        "$ICONDIR/hicolor/scalable/apps/org.loq.Power.svg"

    if [ ! -f "$CONFDIR/profiles.toml" ]; then
        install -m644 "$ROOT/packaging/profiles.toml" "$CONFDIR/profiles.toml"
    fi

    systemctl daemon-reload 2>/dev/null || true
    systemctl reload dbus 2>/dev/null || systemctl restart dbus 2>/dev/null || true
    systemctl restart polkit 2>/dev/null || true

    echo
    echo "LOQ Power installed."
    echo
    echo "Next steps:"
    echo "  1. Try the GUI or the CLI:      loq-power status"
    echo "  2. Hand over from TLP:         sudo loq-power takeover"
    echo "     (stops and disables tlp.service / tlp-pd.service)"
    echo "  3. Roll back at any time:      sudo loq-power restore-tlp"
}

case "${1:-}" in
    --uninstall|-u) uninstall ;;
    "") install_files ;;
    *) echo "unknown option: $1" >&2; exit 1 ;;
esac
