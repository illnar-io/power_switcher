# PowerSwitcher — Rust Migration Handoff

*Written: 2026-10-18, after Phase 2 completion. All work is committed to `main`.*

## What this project is

PowerSwitcher is a Linux Wayland daemon that auto-detects wrong-keyboard-layout
typing and converts the text (Punto/Caramba Switcher analog). The **migration
project**: port the working Python daemon to Rust, using Caramba Switcher
Classic (which is itself Rust) as the design reference, focusing on EN↔RU
parity first while keeping the other 6 layouts functional.

**Read `docs/RUST_MIGRATION.md` first** — it has the research report, the
5-phase strangler plan, the phase status table, and the benchmark numbers.
`docs/CARAMBA_RE.md` has the deep RE of the Caramba installer (its `.clm`
model format, workspace layout, and "don't convert" regexes).
`docs/REVERSE_ENGINEERING.md` covers all three reference apps.

## Current state (all committed)

| Phase | Status |
|---|---|
| 0. Spike | ✅ done |
| 1. Engine parity (ps-core) | ✅ done — golden corpus 31,990 EN↔RU decisions, **0 mismatches**; scores agree to ≤1.8e-15 |
| 2. Input core (ps-input, ps-layout, ps-focus, ps-engine) | ✅ done — full KeyProcessor port + 16 event-replay tests |
| 3. Cutover (ps-daemon) | 🔶 **in progress** — `ps` binary works live on GNOME (see below) |
| 4. Yofikator/autocorrect/model | ❌ not started |
| 5. Native GUI | ❌ not started |

Test counts: **Rust 161** (`cargo test --workspace`), **Python 138**
(`PYTHONPATH=. python3 -m pytest -q`). Both must stay green; clippy/fmt clean.

### Phase 3 status (what works today)

`crates/ps-daemon` builds the `ps` binary. **Validated live on this machine
(GNOME/Wayland, real AT Translated Set 2 keyboard):**

```
timeout 4 ./target/debug/ps daemon --debug
```

produced the full startup path: GNOME backend detected (IBus address +
dconf sources `us`/`ru`), startup banner, real-keyboard grab, initial layout
synced to `ru` (read from GNOME via dconf — correct), reader thread running,
clean SIGTERM shutdown. Direct-injection is proven by a loopback test
(uinput → kernel → /dev/input node, `KEY_FN`, harmless on a live desktop).

Still open in Phase 3:
1. **Manual real-typing validation by the user** — run the daemon, type
   wrong-layout words, confirm conversions land (cannot be automated safely
   from a session where the daemon would grab the typing keyboard).
2. `ps --debug` trace diff vs the Python daemon's audit log (same inputs).
3. 1 h soak (flat RSS, no dropped keys) — migration doc §8.
4. Optional: systemd user unit, `ksni` tray (plan §12).

⚠️ **Never run `ps daemon` (or the Python daemon) in an unattended shell** —
it grabs the real keyboard and the user cannot type until it stops.
`timeout N ./ps daemon` is the safe pattern for validation.

## Workspace layout

```
crates/
├── ps-core/      # Engine: ngram detector, layout tables, LayoutSet decisions,
│                 #   KeyBuffer/SentenceBuffer, UndoHistory, AntiResonance,
│                 #   SnippetStore, ModifierState, Stats, ExceptionStore,
│                 #   char↔keycode maps. Parity gate: tests/parity.rs +
│                 #   tests/data/parity_en_ru.json (31,990 golden decisions).
├── ps-input/     # evdev device discovery/filter + UInput VirtualKeyboard
├── ps-layout/    # GNOME/KDE/Sway/Hyprland layout backends + detector
├── ps-focus/     # Active-window detection (5 strategies, TTL cache)
└── ps-engine/    # KeyProcessor = PURE ACTION-STREAM STATE MACHINE
```

Python source of truth lives in `power_switcher/` (unchanged, still works).
`data/frequency/` = reference word lists → `python -m
power_switcher.detection.build_profiles` regenerates the gz profiles.

### The key design decision: ps-engine's action stream

