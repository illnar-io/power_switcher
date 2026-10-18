# Migrating PowerSwitcher to Rust — research report and plan

> **Scope:** decision document for moving the PowerSwitcher core from Python to a
> high-performance systems language, optimised for the **EN↔RU** use case, using
> **Caramba Switcher Classic** as the design reference.
>
> **Companion:** `docs/CARAMBA_RE.md` (deep reverse engineering of the reference).
> **Status:** research only. No production code has been migrated yet.

---

## 1. Executive summary

**Recommendation: migrate the engine to Rust, incrementally, in a workspace that
mirrors Caramba's split, keeping the multi-layout engine but optimising the
EN↔RU path first.**

The single most important new fact is that **Caramba Switcher Classic is itself
written in Rust** (`caramba-core`, `caramba-app-win`, `caramba-api-client`,
`caramba-cartero`; see `CARAMBA_RE.md §3`). The reference implementation for this
whole product category already chose Rust, uses a DAWG dictionary, weighted
language features, Zstandard, Ed25519-signed models, redb + SQLite state, and a
`mimalloc` allocator. Migrating PowerSwitcher to Rust is therefore not a risky
experiment — it is aligning with the proven reference.

The reason is **not raw throughput**. Python almost certainly computes a
detection fast enough for a human typist. The decisive reasons are:

1. **The keyboard is grabbed.** `power_switcher` takes exclusive (`EVIOCGRAB`)
   ownership of real keyboards and replays events through `uinput`. Any stall in
   the Python process — a GC pause, an import, loading a gzip profile, the GIL
   contending with the tkinter GUI thread — freezes the user's keyboard globally.
   A GC-free, allocation-light Rust daemon on a dedicated input thread removes an
   entire class of user-visible failure.
2. **Distribution.** Today the daemon needs a Python ≥3.11 interpreter,
   `evdev`, `dasbus`, `click`, generated `.json.gz` profiles, and a systemd unit
   that knows about a virtualenv. Rust ships as a single static binary plus data.
3. **Robustness matching the domain.** Concurrency (input thread + focus polling
   + D-Bus + CLI), state machines (buffer/undo/resonance), and strict invariants
   (never reorder a keystroke) are exactly what Rust's type system protects.
4. **Parity with the reference.** Adopting Caramba's model design (DAWG +
   weighted features + signed container) gives a concrete, evidence-based target
   instead of a guess.

**Migration style: strangler, not big-bang.** Build `ps-core` (Rust) behind a
stable boundary, prove parity with an EN↔RU corpus + the existing pytest suite,
then flip the daemon over, then optionally port the GUI.

---

## 2. Goals and constraints

| # | Goal | Notes |
|---|---|---|
| G1 | Never drop, reorder, or delay a physical keystroke | The daemon holds the only copy while grabbed |
| G2 | EN↔RU detection ≥ current accuracy, no new false positives | Baseline in §3.3 |
| G3 | Cross-compositor layout switching (GNOME/KDE/Sway/Hyprland) | keep the existing backend matrix |
| G4 | Per-app exceptions, snippets, learned words, undo, stats | features must survive |
| G5 | Single-binary deploy; no interpreter/venv on target | systemd + udev only |
| G6 | Keep the other 6 layouts working (ES/IT/PL/DE/FR/UA) | not EN↔RU-only |
| C1 | Linux/Wayland only; x86-64 + aarch64 | |
| C2 | No X11-only assumptions; GNOME needs the bundled extension | unchanged from today |

---

## 3. Current system: where the time goes

### 3.1 Hot path (per keystroke)

```
evdev read → KeyProcessor._on_event()
   ├─ modifier/shift bookkeeping          (cheap)
   ├─ UInput passthrough (write)          (cheap, must stay ordered)
   └─ KeyBuffer.add()                     (cheap)
        └─ on word boundary:
             _maybe_auto() → LayoutSet.find_best_layout()
                 ├─ focus refresh (cached, D-Bus/IPC)
                 ├─ per-app exception lookup
                 ├─ n-gram scoring 2..5-grams × N layouts × weighted log-prob
                 └─ word-vocabulary bonus
             if conversion: N backspaces + layout switch + retype
```

