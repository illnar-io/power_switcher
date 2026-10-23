# AGENT INSTRUCTIONS: Build `power_switcher`
## A DE-independent keyboard layout auto-switcher for Linux Wayland

**Read this file top to bottom before writing any code.**  
**Follow phases in order. Do not skip ahead.**

---

## WHAT YOU ARE BUILDING

A Python daemon that:

1. **Grabs the physical keyboard** at the kernel level via `evdev` — no compositor involvement
2. **Passes all keystrokes through** a virtual `UInput` keyboard to the compositor as-is (transparent by default)
3. **Tracks a rolling word buffer** of characters the user is typing
4. **On word boundary** (Space / Enter / Tab / punctuation): runs an n-gram language detector on the buffered word; if the word scores higher when converted to the other layout → auto-converts it
5. **On hotkey press** (e.g. `Pause/Break`): takes the current word buffer, converts it to the other layout, injects `Backspace × N` + converted text via UInput/ydotool, switches layout via DE D-Bus
6. Works on **KDE, GNOME, COSMIC, any Wayland compositor** — no extensions required for core functionality

**This is like PuntoSwitcher / CarambaSwitcher but for Linux Wayland.**

---

## ENVIRONMENT ASSUMPTIONS

- OS: Fedora Linux 44 (Workstation Edition), GNOME 50.1 / Mutter (Wayland)
- Kernel: 7.0.5-200.nobara.fc44.x86_64
- CPU: AMD Ryzen 7 6800H · GPU: AMD Radeon 680M (integrated)
- Shell: fish 4.7.1 · Terminal: ghostty
- Python: 3.11+
- User is in the `input` group (setup instructions in Phase 0)
- `XDG_SESSION_TYPE=wayland`, `XDG_CURRENT_DESKTOP=GNOME`

---

## WHY evdev+UInput IS THE ONLY UNIVERSAL APPROACH

On Wayland, the compositor is a security wall. There is no cross-compositor API for:
- reading global keyboard events
- injecting global key events
- querying text in foreign windows

`evdev` operates **below the compositor** at the kernel input layer. By grabbing the physical device and creating a virtual UInput passthrough, the daemon intercepts 100% of keyboard input before it reaches any compositor. This is the same technique used by `xwaykeyz`, `kanata`, `evsieve`.

**Do not use:** `xdotool`, `xte`, `xautomation`, `xclip`, `xinput` — all X11-only and broken on Wayland.  
**Use instead:** `python-evdev`, `ydotool`, `wl-clipboard`, `gdbus` (for layout switch).

---

## PROJECT STRUCTURE

Create this layout exactly:

```
power_switcher/
├── pyproject.toml
├── README.md
├── power_switcher/
│   ├── __init__.py
│   ├── __main__.py           # CLI entry: `python -m power_switcher daemon`
│   ├── config.py             # TOML config loader
│   ├── core/
│   │   ├── __init__.py
│   │   ├── key_processor.py  # Main event loop: grab → passthrough → detect → convert
│   │   ├── key_buffer.py     # Word buffer tracking
│   │   └── modifier_state.py # Shift/Ctrl/Alt state
│   ├── maps/
│   │   ├── __init__.py
│   │   ├── keycode_map.py    # evdev keycode → (en_lower, en_upper, ru_lower, ru_upper)
│   │   └── char_maps.py      # EN_TO_RU, RU_TO_EN dicts (derived from keycode_map)
│   ├── detection/
│   │   ├── __init__.py
│   │   ├── ngram_detector.py # NGramDetector: bigram log-probability scoring
│   │   ├── wrong_layout.py   # WrongLayoutDetector: should_convert(word, layout) → bool
│   │   └── profiles/
│   │       ├── en_bigrams.json
│   │       └── ru_bigrams.json
│   ├── layout/
│   │   ├── __init__.py
│   │   ├── backends.py       # KDELayoutBackend, GNOMELayoutBackend
│   │   └── detector.py       # detect_layout_backend() → LayoutBackend
│   └── injection/
│       ├── __init__.py
│       └── text_injector.py  # inject_text(str) via ydotool; inject_keys() via UInput
├── data/
│   ├── en_corpus.txt
│   └── ru_corpus.txt
├── systemd/
│   └── power_switcher.service
├── udev/
│   └── 60-power_switcher.rules
└── tests/
    ├── test_char_maps.py
    ├── test_detector.py
    └── test_key_buffer.py
```

---

## PHASE 0: SYSTEM SETUP (run these shell commands first)

```bash
# Install system packages
sudo dnf install -y python3-evdev python3-dasbus wl-clipboard ydotool g3kb-switch

# udev rules for /dev/uinput and /dev/input/event*
sudo tee /etc/udev/rules.d/60-power_switcher.rules << 'EOF'
KERNEL=="uinput", SUBSYSTEM=="misc", GROUP="input", MODE="0660", TAG+="uaccess", OPTIONS+="static_node=uinput"
KERNEL=="event*", SUBSYSTEM=="input", ENV{ID_INPUT_KEYBOARD}=="1", GROUP="input", MODE="0660", TAG+="uaccess"
KERNEL=="event*", SUBSYSTEM=="input", ENV{ID_INPUT_KEYBOARD}=="1", ENV{ID_SEAT}=="", ENV{ID_SEAT}="seat0"
EOF
sudo udevadm control --reload-rules
sudo udevadm trigger

# Enable ydotool daemon
systemctl --user enable --now ydotool

# Verify all components work
echo "=== GNOME g3kb-switch test ==="
g3kb-switch -l    # list layouts
g3kb-switch -p    # print current layout

echo "=== UInput test ==="
python3 -c "from evdev import UInput; ui = UInput(); print('OK:', ui.name); ui.close()"

echo "=== Clipboard test ==="
echo "hello" | wl-copy && wl-paste --no-newline

echo "=== ydotool test (opens a text editor first!) ==="
ydotool type "test"
```

