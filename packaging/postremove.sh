#!/bin/sh
set -eu

if command -v udevadm >/dev/null 2>&1; then
    rm -f /etc/udev/rules.d/60-power_switcher.rules \
        /etc/udev/rules.d/99-power_switcher.rules
    udevadm control --reload-rules >/dev/null 2>&1 || true
fi
