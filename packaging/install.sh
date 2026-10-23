#!/bin/sh
# PowerSwitcher universal installer.
#
# Default: install under ~/.local for the current user, then install the udev
# rule, the systemd user unit, and (on GNOME) the helper extension.
#
#   curl -fsSL https://github.com/illnar-io/power_switcher/releases/latest/download/install.sh | sh
#
# Options:
#   --prefix DIR       install prefix (default: ~/.local)
#   --release VERSION  install a pinned release (e.g. 0.1.0 or v0.1.0)
#   --no-udev          skip the system udev/module files
#   --add-group        fall back to the input group (not needed with uaccess)
#   --no-group         keep the default: do not add the user to input
#   --no-service       do not install/enable the systemd user unit
#   --no-gnome         do not install the GNOME helper extension
#   --uninstall        remove installed files (keeps config and user data)
#
# Environment: TARBALL=/path/to.tar.gz for an offline install, BASE_URL to
# test a different release host, COSIGN_IDENTITY (and optional COSIGN_ISSUER)
# or MINISIGN_PUBKEY to verify SHA256SUMS.

set -eu

if [ "$(id -u)" -eq 0 ] && [ -n "${SUDO_USER:-}" ]; then
    printf 'install.sh: run as your normal user, not via sudo; system files will prompt for root.\n' >&2
    exit 1
fi

APP=power-switcher
REPO=${REPO:-illnar-io/power_switcher}
BASE_URL=${BASE_URL:-https://github.com/$REPO/releases}
PREFIX=${PREFIX:-$HOME/.local}
VERSION=${VERSION:-latest}
TARBALL=${TARBALL:-}
NO_UDEV=0
NO_GROUP=1
NO_SERVICE=0
NO_GNOME=0
UNINSTALL=0
NEED_RELOGIN=0

usage() {
    cat <<'EOF'
PowerSwitcher installer

Usage: install.sh [options]

  --prefix DIR       install prefix (default: ~/.local)
  --release VERSION  install a pinned release (e.g. 0.1.0)
  --no-udev          skip the system udev/module files
  --add-group        add the user to the input group (fallback if uaccess fails)
  --no-group         keep the default: do not modify group membership
  --no-service       do not install/enable the systemd user unit
  --no-gnome         do not install the GNOME helper extension
  --uninstall        remove installed files (keeps config and user data)
  -h, --help         show this help

After installing, run `power-switcher gui`.
EOF
}

die() {
    printf 'install.sh: %s\n' "$*" >&2
    exit 1
}

warn() {
    printf 'install.sh: %s\n' "$*" >&2
}

run_root() {
    if [ "$(id -u)" -eq 0 ]; then
        "$@"
    elif command -v sudo >/dev/null 2>&1; then
        sudo "$@"
    elif command -v pkexec >/dev/null 2>&1; then
        pkexec "$@"
    else
        return 1
    fi
}

download() {
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL --retry 3 -o "$2" "$1"
    elif command -v wget >/dev/null 2>&1; then
        wget -qO "$2" "$1"
    else
        die "curl or wget is required"
    fi
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --prefix)
            [ "$#" -ge 2 ] || die "--prefix needs a directory"
            PREFIX=$2
            shift 2
            ;;
        --release)
            [ "$#" -ge 2 ] || die "--release needs a version"
            VERSION=$2
            shift 2
            ;;
        --no-udev) NO_UDEV=1; shift ;;
        --add-group) NO_GROUP=0; shift ;;
        --no-group) NO_GROUP=1; shift ;;
        --no-service) NO_SERVICE=1; shift ;;
        --no-gnome) NO_GNOME=1; shift ;;
        --uninstall) UNINSTALL=1; shift ;;
        -h|--help) usage; exit 0 ;;
        --version) printf '%s\n' "$APP installer 1"; exit 0 ;;
        *) die "unknown option: $1 (try --help)" ;;
    esac
done