**If g3kb-switch fails:** GNOME extension not enabled. Install and enable `g3kb-switch@g3kb-switch.org` from https://extensions.gnome.org/extension/4145/g3kb-switch/ then retry.  
**If UInput fails:** User not in `input` group. Re-login and retry.  
**If SELinux blocks:** Run `sudo ausearch -m avc -ts recent | sudo audit2allow -M power_switcher && sudo semodule -i power_switcher.pp`

---

## PHASE 1: CHARACTER MAPS

**File: `power_switcher/maps/keycode_map.py`**

Build the master table mapping evdev keycodes to characters in each layout.

```python
from evdev import ecodes as e

# Master table: keycode → (en_lower, en_upper, ru_lower, ru_upper)
# Source: standard QWERTY ↔ ЙЦУКЕН mapping
# Reference: /usr/share/X11/xkb/symbols/ru
KEYCODE_CHARS: dict[int, tuple[str, str, str, str]] = {
    e.KEY_Q:          ('q', 'Q', 'й', 'Й'),
    e.KEY_W:          ('w', 'W', 'ц', 'Ц'),
    e.KEY_E:          ('e', 'E', 'у', 'У'),
    e.KEY_R:          ('r', 'R', 'к', 'К'),
    e.KEY_T:          ('t', 'T', 'е', 'Е'),
    e.KEY_Y:          ('y', 'Y', 'н', 'Н'),
    e.KEY_U:          ('u', 'U', 'г', 'Г'),
    e.KEY_I:          ('i', 'I', 'ш', 'Ш'),
    e.KEY_O:          ('o', 'O', 'щ', 'Щ'),
    e.KEY_P:          ('p', 'P', 'з', 'З'),
    e.KEY_LEFTBRACE:  ('[', '{', 'х', 'Х'),
    e.KEY_RIGHTBRACE: (']', '}', 'ъ', 'Ъ'),
    e.KEY_A:          ('a', 'A', 'ф', 'Ф'),
    e.KEY_S:          ('s', 'S', 'ы', 'Ы'),
    e.KEY_D:          ('d', 'D', 'в', 'В'),
    e.KEY_F:          ('f', 'F', 'а', 'А'),
    e.KEY_G:          ('g', 'G', 'п', 'П'),
    e.KEY_H:          ('h', 'H', 'р', 'Р'),
    e.KEY_J:          ('j', 'J', 'о', 'О'),
    e.KEY_K:          ('k', 'K', 'л', 'Л'),
    e.KEY_L:          ('l', 'L', 'д', 'Д'),
    e.KEY_SEMICOLON:  (';', ':', 'ж', 'Ж'),
    e.KEY_APOSTROPHE: ("'", '"', 'э', 'Э'),
    e.KEY_Z:          ('z', 'Z', 'я', 'Я'),
    e.KEY_X:          ('x', 'X', 'ч', 'Ч'),
    e.KEY_C:          ('c', 'C', 'с', 'С'),
    e.KEY_V:          ('v', 'V', 'м', 'М'),
    e.KEY_B:          ('b', 'B', 'и', 'И'),
    e.KEY_N:          ('n', 'N', 'т', 'Т'),
    e.KEY_M:          ('m', 'M', 'ь', 'Ь'),
    e.KEY_COMMA:      (',', '<', 'б', 'Б'),
    e.KEY_DOT:        ('.', '>', 'ю', 'Ю'),
    e.KEY_SLASH:      ('/', '?', '.', ','),
    e.KEY_GRAVE:      ('`', '~', 'ё', 'Ё'),
    e.KEY_1:          ('1', '!', '1', '!'),
    e.KEY_2:          ('2', '@', '2', '"'),
    e.KEY_3:          ('3', '#', '3', '№'),
    e.KEY_4:          ('4', '$', '4', ';'),
    e.KEY_5:          ('5', '%', '5', '%'),
    e.KEY_6:          ('6', '^', '6', ':'),
    e.KEY_7:          ('7', '&', '7', '?'),
    e.KEY_8:          ('8', '*', '8', '*'),
    e.KEY_9:          ('9', '(', '9', '('),
    e.KEY_0:          ('0', ')', '0', ')'),
    e.KEY_MINUS:      ('-', '_', '-', '_'),
    e.KEY_EQUAL:      ('=', '+', '=', '+'),
}

def keycode_to_char(keycode: int, layout: str, shifted: bool = False) -> str:
    if keycode not in KEYCODE_CHARS:
        return ""
    en_lo, en_hi, ru_lo, ru_hi = KEYCODE_CHARS[keycode]
    if layout == "en":
        return en_hi if shifted else en_lo
    elif layout == "ru":
        return ru_hi if shifted else ru_lo
    return ""

# Build conversion dicts from the table
EN_TO_RU: dict[str, str] = {}
RU_TO_EN: dict[str, str] = {}
for _kc, (_en_lo, _en_hi, _ru_lo, _ru_hi) in KEYCODE_CHARS.items():
    EN_TO_RU[_en_lo] = _ru_lo
    EN_TO_RU[_en_hi] = _ru_hi
    RU_TO_EN[_ru_lo] = _en_lo
    RU_TO_EN[_ru_hi] = _en_hi
```

**Test immediately:**
```python
assert EN_TO_RU['g'] == 'п'
assert EN_TO_RU['h'] == 'р'
assert "".join(EN_TO_RU.get(c, c) for c in "ghbdtn") == "привет"
assert "".join(RU_TO_EN.get(c, c) for c in "привет") == "ghbdtn"
```

---

## PHASE 2: N-GRAM LANGUAGE DETECTOR

### 2a. Build bigram profiles

**File: `power_switcher/detection/build_profiles.py`**

```python
"""
Run once to generate bigram frequency profiles.
Usage: python -m power_switcher.detection.build_profiles
Outputs: power_switcher/detection/profiles/en_bigrams.json
         power_switcher/detection/profiles/ru_bigrams.json
"""
import json, re, math
from collections import Counter
from pathlib import Path

