# Caramba Switcher Classic — deep reverse engineering

> **Artifact:** `ref/CarambaSwitcherClassicSetup-latest.exe`
> **SHA-256:** `e50a1fd912281b25c91f4fc17692533178ea56db088c0b932619cf50c14a1722`
> **Product version:** `2026.10.16.1` (build dated 2026-08-10)
> **Host used:** Linux (Fedora 45), no Windows runtime.
>
> This document supersedes the earlier, less-evidenced section 8 of
> `caramba-switcher-analysis.md`. Everything below is derived from the actual
> shipped binary and the Inno Setup 7 / Caramba sources of truth, unless marked
> as inference.

---

## 1. Why this took custom tooling

The installer is an **Inno Setup `7.0.0.3`** package. That is newer than any
released `innoextract` (1.9 supports "1.2.10 to 6.0.5"; even master stops at
6.5.x), so it refused to extract:

```
innoextract 1.9  ->  supports Inno Setup 1.2.10 to 6.0.5
```

The reason is purely a **loader format change**: Inno Setup 7 bumped the
`SetupLdr` offset-table record from *revision 1* to *revision 2*. `innoextract`
rejects anything that is not revision 1:

```
Warning: Unexpected setup loader revision: 2
Warning: Setup loader checksum mismatch!
```

We therefore decoded the installer directly from the two authoritative sources:
`innoextract`'s chunk model and the **Inno Setup 7 source** (`jrsoftware/issrc`,
`Projects/Src/Shared.Struct.pas`).

### 1.1 The revision-2 offset table

`SetupLdr` stores a `TSetupLdrOffsetTable` as PE **RCDATA resource 11111**:

```pascal
TSetupLdrOffsetTable = record
  ID: array[1..12] of AnsiChar;   { = 'rDlPtS'#$CD#$E6#$D7#$7B#$0B#$2A }
  Version: UInt32;                { = 2 }
  TotalSize: Int64;               { Minimum expected size of setup.exe }
  OffsetEXE: Int64;               { Offset of compressed setup.e32/64 }
  UncompressedSizeEXE: UInt32;    { Size of setup.e32/64 before compression }
  CRCEXE: Int32;                  { CRC of setup.e32/64 before compression }
  Offset0: Int64;                 { Offset of embedded setup-0.bin data }
  Offset1: Int64;                 { Offset of embedded setup-1.bin data (0 = external) }
  ReservedPadding: UInt32;
  TableCRC: Int32;                { CRC of all prior fields }
end;
```

Recovered from the binary (file offset `0xec70c`, 64 bytes):

| field | value |
|---|---|
| ID | `rDlPtS` + `CD E6 D7 7B 0B 2A` |
| Version | `2` |
| TotalSize | `0x00FBC684` (16 500 100) |
| OffsetEXE | `0x00E3A681` (14 915 201) |
| UncompressedSizeEXE | `0x00506140` (5 267 776) |
| Offset0 (setup-0.bin header) | `0x00E306A7` (14 878 375) |

The signature `Inno Setup Setup Data (7.0.0.3)` sits exactly at `Offset0`.

### 1.2 Carving the payload

Inno stores file data as a run of **`zlb\x1a` chunks** (`src/stream/chunk.cpp`:
`chunk_id = {'z','l','b',0x1a}`), each immediately followed by a raw **LZMA2**
stream (1-byte properties header). `SolidCompression=no` here, so **each packed
file is its own chunk**. Scanning the file for `zlb\x1a` gives 8 chunks whose
compressed extents are *exactly contiguous* and whose last one ends precisely at
`Offset0` — confirming the interpretation:

```
chunk0  0x000ED600 .. 0x00687D80
chunk1  0x00687D80 .. 0x00772B99
chunk2  0x00772B99 .. 0x007C66F7
chunk3  0x007C66F7 .. 0x007D3CCA
chunk4  0x007D3CCA .. 0x00D01FFD
chunk5  0x00D01FFD .. 0x00DD70E6
chunk6  0x00DD70E6 .. 0x00E24B5E
chunk7  0x00E24B5E .. 0x00E306A7  (= Offset0)
```