The **only** non-trivial compute is `find_best_layout()`: weighted character
n-gram log-probabilities (n = 2..5, weights 1.0/1.6/2.2/2.8) over the configured
layouts, plus a word-frequency bonus, for a word of typically 3–20 characters.

### 3.2 Latency budget

A fast typist emits ≤ 10 keystrokes/s → a ≥ 100 ms budget between events; the
decision for one word must complete in single-digit milliseconds to be invisible.
That is **comfortable for interpreted code** in the common case, which means the
migration must not be justified on average-case speed. It is justified on
**tail latency and stalls** (G1): Python's worst case (GC, first-touch profile
load, GIL hand-off to the GUI thread, import machinery) is unbounded, and while
grabbed, an unbounded stall is a hung keyboard.

### 3.3 Measured baseline (current Python engine)

Run with `PYTHONPATH=. python3 /tmp/opencode/eval_find.py` against the real
decision path (`LayoutSet.find_best_layout`), 15 000 words per language:

| direction | recall | punctuated words |
|---|---|---|
| **RU typed on EN** (Latin buffer → RU) | **84.1 %** plain / **86.4 %** punctuated | 3952/4575 |
| **EN typed on RU** (Cyrillic buffer → EN) | **93.3 %** | n/a |
| false positives (EN list, converted though valid) | **0.01 %** | — |

These are the parity targets the Rust engine must meet or beat for EN↔RU.

### 3.4 Non-performance pain points (current)

* Profile **load** cost: 8 gzip JSON profiles, parsed into Python dicts at start.
* **Memory**: Python dict-of-str n-grams is heavy relative to a compiled model.
* **Packaging**: interpreter + wheels + venv discovery in the systemd unit.
* **Mixed-thread hazards**: tkinter GUI and the evdev loop in one process.
* **Model format**: gzip JSON is neither compact nor queryable; Caramba uses a
  DAWG + Zstandard + signature.

---

## 4. Language comparison

Weighted criteria for this specific workload (a long-lived, low-latency Linux
daemon that grabs input, talks D-Bus, IPC to compositors, and does small-string
scoring), scored 1–5:

| criterion (weight) | Rust | C++ | Zig | Go |
|---|---|---|---|---|
| Low/tail latency, no GC (×3) | 5 | 5 | 5 | 2 |
| Memory safety without GC (×3) | 5 | 2 | 4 | 3 |
| Input/evdev/uinput ecosystem (×3) | 4 | 4 | 2 | 3 |
| D-Bus / compositor IPC (×2) | 5 (`zbus`, `swayipc`, `hyprland`) | 4 | 2 | 3 |
| Memory footprint of the model (×2) | 5 (`fst`, zero-copy) | 4 | 4 | 3 |
| Cross-compile + single static binary (×2) | 5 | 3 | 5 | 4 |
| Dev velocity / maintainability for one dev (×2) | 4 | 2 | 3 | 4 |
| Reference alignment (×1) | 5 | 1 | 1 | 1 |
| **Weighted total** | **≈ 4.7** | ≈ 3.2 | ≈ 3.4 | ≈ 3.0 |

**Rust wins**, driven by (a) the reference implementation being Rust, (b) the
mature `evdev`/`uinput` + `zbus` + `swayipc`/`hyprland` crates, and (c) memory
safety on the exact code path that owns the user's keyboard.

* **C++** matches on latency but loses on safety/velocity and has zero reference
  alignment.
* **Zig** is attractive for static binaries but lacks a keyboard/D-Bus ecosystem
  and would be bespoke for every compositor.
* **Go** is disqualified by the GC on the grabbed-input path (G1).

---

## 5. Why Rust is the low-risk choice here

Evidence from `CARAMBA_RE.md`:

* The reference product's engine is a Rust crate named **`caramba-core`** with
  `classifier.rs`, `dawg_lite/dictionary.rs`, `file_format/container.rs`.