def build_bigram_profile(text: str) -> dict[str, float]:
    text = text.lower()
    bigrams = Counter()
    total = 0
    for i in range(len(text) - 1):
        a, b = text[i], text[i+1]
        if a.isalpha() and b.isalpha():
            bigrams[a + b] += 1
            total += 1
    if total == 0:
        return {}
    return {bg: count / total for bg, count in bigrams.items()}

def main():
    out_dir = Path(__file__).parent / "profiles"
    out_dir.mkdir(exist_ok=True)
    data_dir = Path(__file__).parents[3] / "data"
    
    for lang, fname in [("en", "en_corpus.txt"), ("ru", "ru_corpus.txt")]:
        corpus_path = data_dir / fname
        if not corpus_path.exists():
            print(f"WARNING: {corpus_path} not found, skipping {lang}")
            continue
        text = corpus_path.read_text(encoding="utf-8", errors="ignore")
        profile = build_bigram_profile(text)
        out_path = out_dir / f"{lang}_bigrams.json"
        with open(out_path, "w", encoding="utf-8") as f:
            json.dump(profile, f, ensure_ascii=False, indent=2)
        print(f"Built {lang} profile: {len(profile)} bigrams → {out_path}")

if __name__ == "__main__":
    main()
```

**Get corpus data:**
```bash
# EN corpus (~1MB is enough)
curl -o data/en_corpus.txt https://www.norvig.com/big.txt

# RU corpus — use any large Russian text file
# Options:
# 1. Download from OpenCorpora: http://opencorpora.org/files/export/annot/
# 2. Use Wikipedia dump extract
# 3. Minimal hardcoded fallback (see below)
```

**Hardcoded minimal fallback** (use if no corpus available):

```python
# In build_profiles.py, add minimal profile as fallback:
EN_FALLBACK = {
    "th":0.035,"he":0.030,"in":0.024,"er":0.021,"an":0.020,
    "re":0.018,"on":0.016,"en":0.016,"at":0.015,"es":0.015,
    "st":0.015,"or":0.014,"te":0.013,"of":0.012,"ed":0.012,
    "is":0.011,"it":0.011,"al":0.010,"ar":0.010,"ti":0.009,
    "to":0.009,"nt":0.009,"ha":0.008,"ng":0.008,"ea":0.008,
    "ou":0.008,"io":0.007,"le":0.007,"nd":0.007,"as":0.007,
    "gh":0.001,"hb":0.00002,"bd":0.00002,"dt":0.00002,"tn":0.00002,
}
RU_FALLBACK = {
    "пр":0.025,"ст":0.022,"ов":0.020,"ен":0.019,"то":0.018,
    "ни":0.017,"ри":0.016,"ко":0.015,"ра":0.015,"ве":0.014,
    "на":0.014,"ть":0.013,"по":0.013,"ет":0.012,"ли":0.012,
    "ед":0.011,"ин":0.011,"ал":0.010,"ар":0.010,"ти":0.009,
    "но":0.009,"нт":0.009,"ха":0.001,"гп":0.00002,"пж":0.00002,
}
```

### 2b. Implement the detector

**File: `power_switcher/detection/ngram_detector.py`**

```python
import math, json
from pathlib import Path
from typing import Optional

class NGramDetector:
    def __init__(self, smoothing: float = 1e-9):
        self._profiles: dict[str, dict[str, float]] = {}
        self._smoothing = smoothing

    def load_profile(self, lang: str, path: Path):
        with open(path, encoding="utf-8") as f:
            self._profiles[lang] = json.load(f)

    def load_profile_from_dict(self, lang: str, bigrams: dict[str, float]):
        self._profiles[lang] = bigrams

    def score(self, word: str, lang: str) -> float:
        """Log-probability per bigram. Higher = more likely this language."""
        if lang not in self._profiles or len(word) < 2:
            return -math.inf
        profile = self._profiles[lang]
        word = word.lower()
        scores = []
        for i in range(len(word) - 1):
            bigram = word[i:i+2]
            freq = profile.get(bigram, self._smoothing)
            scores.append(math.log(freq))
        return sum(scores) / len(scores)  # normalize by length

    def best_language(self, word: str, languages: list[str]) -> Optional[str]:
        if len(word) < 2:
            return None
        scored = {lang: self.score(word, lang) for lang in languages}
        return max(scored, key=scored.get)
```

**File: `power_switcher/detection/wrong_layout.py`**

```python
from .ngram_detector import NGramDetector

class WrongLayoutDetector:
    def __init__(
        self,
        ngram_detector: NGramDetector,
        char_maps: dict,          # {"en_to_ru": {...}, "ru_to_en": {...}}
        threshold: float = 2.5,  # score gap required to trigger
        min_word_len: int = 3,
    ):
        self._ngram = ngram_detector
        self._char_maps = char_maps
        self.threshold = threshold
        self.min_word_len = min_word_len

    def should_convert(self, word: str, current_layout: str, other_layout: str) -> bool:
        """
        Returns True if word was likely typed in the wrong layout.
        Logic:
          1. Score word as-is (interpreted in current_layout)
          2. Convert word to other_layout chars
          3. Score converted word in other_layout
          4. If (score_converted - score_current) > threshold → wrong layout
        """
        if len(word) < self.min_word_len:
            return False
        if any(c.isdigit() for c in word):
            return False

        score_current = self._ngram.score(word, current_layout)

        map_key = f"{current_layout}_to_{other_layout}"
        char_map = self._char_maps.get(map_key, {})
        converted = "".join(char_map.get(c, c) for c in word)

        score_converted = self._ngram.score(converted, other_layout)

        return (score_converted - score_current) > self.threshold
