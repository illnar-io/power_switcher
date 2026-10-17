from __future__ import annotations

import json
import logging
import math
from dataclasses import dataclass, field
from pathlib import Path

from .keycode_map import build_char_map, keycode_to_char, LAYOUTS
from ..detection.ngram_detector import NGramDetector

logger = logging.getLogger(__name__)

# Fallback bigram profile used when no generated profile exists for a language.
EN_BIGRAMS_FALLBACK = {
    "th": 0.035, "he": 0.030, "in": 0.024, "er": 0.021, "an": 0.020,
    "re": 0.018, "on": 0.016, "en": 0.016, "at": 0.015, "es": 0.015,
    "st": 0.015, "or": 0.014, "te": 0.013, "of": 0.012, "ed": 0.012,
}

BIGRAMS_FALLBACK: dict[str, dict[str, float]] = {
    "en": EN_BIGRAMS_FALLBACK,
}

# Vocabulary bonus applied when a word is present in a language's word list.
# Log-frequencies are negative (roughly -9 for the most common word down to
# -13 for the least common stored word), so the term is anchored to a floor to
# stay positive and to grow with word frequency.
_WORD_BONUS = 2.5
_WORD_FREQ_ALPHA = 0.5
_WORD_LOGPROB_FLOOR = -13.0