Decoding each chunk (Python `lzma.FORMAT_RAW` + `FILTER_LZMA2`) yields 8 clean PE
images — the complete Caramba payload, with **no external data files**. The `.clm`
language models are **not shipped**; they are downloaded at runtime (§4).

---

## 2. Installer inventory

All 8 payloads are PE32+ and come in two full architecture sets
(**x86-64** and **Windows on ARM64**):

| # | arch | kind | version | size (dec.) | identity (from version resource) |
|---|------|------|---------|------|----------------------------------|
| 0 | x86-64 | EXE | 2026.10.16.1 | 14 475 960 | `CarambaSwitcherClassic.exe` — "Caramba Switcher Classic" |
| 1 | x86-64 | DLL | 0.9.4 | 2 925 400 | `WinSparkle.dll` — "WinSparkle updater" |
| 2 | x86-64 | EXE | 0.16.6 | 1 003 032 | Crashpad Handler — "Sentry Native SDK" |
| 3 | x86-64 | DLL | 0.16.6 | 121 344 | Crashpad WER Module — "Sentry Native SDK" |
| 4 | ARM64 | EXE | 2026.10.16.1 | 14 771 384 | `CarambaSwitcherClassic.exe` |
| 5 | ARM64 | DLL | 0.9.4 | 2 993 592 | `WinSparkle.dll` |
| 6 | ARM64 | EXE | 0.16.6 | 1 024 200 | Crashpad Handler |
| 7 | ARM64 | DLL | 0.16.6 | 123 352 | Crashpad WER Module |

Observations:

* **Two launch architectures.** Caramba Classic ships native x86-64 *and* native
  ARM64 builds; there is no interpreted/bytecode layer.
* **No bundled Sciter DLL.** The 14.5 MB main EXE statically links the Sciter
  engine (see §3), which explains its size.
* **Updating** = WinSparkle (the Windows Sparkle port). Its export table includes
  `win_sparkle_set_eddsa_public_key` → updates are **Ed25519-signed**.
* **Crash reporting** = Sentry Native SDK (crashpad).
* **No data files.** Only 8 executables are installed by this package.

---

## 3. Language and UI stack

Strings inside `CarambaSwitcherClassic.exe` (x86-64) contain build-machine Rust
source paths, which identify the workspace precisely:

```
C:\Users\im\.cargo\git\checkouts\caramba-core-af53021f7373be8d\c80c92e\src\lib.rs
C:\Users\im\.cargo\git\checkouts\caramba-core-...\src\classifier.rs
C:\Users\im\.cargo\git\checkouts\caramba-core-...\src\dawg_lite\dictionary.rs
C:\Users\im\.cargo\git\checkouts\caramba-core-...\src\file_format\container.rs
C:\Users\im\.cargo\git\checkouts\caramba-core-...\src\util.rs
C:\Users\im\.cargo\git\checkouts\caramba-api-client-3faba0e90c122b4e\...\src\store.rs
C:\Users\im\.cargo\git\checkouts\caramba-api-client-...\src\updater\store.rs
C:\Users\im\.cargo\git\checkouts\caramba-cartero-33910869ad41f0a7\...\src\lib.rs
C:\Users\im\.cargo\git\checkouts\caramba-cartero-...\src\windows.rs
caramba-app-win\src\desktop.rs
```

**Caramba Switcher Classic is written in Rust.** The workspace is:

| crate | role (from module names) |
|---|---|
| `caramba-core` | engine: `classifier`, `dawg_lite/dictionary`, `file_format/container`, `util` |
| `caramba-app-win` | Windows app shell (`src/desktop.rs`) |
| `caramba-api-client` | backend API + `updater/store` (model & license fetching) |
| `caramba-cartero` | low-level Windows input plumbing (`src/windows.rs`) |

### 3.1 UI = Sciter

The binary contains MSVC-mangled C++ symbols for a Sciter host:

```
.?AVMainWindow@caramba@@
.?AVSciterSink@caramba@@
.?AV?$_Func_impl_no_alloc@V<lambda_1>@?1??native_api@MainWindow@caramba@@AEAA?AVvalue@sciter@@XZ@...
dictionary@IUanimated_item@html@@@tool@@
```

plus embedded Sciter resources (`this://user/header-image`, `.htm`, `.css`,
`.svg`) and English/Russian UI text. So the UI is HTML/CSS rendered by **Sciter**
(`sciter::value`, a `SciterSink`, a `native_api` bridge, ~28 lambdas registered
as native functions), not WebView2.

### 3.2 Third-party crate inventory

From `Cargo` registry paths embedded in the binary (exact versions):

```
aho-corasick 1.1.5     boltffi_core 0.31.0    bytes 1.12.1        cipher 0.4.4
crossbeam-channel 0.5.17  crux_core 0.20.0    curve25519-dalek 4.1.3
futures-{channel,core,util} 0.3.34           libmimalloc-sys 0.1.49
libsqlite3-sys 0.38.2  memchr 2.8.3           redb 2.6.3          regex 1.13.1
regex-automata 0.4.18  regex-syntax 0.8.11    rusqlite 0.40.2     serde_json 1.0.151
sha2 0.10.9            slab 0.4.12            uuid 1.28.0         windows 0.62.2
zstd-safe 7.3.0
```

Design signals:

* **`aho-corasick` + `regex`** — multi-pattern search, used for the "do not
  convert" heuristics and special behaviours (§6).
* **`dawg_lite`** (their own crate/module) — the dictionary is a **directed
  acyclic word graph**, not a plain word list.
* **`redb` *and* `rusqlite`/`libsqlite3-sys`** — two embedded stores: redb for
  fast local state, SQLite for relational/query state.
* **`zstd-safe`** — Zstandard compression for the model container.
* **`sha2` + `curve25519-dalek` + `cipher`** — SHA-256 and Ed25519 for signing
  and integrity of models/updates.
* **`windows 0.62.2`** — official Microsoft bindings (the Win32 surface in §7).
* **`libmimalloc-sys`** — mimalloc as the global allocator (latency tuning).
* **`crux_core` + `boltffi_core`** — UI-agnostic core/effects architecture plus
  an FFI shim to the Sciter host.

---

## 4. The language model: `.clm`

`caramba-api-client` fetches models from a CDN; the only model URL in the binary:

```
https://cdn.caramba-switcher.com/files/clm/latest.clm
```

A `.clm` is validated ("`invalid verifying key`", Ed25519) and parsed by
`caramba-core/src/file_format/container.rs`. A serde field-name run in the
binary exposes the container grammar (concatenated, in declaration order):

```
Logos | appex | feats | autocorrect | kbm
```

i.e. the container holds five sections: **`Logos`** (branding), **`appex`**
(application exceptions), **`feats`** (features), **`autocorrect`**, **`kbm`**
(the keyboard language model). The `kbm` schema:

```
Kbm { version, dawg, index, languages, Feats, special_behaviors }
```

* `dawg` — the DAWG dictionary (`dawg_lite/dictionary.rs`)
* `index` — an index over the DAWG
* `languages` — language metadata
* `Feats` — the feature vectors (§5)
* `special_behaviors` — per-layout/behaviour rules

### 4.1 Settings schema (same serde run)

The persisted configuration is declared with these fields:

```
switch_to_en_on_focus
delay_between_layout_activation_and_backspaces
delay_between_backspaces_and_retyping
layout_switch_num_retries
layout_switch_retry_delay
split_shift_delay
```

Notable: Caramba makes the **timing parameters first-class and configurable**
(delays around backspace/retype, layout-switch retry count and delay), plus a
dedicated `split_shift_delay` — exactly the "don't fire too fast after a manual
layout switch" guard that `power_switcher` implements via
`no_correct_after_switch`.

---

## 5. Detection model

`caramba-core/src/classifier.rs` is the scoring engine. The feature schema is:

```
KbmFeatureSet { features: [KbmFeature] }
KbmFeature {
    lang,
    weight,
    is_known_word,
    is_case_sensitive,
    is_double_caps_preventative,
}
```