```

---

## PHASE 3: LAYOUT BACKENDS

**File: `power_switcher/layout/backends.py`**

Implement these three backends. Each must satisfy `LayoutBackend` ABC.

```python
import os, subprocess, logging, re
from abc import ABC, abstractmethod

logger = logging.getLogger(__name__)

class LayoutBackend(ABC):
    @abstractmethod
    def get_current_index(self) -> int: ...

    @abstractmethod
    def get_layout_count(self) -> int: ...

    @abstractmethod
    def set_layout(self, index: int) -> None: ...

    def switch_next(self) -> None:
        count = self.get_layout_count()
        if count >= 2:
            self.set_layout((self.get_current_index() + 1) % count)


class KDELayoutBackend(LayoutBackend):
    """
    KDE Plasma 5.12+ on Wayland only.
    Uses org.kde.keyboard D-Bus interface (no extension needed).
    Verified working: KDE Plasma 6+, Fedora 40+. Secondary target on this system.
    """
    _DEST  = "org.kde.keyboard"
    _PATH  = "/Layouts"
    _IFACE = "org.kde.KeyboardLayouts"

    def _gdbus(self, method: str, *args) -> str:
        cmd = ["gdbus", "call", "--session",
               "--dest", self._DEST, "--object-path", self._PATH,
               "--method", f"{self._IFACE}.{method}", *[str(a) for a in args]]
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=2)
        return r.stdout.strip()

    def get_current_index(self) -> int:
        raw = self._gdbus("getLayout")
        m = re.search(r'\d+', raw)
        return int(m.group()) if m else 0

    def get_layout_count(self) -> int:
        raw = self._gdbus("getLayoutsList")
        return len(re.findall(r"'shortName'", raw))

    def set_layout(self, index: int) -> None:
        self._gdbus("setLayout", index)

    @classmethod
    def is_available(cls) -> bool:
        try:
            r = subprocess.run(
                ["gdbus", "introspect", "--session",
                 "--dest", cls._DEST, "--object-path", cls._PATH],
                capture_output=True, timeout=2)
            return r.returncode == 0
        except Exception:
            return False


class GNOMELayoutBackend(LayoutBackend):
    """
    GNOME Shell with g3kb-switch installed.
    PRIMARY backend on Fedora 44 / GNOME 50.1 / Mutter (Wayland).
    Install: sudo dnf install g3kb-switch
    Also requires g3kb-switch GNOME extension: https://extensions.gnome.org/extension/4145/g3kb-switch/
    Tested on: Fedora 44, GNOME 50.1, Wayland/Mutter.
    """
    def _g3(self, *args) -> str:
        r = subprocess.run(["g3kb-switch", *args],
                           capture_output=True, text=True, timeout=2)
        return r.stdout.strip()

    def get_current_index(self) -> int:
        layouts = self._g3("-l").splitlines()
        current = self._g3("-p")
        try:
            return layouts.index(current)
        except ValueError:
            return 0

    def get_layout_count(self) -> int:
        return len(self._g3("-l").splitlines())

    def set_layout(self, index: int) -> None:
        layouts = self._g3("-l").splitlines()
        if index < len(layouts):
            self._g3("-s", layouts[index])

    def switch_next(self) -> None:
        self._g3("-n")

    @classmethod
    def is_available(cls) -> bool:
        try:
            return subprocess.run(
                ["g3kb-switch", "-p"], capture_output=True, timeout=2
            ).returncode == 0
        except FileNotFoundError:
            return False


```

**File: `power_switcher/layout/detector.py`**

```python
import os, logging
from .backends import KDELayoutBackend, GNOMELayoutBackend, LayoutBackend

logger = logging.getLogger(__name__)

def detect_layout_backend() -> LayoutBackend:
    """
    Auto-detect DE and return the best available layout backend.
    Primary target: GNOME 50.1 / Fedora 44 / Mutter (Wayland).
    """
    desktop = os.environ.get("XDG_CURRENT_DESKTOP", "").upper()
    session = os.environ.get("XDG_SESSION_TYPE", "").lower()
    logger.info(f"XDG_CURRENT_DESKTOP={desktop} XDG_SESSION_TYPE={session}")

    if session and session != "wayland":
        raise RuntimeError(
            f"power_switcher requires a Wayland session. "
            f"XDG_SESSION_TYPE={session!r}"
        )

    # GNOME / Ubuntu / Pop!_OS — PRIMARY target (Fedora 44 / GNOME 50.1)
    if any(x in desktop for x in ("GNOME", "UBUNTU", "UNITY", "POP")):
        if GNOMELayoutBackend.is_available():
            logger.info("Backend: GNOME g3kb-switch (primary)")
            return GNOMELayoutBackend()
        raise RuntimeError(
            "GNOME detected but g3kb-switch is not available.\n"
            "Install: sudo dnf install g3kb-switch\n"
            "Then enable the g3kb-switch GNOME extension:\n"
            "  https://extensions.gnome.org/extension/4145/g3kb-switch/"
        )

    # KDE Plasma — secondary target
    if any(x in desktop for x in ("KDE", "PLASMA")):
        if KDELayoutBackend.is_available():
            logger.info("Backend: KDE D-Bus")
            return KDELayoutBackend()

    # Last-chance probes (non-standard XDG_CURRENT_DESKTOP values)
    if GNOMELayoutBackend.is_available():
        logger.info("Backend: GNOME g3kb-switch (auto-probe)")
        return GNOMELayoutBackend()

    if KDELayoutBackend.is_available():
        logger.info("Backend: KDE D-Bus (auto-probe)")
        return KDELayoutBackend()

    raise RuntimeError(
        "No supported layout backend found.\n"
        "Supported: KDE Plasma (Wayland), GNOME with g3kb-switch.\n"
        f"XDG_CURRENT_DESKTOP={os.environ.get('XDG_CURRENT_DESKTOP', '(unset)')}\n"
        "power_switcher requires a Wayland session."
    )
