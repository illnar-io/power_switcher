# AGENTS.md

## Project Overview

PowerSwitcher is a Linux Wayland daemon that auto-detects when you type in the wrong keyboard layout and converts the text. It works like PuntoSwitcher/CarambaSwitcher but for Wayland. It grabs physical keyboards at the kernel level via `evdev`, passes keystrokes through a virtual `UInput` device, and monitors a word buffer — on word boundaries it runs n-gram language detection; if the word scores higher in the other layout, it auto-converts by sending Backspace keystrokes + corrected text via `ydotool`.

## Commands

```bash
# Run tests
python -m pytest tests/ -v

# Run daemon (requires user in input group + Wayland session)
python -m power_switcher daemon --verbose

# Run daemon with auto-detect disabled (hotkey-only mode)
python -m power_switcher daemon --no-auto

# Debug mode: list keyboards and print keycodes without grabbing
python -m power_switcher debug

# List available layout definitions
python -m power_switcher layouts

# Build/update n-gram profiles from corpus files in data/
python -m power_switcher.detection.build_profiles

# Install as systemd user service
mkdir -p ~/.config/systemd/user
cp systemd/power_switcher.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now power_switcher
```

## Architecture & Data Flow

```
Physical Keyboard(s)
  → evdev grab (KeyProcessor._find_keyboards → InputDevice.grab)
  → KeyProcessor._on_event()
      ├─ Non-EV_KEY / key-repeat (value=2): pass through UInput unchanged
      ├─ Modifier keys: update ModifierState, pass through UInput
      ├─ Hotkey combo match: _do_manual_convert() or _do_selection_convert()
      └─ Regular key-down: pass through UInput, then _update_buf()
            ├─ Backspace → buf.backspace()
            ├─ Buffer reset keys (arrows, Esc, Enter) → buf.clear()
            ├─ Word separators (Space, Tab, punctuation) → buf.flush() → _maybe_auto()
            ├─ Ctrl/Alt held → buf.clear() (shortcut, not typing)
            └─ Plain typing → buf.add(keycode, char) via LayoutPair.keycode_to_char()

  _maybe_auto(word) → LayoutPair.should_convert() → n-gram scoring
      If wrong layout detected → _convert():
          1. Set self._injecting = True (prevents re-entry into buffer)
          2. Erase: N × Backspace via UInput (4ms delay each)
          3. Type converted text via ydotool
          4. Switch layout via LayoutBackend.switch_next()
          5. Set self._injecting = False

  _do_manual_convert(): same as auto, but on hotkey; also supports undo
      (press same hotkey within 3s to revert)

  _do_selection_convert(): Ctrl+C via UInput → wl-paste → convert → ydotool → restore clipboard
```

## Key Design Decisions & Gotchas

- **Self-injection loop prevention**: `_injecting` flag is critical. When `_convert()` injects Backspace keys and ydotool text, those events would re-enter `_on_event()`. The `if pressed and not self._injecting` guard in `_on_event()` prevents buffer updates during injection. Always set `_injecting = True` before injection and `False` in a `finally` block.

- **Text injection uses ydotool, not UInput keycodes**: After layout switch, the compositor interprets keycodes in the new layout. `ydotool type` sends Unicode characters independently of the current keymap, which is correct for mixed-case and Unicode text. UInput keycodes are used only for Backspace (same keycode in both layouts) and modifier combos.

- **UInput must be created with `UInput.from_device(*keyboards)`**: Default `UInput()` has minimal capabilities and would swallow media keys, function keys, etc. `from_device()` inherits all capabilities from the physical keyboards.

- **evdev keycode vs xkb keycode offset**: `xkb_keycode = evdev_keycode + 8`. Matters if ever using `libxkbcommon` directly.

- **GNOMESwitchBackend layout switching**: The primary GNOME backend (for GNOME 50+) does NOT use g3kb-switch or D-Bus Eval. It switches layouts by emitting the GNOME keybinding (Super+Space) via the same UInput device. The `set_uinput()` method must be called after UInput creation — `KeyProcessor.run()` handles this automatically. The backend tracks current index internally, which can diverge if the user switches layout via GNOME's own shortcut.

- **LayoutPair is the central abstraction**: It bundles char maps (bidirectional conversion dicts), n-gram profiles, and scoring logic for a specific layout pair (e.g., en↔ru). `WrongLayoutDetector` is a thin wrapper that delegates to `LayoutPair.should_convert()`.

- **Config `default_layout` is derived from `layout_pair[0]`**: In `Config.load()`, `default_layout` falls back to `layout_pair[0]`, not the hardcoded "en". The config file's `layout_pair` must have exactly 2 entries.

