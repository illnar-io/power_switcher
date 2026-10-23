# Packaging

PowerSwitcher targets **modern Linux releases only** (currently Fedora 44+,
Ubuntu 24.04+, Debian 13+, current Arch/openSUSE).  The settings GUI needs
GTK 4.10+; there is no LTS/old-release support.

## What gets installed

| Path | Purpose |
|---|---|
| `/usr/bin/power-switcher` | daemon CLI (`daemon`, `setup`, `debug`, `gui`) |
| `/usr/bin/power-switcher-gui` | GTK4 settings window |
| `/usr/share/power_switcher/` | n-gram profiles and `layouts.json` |
| `/usr/lib/udev/rules.d/60-power_switcher.rules` | `input`/`uinput` access |
| `/usr/lib/modules-load.d/power_switcher.conf` | load `uinput` at boot |
| `/usr/lib/systemd/user/power_switcher.service` | per-user daemon unit |
| `/usr/share/applications/power-switcher.desktop` | application launcher |
| `/usr/share/gnome-shell/extensions/power-switcher-focus@powerswitcher/` | GNOME helper |

The installed daemon is called `power-switcher`, not `ps`, because `/usr/bin/ps`
already belongs to procps.  Cargo build artifacts keep the short `ps` /
`ps-gui` names.

## Release artifacts

A release contains, per architecture:

- `power-switcher-<version>-<arch>.tar.gz` — portable tree + `install.sh`
- `power-switcher-<arch>.tar.gz` — same tree for `/releases/latest`
- `power-switcher_<version>_<arch>.deb`
- `power-switcher-<version>.<arch>.rpm`
- `power-switcher-<version>-1-<arch>.pkg.tar.zst`
- `PowerSwitcher-<version>-<arch>.AppImage`
- `SHA256SUMS`, `install.sh`

### Native packages

```bash
# Fedora / openSUSE
sudo dnf install ./power-switcher-*.rpm

# Debian / Ubuntu
sudo apt install ./power-switcher_*.deb

# Arch
sudo pacman -U power-switcher-*.pkg.tar.zst
```

Then log out and back in if the daemon cannot open the keyboard (the package
does not silently modify your groups), and start it:

```bash
systemctl --user enable --now power_switcher.service
power-switcher gui
```

### Universal installer

```bash
curl -fsSL https://github.com/illnar-io/power_switcher/releases/latest/download/install.sh | sh
```

It installs under `~/.local` by default, installs the udev rule and module
file with sudo/pkexec, installs the systemd user unit, and installs the GNOME
helper when GNOME is detected. It relies on udev `uaccess` and does not modify
group membership by default.

Useful flags:

```bash
sh install.sh --prefix ~/.local --no-service
sh install.sh --no-udev
sh install.sh --add-group      # only if udev uaccess is unavailable
sh install.sh --uninstall
```

The installer verifies the release tarball against `SHA256SUMS`.  Release
checksums are signed keylessly with sigstore/cosign in CI
(`SHA256SUMS.bundle`).  Set `COSIGN_IDENTITY` to also verify that signature
when `cosign` is installed:

```bash
COSIGN_IDENTITY=https://github.com/illnar-io/power_switcher/.github/workflows/release.yml@refs/tags/v0.1.0 \
  sh install.sh --release 0.1.0
```

`MINISIGN_PUBKEY` is supported as an alternative if a minisign key is used.

### AppImage

The AppImage uses the **host** GTK 4.10+ runtime, keeping the file small and
consistent with the distro theme.  It is a portable/try-it option: the daemon
still needs the udev `uaccess` rule.  Run the installer once for that, or use
the native package.

```bash
chmod +x PowerSwitcher-*.AppImage
./PowerSwitcher-*.AppImage
```

If FUSE is unavailable, use `APPIMAGE_EXTRACT_AND_RUN=1 ./PowerSwitcher-*.AppImage`.

## Building locally

```bash
# tarball; native packages if nfpm is installed
VERSION=0.1.0 ./packaging/build-release.sh

# AppImage too
APPIMAGETOOL=/path/to/appimagetool \
APPIMAGE_EXTRACT_AND_RUN=1 \
VERSION=0.1.0 ./packaging/build-release.sh
```

Build on the oldest supported GTK baseline (CI uses Ubuntu 24.04 with GTK
4.14).  The daemon itself only needs glibc.

`nfpm` is a single binary: <https://github.com/goreleaser/nfpm/releases>.

## Release workflow

`.github/workflows/release.yml` runs on `v*` tags and builds x86_64 and
aarch64 packages in parallel.  It uploads the tarball, native packages,
AppImage, installer and `SHA256SUMS` to the GitHub release.

## GNOME extension

The helper extension is installed system-wide by packages and user-locally by
the installer.  On GNOME Wayland a newly installed extension is loaded only at
the next login:

```bash
gnome-extensions enable power-switcher-focus@powerswitcher
# then log out and back in
```

It is not needed on KDE, Sway or Hyprland.