case "$PREFIX" in
    "~/"*) PREFIX=$HOME/${PREFIX#\~/} ;;
esac
[ -n "${HOME:-}" ] || die "HOME is not set"
[ "${PREFIX#/}" != "$PREFIX" ] || die "prefix must be an absolute path: $PREFIX"

if [ -n "${XDG_DATA_HOME:-}" ] && [ "${XDG_DATA_HOME#/}" != "$XDG_DATA_HOME" ]; then
    DATA_HOME=$XDG_DATA_HOME
else
    DATA_HOME=$HOME/.local/share
fi
if [ -n "${XDG_CONFIG_HOME:-}" ] && [ "${XDG_CONFIG_HOME#/}" != "$XDG_CONFIG_HOME" ]; then
    CONFIG_HOME=$XDG_CONFIG_HOME
else
    CONFIG_HOME=$HOME/.config
fi

BIN_DIR=$PREFIX/bin
SHARE_DIR=$PREFIX/share/power_switcher
DESKTOP_DIR=$DATA_HOME/applications
ICON_DIR=$DATA_HOME/icons/hicolor/scalable/apps
UNIT_DIR=$CONFIG_HOME/systemd/user
GNOME_DIR=$DATA_HOME/gnome-shell/extensions
UNIT_NAME=power_switcher.service
USER_NAME=$(id -un)

if [ "$UNINSTALL" -eq 1 ]; then
    if command -v systemctl >/dev/null 2>&1; then
        systemctl --user disable --now "$UNIT_NAME" >/dev/null 2>&1 || true
    fi
    rm -f "$BIN_DIR/power-switcher" "$BIN_DIR/power-switcher-gui"
    rm -f "$DESKTOP_DIR/power-switcher.desktop" "$ICON_DIR/power-switcher.svg"
    rm -f "$UNIT_DIR/$UNIT_NAME"
    rm -rf "$SHARE_DIR" "$GNOME_DIR/power-switcher-focus@powerswitcher"
    if [ "$NO_UDEV" -eq 0 ]; then
        run_root rm -f /etc/udev/rules.d/60-power_switcher.rules \
            /etc/udev/rules.d/99-power_switcher.rules \
            /etc/modules-load.d/power_switcher.conf || true
        run_root udevadm control --reload-rules >/dev/null 2>&1 || true
    fi
    if command -v systemctl >/dev/null 2>&1; then
        systemctl --user daemon-reload >/dev/null 2>&1 || true
    fi
    printf 'Removed PowerSwitcher from %s. Config and user data were kept.\n' "$PREFIX"
    exit 0
fi

case "$(uname -m)" in
    x86_64|amd64) ARCH=x86_64 ;;
    aarch64|arm64) ARCH=aarch64 ;;
    *) die "unsupported architecture: $(uname -m) (x86_64/aarch64 only)" ;;
esac

TMP=$(mktemp -d "${TMPDIR:-/tmp}/power-switcher.XXXXXX")
trap 'rm -rf "$TMP"' EXIT HUP INT TERM

