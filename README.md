# PowerSwitcher

A DE-independent keyboard layout auto-switcher for Linux Wayland.

Like Punto Switcher / Caramba Switcher / Keyboop, but for Linux Wayland.

## How it works

1. **Grabs the physical keyboard** at the kernel level via `evdev` — no compositor involvement.
2. **Passes all keystrokes through** a virtual `UInput` keyboard to the compositor.
3. **Tracks a rolling word buffer** of characters the user is typing.
4. **On word boundary** (Space / Enter / Tab / punctuation): runs an n-gram language detector on the buffered word; if the word scores higher in another layout → auto-converts it.
5. **Direct Injection**: Erases the word via Backspaces and re-types it in the correct layout directly through the virtual keyboard. No external tools needed!
6. **Compositor Native**: Switches system layout via D-Bus (GNOME, KDE) or compositor commands (Sway, Hyprland).

## Features

- **DE Independent**: Works on GNOME, KDE Plasma, Sway, Hyprland, and more.
- **Multi-Layout Support**: Supports 2, 3, or more layouts (English, Russian, Ukrainian, German, French, Spanish, Italian, Polish).
- **Fast & Reliable**: Uses direct `uinput` injection for near-instant text conversion.
- **Zero Configuration for Core**: Auto-detects your desktop environment and keyboards.
- **Word-frequency n-gram detector**: Weighted 2–5 gram model plus a 50k-word
  vocabulary per language, regenerated from the reference word-frequency lists.
- **Snippets**: Auto-expand abbreviations on word boundary (layout-independent matching).
- **Flexible Undo**: Undo last conversion by word count or time window.
- **Self-learning**: When you undo a conversion, the original word is remembered
  and never converted again.
- **Anti-Resonance Guard**: Prevents conversion loops (A→B→A oscillation).
- **Application Exceptions**: Per-app mode control (off/soft/default) — disable auto in IDEs, terminals, etc.
- **Configurable Triggers**: Choose which keys (Space, Enter, Tab, punctuation) trigger auto-detection and snippet expansion.
- **Mid-word Conversion**: Converts words eagerly during typing when confidence is very high.
- **Double Caps Fix**: Automatically corrects DOUBLE CAPS → Double Caps.
- **Pause hotkey**: Press Left+Right Shift together to pause/resume auto-detection.
- **Guard after layout switch**: Optionally skip corrections for N seconds after
  the layout changes, so manual switching is not fought by the detector.
- **Statistics**: Counts conversions, snippet expansions and undos.
- **Settings GUI**: `ps gui` (GTK4, `ps-gui`) or the legacy
  `python -m power_switcher gui` (tkinter); both edit the same files.


## Requirements

- Linux Wayland session (X11 also works for the daemon; focus detection uses `xdotool`)
- Python 3.11+
- `python-evdev`, `click` (Python packages)
- User in `input` group
- Optional: `paplay` (conversion sound), `tkinter` (settings GUI),
  `gnome-extension/` (per-app exceptions on GNOME)

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

### Rust daemon and native GUI

The production daemon is the Rust port in `crates/`; the Python tree remains the
reference implementation and the profile generator.

```bash
cargo build --release -p ps-daemon -p ps-gui

./target/release/ps setup           # udev rules + default config.toml
./target/release/ps daemon --verbose
./target/release/ps gui             # GTK4 settings window (5 tabs)
```

`ps gui` launches `ps-gui`, a native GTK4 window (Settings, Snippets,
Applications, Words, Status). It edits `config.toml` and the JSON stores
directly — same file contract as the CLI — and has no daemon IPC, so settings
apply after a daemon restart (the Status tab has a restart button).

Requirements for the GUI: GTK 4.10+ development headers (`gtk4-devel` on
Fedora, `libgtk-4-dev` on Debian/Ubuntu).

## Hotkeys

| Hotkey | Action |
|--------|--------|
| Double-tap Shift | Convert current sentence + switch layout (or undo the last conversion) |
| **Left + Right Shift together** | Pause / resume auto-detection |
| Single Left/Right Shift | *Legacy, off by default* — switches to layout 1/2 (`shift_tap_switch_layout = true`). Use your desktop's `Super+Space` instead. |

## Configuration

Config file: `~/.config/power_switcher/config.toml`