* It uses `aho-corasick`, `regex`, `zstd`, `sha2` + `curve25519-dalek`, `redb` +
  `rusqlite`, `windows` — i.e. the same problem is solved in Rust in production,
  supported by the same crate ecosystem we would use.
* Its timing/latency choices (mimalloc allocator, explicit backspace/retype
  delays, layout-switch retry counts) are all expressible directly in Rust.

So we are not inventing a design; we are porting a **known-good architecture**
from Windows to Linux/evdev and reusing the parts of `power_switcher` that were
validated (weighted n-gram detection, anti-resonance, per-app exceptions,
snippets, learned words, flexible undo — all of which have direct Caramba
analogues: `KbmFeature`, `Appex`, `special_behaviors`, `AutocorrectModel`).

---

## 6. Target architecture

A Cargo workspace mirroring Caramba's split but Linux-native:

```
power_switcher/                     # workspace root (Cargo.toml)
├── crates/
│   ├── ps-core/          # engine: buffer, scoring, conversion, undo, resonance
│   │   ├── classifier.rs      # weighted n-gram + word model (EN↔RU first)
│   │   ├── dictionary.rs      # fst-backed word index (DAWG analogue)
│   │   ├── buffer.rs          # KeyBuffer + SentenceBuffer
│   │   ├── convert.rs         # backspace/retype planner (timing, ordering)
│   │   ├── exceptions.rs      # per-app rules (Appex analogue)
│   │   ├── snippets.rs        # layout-independent expansion
│   │   ├── undo.rs            # count/time undo
│   │   └── resonance.rs       # anti-resonance guard
│   ├── ps-input/         # evdev grab + uinput passthrough/injection
│   ├── ps-layout/        # GNOME (uinput+ibus), KDE (D-Bus), Sway, Hyprland
│   ├── ps-focus/         # active-window provider (per compositor + ext)
│   ├── ps-model/         # .psm container: build, sign, load (zstd + ed25519)
│   ├── ps-cli/           # clap CLI: daemon, setup, debug, snippets, words, ...
│   └── ps-gui/           # optional settings UI (egui/eframe or gtk4)
├── power_switcher/       # legacy Python (kept until parity, then removed)
└── tests/                # pytest (legacy) + cargo tests + cross-language parity
```

### 6.1 Threading model (fixes G1 structurally)

```
[evdev reader thread]  blocking read → lock-free channel →  [engine thread]
                                                              │
        ┌─────────────────────────────────────────────────────┤
        ▼                                                     ▼
[uinput writer thread]                                [focus/D-Bus thread]
        (dedicated, always ready)                     (never blocks the hot path)
```

* The **reader** does nothing but `read()` + send raw events.
* The **engine** owns all state (buffer, models) — single-threaded, no locks on
  the hot path.
* The **writer** owns the `uinput` fd and guarantees ordering of passthrough and
  synthetic events.
* **Focus** runs off-thread; the engine reads a cached `Arc<AppId>`.
This is impossible to get wrong for much of what currently needs care in Python.

### 6.2 Crate selection

