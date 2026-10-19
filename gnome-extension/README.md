# PowerSwitcher helper — GNOME Shell extension

This extension runs inside GNOME Shell and exposes two things the daemon
cannot obtain on its own on Wayland:

1. **the focused window's application id** (per-app exceptions) —
   `org.power_switcher.Focus.GetActiveWindow`;
2. **the keyboard input sources** (layout list, current index, switching) —
   `org.power_switcher.Layout.*`.

Wayland gives clients no way to ask *"which window has focus?"*, and GNOME
45+ blocks the legacy `org.gnome.Shell.Eval` and
`org.gnome.Shell.Introspect.GetWindows` APIs.  For layouts it is worse:
`org.gnome.desktop.input-sources current` is deprecated and **stale** on
Wayland, injected `Super+Space` does not trigger the shell's built-in
keybinding, and `gsettings set … current N` is ignored.  An in-shell
extension is the only supported way to switch the input source
programmatically.

## Layout interface

```
org.power_switcher.Layout.GetCurrentIndex        → (uint32 0,)
org.power_switcher.Layout.GetInputSources        → (['us', 'ru'],)
org.power_switcher.Layout.ActivateInputSource(u) → (true,)
```

The Rust daemon picks whichever mechanism is available, in this order:

1. `g3kb-switch` (preinstalled on Fedora; needs its own helper extension),
2. this extension,
3. simulated `Super+Space` — a last resort that GNOME 45+ ignores.

## Install

```bash
EXT=power-switcher-focus@powerswitcher
mkdir -p ~/.local/share/gnome-shell/extensions/
cp -r "gnome-extension/$EXT" ~/.local/share/gnome-shell/extensions/

# On Wayland a newly installed extension is only loaded at the next login:
gnome-extensions enable "$EXT"
# then log out and back in
```

Verify both interfaces:

```bash
gdbus call --session --dest org.power_switcher.Focus \
  --object-path /org/power_switcher/Focus \
  --method org.power_switcher.Focus.GetActiveWindow

gdbus call --session --dest org.power_switcher.Focus \
  --object-path /org/power_switcher/Layout \
  --method org.power_switcher.Layout.GetCurrentIndex
```

If the focus call prints a window class, per-app exceptions work.  Without
the extension PowerSwitcher still runs, but exceptions are inert and layout
switching needs `g3kb-switch`.

## Fedora shortcut: g3kb-switch

Fedora ships `g3kb-switch`, which does the same job through its own
extension.  If it is installed, the daemon prefers it and this extension
stays optional:

```bash
g3kb-switch -p            # print the active xkb layout
g3kb-switch -s ru         # switch to it
```

The packaged extension advertises GNOME ≤49; on newer shells it must be
allowed to load:

```bash
gsettings set org.gnome.shell disable-extension-version-validation true
gdbus call --session --dest org.gnome.Shell --object-path /org/gnome/Shell \
  --method org.gnome.Shell.Extensions.EnableExtension g3kb-switch@g3kb-switch.org
```

## Supported compositors

| Compositor | Needs this extension? |
|------------|-----------------------|
| GNOME      | for per-app exceptions; for layout switching only if `g3kb-switch` is absent |
| KDE Plasma | no — uses `kdotool` / KWin D-Bus |
| Sway       | no — uses `swaymsg` |
| Hyprland   | no — uses `hyprctl` |
| X11        | no — uses `xdotool` |