```toml
[general]
layouts          = ["en", "ru"]    # Supports 2+ layouts
default_layout   = "en"
auto_detect      = true
developer_mode   = false           # Disable auto in IDEs/terminals
arrows_cancel    = true            # Arrow keys clear buffer
live_fix_enabled = true            # Mid-word eager conversion
lr_shift_pause   = true            # L+R Shift pauses auto-detection
manual_only_last_word = false      # Manual convert touches only the last word
self_learning    = true            # Learn words you un-convert
sound_enabled    = true            # Play a sound on conversion
shift_tap_switch_layout = false    # Lone Shift tap switches layout (legacy; desktop owns Super+Space)
no_correct_after_switch = 1.5      # Seconds to leave alone after a layout switch (0=off)

[detection]
threshold     = 2.5
min_word_len  = 3

[triggers]
space          = true    # Space triggers auto-detection
enter          = true    # Enter triggers auto-detection
tab            = true    # Tab triggers auto-detection
punctuation    = true    # . , ; ! ? trigger auto-detection
snippet_space  = true    # Space triggers snippet expansion
snippet_enter  = true    # Enter triggers snippet expansion
snippet_tab    = true    # Tab triggers snippet expansion

[undo]
words    = 0       # 0 = single word undo; >0 = last N words
seconds  = 3.0     # Time window for undo
```

## Snippets

Auto-expand abbreviations on word boundary (layout-independent):

```bash
# Add a snippet
python -m power_switcher snippets add '!ee' 'user@example.com'
python -m power_switcher snippets add 'btw' 'by the way'

# List snippets
python -m power_switcher snippets list

# Remove a snippet
python -m power_switcher snippets remove '!ee'
```

## Application Exceptions

Disable auto-detection in specific applications:

```bash
# Disable auto in VS Code
python -m power_switcher exceptions add 'code' --mode off

# Soft mode (manual convert only)
python -m power_switcher exceptions add 'terminal' --mode soft

# List exceptions
python -m power_switcher exceptions list
```

### Reading the focused application

Per-app exceptions need to know which window is focused. Each compositor
exposes this differently and PowerSwitcher handles them automatically — see
`power_switcher/focus.py`:

| Compositor | Mechanism |
|------------|-----------|
| Sway | `swaymsg -t get_tree` |
| Hyprland | `hyprctl activewindow -j` |
| KDE Plasma | `kdotool`, otherwise KWin D-Bus |
| X11 / XWayland | `xdotool` |
| **GNOME** | **requires the bundled companion extension** |

GNOME Shell 45+ blocks the old `org.gnome.Shell.Eval` and
`org.gnome.Shell.Introspect.GetWindows` D-Bus APIs for third-party callers, so
there is no way for PowerSwitcher to read the focused window on its own. Install
the tiny extension in `gnome-extension/` (see its README) and per-app exceptions
start working. Without it, PowerSwitcher still runs — exceptions are simply
inert.

## Word exceptions & learned words

```bash
# Never auto-convert a word again
python -m power_switcher words add 'макбук'

# List learned words and word exceptions
python -m power_switcher words list

# Forgive a word
python -m power_switcher words remove 'макбук'
```

When you undo an automatic conversion (double-tap Shift on an empty buffer),
the original word is learned automatically so it will not be converted again.
Toggle this with `self_learning` in the config.

## Statistics

```bash
python -m power_switcher stats show
python -m power_switcher stats reset
```

## Supported Layouts

Built-in layouts: `en`, `ru`, `ua`, `de`, `fr`, `es`, `it`, `pl`.

### Which combinations are meaningful

Auto-conversion works by re-reading the same physical keys in another layout.
That only helps when the two layouts put **different characters on the same
key**:

| Pair | Detectable? |
|------|-------------|
| `en` ↔ `ru` / `ua` | yes |
| `en` ↔ `de` (Y↔Z) | yes |
| `en` ↔ `fr` (AZERTY) | yes |
| `ru` ↔ `es` / `it` / `pl` | yes (any Cyrillic ↔ Latin pair) |
| `en` ↔ `es` / `it` / `pl` | **no** — these layouts share the QWERTY letter rows, so the same keys produce the same letters |

`es`, `it` and `pl` also exist to *retype* text correctly (punctuation and
accented keys differ), so they are still useful as the Latin side of a
Cyrillic pair, or for their keycode tables.

### Detection quality

Measured on the bundled word lists (words ≥4 letters, correct-word false
positives counted over 4000 words per language):

| Intended language typed in the wrong layout | Recall | False positives |
|---|---|---|
| Russian typed in `en` | 99.8 % | 0.00 % |
| Ukrainian typed in `en` | 99.8 % | 0.00 % |
| German typed in `en` | 99.7 % | 0.00 % |
| French typed in `en` | 99.9 % | 0.00 % |
| English typed in `ru` | 100 % | 0.12 % |