| need | crate | why |
|---|---|---|
| evdev grab + uinput | `evdev` (also `input-linux`) | mature, maps 1:1 to current `evdev` usage |
| D-Bus (KDE, GNOME ext) | `zbus` | pure Rust, async; replaces `dasbus` |
| Sway IPC | `swayipc` | replaces `swaymsg` shelling |
| Hyprland IPC | `hyprland` | replaces `hyprctl` shelling |
| X11 (KDE/XWayland focus) | `x11rb` | replaces `xdotool` |
| Word dictionary (DAWG) | `fst` (BurntSushi) | ordered FST: memory-mapped, prefix/fuzzy, ideal for words |
| Multi-pattern heuristics | `aho-corasick` / `daachorse` | same crate Caramba uses |
| Regex heuristics | `regex` | same as Caramba's `util.rs` rules |
| Compression | `zstd` | model container (Caramba uses it) |
| Signing/integrity | `ed25519-dalek`, `sha2` | signed model, like `.clm` |
| Local state (derived) | `redb` (+ optional `rusqlite`) | internal caches only — see §12.4 |
| Public state files | `serde` + `toml` + `serde_json` | `config.toml` + the 4 JSON stores stay the documented interface |
| Config | `serde` + `toml` + `figment` | replaces `config.py` |
| CLI | `clap` | replaces `click` |
| Logging | `tracing` + `tracing-subscriber` | structured logs |
| Allocator | `mimalloc` (optional) | tail-latency, as Caramba does |
| Tray indicator | `ksni` (StatusNotifierItem) | new; Caramba parity + "Display Layout Flag" roadmap item — §12.3 |
| Settings GUI | `gtk4` + `libadwaita` (native) **or** `eframe`/`egui` (single-binary) — §12 | replaces tkinter |

### 6.3 Model redesign (`ps-model`)

Replace the gzip-JSON profiles with a single signed container, informed by
Caramba's `.clm`:

```
psm v1 {
  version
  languages: [ { code, name } ]
  dictionary: fst bytes        # words + relative frequencies (DAWG analogue)
  features: [ { lang, weight, is_known_word, is_case_sensitive,
                is_double_caps_preventative } ]   # mirrors KbmFeature
  ngram:    { 2: .., 3: .., 4: .., 5: .. }        # weighted tables
  heuristics: [ regex ]                           # port of util.rs rules
  appex:      { rules }                           # shipped app exceptions
  autocorrect: { graph, insert_pool, rules }      # optional (phase 4, §13)
  signature: ed25519(sha256(above))
}
```

Benefits over today: **memory-mapped** (no parse cost), **~10× smaller**,
**verified**, and directly extensible to autocorrect/yofikator later. The
existing `data/frequency/` word lists remain the build-time source, and
`build_profiles.py`'s logic is rewritten as a Rust `ps-model` builder (or kept in
Python as a one-time generator).

---

## 7. Migration strategy (strangler)

We do **not** rewrite in one shot. The boundary is the engine interface:

```
trait Engine {
    fn feed(&mut self, ev: KeyEvent) -> Vec<Action>;   // Passthrough | Backspaces | Type | SwitchLayout
    fn undo(&mut self, ...) -> Vec<Action>;
    fn learn(&mut self, word: &str, lang: Lang);
}
```

### Option A — Rust daemon, Python GUI (recommended)

1. Implement `ps-input`, `ps-core`, `ps-layout`, `ps-focus`, `ps-cli` in Rust.
2. Delete the Python daemon path once parity passes; keep Python only for the
   tkinter GUI (or drop it).
3. Lowest risk on the grabbed-input path; no FFI on the hot path.

### Option B — PyO3 extension

Port `ps-core` to Rust, expose it to the existing Python daemon via `pyo3`.
Faster to try, but keeps the GIL/stall problem (G1) and the interpreter runtime
on the hot path. **Reject for the daemon**; acceptable only as a throwaway spike.

### Option C — sidecar process

Rust engine as a subprocess, Python as IPC shim. Adds a round-trip and a second
failure domain. **Reject.**

### Phasing

| Phase | Deliverable | Exit criteria |
|---|---|---|
| **0. Spike** (1–2 d) | `ps-core` prototype: `fst` dictionary + weighted n-gram scorer for EN↔RU; benchmark vs Python | same recall, p99 scoring < 1 ms |
| **1. Engine parity** (1–2 wk) | `ps-core` + `ps-model` builder; corpus tests reproduce §3.3 numbers and all Python unit tests are mirrored | 100 % of `find_best_layout` tests green in Rust |
| **2. Input core** (1–2 wk) | `ps-input` + `ps-layout` + `ps-focus`; run as `ps --debug` alongside Python; diff event traces | no dropped/reordered keys over 1 h soak |
| **3. Cutover** (1 wk) | systemd unit → Rust daemon; Python daemon retired; udev/setup unchanged; `ksni` tray added | G2–G5 verified on GNOME + KDE + Sway |
| **4a. Yofikator** (3–5 d) | RU ё-restoration (needs a ё-annotated lexicon) | restores ё only where the dictionary requires it |
| **4b. Autocorrect** (1–2 wk) | `ps-autocorrect`: bounded-edit typo model, `graph`/`insert_pool`/`rules` (§13) | no regressions on the parity corpus; off in code apps |
| **4c. Model delivery** | bundled `psm` (+ reserved signature); optional Caramba-style CDN updater | offline install works without a server |
| **5. Native GUI (optional)** | `gtk4`/`libadwaita` settings window; tkinter retired | feature parity with the 5 tkinter tabs |

