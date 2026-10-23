#!/bin/sh
set -eu

if command -v udevadm >/dev/null 2>&1; then
    # Remove the pre-rename rule from the tarball installer, if present.
    rm -f /etc/udev/rules.d/99-power_switcher.rules
    udevadm control --reload-rules >/dev/null 2>&1 || true
    udevadm trigger --subsystem-match=input --action=change >/dev/null 2>&1 || true
    udevadm trigger --name-match=uinput >/dev/null 2>&1 || true
fi
if command -v modprobe >/dev/null 2>&1; then
    modprobe uinput >/dev/null 2>&1 || true
fi

cat <<'EOF'
PowerSwitcher installed.

Most desktops grant the active user access through udev uaccess. If the
daemon cannot open your keyboard, first log out and back in, then check:

    power-switcher setup

Only if uaccess is unavailable, opt into the broader input group and log
out and back in:

    sudo usermod -aG input <your-user>

Start the daemon at login with:

    systemctl --user enable --now power_switcher.service

Open the settings window with:

    power-switcher gui
EOF