Reproduce with `tests/test_detector.py` plus the ad-hoc harness described in
`docs/REVERSE_ENGINEERING.md`.

### Adding a new layout
1. Add a keycode→char table in `power_switcher/maps/keycode_map.py`
   (derive it from `/usr/share/X11/xkb/symbols/<layout>`).
2. Register it in the `LAYOUTS` dict.
3. Provide a word-frequency list in `data/frequency/<lang>_frequency.txt`
   (one `word<TAB>count` per line, sorted by frequency) and run
   `python -m power_switcher.detection.build_profiles`.

## Architecture

```
Physical Keyboard → evdev grab → KeyProcessor → UInput passthrough → Compositor
                                  │
                                  ├─ KeyBuffer (word tracking)
                                  ├─ SentenceBuffer (sentence context)
                                  ├─ ModifierState (shift/ctrl/alt, Caramba-style double-tap)
                                  ├─ LayoutSet (multi-layout maps + n-gram profiles + learned words)
                                  ├─ NGramDetector (weighted 2–5 grams + word vocabulary)
                                  ├─ LayoutBackend (D-Bus / IPC layout switch)
                                  ├─ ActiveWindowProvider (per-app focus detection)
                                  ├─ SnippetStore (layout-independent abbreviation expansion)
                                  ├─ UndoHistory (flexible undo by count/time)
                                  ├─ AntiResonanceGuard (prevent conversion loops)
                                  ├─ ExceptionStore (per-app mode control)
                                  ├─ Stats (conversion counters)
                                  └─ UInput (direct text injection)
```

## Project layout

```
power_switcher/
├── focus.py                 # per-compositor active-window detection
├── config.py                # TOML config loader
├── gui.py                   # tkinter settings UI
├── core/                    # event loop, buffers, snippets, undo, stats, exceptions
├── maps/                    # keycode tables + LayoutSet
├── detection/               # n-gram detector, profile builder, generated profiles
└── layout/                  # compositor layout backends
gnome-extension/             # optional GNOME Shell helper for per-app exceptions
data/frequency/              # reference word-frequency lists (profile source)
crates/                      # Rust port: ps-core, ps-input, ps-layout, ps-focus,
                             # ps-engine, ps-daemon (`ps`), ps-gui (GTK4, `ps-gui`)
docs/REVERSE_ENGINEERING.md  # how the reference apps were analysed
```

## Supported DEs/Compositors

| DE/Compositor | Backend | Notes |
|---------------|---------|-------|
| **GNOME** | `GNOMESwitchBackend` | Simulates Super+Space via UInput (GNOME Wayland ignores programmatic switches). |
| **KDE Plasma** | `KDELayoutBackend` | Uses D-Bus `org.kde.keyboard`. |
| **Sway** | `SwayLayoutBackend` | Uses `swaymsg`. |
| **Hyprland** | `HyprlandLayoutBackend` | Uses `hyprctl`. |

## Development

```bash
PYTHONPATH=. pytest          # 111 tests
PYTHONPATH=. python3 -m pytest -q
```

## License

MIT

---

## 💬 Connect

[![GitHub Sponsors](https://img.shields.io/badge/Sponsor-illnar--io-ea4aaa?logo=github-sponsors&logoColor=white)](https://github.com/sponsors/illnar-io)
[![Telegram](https://img.shields.io/badge/Telegram-ILLnar-26A5E4?logo=telegram&logoColor=white)](https://t.me/ILLnar)
[![GitHub](https://img.shields.io/badge/GitHub-illnar--io-181717?logo=github&logoColor=white)](https://github.com/illnar-io)
[![LinkedIn](https://img.shields.io/badge/LinkedIn-illnar-0077B5?logo=linkedin&logoColor=white)](https://linkedin.com/in/illnar)
[![Email](https://img.shields.io/badge/Email-nizametdinov%40gmail.com-D14836?logo=gmail&logoColor=white)](mailto:nizametdinov@gmail.com)

## 💝 Support the Project

If my work helps you, consider supporting its development:

- ⭐ **GitHub Sponsors**: <https://github.com/sponsors/illnar-io>
- **Ko-Fi**: <https://ko-fi.com/illnar>
- **PayPal**: [nizametdinov@gmail.com](mailto:nizametdinov@gmail.com)
- **Bunq**: <https://bunq.me/INizametdinov>
- **Revolut**: `@illnar_nizami`
