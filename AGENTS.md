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

# Settings GUI (tkinter; the GTK4 one is below)
python -m power_switcher gui
```

Rust port (`crates/`), the production daemon and the native GUI:

```bash
# Build everything (ps = daemon, ps-gui = GTK4 settings window)
cargo build --release -p ps-daemon -p ps-gui

# Run the Rust daemon (grabs the keyboard — use a timeout when testing)
timeout 30 ./target/release/ps daemon --verbose

# Native settings GUI (5 tabs: Settings, Snippets, Applications, Words, Status)
./target/release/ps gui          # execs the sibling ps-gui binary

# Rust checks
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check

# Release artifacts (tarball, deb/rpm/Arch when nfpm is installed, AppImage when
# APPIMAGETOOL is set)
VERSION=0.1.0 ./packaging/build-release.sh

# Universal user-local installer (from a release tarball)
sh packaging/install.sh --no-udev --no-service --no-gnome
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
- **Compositor Backends**: Supports GNOME (real layout read/switch through `g3kb-switch` or the bundled shell extension, retried on every call because the shell helpers may not exist yet when the user service starts; simulated Super+Space is only a last-resort fallback — GNOME 45+ ignores it), KDE (D-Bus), Sway (IPC), and Hyprland (IPC).
- **Device Filtering**: Only actual keyboards are grabbed. Mice and virtual bus devices are ignored based on their capabilities and names.
- **Snippets**: Layout-independent matching via canonical form (RU→EN mapping + lowercase).
- **Anti-Resonance**: Prevents A→B→A conversion loops by freezing auto for N seconds.
- **Autocorrect is opt-out and conservative** (`spell_check = true`): after the layout gate declines, an unknown word is corrected only to a single-edit neighbour from the current vocabulary, ranked by `word_logprob + ln(edit_prior)`. The prior follows the typo research: adjacent-key substitution 1.0, vowel↔vowel 0.6, transposition 0.85, missing letter 0.8, extra letter 0.6, same-row distance-2 0.35, far consonant substitution 0.05 (below the 0.1 floor, so `coding`→`coming` no longer fires). The candidate must be at least the vocabulary median and 1.5× the runner-up; learned/exception words, keyboard runs, ALL-CAPS and first-letter edits are skipped. A leading capital is accepted only at a sentence start (`Thsi`→`This`), so mid-sentence names (`Groq`) are protected. It reuses the conversion path, so undo/stats apply. Known limit: valid technical words missing from the 20k subtitle list (`backend`→`backed`, `switcher`→`switched`) can still be corrected; a larger/technical vocabulary or a learning path for corrected originals is the fix, not a stronger edit prior.
- **No conversion without a real word**: if the typed word is not a known word of the current language, the re-mapped form must normally be a known word of the target language. The 20k subtitle vocabulary misses technical terms and names (`plugin`, `config`, `клавиатура`, `раскладка`), so when *neither* side is known the n-gram gain decides — but it must clear `threshold + 0.3` and the typed text must not be a keyboard run (`fghjkl`, `zxcvbnm`, `vbn` are detected by same-row key adjacency). N-gram noise alone used to flip mashing and vim keys (`hjkl` → `ролд`, `lrkf` → `дкла`); the gate keeps random-string false positives near 0.4% while the golden 31,990-case corpus stays unchanged.
- **L2 plausibility path** (Russian speakers typing English/Russian, EN↔RU only): a word in neither vocabulary is also accepted when the typed form is implausible in the current language (score ≤ -12.0) and the re-mapped form is plausible in the target (score ≥ -12.5, gain ≥ 1.5, 5+ chars, target has a vowel, target is not a keyboard run). This fixes Russian IT jargon typed in the EN layout (`переключение`, `разработка`, `фронтенд`, `плагин`, `сниппет`, `браузер`, `бэкенд`) and Russian words containing `б`/`ю`, which arrive as `,`/`.` and used to hit the punctuation gate. Correctly typed English technical terms (`backend`, `plugin`, `kubernetes`) score -8..-11 in EN, so they stay. `live_fix`'s 15.0 threshold disables the path, so mid-word typing never injects. The other six layouts keep the old relative-gain gate. RILEC/REALEC list transliteration as a core Russian-L1 interference type; autocorrect still needs a guard for transliterated words (`servis` → `serves`).
- **Flexible Undo**: Undo by word count or time window, not just single word.
- **Active-window detection** (`focus.py`): each compositor is queried through its own interface (swaymsg / hyprctl / kdotool / xdotool). GNOME exposes nothing to clients, so per-app exceptions there need the bundled extension in `gnome-extension/`; the same extension also exposes keyboard-layout read/switch (`org.power_switcher.Layout`). Without it exceptions are inert and GNOME layout switching falls back to simulation (unreliable).
- **Live layout sync**: a manual layout switch (Super+Space) is consumed by the compositor and never reaches the daemon, so a worker thread re-reads the *real* layout every 250 ms and publishes the index; the key loop only reads an atomic. Never spawn `g3kb-switch` from the key loop. On GNOME Wayland, dconf's `input-sources current` is stale and must not be trusted (it caused wrong-layout detection to silently do nothing).
- **Layout switching is the desktop's job**: `Super+Space` is the only layout hotkey the daemon relies on. The legacy "single Shift tap switches layout" gesture is opt-in (`shift_tap_switch_layout = false` by default) — stray taps while typing must never flip the layout.
- **Manual-convert hotkey**: `hotkey_convert = "ctrl+alt+z"` by default (laptops have no Pause). The value is parsed by `ps-engine::hotkey` into an exact modifier set + key: `ctrl+alt+z`, `pause`/`break`/`pause/break` (Caramba), any single key (`f12`, `insert`), or `off`. The combo swallows press and release; `Ctrl`/`Alt` presses must not clear the sentence (modifier-only presses are exempt from the shortcut buffer reset) or the last word would be gone before `Z` arrives. Double-tap Shift always works. `hotkey_selection = "alt+shift"` is the documented Caramba selection combo but is not implemented on Wayland (needs clipboard access); the GUI states this.
- **Service install lives in the GUI**: the Status tab embeds `systemd/power_switcher.service`, writes it to `~/.config/systemd/user/`, then runs `systemctl --user daemon-reload` + `enable --now`; "Restart daemon" uses `systemctl --user restart`. Always `--user`, never the invalid `systemctl user …` form. Both shipped units set `NoNewPrivileges=yes`, `PrivateTmp=yes`, `ProtectSystem=full`, `ProtectKernelTunables/Modules`, `ProtectControlGroups` and `RestrictAddressFamilies=AF_UNIX AF_NETLINK`; keep those compatible with D-Bus/compositor IPC. Do not add `ProtectHome=read-only` without `ReadWritePaths` and ensuring the config directory exists, or a fresh daemon cannot create its stores.
- **Installed names**: packages and the installer expose `power-switcher` / `power-switcher-gui`, never `/usr/bin/ps` (that is procps). Cargo artifacts stay `ps` / `ps-gui`; `cmd_gui` and the Status tab search both names, preferring the installed one. The udev rule is `udev/60-power_switcher.rules`: it must load before systemd's `73-seat-late.rules` (a `99-` rule is too late and applies no ACL), and it pins internal keyboards with no `ID_SEAT` to `seat0`. Packages and the installer do not modify group membership. `--add-group` is the explicit fallback that adds the user to `input`.
- **Private user data**: `config.toml`, `snippets.json`, `words.json`, `exceptions.json` and `stats.json` are written `0600` atomically via `ps_core::fsutil`; new app directories are `0700`, existing files are tightened on load. Typed words and snippet contents must never be logged — audit lines use layouts, lengths and counters only. Snippets remain plaintext; do not store passwords there.
- **Packaging targets modern releases only** (Fedora 44+, Ubuntu 24.04+, Debian 13+, current Arch/openSUSE). `nfpm` emits deb/rpm/Arch packages, `packaging/install.sh` is the universal fallback, and the AppImage uses the host GTK 4.10+ runtime. Never install the daemon as `/usr/bin/ps`.
- **Learned words persist**: `LayoutSet::add_learned` / `add_exception` write `words.json` immediately (Python parity), so a word the user un-converts stays protected across daemon restarts.
- **Vocabulary is learned from usage** (`self_learning = true`): undo of a conversion learns the reverted word immediately; an unknown word that survives both the layout gate and autocorrect is counted per word boundary and learned on the third sighting. Counts are in-memory and capped, only the word is persisted. This keeps the user's jargon, names and technical terms out of both conversion and autocorrect.
- **State file separation**: per-app modes live in `~/.config/power_switcher/exceptions.json`, learned/word exceptions in `words.json`, counters in `stats.json` — they must never share a file.
- **GUI shares files, not IPC**: `power-switcher-gui` / `ps-gui` (GTK4, `crates/ps-gui`) reads and writes `config.toml` plus the JSON stores through the same loaders the daemon uses (`Config`, `WordStore`, `SnippetStore`, `ExceptionStore`, `Stats`). It has no daemon socket; changes apply on daemon restart (Status tab has a restart button). GTK must stay out of the daemon binary — `power-switcher gui` execs the sibling GUI.
- **Detection profiles** are generated from the reference word-frequency lists in `data/frequency/` (see `docs/REVERSE_ENGINEERING.md`).
- **Latency**: measure with `cargo test --release -p ps-engine --test latency -- --ignored --nocapture`. Processor cost is microseconds (unknown-word boundary ~53 µs, autocorrect scan ~57 µs); the visible pauses are the deliberate 170 ms autocorrect / 300 ms layout-conversion injection pacing in `replace_text`, not detection. Keep subprocess poll intervals at 1 ms — a 10 ms poll became a 10 ms word-boundary stall.

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
crates/                      # Rust port: ps-core, ps-input, ps-layout, ps-focus,
                             # ps-engine, ps-daemon (`ps`), ps-gui (GTK4, `ps-gui`)
packaging/                   # install.sh, nfpm.yaml, tarball/AppImage builders
docs/REVERSE_ENGINEERING.md  # how the reference apps were analysed
tests/                       # pytest suite (PYTHONPATH=. pytest)
```