```

---

## PHASE 4: CORE EVENT LOOP

**File: `power_switcher/core/key_processor.py`**

This is the most critical file. Implement it completely.

### Key constants

```python
from evdev import ecodes as e

WORD_SEPARATORS = frozenset({
    e.KEY_SPACE, e.KEY_ENTER, e.KEY_KPENTER, e.KEY_TAB,
    e.KEY_COMMA, e.KEY_DOT, e.KEY_SEMICOLON,
})

BUFFER_RESET_KEYS = frozenset({
    e.KEY_ESC, e.KEY_ENTER, e.KEY_KPENTER,
    e.KEY_UP, e.KEY_DOWN, e.KEY_LEFT, e.KEY_RIGHT,
    e.KEY_HOME, e.KEY_END, e.KEY_PAGEUP, e.KEY_PAGEDOWN,
})

MODIFIER_KEYS = frozenset({
    e.KEY_LEFTSHIFT, e.KEY_RIGHTSHIFT,
    e.KEY_LEFTCTRL,  e.KEY_RIGHTCTRL,
    e.KEY_LEFTALT,   e.KEY_RIGHTALT,
    e.KEY_LEFTMETA,  e.KEY_RIGHTMETA,
})
```

### KeyBuffer class

```python
from dataclasses import dataclass, field

@dataclass
class KeyBuffer:
    word: list[tuple[int, str]] = field(default_factory=list)
    max_len: int = 50

    def add(self, keycode: int, char: str):
        if len(self.word) < self.max_len:
            self.word.append((keycode, char))

    def backspace(self):
        if self.word:
            self.word.pop()

    def flush(self) -> str:
        text = "".join(c for _, c in self.word)
        self.word.clear()
        return text

    def clear(self):
        self.word.clear()

    @property
    def text(self) -> str:
        return "".join(c for _, c in self.word)

    @property
    def keycodes(self) -> list[int]:
        return [kc for kc, _ in self.word]
```

### ModifierState class

```python
@dataclass
class ModifierState:
    shift: bool = False
    ctrl:  bool = False
    alt:   bool = False
    meta:  bool = False

    def update(self, keycode: int, pressed: bool):
        if keycode in (e.KEY_LEFTSHIFT, e.KEY_RIGHTSHIFT): self.shift = pressed
        elif keycode in (e.KEY_LEFTCTRL,  e.KEY_RIGHTCTRL):  self.ctrl  = pressed
        elif keycode in (e.KEY_LEFTALT,   e.KEY_RIGHTALT):   self.alt   = pressed
        elif keycode in (e.KEY_LEFTMETA,  e.KEY_RIGHTMETA):  self.meta  = pressed

    @property
    def is_plain(self) -> bool:
        """True if no modifiers except possibly Shift."""
        return not (self.ctrl or self.alt or self.meta)
```

### KeyProcessor class (full implementation)

```python
import asyncio, subprocess, time, logging
from evdev import InputDevice, UInput, ecodes as e, list_devices

logger = logging.getLogger(__name__)

