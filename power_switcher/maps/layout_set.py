from __future__ import annotations

import json
import logging
from dataclasses import dataclass, field
from pathlib import Path

from .keycode_map import build_char_map, keycode_to_char, LAYOUTS
from ..detection.ngram_detector import NGramDetector

logger = logging.getLogger(__name__)

EN_BIGRAMS_FALLBACK = {
    "th": 0.035, "he": 0.030, "in": 0.024, "er": 0.021, "an": 0.020,
    "re": 0.018, "on": 0.016, "en": 0.016, "at": 0.015, "es": 0.015,
    "st": 0.015, "or": 0.014, "te": 0.013, "of": 0.012, "ed": 0.012,
}

BIGRAMS_FALLBACK: dict[str, dict[str, float]] = {
    "en": EN_BIGRAMS_FALLBACK,
    "ru": {},
}

@dataclass
class LayoutSet:
    layouts: list[str]
    _ngram: NGramDetector = field(default_factory=NGramDetector)
    _char_maps: dict[tuple[str, str], dict[str, str]] = field(default_factory=dict)
    _exceptions: set[str] = field(default_factory=set)
    _exceptions_path: Path | None = None

    @classmethod
    def create(cls, layouts: list[str], profiles_dir: Path | None = None) -> LayoutSet:
        ls = cls(layouts=layouts)
        ls._load_ngrams(profiles_dir)
        ls._load_exceptions()
        return ls

    def _load_ngrams(self, profiles_dir: Path | None = None):
        if profiles_dir is None:
            profiles_dir = Path(__file__).parents[1] / "detection" / "profiles"
        for lang in self.layouts:
            p = profiles_dir / f"{lang}_ngrams.json"
            if p.exists():
                self._ngram.load_profile(lang, p)
            elif lang in BIGRAMS_FALLBACK:
                self._ngram.load_profile_from_dict(lang, BIGRAMS_FALLBACK[lang])

    def _load_exceptions(self):
        self._exceptions_path = Path("~/.config/power_switcher/exceptions.json").expanduser()
        if self._exceptions_path.exists():
            try:
                self._exceptions = set(json.loads(self._exceptions_path.read_text()))
            except Exception: pass

    def add_exception(self, word: str):
        if len(word) < 2: return
        self._exceptions.add(word.lower())
        if self._exceptions_path:
            self._exceptions_path.parent.mkdir(parents=True, exist_ok=True)
            self._exceptions_path.write_text(json.dumps(list(self._exceptions), indent=2))

    def is_password(self, word: str) -> bool:
        if len(word) <= 6: return False
        has_upper = any(c.isupper() for c in word)
        has_lower = any(c.islower() for c in word)
        has_digit = any(c.isdigit() for c in word)
        has_special = any(not c.isalnum() for c in word)
        return sum([has_upper, has_lower, has_digit, has_special]) >= 3

    def convert(self, word: str, from_layout: str, to_layout: str) -> str:
        if from_layout == to_layout: return word
        key = (from_layout, to_layout)
        if key not in self._char_maps:
            self._char_maps[key] = build_char_map(from_layout, to_layout)
        cmap = self._char_maps[key]
        return "".join(cmap.get(c, c) for c in word)

    def find_best_layout(self, word: str, current_layout: str, threshold: float = 2.5, min_word_len: int = 3) -> str | None:
        if word.lower() in self._exceptions: return None
        if len(word) < min_word_len: return None
        if word.isupper() and len(word) > 1: return None
            
        score_current = self._ngram.score(word, current_layout)
        best_other = None
        max_gain = 0.0
        
        # LOG SCORES FOR AUDIT
        audit_msg = f"AUDIT SCORE: word='{word}' {current_layout}={score_current:.2f}"
        
        for other in self.layouts:
            if other == current_layout: continue
            converted = self.convert(word, current_layout, other)
            score_other = self._ngram.score(converted, other)
            gain = score_other - score_current
            audit_msg += f" | {other}={score_other:.2f} (gain={gain:.2f})"
            if gain > threshold and gain > max_gain:
                max_gain = gain
                best_other = other
        
        import logging
        logging.getLogger("audit").debug(audit_msg)
        return best_other

    def char_to_keycode(self, char: str, layout: str) -> tuple[int, bool] | None:
        from .keycode_map import char_to_keycode
        return char_to_keycode(char, layout)

    def keycode_to_char(self, keycode: int, layout: str, shifted: bool = False) -> str:
        return keycode_to_char(keycode, layout, shifted)
