# PowerSwitcher Focus — GNOME Shell extension

Wayland gives applications no way to ask *"which window has focus?"*. GNOME
Shell 45+ additionally blocks the legacy `org.gnome.Shell.Eval` and the
`org.gnome.Shell.Introspect.GetWindows` D-Bus APIs for third-party callers, so
PowerSwitcher cannot read the focused application on its own.

This extension runs inside GNOME Shell and re-exports the answer on the session
bus, which PowerSwitcher's per-application exceptions need:

```
org.power_switcher.Focus.GetActiveWindow → ('org.gnome.Terminal',)
```

## Install

```bash
EXT=power-switcher-focus@powerswitcher
mkdir -p ~/.local/share/gnome-shell/extensions/
cp -r "gnome-extension/$EXT" ~/.local/share/gnome-shell/extensions/

# Enable it (log out and back in on Wayland, or press Alt+F2, type 'r', Enter
# if you are on X11):
gnome-extensions enable "$EXT"
```

Verify it is answering:

```bash
gdbus call --session \
  --dest org.power_switcher.Focus \
  --object-path /org/power_switcher/Focus \
  --method org.power_switcher.Focus.GetActiveWindow
```

If that prints a window class, per-app exceptions work. Without the extension
PowerSwitcher still runs — it simply cannot tell which app is focused, so
exceptions are inert and auto-detection stays enabled everywhere.

## Supported compositors

| Compositor | Needs this extension? |
|------------|-----------------------|
| GNOME      | yes (this extension) |
| KDE Plasma | no — uses `kdotool` / KWin D-Bus |
| Sway       | no — uses `swaymsg` |
| Hyprland   | no — uses `hyprctl` |
| X11        | no — uses `xdotool` |