class KeyProcessor:
    def __init__(
        self,
        layout_backend,
        detector,
        hotkey_convert: frozenset,
        hotkey_selection: frozenset,
        char_maps: dict,           # {"en_to_ru": {...}, "ru_to_en": {...}}
        current_layout: str = "en",
        auto_detect: bool = True,
    ):
        self.layout_backend = layout_backend
        self.detector = detector
        self.hotkey_convert = hotkey_convert
        self.hotkey_selection = hotkey_selection
        self.char_maps = char_maps
        self.auto_detect = auto_detect
        self._layout = current_layout   # internal layout tracking
        self._other_layout = "ru" if current_layout == "en" else "en"

        self._buf = KeyBuffer()
        self._mods = ModifierState()
        self._pressed: set[int] = set()
        self._injecting = False
        self._ui: UInput | None = None

        # Undo state
        self._undo_data: dict | None = None
        self._undo_ts: float = 0.0
        self.undo_window = 3.0

    # ── startup ──────────────────────────────────────────────────────────

    def _find_keyboards(self) -> list[InputDevice]:
        result = []
        for path in list_devices():
            try:
                dev = InputDevice(path)
                if e.EV_KEY in dev.capabilities() and e.KEY_A in dev.capabilities()[e.EV_KEY]:
                    result.append(dev)
            except Exception:
                pass
        return result

    async def run(self):
        keyboards = self._find_keyboards()
        if not keyboards:
            raise RuntimeError("No keyboard devices found")

        logger.info(f"Keyboards: {[k.name for k in keyboards]}")
        self._ui = UInput.from_device(*keyboards, name="power_switcher-virtual")
        await asyncio.sleep(0.15)  # let udev settle

        for kbd in keyboards:
            kbd.grab()
        logger.info("All keyboards grabbed")

        try:
            await asyncio.gather(*[self._read(kbd) for kbd in keyboards])
        finally:
            for kbd in keyboards:
                try: kbd.ungrab()
                except: pass
            if self._ui:
                self._ui.close()

    async def _read(self, dev: InputDevice):
        async for ev in dev.async_read_loop():
            await self._on_event(ev)

    # ── event handling ────────────────────────────────────────────────────

    async def _on_event(self, ev):
        # Non-key events: pass through unchanged
        if ev.type != e.EV_KEY:
            self._ui.write_event(ev)
            self._ui.syn()
            return

        # Key repeat (value=2): pass through, skip buffer logic
        if ev.value == 2:
            self._ui.write_event(ev)
            self._ui.syn()
            return

        kc = ev.code
        pressed = ev.value == 1

        # Track modifiers
        if kc in MODIFIER_KEYS:
            self._mods.update(kc, pressed)
            if pressed: self._pressed.add(kc)
            else:        self._pressed.discard(kc)
            self._ui.write_event(ev)
            self._ui.syn()
            return

        if pressed: self._pressed.add(kc)
        else:        self._pressed.discard(kc)

        # Check hotkeys on key-down only
        if pressed:
            combo = frozenset(self._pressed)
            if combo == self.hotkey_convert:
                await self._do_manual_convert()
                return          # consume — do NOT pass through
            if combo == self.hotkey_selection:
                await self._do_selection_convert()
                return

        # Pass through to compositor
        self._ui.write_event(ev)
        self._ui.syn()

        # Update buffer (key-down, not injecting)
        if pressed and not self._injecting:
            await self._update_buf(kc)

    # ── buffer management ─────────────────────────────────────────────────

    async def _update_buf(self, kc: int):
        if kc == e.KEY_BACKSPACE:
            self._buf.backspace()
            return

        if kc in BUFFER_RESET_KEYS:
            self._buf.clear()
            return

        if kc in WORD_SEPARATORS:
            word = self._buf.flush()
            if self.auto_detect and word:
                await self._maybe_auto(word)
            return

        # Ctrl/Alt combos (shortcuts): reset context
        if not self._mods.is_plain:
            self._buf.clear()
            return

        from ..maps.keycode_map import keycode_to_char
        char = keycode_to_char(kc, self._layout, self._mods.shift)
        if char:
            self._buf.add(kc, char)

    # ── auto-detection ────────────────────────────────────────────────────

    async def _maybe_auto(self, word: str):
        if self.detector.should_convert(word, self._layout, self._other_layout):
            logger.info(f"Auto-detect: converting '{word}'")
            await self._convert(word)

    # ── manual convert (hotkey) ───────────────────────────────────────────

    async def _do_manual_convert(self):
        # Undo if within window and previous conversion exists
        if self._undo_data and (time.time() - self._undo_ts) < self.undo_window:
            await self._undo()
            return

        word = self._buf.text
        if not word:
            logger.debug("Manual convert: buffer empty")
            return
        await self._convert(word)

    # ── selection convert (second hotkey) ─────────────────────────────────

    async def _do_selection_convert(self):
        saved = subprocess.run(
            ["wl-paste", "--no-newline"], capture_output=True, text=True
        ).stdout

        # Copy current selection
        self._raw_keys([e.KEY_LEFTCTRL, e.KEY_C])
        await asyncio.sleep(0.08)

        selected = subprocess.run(
            ["wl-paste", "--no-newline"], capture_output=True, text=True
        ).stdout

        if not selected or selected == saved:
            return

        map_key = f"{self._layout}_to_{self._other_layout}"
        char_map = self.char_maps.get(map_key, {})
        converted = "".join(char_map.get(c, c) for c in selected)

        self._inject_text(converted)
        self._toggle_layout()

        await asyncio.sleep(0.05)
        subprocess.run(["wl-copy"], input=saved, text=True)

    # ── core conversion ───────────────────────────────────────────────────

    async def _convert(self, word: str):
        map_key = f"{self._layout}_to_{self._other_layout}"
        char_map = self.char_maps.get(map_key, {})
        converted = "".join(char_map.get(c, c) for c in word)

        if converted == word:
            return

        self._undo_data = {
            "original": word, "converted": converted, "layout": self._layout
        }
        self._undo_ts = time.time()

        self._injecting = True
        try:
            # Erase the original word
            for _ in range(len(word)):
                self._raw_key(e.KEY_BACKSPACE)
                await asyncio.sleep(0.004)

            # Type the converted text
            self._inject_text(converted)

            # Switch layout
            self._toggle_layout()
            self._buf.clear()
        finally:
            self._injecting = False

        logger.info(f"Converted: '{word}' → '{converted}' (now {self._layout})")

    async def _undo(self):
        if not self._undo_data:
            return
        converted = self._undo_data["converted"]
        original  = self._undo_data["original"]

        self._injecting = True
        try:
            for _ in range(len(converted)):
                self._raw_key(e.KEY_BACKSPACE)
                await asyncio.sleep(0.004)
            self._inject_text(original)
            self._layout, self._other_layout = (
                self._undo_data["layout"],
                "ru" if self._undo_data["layout"] == "en" else "en",
            )
            self.layout_backend.switch_next()
            self._buf.clear()
        finally:
            self._injecting = False

        self._undo_data = None
        logger.info(f"Undo: '{converted}' → '{original}'")

    # ── low-level injection helpers ───────────────────────────────────────

    def _raw_key(self, kc: int):
        """Single key press+release via UInput."""
        self._ui.write(e.EV_KEY, kc, 1); self._ui.syn()
        self._ui.write(e.EV_KEY, kc, 0); self._ui.syn()

    def _raw_keys(self, keys: list[int]):
        """Key combo via UInput (all down, then all up)."""
        for k in keys:
            self._ui.write(e.EV_KEY, k, 1)
        self._ui.syn()
        time.sleep(0.01)
        for k in reversed(keys):
            self._ui.write(e.EV_KEY, k, 0)
        self._ui.syn()

    def _inject_text(self, text: str):
        """
        Type unicode text via ydotool.
        ydotool handles Unicode independently of current keymap.
        This is intentional: after layout switch, we need to type in the
        new layout's characters, and ydotool does this correctly.
        """
        subprocess.run(
            ["ydotool", "type", "--delay", "5", "--", text],
            capture_output=True
        )

    def _toggle_layout(self):
        self.layout_backend.switch_next()
        self._layout, self._other_layout = self._other_layout, self._layout
