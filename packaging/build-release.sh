#!/usr/bin/env bash
# Build the release tarball and, when the tools are available, native
# packages and an AppImage.  Runs from any directory.
set -euo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT"

VERSION=${VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)}
ARCH=${ARCH:-$(uname -m)}
OUT=${OUT:-$ROOT/dist}
NFPM=${NFPM:-nfpm}

case "$ARCH" in
    x86_64|amd64)
        ARCH=x86_64
        NFPM_ARCH=amd64
        ;;
    aarch64|arm64)
        ARCH=aarch64
        NFPM_ARCH=arm64
        ;;
    *)
        echo "unsupported architecture: $ARCH" >&2
        exit 1
        ;;
esac

STAGE=$OUT/stage/power-switcher-$VERSION-$ARCH
mkdir -p "$OUT" "$STAGE"

cargo build --release -p ps-daemon -p ps-gui

install -d "$STAGE/bin" "$STAGE/share/power_switcher/profiles" \
    "$STAGE/share/applications" "$STAGE/share/icons/hicolor/scalable/apps" \
    "$STAGE/lib/systemd/user" "$STAGE/lib/udev/rules.d" "$STAGE/lib/modules-load.d" \
    "$STAGE/share/gnome-shell/extensions"

install -m 755 target/release/ps "$STAGE/bin/power-switcher"
install -m 755 target/release/ps-gui "$STAGE/bin/power-switcher-gui"
install -m 644 power_switcher/detection/profiles/*.json.gz "$STAGE/share/power_switcher/profiles/"
install -m 644 crates/ps-core/data/layouts.json "$STAGE/share/power_switcher/layouts.json"
install -m 644 packaging/power-switcher.desktop "$STAGE/share/applications/"
install -m 644 packaging/power-switcher.svg "$STAGE/share/icons/hicolor/scalable/apps/"
install -m 644 packaging/power_switcher.service "$STAGE/lib/systemd/user/"
install -m 644 udev/60-power_switcher.rules "$STAGE/lib/udev/rules.d/"
install -m 644 packaging/power_switcher.modules-load.conf \
    "$STAGE/lib/modules-load.d/power_switcher.conf"
cp -R gnome-extension/power-switcher-focus@powerswitcher \
    "$STAGE/share/gnome-shell/extensions/"
install -m 755 packaging/install.sh "$STAGE/install.sh"
if [ -f packaging/README.md ]; then
    install -m 644 packaging/README.md "$STAGE/README.md"
fi

tar -C "$OUT/stage" -czf "$OUT/power-switcher-$VERSION-$ARCH.tar.gz" \
    "power-switcher-$VERSION-$ARCH"
cp "$OUT/power-switcher-$VERSION-$ARCH.tar.gz" "$OUT/power-switcher-$ARCH.tar.gz"

if command -v "$NFPM" >/dev/null 2>&1; then
    export VERSION NFPM_ARCH
    "$NFPM" package --config packaging/nfpm.yaml --packager deb --target "$OUT/"
    "$NFPM" package --config packaging/nfpm.yaml --packager rpm --target "$OUT/"
    "$NFPM" package --config packaging/nfpm.yaml --packager archlinux --target "$OUT/"
else
    echo "nfpm not found; skipping .deb/.rpm/.pkg.tar.zst" >&2
fi

if [ -n "${APPIMAGETOOL:-}" ] || command -v appimagetool >/dev/null 2>&1; then
    VERSION=$VERSION ARCH=$ARCH OUT=$OUT ./packaging/build-appimage.sh
else
    echo "appimagetool not found; skipping AppImage (set APPIMAGETOOL)" >&2
fi

install -m 755 packaging/install.sh "$OUT/install.sh"
rm -rf "$OUT/stage" "$OUT/PowerSwitcher.AppDir"

echo "Artifacts in $OUT"
