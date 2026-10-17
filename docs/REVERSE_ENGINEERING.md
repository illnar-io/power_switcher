# Reverse engineering notes

PowerSwitcher is a from-scratch Linux/Wayland reimplementation of the behaviour
of three commercial keyboard-layout switchers:

| Reference | Platform | In repo as |
|-----------|----------|------------|
| Lang Switcher | macOS (Swift/SwiftUI) | `ref/Lang Switcher.app` |
| Caramba Switcher Classic | Windows (Inno Setup) | `ref/CarambaSwitcherClassicSetup-latest.exe` |
| Punto Switcher | macOS (`.pkg`) | `ref/PuntoSwitcher Installer.pkg` |

The host is Linux, so macOS-specific reverse-engineering tooling was not
available; the analysis below was done with standard archive/`strings`/plist
inspection plus the REA MCP tooling where it applied to local artifacts.

---

## 1. Lang Switcher (macOS) — the highest-value target

`Contents/MacOS/Lang Switcher` is a Mach-O Swift binary; the app is SwiftUI. The
interesting parts are not the machine code but the **declared feature set** and
the **bundled linguistic data**.

### 1.1 Feature set

`Contents/Resources/{en,ru,uk}.lproj/Localizable.strings` (UTF-16 plist) lists
every user-facing setting. Extracted keys (English):

```
Auto Switching
Auto Switching Disabled In
Autostart After Login
Display Layout Flag
Don't Correct After Layout Change
Don't Switch Words
Instant text input correction
Intelligent auto layout switch
Learned Words
Manual Switching
Play Switching Sound
Switch Only Last Word
Running Applications
```

These map directly onto PowerSwitcher features:

| Lang Switcher setting | PowerSwitcher equivalent |
|-----------------------|--------------------------|
| Auto Switching | `auto_detect` in `config.toml` |
| Auto Switching Disabled In / Running Applications | `exceptions` (per-app modes) |
| Don't Switch Words / Learned Words | `words` group + `LayoutSet` learned words |
| Don't Correct After Layout Change | `no_correct_after_switch` |
| Manual Switching | double-tap Shift |
| Instant text input correction | `live_fix_enabled` (mid-word conversion) |
| Switch Only Last Word | `manual_only_last_word` |
| Play Switching Sound | `sound_enabled` |
| Display Layout Flag | (not implemented — needs a tray/applet) |

### 1.2 Word-frequency lists (the extractable asset)

`Contents/Resources/` contains eight plain-text frequency lists:

```
de_frequency.txt   en_frequency.txt   es_frequency.txt   fr_frequency.txt
it_frequency.txt   pl_frequency.txt   ru_frequency.txt   uk_frequency.txt
```

Format: one `word<TAB>raw_count` pair per line, sorted descending, ~50 000
entries each. Example (`en_frequency.txt`):

```
you	28787591
i	27086011
the	22761659
...
```

These are the linguistic core of the app and are exactly what an n-gram
detector needs. They are copied (gzipped) into `data/frequency/` and used by
`power_switcher/detection/build_profiles.py` to generate the runtime detector
profiles. `uk` (Ukrainian) is mapped to the internal layout code `ua`.

Profile schema (`<layout>_ngrams.json.gz`):

```json
{
  "version": 2,
  "language": "en",
  "source": "en_frequency.txt.gz",
  "word_count": 20000,
  "grams":  {"th": 0.0156, "...": 0.0},
  "words":  {"you": 0.000167, "...": 0.0}
}
```

- `grams` — smoothed log-probabilities for 2–5 grams, weighted by
  `log1p(word_freq)` so frequent words shape the model more than rare ones.
  All bigrams plus the top 40 000 trigrams / 25 000 4-grams / 12 000 5-grams.
- `words` — per-word relative frequency (probability), the top 20 000 words,
  used as a vocabulary bonus so short words that the n-grams alone would miss
  are still recovered. At load time these are converted to log-probabilities
  (`log(p)`, e.g. `you → -8.70`, `the → -9.03`).

The detector (`power_switcher/detection/ngram_detector.py`) scores
`Σ weight(n) · log P(grams) + word_bonus(word)` with weights
`{2: 1.0, 3: 1.6, 4: 2.2, 5: 2.8}`. `LayoutSet._word_term` anchors the word
log-probability at a floor of `-13.0` so the bonus stays positive and grows
with word frequency.