@dataclass
class LayoutSet:
    layouts: list[str]
    _ngram: NGramDetector = field(default_factory=NGramDetector)
    _char_maps: dict[tuple[str, str], dict[str, str]] = field(default_factory=dict)
    _exceptions: set[str] = field(default_factory=set)
    _exceptions_path: Path | None = None
    # Words the user taught us never to convert ("learned words").
    _learned: set[str] = field(default_factory=set)

    @classmethod
    def create(
        cls,
        layouts: list[str],
        profiles_dir: Path | None = None,
        words_path: Path | None = None,
    ) -> LayoutSet:
        ls = cls(layouts=layouts)
        ls._load_ngrams(profiles_dir)
        ls._load_exceptions(words_path)
        return ls

    # ── profiles ────────────────────────────────────────────────────────

    def _load_ngrams(self, profiles_dir: Path | None = None):
        if profiles_dir is None:
            profiles_dir = Path(__file__).parents[1] / "detection" / "profiles"
        for lang in self.layouts:
            loaded = False
            for name in (f"{lang}_ngrams.json.gz", f"{lang}_ngrams.json"):
                p = profiles_dir / name
                if p.exists():
                    try:
                        self._ngram.load_profile(lang, p)
                        loaded = True
                    except Exception as exc:  # corrupt profile should not crash
                        logger.warning("Failed to load profile %s: %s", p, exc)
                    break
            if not loaded and lang in BIGRAMS_FALLBACK:
                self._ngram.load_profile_from_dict(lang, BIGRAMS_FALLBACK[lang])
            elif not loaded:
                logger.warning("No n-gram profile for layout %r", lang)

    # ── exceptions / learned words ──────────────────────────────────────

    def _load_exceptions(self, path: Path | None = None):
        # Word-level state lives in its own file so it never collides with the
        # per-application ``exceptions.json`` written by ExceptionStore.
        self._exceptions_path = path or Path(
            "~/.config/power_switcher/words.json"
        ).expanduser()
        if self._exceptions_path.exists():
            try:
                data = json.loads(self._exceptions_path.read_text(encoding="utf-8"))
                if isinstance(data, dict):
                    self._learned = set(data.get("learned_words", []))
                    self._exceptions = set(data.get("exceptions", []))
                elif isinstance(data, list):
                    self._exceptions = set(data)
            except Exception:
                pass

    def add_exception(self, word: str):
        if len(word) < 2:
            return
        self._exceptions.add(word.lower())
        self._save_state()

    def remove_exception(self, word: str) -> bool:
        w = word.lower()
        if w in self._exceptions:
            self._exceptions.discard(w)
            self._save_state()
            return True
        return False

    def add_learned(self, word: str):
        if len(word) < 2:
            return
        self._learned.add(word.lower())
        self._save_state()

    def remove_learned(self, word: str) -> bool:
        w = word.lower()
        if w in self._learned:
            self._learned.discard(w)
            self._save_state()
            return True
        return False

    def exceptions(self) -> list[str]:
        return sorted(self._exceptions)

    def learned_words(self) -> list[str]:
        return sorted(self._learned)

    def is_exception(self, word: str) -> bool:
        w = word.lower()
        return w in self._exceptions or w in self._learned

    def _save_state(self):
        if not self._exceptions_path:
            return
        self._exceptions_path.parent.mkdir(parents=True, exist_ok=True)
        self._exceptions_path.write_text(
            json.dumps({
                "exceptions": sorted(self._exceptions),
                "learned_words": sorted(self._learned),
            }, ensure_ascii=False, indent=2),
            encoding="utf-8",
        )

    # ── password heuristic ──────────────────────────────────────────────

    def is_password(self, word: str) -> bool:
        if len(word) <= 6:
            return False
        has_upper = any(c.isupper() for c in word)
        has_lower = any(c.islower() for c in word)
        has_digit = any(c.isdigit() for c in word)
        has_special = any(not c.isalnum() for c in word)
        return sum([has_upper, has_lower, has_digit, has_special]) >= 3

    # ── conversion ──────────────────────────────────────────────────────

    def convert(self, word: str, from_layout: str, to_layout: str) -> str:
        if from_layout == to_layout:
            return word
        key = (from_layout, to_layout)
        if key not in self._char_maps:
            try:
                self._char_maps[key] = build_char_map(from_layout, to_layout)
            except ValueError:
                self._char_maps[key] = {}
        cmap = self._char_maps[key]
        return "".join(cmap.get(c, c) for c in word)

    # ── detection ───────────────────────────────────────────────────────

    def is_known_word(self, word: str, lang: str) -> bool:
        return self._ngram.is_known_word(word, lang)

    def _eligible(self, word: str, min_word_len: int) -> bool:
        if len(word) < min_word_len:
            return False
        # A word must contain at least one letter, but may also contain
        # punctuation/digits: when typing in the wrong layout the comma key
        # yields ',' which is really 'б', so "rf,jnf" is a valid candidate for
        # "работа".  Such words are only converted when the re-mapped form is a
        # real word (see find_best_layout), so "don't" / "v1.2" are safe.
        if not any(c.isalpha() for c in word):
            return False
        if word.isupper() and len(word) > 1:
            # Abbreviations typed in caps are never converted.
            return False
        return True

    def _word_term(self, word: str, lang: str) -> float:
        """Frequency-aware bonus for a whole word that is in the language's
        vocabulary.  Returns 0 for out-of-vocabulary words."""
        lp = self._ngram.word_logprob(word.lower(), lang)
        if lp is None:
            return 0.0
        return _WORD_BONUS + _WORD_FREQ_ALPHA * (lp - _WORD_LOGPROB_FLOOR)

    def decision_score(self, word: str, lang: str) -> float:
        """Composite score combining n-gram plausibility and word frequency."""
        return self._ngram.score(word, lang) + self._word_term(word, lang)

    def find_best_layout(
        self,
        word: str,
        current_layout: str,
        threshold: float = 2.5,
        min_word_len: int = 3,
    ) -> str | None:
        """Return the layout *word* should be re-typed in, or ``None``.

        The decision compares a composite score (n-gram plausibility plus a
        vocabulary-frequency bonus) of the typed word in the current layout
        against every other configured layout.  A known word in the current
        language is heavily favoured, so real words are rarely touched, while a
        converted form that is a known word of another language is strongly
        favoured, which recovers short words the n-grams alone miss.
        """
        if self.is_exception(word):
            return None
        if not self._eligible(word, min_word_len):
            return None

        current_score = self.decision_score(word, current_layout)
        current_known = self.is_known_word(word, current_layout)

        best = None
        best_gain = 0.0
        best_converted = ""
        for other in self.layouts:
            if other == current_layout or not self._ngram.has_language(other):
                continue
            converted = self.convert(word, current_layout, other)
            gain = self.decision_score(converted, other) - current_score
            if gain > best_gain:
                best_gain = gain
                best = other
                best_converted = converted

        if best is None:
            return None

        # When the typed word contains characters that are punctuation in the
        # current layout but letters in another one (',' is 'б', ';' is 'ж'),
        # only convert if the re-mapped form is an actual word of the target
        # language.  This keeps genuine punctuation words ("don't", "v1.2")
        # untouched while still recovering "rf,jnf" → "работа".
        if not word.isalpha():
            if self.is_known_word(best_converted, best) and best_gain >= max(
                threshold, 2.5
            ):
                logger.debug(
                    "detect %r (%s) -> %r (%s) gain=%.2f (punctuated)",
                    word, current_layout, best_converted, best, best_gain,
                )
                return best
            return None

        # When the typed word is itself a valid word of the current language we
        # require much stronger evidence before touching it: the user probably
        # meant what they typed.
        if current_known:
            required = max(threshold + 1.5, 3.5)
            target_known = self.is_known_word(best_converted, best)
            if not (target_known and best_gain >= required):
                return None
        elif best_gain < threshold:
            return None

        logger.debug(
            "detect %r (%s) -> %r (%s) gain=%.2f",
            word, current_layout, best_converted, best, best_gain,
        )
        return best

    def score_word(self, word: str, lang: str) -> float:
        return self._ngram.score(word, lang)

    # ── keycode helpers ─────────────────────────────────────────────────

    def char_to_keycode(self, char: str, layout: str) -> tuple[int, bool] | None:
        from .keycode_map import char_to_keycode
        return char_to_keycode(char, layout)

    def keycode_to_char(self, keycode: int, layout: str, shifted: bool = False) -> str:
        return keycode_to_char(keycode, layout, shifted)