### Phase status (updated as work lands)

| phase | status | evidence |
|---|---|---|
| 0. Spike | **done** | `crates/ps-core` scorer prototype; profile loading verified |
| 1. Engine parity — detection core | **done** | golden corpus: **31 990 EN↔RU decisions, 0 mismatches**; 560 score cases agree to ≤ 1.8e-15 (`cargo test -p ps-core`) |
| 1. Engine parity — remainder | **done** | `KeyBuffer`/`SentenceBuffer`, `UndoHistory`, `AntiResonanceGuard`, `SnippetStore`, `ModifierState`, text helpers (`split_last_word`, `changed_char_count`, `check_double_caps`, `is_password`) + `keycode_to_char`/`ambiguous_separator` ported; 72 lib tests (Python suites mirrored, incl. verified quirks like `soft_reset` char-splitting) + 2 parity tests green |
| 2. Input core | **done** | `ps-input` (device discovery/filter + UInput virtual keyboard, hardware-tested), `ps-layout` (GNOME/KDE/Sway/Hyprland backends + detector, parsers unit-tested; **fixed upstream KDE bug** — Python `\d+` matched `32` inside `uint32`), `ps-focus` (5 strategies + TTL cache), **`ps-engine`: full `KeyProcessor` port as a pure action-stream state machine** (event+timestamp → ordered `Action`s; injection queue + 50 ms cooldown, Caramba gestures, snippets, undo/learning, stats, per-app exceptions) with **16 event-replay tests** (conversion, gestures, undo+learning, snippets, pacing, cooldown). Parity finding: the mid-word eager threshold (15.0) is unreachable for real words — gain max ~11.9 — in Python too. 149 Rust tests, clippy/fmt clean. Remaining for cutover: `ps-daemon` binary (event loop wiring: evdev grab + action executor) |
| 3. Cutover | **in progress** | `ps-daemon` (`ps` binary) built and **validated live**: startup banner, GNOME backend detection (IBus + dconf sources), real-keyboard grab, initial layout sync (correctly read "ru" from GNOME), SIGTERM shutdown. Config loader (`config.toml`, section-tolerant like Python — fixed a missing-`triggers` regression against the real user config). `--no-auto` applied (Python only echoed it — upstream bug). Direct-injection loopback test (uinput → /dev/input node, `KEY_FN`). Remaining: config.toml CLI sections for snippets/exceptions/words/stats stay in Python; manual real-typing validation by the user; 1 h soak |
| 4a–4c / 5 | not started | yofikator, autocorrect, model bundling, native GUI |

Measured on the golden corpus (31 990 cases × 3 rounds, identical inputs,
release build vs CPython 3.15):

| engine | mean | p50 | p99 | max | throughput |
|---|---|---|---|---|---|
| Python `find_best_layout` | 23.5 µs | 22.1 µs | 48.4 µs | **1414 µs** | 42 250/s |
| Rust `find_best_layout` | **4.7 µs** | **4.4 µs** | **10.7 µs** | **390 µs** | **210 291/s** |

The headline is not the ~5× mean speed-up but the **tail**: Python's worst call
on this corpus already exceeds the 1 ms decision budget, while Rust's p99 is
~10 µs — roughly 100× inside it.  (Run both benchmarks with
`cargo run --release -p ps-core --example bench` and
`PYTHONPATH=. python3 tools/bench_python.py`.)

