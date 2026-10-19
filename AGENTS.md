# AGENTS.md

## Project Overview

PowerSwitcher is a Linux Wayland daemon that auto-detects when you type in the wrong keyboard layout and converts the text. It works at the kernel level via `evdev`, grabbing physical keyboards, passing keystrokes through a virtual `UInput` device, and monitoring a word buffer. On word boundaries, it runs n-gram language detection. If a wrong layout is detected, it auto-converts by sending Backspace keystrokes and re-typing the corrected text directly through the `UInput` device.

Like Punto Switcher / Caramba Switcher / Keyboop, but for Linux Wayland.

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

# Build/update n-gram profiles
python -m power_switcher.detection.build_profiles

# Manage snippets
python -m power_switcher snippets list
python -m power_switcher snippets add '!ee' 'user@example.com'
python -m power_switcher snippets remove '!ee'

# Manage application exceptions
python -m power_switcher exceptions list
python -m power_switcher exceptions add 'code' --mode off
python -m power_switcher exceptions remove 'code'

# Manage word exceptions / learned words
python -m power_switcher words list
python -m power_switcher words add 'макбук'
python -m power_switcher words remove 'макбук'

# Statistics
python -m power_switcher stats show
python -m power_switcher stats reset

# Settings GUI (tkinter)
python -m power_switcher gui
```

## Architecture & Data Flow

```
Physical Keyboard(s)
  → evdev grab
  → KeyProcessor._on_event()
      ├─ L+R Shift held together: _toggle_auto() (pause/resume auto-detect)
      ├─ Double-tap Shift: _do_manual_convert() (sentence convert + undo + learn)
      ├─ Single Shift: legacy gesture, off by default (`shift_tap_switch_layout`); desktop owns Super+Space
      └─ Regular key-down: pass through UInput, then _update_buf()
            ├─ Word separators:
            │   ├─ Snippet check: SnippetStore.expansion() → _expand_snippet()
            │   └─ Auto-detect: _maybe_auto() → LayoutSet.find_best_layout()
            ├─ Mid-word eager conversion (threshold=15.0, confidence very high)
            └─ Plain typing → buf.add(keycode, char)

  _maybe_auto(word):
      ├─ _refresh_front_app() — ActiveWindowProvider (cached) → ExceptionStore
      ├─ Skips when: auto paused, app mode "off", dev app, or within
      │  no_correct_after_switch seconds of the last layout change
      ├─ Double-caps fix (DOUBLE → Double)
      ├─ AntiResonanceGuard.allow() — prevent oscillation loops
      └─ LayoutSet.find_best_layout() → n-gram scoring across all layouts
          If better layout detected → _replace_text():
              1. Release modifiers (prevent ⌥⌫ / ⇧⌫)
              2. Erase: N × Backspace via UInput
              3. Switch: layout_backend.set_layout()
              4. Type: char_to_keycode() re-types chars using target layout keycodes
              5. Record in UndoHistory + Stats
```

## Key Design Decisions

- **Direct Injection**: We no longer use `ydotool`. We find the (keycode, shift) pair for each character in the target layout using `LayoutSet.char_to_keycode()` and emit them via our own `UInput` device.
- **Multi-Layout**: The system uses `LayoutSet` instead of `LayoutPair`. It supports any number of layouts configured in `config.toml`.
- **Compositor Backends**: Supports GNOME (real layout read/switch through `g3kb-switch` or the bundled shell extension; simulated Super+Space is only a last-resort fallback — GNOME 45+ ignores it), KDE (D-Bus), Sway (IPC), and Hyprland (IPC).
- **Device Filtering**: Only actual keyboards are grabbed. Mice and virtual bus devices are ignored based on their capabilities and names.
- **Snippets**: Layout-independent matching via canonical form (RU→EN mapping + lowercase).
- **Anti-Resonance**: Prevents A→B→A conversion loops by freezing auto for N seconds.
- **No conversion without a real word**: if the typed word is not a known word of the current language, the re-mapped form must be a known word of the target language. N-gram noise alone used to flip keyboard mashing and vim keys (`hjkl` → `ролд`, `lrkf` → `дкла`); with the gate, random-string false positives drop from ~2.9% to ~0.44% while the golden 31,990-case corpus is unchanged.
- **Flexible Undo**: Undo by word count or time window, not just single word.
- **Active-window detection** (`focus.py`): each compositor is queried through its own interface (swaymsg / hyprctl / kdotool / xdotool). GNOME exposes nothing to clients, so per-app exceptions there need the bundled extension in `gnome-extension/`; the same extension also exposes keyboard-layout read/switch (`org.power_switcher.Layout`). Without it exceptions are inert and GNOME layout switching falls back to simulation (unreliable).
- **Live layout sync**: a manual layout switch (Super+Space) is consumed by the compositor and never reaches the daemon, so the Rust daemon re-reads the *real* layout every 250 ms. On GNOME Wayland, dconf's `input-sources current` is stale and must not be trusted (it caused wrong-layout detection to silently do nothing).
- **Layout switching is the desktop's job**: `Super+Space` is the only layout hotkey the daemon relies on. The legacy "single Shift tap switches layout" gesture is opt-in (`shift_tap_switch_layout = false` by default) — stray taps while typing must never flip the layout.
- **Learned words persist**: `LayoutSet::add_learned` / `add_exception` write `words.json` immediately (Python parity), so a word the user un-converts stays protected across daemon restarts.
- **State file separation**: per-app modes live in `~/.config/power_switcher/exceptions.json`, learned/word exceptions in `words.json`, counters in `stats.json` — they must never share a file.
- **Detection profiles** are generated from the reference word-frequency lists in `data/frequency/` (see `docs/REVERSE_ENGINEERING.md`).

## Code Organization

```
power_switcher/
├── __main__.py              # CLI (daemon, setup, debug, gui, snippets, exceptions, words, stats)
├── config.py                # TOML config loader
├── gui.py                   # tkinter settings window
├── focus.py                 # ActiveWindowProvider: per-compositor focus detection
├── core/
│   ├── key_buffer.py        # KeyBuffer + SentenceBuffer (word/sentence tracking)
│   ├── key_processor.py     # Main event loop and conversion logic
│   ├── modifier_state.py    # Shift/Ctrl/Alt tracking (Caramba-style double-tap)
│   ├── snippets.py          # SnippetStore: layout-independent abbreviation expansion
│   ├── undo_history.py      # UndoHistory: flexible undo by count/time
│   ├── anti_resonance.py    # AntiResonanceGuard: prevent conversion loops
│   ├── app_exceptions.py    # ExceptionStore: per-app mode control
│   └── stats.py             # Stats: persistent conversion counters
├── maps/
│   ├── keycode_map.py       # Keycode tables for EN, RU, UA, DE, FR, ES, IT, PL
│   └── layout_set.py        # LayoutSet: multi-layout scoring, conversion, learned words
├── detection/
│   ├── ngram_detector.py    # N-gram scoring (2-5 grams, weighted) + word vocabulary
│   ├── build_profiles.py    # Profile generator (reads data/frequency/)
│   └── profiles/            # Generated <lang>_ngrams.json.gz (8 languages)
└── layout/
    ├── backends.py           # Compositor-specific layout switchers
    └── detector.py           # Auto-detector for backends

gnome-extension/             # optional GNOME Shell helper for per-app exceptions
data/frequency/              # reference word-frequency lists (profile source)
docs/REVERSE_ENGINEERING.md  # how the reference apps were analysed
tests/                       # pytest suite (PYTHONPATH=. pytest)
```

