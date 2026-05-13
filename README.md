# PowerSwitcher

A DE-independent keyboard layout auto-switcher for Linux Wayland.

Like PuntoSwitcher / CarambaSwitcher, but for Linux Wayland.

## How it works

1. **Grabs the physical keyboard** at the kernel level via `evdev` — no compositor involvement.
2. **Passes all keystrokes through** a virtual `UInput` keyboard to the compositor.
3. **Tracks a rolling word buffer** of characters the user is typing.
4. **On word boundary** (Space / Enter / Tab / punctuation): runs an n-gram language detector on the buffered word; if the word scores higher in another layout → auto-converts it.
5. **Direct Injection**: Erases the word via Backspaces and re-types it in the correct layout directly through the virtual keyboard. No external tools like `ydotool` needed!
6. **Compositor Native**: Switches system layout via D-Bus (GNOME, KDE) or compositor commands (Sway, Hyprland).

## Features

- **DE Independent**: Works on GNOME, KDE Plasma, Sway, Hyprland, and more.
- **Multi-Layout Support**: Supports 2, 3, or more layouts (e.g., English, Russian, and Ukrainian).
- **Fast & Reliable**: Uses direct `uinput` injection for near-instant text conversion.
- **Zero Configuration for Core**: Auto-detects your desktop environment and keyboards.

## Requirements

- Linux Wayland session
- Python 3.11+
- `python-evdev`, `dasbus`, `click` (Python packages)
- User in `input` group

## Quick start

```bash
# 1. Install dependencies
pip install .

# 2. Setup permissions and config
python -m power_switcher setup

# 3. Build n-gram profiles (optional, built-in fallbacks provided)
python -m power_switcher.detection.build_profiles

# 4. Run daemon
python -m power_switcher daemon --verbose
```

## Hotkeys

| Hotkey | Action |
|--------|--------|
| `Alt+Pause` | Convert current word + switch layout |
| `Alt+Pause` (within 3s) | Undo last conversion |
| `Ctrl+Alt+Z` | Convert selected text + switch layout |

## Configuration

Config file: `~/.config/power_switcher/config.toml`

```toml
[general]
layouts          = ["en", "ru"]    # Supports 2+ layouts
hotkey_convert   = "alt+pause"
hotkey_selection = "ctrl+alt+z"
default_layout   = "en"
auto_detect      = true

[detection]
threshold     = 2.5
min_word_len  = 3
```

## Supported Layouts

Built-in layouts: `en`, `ru`, `ua`, `de`.

Adding a new layout:
1. Add a keycode→char table in `power_switcher/maps/keycode_map.py`.
2. Register it in the `LAYOUTS` dict.
3. Run `python -m power_switcher.detection.build_profiles`.

## Architecture

```
Physical Keyboard → evdev grab → KeyProcessor → UInput passthrough → Compositor
                                  │
                                  ├─ KeyBuffer (word tracking)
                                  ├─ ModifierState (shift/ctrl/alt)
                                  ├─ LayoutSet (multi-layout maps + n-gram profiles)
                                  ├─ LayoutBackend (D-Bus / IPC layout switch)
                                  └─ UInput (direct text injection)
```

## Supported DEs/Compositors

| DE/Compositor | Backend | Notes |
|---------------|---------|-------|
| **GNOME** | `GNOMESwitchBackend` | Uses UInput key simulation for layout switching. |
| **KDE Plasma** | `KDELayoutBackend` | Uses D-Bus `org.kde.keyboard`. |
| **Sway** | `SwayLayoutBackend` | Uses `swaymsg`. |
| **Hyprland** | `HyprlandLayoutBackend` | Uses `hyprctl`. |

## License

MIT