---

## 8. Testing and parity

* **Golden corpus parity harness.** For each EN↔RU word (and punctuated variant)
  in `data/frequency/`, assert the Rust engine returns the same
  (layout, convert?) decision as the Python engine. Any divergence is a
  regression gate. This is the mechanism that makes the "focus on EN↔RU"
  requirement verifiable.
* **Replay tests.** Capture real `evdev` event traces (key-down/up + timestamps)
  and replay them through both engines; assert identical `uinput` output
  streams (ordering, backspaces, shifts, layout switches).
* **Property tests** (`proptest`): never emit a keystroke before the triggering
  one; never exceed N backspaces for a word of length L; anti-resonance always
  terminates.
* **Fuzzing** of the model container parser and the heuristic regexes
  (`cargo-fuzz`).
* **Soak test**: 1 h of synthetic typing under load; assert zero input stalls
  > threshold and flat RSS.

---

## 9. Risks and mitigations

| risk | impact | mitigation |
|---|---|---|
| Input grabbed while engine panics/stalls | hung keyboard for user | engine thread is panic=abort with a **watchdog** that releases the grab on death; systemd `Restart=always`; keep the writer thread independent |
| Wayland compositor APIs drift | layout switch breaks | keep backend per compositor behind a trait; integration tests per backend (as today) |
| GNOME focus needs the extension | per-app exceptions inert | unchanged; ship the extension (already done) |
| Model format churn breaks parity | regressions | golden corpus + versioned container |
| Scope creep to 8 layouts | delays EN↔RU parity | build EN+RU first, add the rest behind the same trait once green |
| Effort overrun | never ships | phased plan; each phase independently useful (engine parity alone proves the concept) |

---

## 10. Effort estimate

Assuming one experienced developer (with the existing Python code as the spec):

| phase | estimate |
|---|---|
| 0. Spike (scorer + benchmark) | 1–2 days |
| 1. Engine + model parity (EN↔RU) | 1–2 weeks |
| 2. Input/layout/focus | 1–2 weeks |
| 3. Cutover + packaging + tray | ~1–1.5 weeks |
| 4a. Yofikator | 3–5 days |
| 4b. Autocorrect | 1–2 weeks |
| 4c. Model delivery (bundled + reserved signature) | ~1 day |
| 5. Native GUI (`gtk4`/`libadwaita`) | 1–2 weeks (optional) |

**MVP (phases 0–3, including the tray), EN↔RU-first with the other layouts
carried along: ~4–6 weeks.** The whole Python codebase is ~4.2 kLOC; the Rust
port of the core is realistically 3–5 kLOC, which is why phase 1 is the bulk of
the risk and the spike de-risks it cheaply. Phases 4–5 are additive and
independently valuable.

---

## 11. Recommendation

1. **Do it in Rust**, as a workspace mirroring `caramba-*`, Linux-native.
2. **Start with a 1–2 day spike** on `ps-core` (fst dictionary + weighted n-gram)
   measured against §3.3 — this proves or kills the premise cheaply.
3. **Port the engine first, keep Python for GUI**, cut over the daemon only after
   the golden corpus and event-replay parity pass.
4. **Adopt Caramba's model ideas** (DAWG/`fst`, weighted features with
   `is_known_word`/`is_case_sensitive`/`is_double_caps_preventative`, Zstandard,
   Ed25519, redb) — they are proven and map cleanly onto what `power_switcher`
   already does.
5. **Keep the multi-layout engine** but make EN↔RU the first thing that must be
   bit-for-bit at parity.

### Decisions (resolved — see §12–§14)

* **GUI:** CLI-first; **tkinter kept as-is through cutover, then optional**; a
  native `gtk4`/`libadwaita` window is phase 5, `egui` is the fallback if a
  single-binary/no-runtime GUI is preferred. (§12)