`Processor::on_event(RawEvent, now_f64) -> Vec<Action>` is pure — no I/O.
`Action` = `Emit` (pass-through echo) | `Inject{code, value, delay_ms}`
(paced injection) | `SetLayout{index}` | `Pause{ms}` | `Sound{name}`.
The future daemon loop executes actions against real hardware; replay tests
assert on the stream. **All time is injected** (`now: f64` unix seconds) —
deterministic tests, no sleeps. Every engine component follows this
explicit-clock pattern (`push_at`, `get_class_at`, `record_auto_at`, …).

## Commands

```bash
cargo test --workspace          # all Rust tests (parity gate included)
cargo clippy --workspace --all-targets && cargo fmt --all   # must stay clean
PYTHONPATH=. python3 -m pytest -q                           # Python suite
PYTHONPATH=. python3 tools/export_parity_data.py            # regen golden corpus
cargo run --release -p ps-core --example bench              # Rust bench
PYTHONPATH=. python3 tools/bench_python.py                  # Python bench
```

⚠️ **Never run `python3 -m power_switcher daemon`** — it grabs real keyboards.

## Parity rules (do not break)

1. **Engine decisions must match Python exactly.** The golden corpus
   (`crates/ps-core/tests/parity.rs`) replays 31,990 Python-made decisions.
   If you change scoring/decision logic, regenerate with
   `tools/export_parity_data.py` and justify any mismatch.