---

## 2. Punto Switcher (macOS `.pkg`)

`xar` payloads were unpacked (`extract_ru.yandex.desktop.PuntoSwitcher.PS.pkg/`).
The binary is `Contents/MacOS/PuntoSwitcher`.

### 2.1 Feature set (Objective-C symbols)

`strings` over the binary exposes the class/method names worth copying:

```
ApplicationLayoutsManager
+[ApplicationLayoutsManager currentApplicationID]
-[ApplicationLayoutsManager ...]
isPuntoDisabledInCurrentApplication
addUserRuleWithString:rule:shouldSwitchLayout:isRuleActive:isRegExp:
autocorrectPartialWordLayout
ParseTriggers
```

Interpretation:

- `ApplicationLayoutsManager` / `isPuntoDisabledInCurrentApplication` — per-app
  enable/disable, i.e. the same idea as PowerSwitcher's `ExceptionStore`.
- `addUserRuleWithString:rule:shouldSwitchLayout:isRuleActive:isRegExp:` — the
  user rule engine: every rule carries an action (`shouldSwitchLayout`), an
  active flag, and whether the pattern is a regexp. PowerSwitcher's `words`
  group is the simplified version (literal words, always "do not switch").
- `autocorrectPartialWordLayout` — mid-word correction, mirroring
  `live_fix_enabled`.
- `ParseTriggers` — the word-boundary trigger set (Space/Enter/Tab/punctuation).

### 2.2 Dictionary not cracked

`Contents/Resources/ps.dat` is 292 977 bytes with only 163 distinct byte values
— a proprietary, likely obfuscated, dictionary/trie. The word lists from Lang
Switcher were judged strictly better for our purposes (plain text, 8 languages,
same information class), so `ps.dat` was not decoded. `triggers.dat` was also
left alone for the same reason.

---

## 3. Caramba Switcher Classic (Windows)

`ref/CarambaSwitcherClassicSetup-latest.exe` is an Inno Setup installer whose
setup data version is `Inno Setup Setup Data (7.0.0.3)` — newer than any
released `innoextract`, which is why the tool refuses to unpack it.

Caramba was subsequently **fully extracted** by decoding the Inno Setup 7
*revision-2* loader offset table (PE RCDATA resource `11111`) and carving the
contiguous LZMA2 `zlb\x1a` chunks, yielding all 8 payload binaries. The result
is that **Caramba Switcher Classic is written in Rust**, uses a DAWG dictionary,
a signed `.clm` language model downloaded from a CDN, and a Sciter UI.

The complete analysis (method, binaries, schemas, dependency inventory, heuristic
regexes, Windows API surface) lives in **`docs/CARAMBA_RE.md`**, and the resulting
migration recommendation in **`docs/RUST_MIGRATION.md`**. Still, **no code in
PowerSwitcher is derived from the Caramba binary** — it is used only as a design
reference.

---

## 4. What was reused in the codebase

| Source | Reused as |
|--------|-----------|
| Lang Switcher `*_frequency.txt` (8 langs) | `data/frequency/*.txt.gz` → generated n-gram profiles |
| Lang Switcher settings vocabulary | feature set / config keys (`no_correct_after_switch`, `manual_only_last_word`, learned words, `live_fix_enabled`, `sound_enabled`) |
| Punto rule engine | `words` exceptions + learned words, `ExceptionStore` modes |
| Punto `ParseTriggers` | configurable triggers (space/enter/tab) |
| Punto `autocorrectPartialWordLayout` | mid-word eager conversion |

## 5. What was **not** reused (and why)

- **Caramba installer contents** — unextractable with available tooling.
- **Punto `ps.dat` / `triggers.dat`** — proprietary encoded formats; the
  Lang Switcher word lists supersede them.
- **macOS/SwiftUI UI code** — not portable; PowerSwitcher ships a tkinter GUI
  and a CLI instead.
- **Compositor APIs** — GNOME/KDE/Sway/Hyprland specifics have no equivalent in
  the macOS/Windows references; they come from Linux documentation. GNOME's
  refusal to expose the focused window to clients (see
  `gnome-extension/`) is a Linux-only obstacle with no counterpart upstream.