case "$0" in
    */*) SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" 2>/dev/null && pwd || pwd) ;;
    install.sh) SCRIPT_DIR=$(pwd) ;;
    *) SCRIPT_DIR= ;;
esac
SRC=

if [ -n "$TARBALL" ]; then
    [ -f "$TARBALL" ] || die "tarball not found: $TARBALL"
    tar -xzf "$TARBALL" -C "$TMP"
    SRC=$(find "$TMP" -mindepth 1 -maxdepth 1 -type d -name 'power-switcher-*' | head -n 1)
    [ -n "$SRC" ] || die "release tarball has no power-switcher directory"
elif [ -n "$SCRIPT_DIR" ] && [ -x "$SCRIPT_DIR/bin/power-switcher" ]; then
    SRC=$SCRIPT_DIR
fi

if [ -z "$SRC" ]; then
    if [ "$VERSION" = latest ]; then
        ASSET=power-switcher-$ARCH.tar.gz
        ASSET_URL=$BASE_URL/latest/download/$ASSET
        SUMS_URL=$BASE_URL/latest/download/SHA256SUMS
    else
        case "$VERSION" in
            v*) TAG=$VERSION ;;
            *) TAG=v$VERSION ;;
        esac
        REL=${TAG#v}
        ASSET=power-switcher-$REL-$ARCH.tar.gz
        ASSET_URL=$BASE_URL/download/$TAG/$ASSET
        SUMS_URL=$BASE_URL/download/$TAG/SHA256SUMS
    fi

    download "$ASSET_URL" "$TMP/$ASSET" || die "could not download $ASSET"

    if download "$SUMS_URL" "$TMP/SHA256SUMS" 2>/dev/null; then
        if [ -n "${COSIGN_IDENTITY:-}" ] && command -v cosign >/dev/null 2>&1; then
            download "$SUMS_URL.bundle" "$TMP/SHA256SUMS.bundle" || \
                die "SHA256SUMS is signed but its sigstore bundle could not be downloaded"
            cosign verify-blob --bundle "$TMP/SHA256SUMS.bundle" \
                --certificate-identity "$COSIGN_IDENTITY" \
                --certificate-oidc-issuer "${COSIGN_ISSUER:-https://token.actions.githubusercontent.com}" \
                "$TMP/SHA256SUMS" >/dev/null
        fi
        if [ -n "${MINISIGN_PUBKEY:-}" ] && command -v minisign >/dev/null 2>&1; then
            download "$SUMS_URL.minisig" "$TMP/SHA256SUMS.minisig" || \
                die "SHA256SUMS is signed but its signature could not be downloaded"
            minisign -Vm "$TMP/SHA256SUMS" -P "$MINISIGN_PUBKEY" >/dev/null
        fi
        if grep -F " $ASSET" "$TMP/SHA256SUMS" >/dev/null 2>&1; then
            (cd "$TMP" && grep -F " $ASSET" SHA256SUMS | sha256sum -c -)
        else
            die "SHA256SUMS has no entry for $ASSET"
        fi
    else
        die "SHA256SUMS not available; refusing to install an unverified download"
    fi

    tar -xzf "$TMP/$ASSET" -C "$TMP"
    SRC=$(find "$TMP" -mindepth 1 -maxdepth 1 -type d -name 'power-switcher-*' | head -n 1)
    [ -n "$SRC" ] || die "release tarball has no power-switcher directory"
fi

[ -x "$SRC/bin/power-switcher" ] || die "tarball is missing bin/power-switcher"
[ -x "$SRC/bin/power-switcher-gui" ] || die "tarball is missing bin/power-switcher-gui"

install -d "$BIN_DIR" "$SHARE_DIR" "$DESKTOP_DIR" "$ICON_DIR" "$UNIT_DIR"
install -m 755 "$SRC/bin/power-switcher" "$BIN_DIR/power-switcher"
install -m 755 "$SRC/bin/power-switcher-gui" "$BIN_DIR/power-switcher-gui"

if [ -d "$SRC/share/power_switcher" ]; then
    cp -R "$SRC/share/power_switcher/." "$SHARE_DIR/"
fi
if [ -f "$SRC/share/applications/power-switcher.desktop" ]; then
    sed "s|^Exec=.*|Exec=$BIN_DIR/power-switcher-gui|" \
        "$SRC/share/applications/power-switcher.desktop" > "$DESKTOP_DIR/power-switcher.desktop"
fi
if [ -f "$SRC/share/icons/hicolor/scalable/apps/power-switcher.svg" ]; then
    install -m 644 "$SRC/share/icons/hicolor/scalable/apps/power-switcher.svg" \
        "$ICON_DIR/power-switcher.svg"
fi
if [ -f "$SRC/lib/systemd/user/$UNIT_NAME" ]; then
    UNIT_EXEC=$BIN_DIR/power-switcher
    case "$PREFIX" in
        "$HOME/.local") UNIT_EXEC=%h/.local/bin/power-switcher ;;
    esac
    sed "s|/usr/bin/power-switcher|$UNIT_EXEC|g" \
        "$SRC/lib/systemd/user/$UNIT_NAME" > "$UNIT_DIR/$UNIT_NAME"
fi

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$DESKTOP_DIR" >/dev/null 2>&1 || true
fi

if [ "$NO_GNOME" -eq 0 ] && [ -d "$SRC/share/gnome-shell/extensions/power-switcher-focus@powerswitcher" ]; then
    INSTALL_GNOME=0
    if command -v gnome-shell >/dev/null 2>&1; then
        INSTALL_GNOME=1
    fi
    case "${XDG_CURRENT_DESKTOP:-}" in
        *GNOME*|*gnome*) INSTALL_GNOME=1 ;;
    esac
    if [ "$INSTALL_GNOME" -eq 1 ]; then
        install -d "$GNOME_DIR"
        cp -R "$SRC/share/gnome-shell/extensions/power-switcher-focus@powerswitcher" "$GNOME_DIR/"
        if command -v gnome-extensions >/dev/null 2>&1; then
            gnome-extensions enable power-switcher-focus@powerswitcher >/dev/null 2>&1 || true
        fi
        printf 'GNOME helper installed. Log out and back in on Wayland to load it.\n'
    fi
fi

if [ "$NO_UDEV" -eq 0 ]; then
    run_root rm -f /etc/udev/rules.d/99-power_switcher.rules >/dev/null 2>&1 || true
    if run_root install -Dm644 "$SRC/lib/udev/rules.d/60-power_switcher.rules" \
        /etc/udev/rules.d/60-power_switcher.rules; then
        run_root install -Dm644 "$SRC/lib/modules-load.d/power_switcher.conf" \
            /etc/modules-load.d/power_switcher.conf || true
        run_root modprobe uinput >/dev/null 2>&1 || true
        run_root udevadm control --reload-rules >/dev/null 2>&1 || true
        run_root udevadm trigger --subsystem-match=input --action=change >/dev/null 2>&1 || true
        run_root udevadm trigger --name-match=uinput >/dev/null 2>&1 || true
    else
        warn "could not install udev rules; run with sudo or rerun with --no-udev"
    fi
fi

if [ "$NO_GROUP" -eq 0 ] && ! id -nG "$USER_NAME" | tr ' ' '\n' | grep -qx input; then
    if run_root usermod -aG input "$USER_NAME"; then
        NEED_RELOGIN=1
    else
        warn "could not add $USER_NAME to input; udev uaccess may still work"
    fi
fi

"$BIN_DIR/power-switcher" setup >/dev/null || warn "could not create the default config"

if [ "$NO_SERVICE" -eq 0 ] && command -v systemctl >/dev/null 2>&1; then
    systemctl --user daemon-reload >/dev/null 2>&1 || true
    systemctl --user enable "$UNIT_NAME" >/dev/null 2>&1 || \
        warn "could not enable $UNIT_NAME"
    if [ "$NEED_RELOGIN" -eq 0 ]; then
        systemctl --user restart "$UNIT_NAME" >/dev/null 2>&1 || \
            warn "could not start $UNIT_NAME (run: systemctl --user status $UNIT_NAME)"
    fi
fi

printf '\nInstalled PowerSwitcher to %s\n' "$BIN_DIR"
case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) printf 'Add %s to PATH to run `power-switcher` from a terminal.\n' "$BIN_DIR" ;;
esac
printf 'Open settings: %s/power-switcher gui\n' "$BIN_DIR"
if [ "$NEED_RELOGIN" -eq 1 ]; then
    printf 'Log out and back in so the input-group change takes effect, then run:\n'
    printf '  systemctl --user start %s\n' "$UNIT_NAME"
    printf 'Until then the daemon may not be able to open your keyboard.\n'
fi
if [ "$NO_GNOME" -eq 0 ] && [ -d "$GNOME_DIR/power-switcher-focus@powerswitcher" ]; then
    printf 'On GNOME Wayland, log out and back in once to load the helper extension.\n'
fi