* **Tray:** add a `ksni` StatusNotifierItem indicator in phase 3 (Caramba has
  one; it also delivers the "Display Layout Flag" roadmap item). (§12.3)
* **Model delivery:** **bundled and versioned**, with a *reserved* signature
  field; no CDN up front. (§14)
* **Autocorrect + yofikator:** **after** the daemon cutover (phase 4), outside
  the EN↔RU parity gate; yofikator first. (§13)

---

## 12. GUI decision

### 12.1 What the GUI actually is today

`power_switcher/gui.py` (678 lines, tkinter) is **not** a live control panel. It
is:

* a **settings editor** with 5 tabs — `Snippets`, `Exceptions`, `Words`,
  `Settings`, `Status`;
* it writes `~/.config/power_switcher/config.toml` and the four JSON stores
  **directly** (`SnippetStore`, `ExceptionStore`, `Stats`, config writer);
* the `Status` tab shells out to `systemctl --user` and reads
  `~/.cache/power_switcher/audit.log` + `stats.json`.

**There is no daemon IPC.** Consequence: the GUI is decoupled from the daemon by
a *file contract*. Any toolkit — or the CLI — can drive the same state, and the
GUI version can evolve independently of the daemon version. This is what makes
"keep tkinter through cutover" safe.

Almost every GUI action already has a CLI equivalent (`ps snippets …`,
`ps exceptions …`, `ps words …`, `ps stats …`), so the GUI is **convenience, not
capability**.

### 12.2 Options

| toolkit | native look | single binary (no runtime) | extra system deps | licence | Wayland | fit |
|---|---|---|---|---|---|---|
| **`gtk4-rs` + `libadwaita`** (optionally `relm4`) | ✅ best on GNOME | ❌ needs GTK4 present | GTK4 + libadwaita | MIT/LGPL | ✅ | ⭐ **recommended if a native GUI is wanted** |
| **`eframe`/`egui`** | ➖ custom look | ✅ | none (glow/wgpu) | MIT/Apache | ✅ | ⭐ **recommended fallback** |
| `iced` | ➖ custom | ✅ (wgpu) | GPU | MIT | ✅ | fine |
| `slint` | ➖ custom | ✅ | none | **GPLv3 / commercial** | ✅ | licence friction for an MIT project |
| `tauri` (WebKitGTK) | ➖ web UI | ❌ | WebKitGTK + web assets | MIT/Apache | ✅ | heavyweight |
| `sciter-rs` (what Caramba uses) | ➖ HTML/CSS | ❌ | proprietary engine | **proprietary** | ⚠️ historically GTK3/X11-oriented | **rejected** |
| tkinter (keep) | ➖ dated | ❌ needs Python + tk | interpreter | PSF | ✅ | **interim only** |

**Rejected — Sciter.** It is tempting for exact parity with Caramba, but Sciter
is a proprietary engine and its Linux backend is GTK/X11-oriented rather than
Wayland-first. We are porting the *engine*, not the reference's UI toolkit.

### 12.3 Decision

1. **CLI-first.** `ps` (clap) is the canonical interface; it already covers every
   GUI action.
2. **Keep tkinter unchanged through phases 2–3.** Because it is file-based it
   keeps working against the Rust daemon with zero changes, and it is not on the
   grabbed-input path. It is the only remaining Python; treat it as an optional
   extra or retire it in phase 5.
3. **Add a tray indicator in phase 3** with `ksni` (freedesktop/KDE
   StatusNotifierItem; `ksni` 0.3, used by Slint for its tray backend). This is
   small, matches Caramba, and delivers the "Display Layout Flag" roadmap item
   (current layout / auto on-off in the panel). *Caveat:* GNOME needs the
   AppIndicator extension for any SNI tray item — document it alongside the
   existing focus extension.
4. **Native GUI (optional, phase 5): `gtk4` + `libadwaita`.** It is native on
   GNOME/Wayland, has the best accessibility, and mirrors the current 5-tab
   layout. Choose `eframe`/`egui` instead **only** if a self-contained binary
   with no GTK runtime dependency is a hard requirement.

