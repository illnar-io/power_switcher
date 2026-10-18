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
| 3. Cutover (ps-daemon binary) | ❌ next — the only remaining piece |
| 4. Yofikator/autocorrect/model | ❌ not started |
| 5. Native GUI | ❌ not started |

Test counts: **Rust 149** (`cargo test --workspace`), **Python 135**
(`PYTHONPATH=. python3 -m pytest -q`). Both must stay green; clippy/fmt clean.

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

## Next move — Phase 3 (cutover)

Build `crates/ps-daemon` (binary `ps`):

1. **Event loop**: grab keyboards (`ps_input::find_keyboards` + `Device::grab`),
   read events (blocking thread per keyboard or one poll loop), call
   `processor.on_event(ev, now)`, execute the returned `Action`s:
   - `Emit`/`Inject` → `VirtualKeyboard::key` (inject: sleep `delay_ms` after);
   - `SetLayout` → the `ps_layout::LayoutBackend`;
   - `Pause` → sleep;
   - `Sound` → spawn `paplay` (best-effort).
2. **Startup parity** with Python `run()`: create uinput from keyboards[0],
   hand it to the GNOME backend (`set_virtual_keyboard`), sync initial layout
   from `backend.get_current_index()`, `refresh_front_app`.
3. **`ps --debug` mode**: dump per-event action traces to compare against the
   Python daemon's audit log (exit criterion: no dropped/reordered keys).
4. CLI: `ps daemon --verbose`, `ps debug`, `ps setup`; keep the tkinter GUI
   until phase 5. Signal handling: on SIGINT/SIGTERM — ungrab, save stats.
5. Config loading (`config.toml`) — port `power_switcher/config.py` fields
   into `ProcessorConfig`.
6. Soak test (1 h synthetic typing, flat RSS) — see migration doc §8.

Then phases 4a/4b/4c (yofikator, autocorrect, bundled `psm` model) and 5
(native GUI) per the plan.

## Session log (what happened, newest last)

1. Phase 1: ps-core detection core + golden parity (31,990 cases, 0 mismatch).
2. Phase 1 remainder: KeyBuffer/SentenceBuffer, UndoHistory, AntiResonance,
   Snippets, ModifierState, text helpers, keycode↔char maps.
3. Committed everything in 3 logical commits (docs / python engine / rust core).
4. Phase 2: ps-input (evdev 0.13 devices + uinput), ps-layout (4 backends +
   detector + timeout proc helper), ps-focus (strategies + TTL), ps-core
   additions (Stats, ExceptionStore, char_to_keycode), ps-engine (full
   KeyProcessor port + 16 replay tests). Committed as 2 further commits.