```

---

## PHASE 5: ENTRY POINT AND CONFIG

**File: `power_switcher/config.py`**

```python
import tomllib, os
from dataclasses import dataclass, field
from pathlib import Path

@dataclass
class Config:
    hotkey_convert:   str   = "pause"
    hotkey_selection: str   = "alt+shift"
    default_layout:   str   = "en"
    other_layout:     str   = "ru"
    auto_detect:      bool  = True
    threshold:        float = 2.5
    min_word_len:     int   = 3
    undo_window:      float = 3.0

    @classmethod
    def load(cls, path: str) -> "Config":
        p = Path(path)
        if not p.exists():
            return cls()
        with open(p, "rb") as f:
            d = tomllib.load(f)
        c = cls()
        g = d.get("general", {})
        c.hotkey_convert   = g.get("hotkey_convert",   c.hotkey_convert)
        c.hotkey_selection = g.get("hotkey_selection", c.hotkey_selection)
        c.default_layout   = g.get("default_layout",   c.default_layout)
        c.auto_detect      = g.get("auto_detect",      c.auto_detect)
        det = d.get("detection", {})
        c.threshold    = det.get("threshold",     c.threshold)
        c.min_word_len = det.get("min_word_len",  c.min_word_len)
        return c
```

**File: `power_switcher/__main__.py`**

```python
import asyncio, click, logging, os
from pathlib import Path
from evdev import ecodes as e

from .config import Config
from .core.key_processor import KeyProcessor
from .detection.ngram_detector import NGramDetector
from .detection.wrong_layout import WrongLayoutDetector
from .layout.detector import detect_layout_backend
from .maps.keycode_map import EN_TO_RU, RU_TO_EN

_KEY_NAMES = {
    "alt": e.KEY_LEFTALT,   "lalt": e.KEY_LEFTALT,   "ralt": e.KEY_RIGHTALT,
    "ctrl": e.KEY_LEFTCTRL, "lctrl": e.KEY_LEFTCTRL, "rctrl": e.KEY_RIGHTCTRL,
    "shift": e.KEY_LEFTSHIFT,
    "meta": e.KEY_LEFTMETA, "super": e.KEY_LEFTMETA,
    "pause": e.KEY_PAUSE,
    "z": e.KEY_Z, "x": e.KEY_X, "c": e.KEY_C, "v": e.KEY_V,
    "f1": e.KEY_F1, "f2": e.KEY_F2, "f12": e.KEY_F12,
}

def parse_hotkey(s: str) -> frozenset:
    codes = set()
    for part in s.lower().split("+"):
        kc = _KEY_NAMES.get(part.strip())
        if kc:
            codes.add(kc)
    return frozenset(codes)


@click.group()
def cli():
    """PowerSwitcher — DE-independent layout auto-switcher for Linux Wayland"""

@cli.command()
@click.option("--config", default="~/.config/power_switcher/config.toml")
@click.option("-v", "--verbose", is_flag=True)
@click.option("--no-auto", is_flag=True, help="Disable auto-detect, hotkey only")
def daemon(config, verbose, no_auto):
    """Run power_switcher daemon (main mode)"""
    logging.basicConfig(
        level=logging.DEBUG if verbose else logging.INFO,
        format="%(asctime)s [%(name)s] %(levelname)s: %(message)s",
    )
    cfg = Config.load(os.path.expanduser(config))
    backend = detect_layout_backend()

    # Load n-gram profiles
    ngram = NGramDetector()
    profiles_dir = Path(__file__).parent / "detection" / "profiles"
    for lang, fname in [("en", "en_bigrams.json"), ("ru", "ru_bigrams.json")]:
        p = profiles_dir / fname
        if p.exists():
            ngram.load_profile(lang, p)
        else:
            click.echo(f"WARNING: {p} not found — auto-detect will be inaccurate")

    detector = WrongLayoutDetector(
        ngram_detector=ngram,
        char_maps={"en_to_ru": EN_TO_RU, "ru_to_en": RU_TO_EN},
        threshold=cfg.threshold,
        min_word_len=cfg.min_word_len,
    )

    processor = KeyProcessor(
        layout_backend=backend,
        detector=detector,
        hotkey_convert=parse_hotkey(cfg.hotkey_convert),
        hotkey_selection=parse_hotkey(cfg.hotkey_selection),
        char_maps={"en_to_ru": EN_TO_RU, "ru_to_en": RU_TO_EN},
        current_layout=cfg.default_layout,
        auto_detect=not no_auto,
    )

    click.echo(f"Backend:   {type(backend).__name__}")
    click.echo(f"Hotkeys:   convert={cfg.hotkey_convert}  selection={cfg.hotkey_selection}")
    click.echo(f"AutoDetect: {'OFF' if no_auto else 'ON'} (threshold={cfg.threshold})")

    asyncio.run(processor.run())


@cli.command()
def debug():
    """Print keyboard events without grabbing (for device discovery)"""
    import evdev
    devs = [evdev.InputDevice(p) for p in evdev.list_devices()]
    kbds = [d for d in devs if evdev.ecodes.EV_KEY in d.capabilities()]
    click.echo(f"Found keyboards: {[k.name for k in kbds]}")
    if not kbds:
        return
    click.echo(f"\nListening to: {kbds[0].name} (no grab)")
    for ev in kbds[0].read_loop():
        if ev.type == evdev.ecodes.EV_KEY and ev.value == 1:
            click.echo(f"  code={ev.code:4d}  name={evdev.ecodes.KEY[ev.code]}")


if __name__ == "__main__":
    cli()
```

**File: `pyproject.toml`**

```toml
[build-system]
requires = ["setuptools>=68"]
build-backend = "setuptools.backends.legacy:build"

[project]
name = "power_switcher"
version = "0.1.0"
requires-python = ">=3.11"
dependencies = [
    "evdev>=1.7.0",
    "dasbus>=1.7",
    "click>=8.1",
]

