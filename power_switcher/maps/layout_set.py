from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path

from .keycode_map import build_char_map, keycode_to_char, LAYOUTS
from ..detection.ngram_detector import NGramDetector

EN_BIGRAMS_FALLBACK = {
    "th": 0.035, "he": 0.030, "in": 0.024, "er": 0.021, "an": 0.020,
    "re": 0.018, "on": 0.016, "en": 0.016, "at": 0.015, "es": 0.015,
    "st": 0.015, "or": 0.014, "te": 0.013, "of": 0.012, "ed": 0.012,
    "is": 0.011, "it": 0.011, "al": 0.010, "ar": 0.010, "ti": 0.009,
    "to": 0.009, "nt": 0.009, "ha": 0.008, "ng": 0.008, "ea": 0.008,
    "ou": 0.008, "io": 0.007, "le": 0.007, "nd": 0.007, "as": 0.007,
}

RU_BIGRAMS_FALLBACK = {
    "пр": 0.025, "ст": 0.022, "ов": 0.020, "ен": 0.019, "то": 0.018,
    "ни": 0.017, "ри": 0.016, "ко": 0.015, "ра": 0.015, "ве": 0.014,
    "на": 0.014, "ть": 0.013, "по": 0.013, "ет": 0.012, "ли": 0.012,
}

UA_BIGRAMS_FALLBACK = {
    "пр": 0.025, "ст": 0.022, "ов": 0.020, "ен": 0.019, "то": 0.018,
    "ни": 0.017, "ри": 0.016, "ко": 0.015, "ра": 0.015, "ве": 0.014,
    "на": 0.014, "ть": 0.013, "по": 0.013, "ет": 0.012, "лі": 0.012,
}

DE_BIGRAMS_FALLBACK = {
    "er": 0.039, "en": 0.032, "ch": 0.028, "de": 0.024, "ei": 0.022,
    "in": 0.021, "te": 0.018, "nd": 0.017, "ie": 0.016, "ge": 0.015,
}

BIGRAMS_FALLBACK: dict[str, dict[str, float]] = {
    "en": EN_BIGRAMS_FALLBACK,
    "ru": RU_BIGRAMS_FALLBACK,
    "ua": UA_BIGRAMS_FALLBACK,
    "de": DE_BIGRAMS_FALLBACK,
}

@dataclass
class LayoutSet:
    """Handles 2 or more layouts for conversion and detection."""
    layouts: list[str]
    _ngram: NGramDetector = field(default_factory=NGramDetector)
    _char_maps: dict[tuple[str, str], dict[str, str]] = field(default_factory=dict)

    @classmethod
    def create(cls, layouts: list[str], profiles_dir: Path | None = None) -> LayoutSet:
        ls = cls(layouts=layouts)
        ls._load_ngrams(profiles_dir)
        return ls

    def _load_ngrams(self, profiles_dir: Path | None = None):
        if profiles_dir is None:
            profiles_dir = Path(__file__).parents[1] / "detection" / "profiles"
        for lang in self.layouts:
            p = profiles_dir / f"{lang}_bigrams.json"
            if p.exists():
                self._ngram.load_profile(lang, p)
            elif lang in BIGRAMS_FALLBACK:
                self._ngram.load_profile_from_dict(lang, BIGRAMS_FALLBACK[lang])

    def convert(self, word: str, from_layout: str, to_layout: str) -> str:
        if from_layout == to_layout:
            return word
        key = (from_layout, to_layout)
        if key not in self._char_maps:
            self._char_maps[key] = build_char_map(from_layout, to_layout)
        cmap = self._char_maps[key]
        return "".join(cmap.get(c, c) for c in word)

    def find_best_layout(self, word: str, current_layout: str, threshold: float = 2.5, min_word_len: int = 3) -> str | None:
        """Returns the layout that is most likely intended, if better than current by threshold."""
        if len(word) < min_word_len or any(c.isdigit() for c in word):
            return None
        
        score_current = self._ngram.score(word, current_layout)
        best_other = None
        max_gain = 0.0
        
        for other in self.layouts:
            if other == current_layout:
                continue
            converted = self.convert(word, current_layout, other)
            score_other = self._ngram.score(converted, other)
            gain = score_other - score_current
            if gain > threshold and gain > max_gain:
                max_gain = gain
                best_other = other
        
        return best_other

    def char_to_keycode(self, char: str, layout: str) -> tuple[int, bool] | None:
        from .keycode_map import char_to_keycode
        return char_to_keycode(char, layout)

    def keycode_to_char(self, keycode: int, layout: str, shifted: bool = False) -> str:
        return keycode_to_char(keycode, layout, shifted)
