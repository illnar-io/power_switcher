# AGENTS.md

## Project Overview

PowerSwitcher is a Linux Wayland daemon that auto-detects when you type in the wrong keyboard layout and converts the text. It works at the kernel level via `evdev`, grabbing physical keyboards, passing keystrokes through a virtual `UInput` device, and monitoring a word buffer. On word boundaries, it runs n-gram language detection. If a wrong layout is detected, it auto-converts by sending Backspace keystrokes and re-typing the corrected text directly through the `UInput` device.

## Commands

```bash
# Run tests
PYTHONPATH=. pytest

# Run setup (generate config and show udev rules)
python -m power_switcher setup

# Run daemon
python -m power_switcher daemon --verbose

# Debug mode: list keyboards and print keycodes without grabbing
python -m power_switcher debug

# List available layout definitions
python -m power_switcher layouts

# Build/update n-gram profiles
python -m power_switcher.detection.build_profiles
```

## Architecture & Data Flow

```
Physical Keyboard(s)
  → evdev grab
  → KeyProcessor._on_event()
      ├─ Hotkey combo match: _do_manual_convert() or _do_selection_convert()
      └─ Regular key-down: pass through UInput, then _update_buf()
            ├─ Word separators → buf.flush() → _maybe_auto()
            └─ Plain typing → buf.add(keycode, char)

  _maybe_auto(word) → LayoutSet.find_best_layout() → n-gram scoring across all layouts
      If better layout detected → _convert(target_layout):
          1. Erase: N × Backspace via UInput
          2. Switch: layout_backend.set_layout()
          3. Type: _inject_text() re-types chars using target layout keycodes
```

## Key Design Decisions

- **Direct Injection**: We no longer use `ydotool`. We find the (keycode, shift) pair for each character in the target layout using `LayoutSet.char_to_keycode()` and emit them via our own `UInput` device.
- **Multi-Layout**: The system uses `LayoutSet` instead of `LayoutPair`. It supports any number of layouts configured in `config.toml`.
- **Compositor Backends**: Supports GNOME (D-Bus/UInput), KDE (D-Bus), Sway (IPC), and Hyprland (IPC).
- **Device Filtering**: Only actual keyboards are grabbed. Mice and virtual bus devices are ignored based on their capabilities and names.

## Code Organization

```
power_switcher/
├── config.py                # TOML config loader
├── core/
│   ├── key_buffer.py        # Word tracking buffer
│   ├── key_processor.py    # Main event loop and conversion logic
│   └── modifier_state.py   # Shift/Ctrl/Alt tracking
├── maps/
│   ├── keycode_map.py       # Keycode tables for EN, RU, UA, DE
│   └── layout_set.py        # LayoutSet: Multi-layout scoring and conversion
├── detection/
│   ├── ngram_detector.py    # N-gram scoring
│   ├── build_profiles.py    # Profile generator
│   └── profiles/            # JSON profiles
└── layout/
    ├── backends.py           # Compositor-specific layout switchers
    └── detector.py           # Auto-detector for backends
```