This confirms a **weighted, per-language feature model** with the same safety
concepts `power_switcher` already implements:

* `weight` per `lang` — weighted evidence, analogous to our weighted n-grams.
* `is_known_word` — the detector distinguishes known vocabulary (like our
  `_WORD_BONUS` / known-word branch).
* `is_case_sensitive` — case-aware scoring.
* `is_double_caps_preventative` — the "DOUBLE → Double" de-duplication guard.

Caramba scores candidate layouts by summing weighted `KbmFeature`s from the DAWG
dictionary and index; the "best" layout wins, gated by the heuristics in §6.

---

## 6. Heuristics, autocorrect and yofikator

### 6.1 "Should this word be converted at all?" (`caramba-core/src/util.rs`)

The binary embeds these regexes, which encode Caramba's *negative* rules (things
that look like data, not prose, and must be left alone):

```
[0-9]+([a-zA-Z]+)[0-9]+
^([a-zA-Z]+)[0-9]{2,}([a-zA-Z]+)
\d+
[[:punct:]]+
[a-z]+
[A-Z]+
^[A-Z]/[A-Z]/[A-Z][a-z\[\];',.]+$
[0123456789]+[a-zA-Z]+[0123456789]+
^.{1}[A-Z]{1}
^([A-Za-z<]|ek|gun|gth|ghjcg|Ek|Gth|Ghjcg)/[A-Z][a-z\[\];',.]+$
^.{2,}[A-Z]+
^[a-zA-Z]+[!?]+$
^[QWERTYUIOP{}ASDFGHJKL":|ZXCVBNM<>?]+[!?/]*$
(?i)^[АВЕКМНОРСТУХ]{1}\d{3}[АВЕКМНОРСТУХ]{2}
(?i)^[АВЕКМНОРСТУХ]{2}\d{3}[АВЕКМНОРСТУХ]{1}
(?i)^\d{4}[АВЕКМНОРСТУХ]{2}
(?i)^[АВЕКМНОРСТУХ]{2}\d{3}
(?i)^[АВЕКМНОРСТУХ]{2}\d{4}
```

This is the direct counterpart of `power_switcher`'s `_ambiguous_separator` plus
the "known-word search bar" logic. It covers: version strings (`v1.2`), mixed
alphanumerics (`abc123`), pure punct/uppercase/lowercase tokens, C++/path-like
tokens (`ns/Class`), anything typed on the QWERTY rows (`qwerty`), and **Russian
vehicle registration plates** (Cyrillic letters drawn from the Latin-homoglyph
set `АВЕКМНОРСТУХ`). The Russian plate rules are a strong hint that Caramba
deliberately protects structured, layout-confusable data.

### 6.2 Autocorrect + "yofikator"

```
AutocorrectModel { graph, rules, insert_pool }
AutocorrectRule  { prefix_len, suffix_len, insert_id, case_sensitive, is_yofikator }
```

* `graph` + `insert_pool` — a **trie/automaton plus an insert pool** for
  typo correction (insertions/replacements).
* `prefix_len` / `suffix_len` — bounded edit windows.
* `is_yofikator` — the Russian **ё (yo) restoration** feature (turn `е` into `ё`
  where the dictionary expects it).

This is the one large capability `power_switcher` does not yet have (currently a
"next step" in the roadmap).

### 6.3 Per-app exceptions

```
Appex { ... }  /  AppexRuleSet { ... }
```

`appex` is a top-level `.clm` section *and* a distinct type — i.e. Caramba ships
a **curated application-exception database** (window/process → behaviour), loaded
with the model rather than maintained by the user alone. `power_switcher`'s
`ExceptionStore` is the user-editable equivalent.

---

## 7. Windows implementation surface (what a Linux port must replace)

`caramba-cartero/src/windows.rs` plus `windows 0.62.2` appear in these imports
found in the binaries:

```
Global input hook :  SetWindowsHookExW, GetAsyncKeyState, SetWinEventHook
Inject keystrokes :  SendInput, keybd_event, PostMessageW / PostMessage
Virtual-key map   :  MapVirtualKey, VkKeyScan
Layout management :  GetKeyboardLayout, GetKeyboardLayoutList,
                     GetKeyboardLayoutNameW, ActivateKeyboardLayout, LoadKeyboardLayout
Foreground window :  GetForegroundWindow, GetGUIThreadInfo
IME               :  ImmGetContext
```

So Caramba's runtime is: a **low-level keyboard hook** (`SetWindowsHookExW`) that
observes keystrokes, **`SendInput`** to retype, **layout APIs** to switch, and
**`GetForegroundWindow`/`GetGUIThreadInfo`** for per-app focus — with **IME**
integration. On Linux these map to:

| Caramba (Windows) | power_switcher (Linux/Wayland) |
|---|---|
| `SetWindowsHookExW` (WH_KEYBOARD_LL) | `evdev` grab on real keyboards |
| `SendInput` / `keybd_event` | `UInput` synthetic device |
| `ActivateKeyboardLayout` / `GetKeyboardLayout` | compositor backends (GNOME/KDE/Sway/Hyprland) |
| `GetForegroundWindow` / `GetGUIThreadInfo` | `ActiveWindowProvider` (`focus.py`) |
| `ImmGetContext` (IME) | (no direct equivalent; fcitx5/ibus integration in future) |
| WinSparkle (Ed25519 updates) | (n/a — no updater yet) |

This is the strongest possible evidence that the *shape* of `power_switcher` is
right, and that the language/architecture choice should converge on Rust.

---

## 8. Caramba vs power_switcher — feature and design matrix

| Capability | Caramba Classic 2026 | power_switcher (current) |
|---|---|---|
| Engine language | **Rust** (`caramba-core`) | Python 3 |
| UI | Sciter (HTML/CSS, native bridge) | tkinter |
| Layouts | EN↔RU (+ DE/UK assets) | 8 layouts (en,ru,ua,de,fr,es,it,pl) |
| Dictionary | **DAWG** (`dawg_lite`) | gzip JSON word lists + n-grams |
| Scoring | weighted `KbmFeature{lang,weight,is_known_word,is_case_sensitive,is_double_caps_preventative}` | weighted char n-grams + word-vocab bonus |
| Model delivery | signed `.clm` from CDN, **not** in installer | bundled `.json.gz` profiles |
| Compression | Zstandard | gzip |
| Integrity | SHA-256 + Ed25519 | — |
| Local storage | redb + SQLite | JSON files |
| Per-app exceptions | shipped `appex` DB + rules | user `exceptions.json` |
| Typo autocorrect | yes (`AutocorrectModel`) | no |
| Yofikator (ё) | yes (`is_yofikator`) | no |
| Timing knobs | 6 explicit settings | `no_correct_after_switch`, `AntiResonanceGuard` |
| Wrong-layout safety regexes | 17 patterns (`util.rs`) | `_ambiguous_separator` + known-word branch |
| Updater | WinSparkle (Ed25519) | — |
| Crash reporting | Sentry Native | — |
| Platform | Windows (x86-64 + ARM64) | Linux/Wayland |

---

## 9. Reproduction notes

The extracted payload is **not** committed (it is 37 MB of third-party
binaries). To reproduce, from the repo root:

```bash
python3 - <<'PY'
import re, lzma
d = open("ref/CarambaSwitcherClassicSetup-latest.exe", "rb").read()
for i, m in enumerate(re.finditer(rb"zlb\x1a", d)):
    o = m.start(); prop = d[o+4]
    ds = 0xffffffff if prop == 40 else ((2 | (prop & 1)) << (prop // 2 + 11))
    dec = lzma.LZMADecompressor(format=lzma.FORMAT_RAW,
                                filters=[{"id": lzma.FILTER_LZMA2, "dict_size": ds}])
    open(f"/tmp/caramba_chunk{i}.bin", "wb").write(dec.decompress(d[o+5:]))
PY
```

The revision-2 offset table can be re-read from PE RCDATA resource `11111`
(see the `TSetupLdrOffsetTable` definition in §1.1).