### 12.4 Persistence boundary (protects the GUI/CLI decoupling)

Moving state to `redb`/SQLite (as Caramba does) must **not** break the file
contract that keeps tooling decoupled:

* **Public, documented interface:** `config.toml` + `snippets.json`,
  `exceptions.json`, `words.json`, `stats.json`. Keep these formats (or provide
  `ps migrate` + a documented schema bump).
* **Internal only:** `redb` for derived/rebuildable state (compiled word index,
  stats rollups, caches). Losing it must never lose user data.
* Result: the tkinter GUI stays compatible through cutover, and any future GUI
  (or script) can drive the daemon through files.

---

## 13. Autocorrect and yofikator — scope

These are the two Caramba capabilities `power_switcher` lacks. They are
**additive and orthogonal** to layout switching and are explicitly **outside the
EN↔RU parity gate** (they must never change a layout-switch decision).

Reference evidence (`CARAMBA_RE.md §6.2`):

```
AutocorrectModel { graph, rules, insert_pool }
AutocorrectRule  { prefix_len, suffix_len, insert_id, case_sensitive, is_yofikator }
```

### 13.1 Yofikator (RU ё-restoration) — phase 4a

* **What:** when the user types a Russian word with `е` where the orthography
  requires `ё`, offer to restore `ё` (`все`→`всё`, `еще`→`ещё`).
* **Prerequisite (the real work):** a **ё-annotated RU lexicon**. The corpora
  behind `data/frequency/` are typically ё-normalised (ё folded to е), so a
  dedicated source is needed (a spell-check dictionary / Wiktionary extract).
  This is a **data task**, not an algorithm task.
* **Algorithm:** for a finished RU word, if the lexicon has a ё-variant whose
  е-form matches the typed word, propose the ё-form; apply on word boundary or
  via the existing manual hotkey. Conservative: only when the match is
  unambiguous, and never on learned words or under a per-app `off` mode.
* **Effort:** ~3–5 days (mostly building/validating the lexicon).

### 13.2 Autocorrect (typo repair) — phase 4b

* **What:** fix common typos (transpositions, insertions, deletions) in the
  current language, e.g. `teh`→`the`, `приветсвую`→`приветствую`.
* **Model (mirrors Caramba):**
  * `graph` — a typo/candidate automaton over the `fst` dictionary;
  * `insert_pool` — the bounded set of candidate insertions;
  * `rules` — gated by `prefix_len` / `suffix_len` / `case_sensitive`.
* **Algorithm:** bounded Damerau–Levenshtein within a small window over the
  `fst` dictionary; act only when a **single candidate dominates** and the case
  pattern is compatible. Never fire on words the user has learned, and never
  while a per-app `off`/`soft` mode is active.
* **Effort:** ~1–2 weeks.

### 13.3 Guardrails (non-negotiable)

* Autocorrect/yofikator **must not** alter `find_best_layout()` results — keep
  them in a separate pass/crate (`ps-autocorrect`) and out of the parity corpus.
* Measured false-positive rate on the corpus must be reported; default to
  conservative (suggest-only via hotkey) before enabling auto-apply.
* Respect the same `Appex` per-app modes as layout switching (off in code
  editors/terminals is already the right default via `developer_mode`).

---

## 14. Model delivery

**Decision: bundle a versioned model; reserve signing; no server up front.**

* The built `psm` container (§6.3) ships **with the package**. The OS package
  manager already authenticates the payload, so Ed25519 verification adds no
  security benefit for a bundled file.
* **Reserve the `signature` field now** so a future out-of-band updater
  (a Caramba-style `latest.clm` CDN in `CARAMBA_RE.md §4`) can be added **without
  a format break**. Wire in `ed25519-dalek` + `sha2` only when that updater
  exists.
* `data/frequency/` stays the build-time source; the `ps-model` builder produces
  the container (in Rust, or kept in Python as a one-time generator).
* Rationale: offline install, no infrastructure, no ops surface — while keeping
  the door open to remote model updates later.