- **Legacy `EN_TO_RU`/`RU_TO_EN` aliases**: These module-level dicts in `keycode_map.py` are built at import time via `build_char_map("en", "ru")`. They exist for backward compatibility; new code should use `LayoutPair.convert()` or `build_char_map()`.

- **N-gram profiles**: Stored as JSON in `power_switcher/detection/profiles/`. If a profile file is missing, hardcoded fallback bigrams (in `layout_pair.py`) are used. To regenerate from corpora: `python -m power_switcher.detection.build_profiles` (reads `data/<lang>_corpus.txt`).

- **ydotool requires `ydotoold` daemon**: The socket must exist at `/run/user/$UID/.ydotool_socket`. Start with `systemctl --user start ydotool`.

- **User must be in `input` group**: Required for evdev grab and UInput creation. Re-login needed after `usermod -aG input $USER`.

## Code Organization

```
power_switcher/
├── __init__.py              # version
├── __main__.py              # CLI entry (click): daemon, debug, layouts commands
├── config.py                # TOML config loader (~/.config/power_switcher/config.toml)
├── core/
│   ├── key_buffer.py        # KeyBuffer dataclass + key category frozensets (WORD_SEPARATORS, etc.)
│   ├── key_processor.py    # KeyProcessor: main event loop, conversion logic, hotkey handling
│   └── modifier_state.py   # ModifierState dataclass (shift/ctrl/alt/meta tracking)
├── maps/
│   ├── char_maps.py         # Re-exports from keycode_map (legacy __all__)
│   ├── keycode_map.py       # Per-layout keycode→char tables (EN, RU, UA, DE), LAYOUTS dict,
│   │                        # build_char_map(), keycode_to_char()
│   └── layout_pair.py       # LayoutPair dataclass: char maps + n-gram scoring + conversion;
│                            # BIGRAMS_FALLBACK dicts for en/ru/ua/de
├── detection/
│   ├── ngram_detector.py    # NGramDetector: bigram log-prob scoring
│   ├── wrong_layout.py      # WrongLayoutDetector: thin wrapper delegating to LayoutPair
│   ├── build_profiles.py    # CLI: generates <lang>_bigrams.json from corpus files
│   └── profiles/            # Generated JSON bigram frequency files
├── layout/
│   ├── backends.py           # LayoutBackend ABC + KDELayoutBackend + GNOMESwitchBackend
│   │                         # + GNOMEDBusLayoutBackend + GNOMELayoutBackend (g3kb-switch)
│   └── detector.py           # detect_layout_backend(): auto-detect DE, return best backend
└── injection/
    └── text_injector.py      # inject_text() via ydotool
```

## Layout System

Adding a new keyboard layout requires:

1. Add a keycode→char table in `power_switcher/maps/keycode_map.py` (dict of `evdev keycode → (lower, upper)`)
2. Register it in the `LAYOUTS` dict
3. Add a bigram fallback in `power_switcher/maps/layout_pair.py` (`BIGRAMS_FALLBACK`)
4. (Optional) Provide a corpus in `data/<lang>_corpus.txt` and run `python -m power_switcher.detection.build_profiles`

Current layouts: `en` (US QWERTY), `ru` (ЙЦУКЕН), `ua` (Ukrainian ЙЦУКЕН-УК), `de` (German QWERTZ).

Any two layouts can be paired via config: `layout_pair = ["en", "de"]`. The `LayoutPair` class auto-builds bidirectional char maps and loads n-gram profiles.

## GNOME Backend Hierarchy

The detector tries GNOME backends in priority order:

1. **`GNOMESwitchBackend`** (primary, GNOME 50+): Reads layout count/current from `gsettings`, switches by emitting Super+Space via UInput. No extension needed.
2. **`GNOMEDBusLayoutBackend`** (fallback, GNOME 40–44): Uses `org.gnome.Shell.Eval` D-Bus method. Broken on GNOME 45+ (security restriction).
3. **`GNOMELayoutBackend`** (fallback): Uses `g3kb-switch` CLI + GNOME extension. Requires manual extension install.

For KDE: `KDELayoutBackend` uses `org.kde.keyboard` D-Bus (no extension needed).

## Testing

- **Runner**: `pytest` (configured in `pyproject.toml`)
- **32 tests** across 3 files: `test_char_maps.py` (char map correctness, all 4 layouts), `test_detector.py` (LayoutPair conversion, n-gram scoring, should_convert logic), `test_key_buffer.py` (buffer add/backspace/flush/clear/max_len)
- Tests are pure unit tests (no hardware/evdev required). They import from `power_switcher.maps` and `power_switcher.core.key_buffer`.
- **No tests exist yet for**: `KeyProcessor`, `ModifierState`, layout backends, `text_injector`, config loading, CLI commands.
