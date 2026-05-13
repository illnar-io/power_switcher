# PowerSwitcher

A DE-independent keyboard layout auto-switcher for Linux Wayland.

Like PuntoSwitcher / CarambaSwitcher, but for Linux Wayland.

## How it works

1. **Grabs the physical keyboard** at the kernel level via `evdev` — no compositor involvement
2. **Passes all keystrokes through** a virtual `UInput` keyboard to the compositor as-is (transparent by default)
3. **Tracks a rolling word buffer** of characters the user is typing
4. **On word boundary** (Space / Enter / Tab / punctuation): runs an n-gram language detector on the buffered word; if the word scores higher when converted to the other layout → auto-converts it
5. **On hotkey press** (e.g. `Alt+Pause`): takes the current word buffer, converts it to the other layout, injects `Backspace × N` + converted text via ydotool, switches layout via DE D-Bus
6. Works on **KDE, GNOME, COSMIC, any Wayland compositor** — no extensions required for core functionality

## Requirements

- Linux Wayland session
- Python 3.11+
- `python-evdev`, `dasbus`, `click` (Python packages)
- `ydotool` + `ydotoold` (for text injection)
- `wl-clipboard` (for selection convert)
- `g3kb-switch` + GNOME extension (for GNOME layout switching)
- User in `input` group

## Quick start

```bash
# System setup (one-time)
sudo dnf install -y python3-evdev python3-dasbus wl-clipboard ydotool g3kb-switch
sudo usermod -aG input $USER
# Re-login after adding to input group

# Build n-gram profiles
python -m power_switcher.detection.build_profiles

# Run daemon
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
hotkey_convert   = "alt+pause"
hotkey_selection = "ctrl+alt+z"
layout_pair      = ["en", "ru"]    # any pair from: en, ru, ua, de
default_layout   = "en"
auto_detect      = true

[detection]
threshold     = 2.5
min_word_len  = 3
```

## Supported layout pairs

The `layout_pair` config option accepts any two layout names. Built-in layouts:

| Code | Layout | Key positions |
|------|--------|---------------|
| `en` | English (US QWERTY) | Standard |
| `ru` | Russian (ЙЦУКЕН) | Standard |
| `ua` | Ukrainian (ЙЦУКЕН-УК) | `і` on S, `є` on ', `ї` on ], `ґ` on ` |
| `de` | German (QWERTZ) | Z/Y swapped, umlauts on ;/'/[

Examples:

```toml
layout_pair = ["en", "ru"]   # English ↔ Russian (default)
layout_pair = ["en", "ua"]   # English ↔ Ukrainian
layout_pair = ["en", "de"]   # English ↔ German
layout_pair = ["ru", "ua"]   # Russian ↔ Ukrainian
```

Adding a new layout requires:
1. Add a keycode→char table in `power_switcher/maps/keycode_map.py`
2. Register it in the `LAYOUTS` dict
3. Add a bigram fallback in `power_switcher/maps/layout_pair.py` (or provide a corpus in `data/<lang>_corpus.txt`)
4. Run `python -m power_switcher.detection.build_profiles` to generate the n-gram profile

## Systemd user service

```bash
mkdir -p ~/.config/systemd/user
cp systemd/power_switcher.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now power_switcher
```

## Architecture

```
Physical Keyboard → evdev grab → KeyProcessor → UInput passthrough → Compositor
                                  │
                                  ├─ KeyBuffer (word tracking)
                                  ├─ ModifierState (shift/ctrl/alt)
                                  ├─ LayoutPair (char maps + n-gram profiles)
                                  ├─ WrongLayoutDetector (delegates to LayoutPair)
                                  ├─ LayoutBackend (D-Bus layout switch)
                                  └─ ydotool (text injection)
```

## Supported DEs

| DE | Backend | Notes |
|----|---------|-------|
| GNOME | `g3kb-switch` | Primary target (Fedora 44 / GNOME 50.1) |
| KDE Plasma | D-Bus `org.kde.keyboard` | Secondary target |

## License

MIT