2. **Ported unit tests mirror the Python suites** (`tests/test_*.py` →
   `#[cfg(test)]` modules). Verified Python quirks are preserved and
   documented in test comments, e.g.:
   - `SentenceBuffer::soft_reset` splits the word into per-char parts (so
     `pop()` removes one char; a second soft_reset keeps only the last char);
   - snippets `set_all` dedup replaces the whole (trigger, expansion) value at
     the first position (dict semantics);
   - "ПРИвет" is NOT a double-caps case (3rd char is uppercase);
   - the injection replay drops subsequent queued events after a nested
     conversion (Python's `time.time()+0.05` quirk — constant `now` preserves it).
3. **Intentional divergences** (documented in code + migration doc §7):
   - KDE `getLayout` parser: Python's `\d+` matched "32" inside "uint32"
     (always returned 32 — upstream bug); Rust uses `\b\d+\b`.
   - Mid-word eager conversion is synchronous (Python used
     `asyncio.create_task` — racy).
   - Audit messages go through the `log` crate with `AUDIT:` prefix.
   - **Parity finding**: the mid-word eager threshold (15.0) is unreachable
     for real words (max gain ~11.9 across the 8k frequency list) — dead code
     in Python too. Pinned by a replay test.
   - Focus `clean()` rejects "XGetClassHint"/"Error" prefixes; gdbus string
     parser matches single quotes only (Python regex semantics).

## Gotchas

- **evdev crate is 0.13** (not 6.x): `evdev::enumerate()` yields
  `(PathBuf, Device)` directly (not Results); `Device::open/name/grab`;
  `VirtualDevice::builder()?.name(..)?.with_keys(&AttributeSet<KeyCode>)?`.
  `VirtualDevice::emit(&[InputEvent])` auto-appends SYN_REPORT.
  `EventType::KEY` (not `EV_KEY`).
- Rust `char::is_uppercase` ≈ Python `isupper` for single chars; titlecase
  (Lt) handled via `unicode_titlecase()` in `layout_set.rs`.
- `String::extend(char.to_lowercase())` replicates Python's possibly-expanding
  `str.lower()` (e.g. 'İ').
- Python `str.split()` == Rust `split_whitespace()`; `str.rstrip()` ==
  `trim_end()` (Python also trims U+001C–1F — irrelevant here).
- Workspace `members` list in root `Cargo.toml` must include every crate.
- Keycode constants exist in TWO places (pinned equal by tests):
  `ps-core/src/key_buffer.rs` and `ps-input/src/lib.rs::keycodes`.
- The GNOME backend needs `VirtualKeyboard` (Super+Space injection) — that's
  why ps-layout depends on ps-input; the engine crate is ps-engine precisely
  to avoid a ps-input↔ps-layout cycle.
- Tests that need `/dev/uinput` skip gracefully when it's absent; focus tests
  adapt to which tools (swaymsg/hyprctl/kdotool/qdbus/xdotool/gdbus) exist.

## Next move — finish Phase 3

1. **User validation of live typing**: start `./target/debug/ps daemon`,
   type `ghbdtn` + Space in a text field → expect `привет`; double-tap Shift →
   back to `ghbdtn`; L+R Shift pauses.  (The engine and replay tests cover
   this logic; only the live compositing needs human eyes.)
2. Trace comparison: `ps daemon --debug` prints one line per event with its
   action stream — diff against the Python daemon's
   `~/.cache/power_switcher/audit.log` for identical typing sessions.
3. Soak test (1 h synthetic typing, flat RSS) — migration doc §8.
4. systemd user unit + `ksni` tray (optional, plan §12).
5. Then phases 4a/4b/4c (yofikator, autocorrect, bundled `psm` model) and 5
   (native GUI) per the plan.

### Daemon architecture (as built)

* one reader thread per grabbed keyboard → `mpsc` channel of `RawEvent`;
* main loop: `recv_timeout(50ms)` (doubles as shutdown poll) →
  `processor.on_event(ev, now)` → execute `Action`s:
  `Emit`/`Inject{delay_ms}` → `VirtualKeyboard::key` (shared with the GNOME
  backend via `Arc<Mutex<_>>`, Python's `set_uinput` equivalent),
  `SetLayout` → backend, `Pause`/`Sound` → sleep/`paplay`;
* signal thread (signal-hook) raises an `AtomicBool`; cleanup = forced stats
  save + exit (kernel releases grabs on fd close — Python did ungrab +
  `os._exit(0)`, same observable result);
* reader threads poll `fetch_events()` with a 1 ms idle sleep (fd is
  non-blocking in evdev 0.13);
* profiles/layouts data resolution: `PS_PROFILES_DIR` / `PS_LAYOUTS_JSON`
  env vars, else the repo checkout paths (packaging comes with phase 4c);
* `ps` subcommands: `daemon [--config] [-v] [--no-auto] [--debug]`,
  `setup`, `debug` (passive listen, no grab — same loose EV_KEY filter and
  first-device quirk as Python).  Snippets/exceptions/words/stats CLI stay in
  the Python module — they share file formats and don't need the event loop.

### Config gotcha (fixed, regression-tested)

The real user config at `~/.config/power_switcher/config.toml` only has
`[general]` and `[detection]` sections.  The first daemon attempt crashed on
`missing field triggers` — Rust struct fields now use `#[serde(default)]`
per section, matching Python's `d.get("triggers", {})`.  Also: Python's
`default_layout` falls back to the *loaded* `layouts[0]` (ported), and
`--no-auto` actually starts paused in Rust (Python only echoed it — upstream
bug, documented in the code).

## Session log (what happened, newest last)

1. Phase 1: ps-core detection core + golden parity (31,990 cases, 0 mismatch).
2. Phase 1 remainder: KeyBuffer/SentenceBuffer, UndoHistory, AntiResonance,
   Snippets, ModifierState, text helpers, keycode↔char maps.
3. Committed everything in 3 logical commits (docs / python engine / rust core).
4. Phase 2: ps-input (evdev 0.13 devices + uinput), ps-layout (4 backends +
   detector + timeout proc helper), ps-focus (strategies + TTL), ps-core
   additions (Stats, ExceptionStore, char_to_keycode), ps-engine (full
   KeyProcessor port + 16 replay tests). Committed as 2 further commits.
5. Phase 3: ps-daemon (`ps` binary: daemon/setup/debug), TOML config loader
   (section-tolerant; regression-tested against the real user config),
   signal handling, `--debug` action traces, `--no-auto` (actually applied,
   unlike Python). Validated live on GNOME: grab, IBus discovery, layout sync
   to `ru`, clean SIGTERM shutdown. Direct-injection loopback test added to
   ps-input (uinput → /dev/input node; note the udev permission race — open
   with retries). ydotoold present but its socket is dead → cannot inject
   synthetic keys externally; loopback test covers this instead.
6. Live-test failure root cause (2026-10-19, fixed): the daemon synced its
   layout from dconf `org.gnome.desktop.input-sources current`, which is
   **deprecated and stale on GNOME Wayland** — it reported `ru` while the
   real layout was `us`.  Every typed word was therefore decoded with the
   wrong layout (`ghbdtn` → buffer `привет`, i.e. "valid Russian"), so
   detection correctly saw nothing to fix and the daemon looked dead.
   Two further facts were established experimentally on GNOME 51:
   - injected `Super+Space` (and `XF86Keyboard`) does **not** trigger the
     shell's built-in layout keybinding, although injected `Super+F12` does
     fire a gsd custom keybinding;
   - `org.gnome.Shell.Eval` is blocked, and `gsettings set … current N` is
     ignored (writes the key, does not move the layout).
   Working mechanism: the Fedora-preinstalled `g3kb-switch` CLI + its shell
   extension (`g3kb-switch -p` / `-s <xkb-id>`), or our own bundled
   extension (`org.power_switcher.Layout.GetCurrentIndex` /
   `ActivateInputSource`) which requires one logout/login to load.
   Environment changes made for the live session (user-level, reversible):
   `gnome-extensions` → `g3kb-switch@g3kb-switch.org` enabled, and
   `org.gnome.shell disable-extension-version-validation = true` (the
   packaged extension advertises shell-version ≤49; it works on 51).
   Rust changes: GNOME backend auto-detects g3kb-switch → extension →
   simulation (with a loud warning); `get_current_index`/`set_layout` use
   the real mechanism; the daemon now polls the real layout every 250 ms and
   updates the processor (fixes manual Super+Space switches mid-session).
   Added an env-lock to the detector tests (parallel `set_var` race).
   Not ported to the Python daemon (Rust is the live path; the Python
   backend keeps the old, broken dconf/Super+Space logic).
   Verified live: `layout control via g3kb-switch`, `Initial layout synced
   to en`, and a direct backend probe switching 0 → 1 → 0.
7. Live retest worked (2026-10-19): 6 auto conversions + 1 undo, 28 chars
   retyped; stats.json shared with the Python CLI (`stats show` agreed).
   Two follow-ups from the trace:
   - **Lone Shift tap no longer switches layout.** Every individual L/R
     Shift release produced `set_layout(...)` (visible as 0.5 s layout
     flapping at the end of the trace) and fought the user's `Super+Space`.
     The gesture is now opt-in: `shift_tap_switch_layout` (default `false`)
     in `[general]`, wired through `ProcessorConfig`, the daemon `Config`,
     `ps setup`, the banner (`Layout switch: desktop shortcut (Super+Space)`)
     and the Python `KeyProcessor`/`Config`. Double-tap Shift is unchanged.
     Tests: `single_shift_tap_is_inert_by_default` (Rust + Python) and the
     legacy test now sets the flag.
   - **Learned words were not persisted in Rust.** `LayoutSet::add_learned`
     updated an in-memory set only, so `AUDIT: learned word 'ghbdtn'`
     vanished on shutdown (Python writes `words.json`). `LayoutSet` now
     remembers its `words.json` path and `add_exception` / `add_learned`
     (plus new `remove_exception` / `remove_learned` / `exceptions()` /
     `learned_words()`) save immediately. Regression test
     `learned_words_are_persisted`.
   Note for the next session: because "undo → learn" is by design, a stray
   double-tap Shift after an auto conversion blacklists that word.  If a
   demo word stops converting, check `~/.config/power_switcher/words.json`
   (`python -m power_switcher words list`, `words remove <word>`).
8. False-positive audit (2026-10-19, after the second live retest with 12
   conversions): the trace converted `lrkf` → `дкла` — gibberish to
   gibberish.  Root cause: for a word that is **not** in the current
   language's vocabulary the decision used the n-gram gain alone
   (`gain >= 2.5`), and n-gram scores are noise on keyboard mashing.
   Fix (both engines, `find_best_layout`): an unknown source word must
   re-map to a **known word of the target language** (same rule the
   punctuated branch already used).  Measured before/after:
   - golden corpus: 31,990 decisions unchanged, 0 mismatches (every
     positive already had a known-word target);
   - random-letter strings: 1,178 → 175 conversions per 40,000 (2.9% → 0.4%);
   - vim/mashing keys `hjkl jkl; lrkf testtest dfgh wert vbn фыва олдж
     йцук ячсм` now inert (`ghj` → `про` still converts);
   - rare-word recall (frequency ranks 20k-50k) 34 → 24 of 53,141, i.e.
     the gate costs 0.02%.
   Tests: `gibberish_is_not_converted` (Rust + Python),
   `test_known_target_still_converts`.
