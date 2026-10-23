#!/usr/bin/env bash
# Build an AppImage that uses the host GTK 4.10+ runtime (modern distros only).
set -euo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT"

VERSION=${VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)}
ARCH=${ARCH:-$(uname -m)}
OUT=${OUT:-$ROOT/dist}
APPIMAGETOOL=${APPIMAGETOOL:-appimagetool}
APPDIR=$OUT/PowerSwitcher.AppDir

case "$ARCH" in
    x86_64|amd64) ARCH=x86_64 ;;
    aarch64|arm64) ARCH=aarch64 ;;
    *) echo "unsupported architecture: $ARCH" >&2; exit 1 ;;
esac

command -v "$APPIMAGETOOL" >/dev/null 2>&1 || {
    echo "appimagetool not found; set APPIMAGETOOL=/path/to/appimagetool" >&2
    exit 1
}
if ! command -v rsvg-convert >/dev/null 2>&1 \
    && ! command -v magick >/dev/null 2>&1 \
    && ! command -v convert >/dev/null 2>&1; then
    echo "rsvg-convert or ImageMagick is required for the AppImage icon" >&2
    exit 1
fi

cargo build --release -p ps-daemon -p ps-gui

rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/power_switcher/profiles" \
    "$APPDIR/usr/share/applications" "$APPDIR/usr/share/icons/hicolor/scalable/apps" \
    "$APPDIR/usr/share/gnome-shell/extensions"

install -m 755 target/release/ps "$APPDIR/usr/bin/power-switcher"
install -m 755 target/release/ps-gui "$APPDIR/usr/bin/power-switcher-gui"
install -m 644 power_switcher/detection/profiles/*.json.gz "$APPDIR/usr/share/power_switcher/profiles/"
install -m 644 crates/ps-core/data/layouts.json "$APPDIR/usr/share/power_switcher/layouts.json"
install -m 644 packaging/power-switcher.desktop "$APPDIR/usr/share/applications/"
install -m 644 packaging/power-switcher.svg \
    "$APPDIR/usr/share/icons/hicolor/scalable/apps/"
cp -R gnome-extension/power-switcher-focus@powerswitcher \
    "$APPDIR/usr/share/gnome-shell/extensions/"

if command -v rsvg-convert >/dev/null 2>&1; then
    rsvg-convert -w 256 -h 256 -o "$APPDIR/power-switcher.png" \
        packaging/power-switcher.svg
elif command -v magick >/dev/null 2>&1; then
    magick -background none packaging/power-switcher.svg -resize 256x256 \
        "$APPDIR/power-switcher.png"
else
    convert -background none packaging/power-switcher.svg -resize 256x256 \
        "$APPDIR/power-switcher.png"
fi
ln -s power-switcher.png "$APPDIR/.DirIcon"
install -m 644 packaging/power-switcher.desktop "$APPDIR/power-switcher.desktop"

cat > "$APPDIR/AppRun" <<'EOF'
#!/bin/sh
set -eu
HERE=$(dirname "$(readlink -f "$0")")
export PATH="$HERE/usr/bin:$PATH"
export PS_PROFILES_DIR="$HERE/usr/share/power_switcher/profiles"
export PS_LAYOUTS_JSON="$HERE/usr/share/power_switcher/layouts.json"
case "${1:-gui}" in
    gui) exec "$HERE/usr/bin/power-switcher-gui" ;;
    daemon|debug|setup) exec "$HERE/usr/bin/power-switcher" "$@" ;;
    *) exec "$HERE/usr/bin/power-switcher-gui" "$@" ;;
esac
EOF
chmod 755 "$APPDIR/AppRun"

if [ "${APPIMAGE_EXTRACT_AND_RUN:-0}" = 1 ]; then
    ARCH=$ARCH "$APPIMAGETOOL" --appimage-extract-and-run "$APPDIR" \
        "$OUT/PowerSwitcher-$VERSION-$ARCH.AppImage"
else
    ARCH=$ARCH "$APPIMAGETOOL" "$APPDIR" "$OUT/PowerSwitcher-$VERSION-$ARCH.AppImage"
fi

echo "AppImage: $OUT/PowerSwitcher-$VERSION-$ARCH.AppImage"