[project.optional-dependencies]
lingua = ["lingua-language-detector>=2.0"]
tray   = ["pystray>=0.19", "Pillow>=10.0"]

[project.scripts]
power_switcher = "power_switcher.__main__:cli"
```

---

## PHASE 6: SYSTEMD SERVICE

**File: `systemd/power_switcher.service`**

```ini
[Unit]
Description=PowerSwitcher keyboard layout auto-switcher
After=graphical-session.target
PartOf=graphical-session.target

[Service]
Type=simple
ExecStart=%h/.local/bin/power_switcher daemon
Restart=on-failure
RestartSec=3s
Environment=PYTHONUNBUFFERED=1
Environment=XDG_RUNTIME_DIR=/run/user/%U
Environment=DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/%U/bus

[Install]
WantedBy=graphical-session.target
```

```bash
# Install
mkdir -p ~/.config/systemd/user
cp systemd/power_switcher.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now power_switcher
systemctl --user status power_switcher
journalctl --user -u power_switcher -f
```

---

## PHASE 7: CONFIG FILE

**Create: `~/.config/power_switcher/config.toml`**

```toml
[general]
hotkey_convert   = "pause"        # Pause/Break: convert last word + switch layout
hotkey_selection = "alt+shift"    # convert selected text + switch layout
default_layout   = "en"
auto_detect      = true           # set false to disable auto-mode

[detection]
threshold     = 2.5   # higher = fewer false positives
min_word_len  = 3
```

---

## CRITICAL IMPLEMENTATION NOTES

### NOTE 1: Self-injection loop prevention
When `_convert()` runs, it injects Backspace keys and then text via ydotool. Those injected events would re-enter `_on_event()` through the UInput device. **Always set `self._injecting = True` before any injection and `False` in a `finally` block.** The `_update_buf()` method checks this flag and skips buffer updates while injecting.

### NOTE 2: Text injection — use ydotool, not UInput keycodes
After switching layout, the compositor interprets keycodes in the new layout. If you inject `KEY_G` after switching to RU, the compositor types `п` — correct! But for the general case (Unicode characters, mixed case), use `ydotool type -- "text"` which sends characters as Unicode regardless of current keymap. This is the reliable path.

### NOTE 3: xkb keycode offset
`evdev` keycodes and `xkb` keycodes differ by 8: `xkb_keycode = evdev_keycode + 8`. This matters if you ever use `libxkbcommon` directly.

### NOTE 4: UInput.from_device() is mandatory
Create UInput with `UInput.from_device(*keyboards)` so the virtual device inherits all capabilities of the physical keyboards. If you use `UInput()` with default capabilities, media keys, function keys, etc. will be swallowed and never reach the compositor.

### NOTE 5: Layout index tracking
The daemon tracks `_layout` ("en"/"ru") internally. This can diverge if the user switches layout via the compositor's own hotkey. For GNOME (primary target), poll `g3kb-switch -p` on a 500ms interval or connect to GNOME Shell's D-Bus signals to stay in sync. Implement this in a follow-up iteration.

### NOTE 6: ydotool socket
ydotool requires the daemon `ydotoold` to be running. Verify the socket exists: `ls /run/user/$UID/.ydotool_socket`. If missing, start it: `systemctl --user start ydotool`.

---

## QUICK SMOKE TEST

After implementing all phases, run this sequence:

```bash
# 1. Build profiles
python -m power_switcher.detection.build_profiles

# 2. Run in verbose mode (no grab yet, just debug)
python -m power_switcher debug
# → should list keyboards and print keycodes as you type

# 3. Test char maps
python3 -c "
from power_switcher.maps.keycode_map import EN_TO_RU
word = 'ghbdtn'
converted = ''.join(EN_TO_RU.get(c, c) for c in word)
assert converted == 'привет', f'Got: {converted}'
print('char_maps OK:', word, '→', converted)
"

# 4. Test layout backend
python3 -c "
from power_switcher.layout.detector import detect_layout_backend
b = detect_layout_backend()
print('Backend:', type(b).__name__)
print('Layout count:', b.get_layout_count())
print('Current index:', b.get_current_index())
"

# 5. Run daemon (requires input group membership + re-login)
python -m power_switcher daemon --verbose

# 6. Open a text editor, type "ghbdtn ", observe auto-convert
# 7. Type "ghbdtn" and press Alt+Pause, observe manual convert
# 8. Press Alt+Pause again within 3s, observe undo
```

---

## REFERENCES

| Resource | URL |
|---|---|
| python-evdev docs | https://python-evdev.readthedocs.io/ |
| evdev passthrough annotated example | https://gist.github.com/t184256/f4994037a2a204774ef3b9a2b38736dc |
| KDE KeyboardLayouts D-Bus (D4323) | https://phabricator.kde.org/D4323 |
| KDE Wayland keyboard internals | https://euroquis.nl/kde/2024/09/14/keyboard.html |
| Linux input-event-codes.h | https://github.com/torvalds/linux/blob/master/include/uapi/linux/input-event-codes.h |
| ydotool | https://github.com/ReimuNotMoe/ydotool |
| wl-clipboard | https://github.com/bugaevc/wl-clipboard |
| lingua-language-detector (Python) | https://github.com/pemistahl/lingua-py |
| xwaykeyz (Python evdev remapper, reference) | https://github.com/RedBearAK/xwaykeyz |
| kanata-switcher (DE backend patterns, Rust) | https://github.com/7mind/kanata-switcher |
| g3kb-switch (GNOME layout switch) | https://github.com/lyokha/g3kb-switch |
| EN corpus for profiles | https://www.norvig.com/big.txt |
| XKB symbols for RU layout (reference only) | `/usr/share/X11/xkb/symbols/ru` (local system file, not an X11 dependency) |
