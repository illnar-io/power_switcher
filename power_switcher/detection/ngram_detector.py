"""
Character n-gram language model used for wrong-layout detection.

A word is scored against each candidate language by summing weighted
log-probabilities of its character n-grams (n = 2..5).  Wrong-layout words
contain n-grams that are rare or absent in the *current* language but common in
the language produced by re-mapping the physical keys.

The profile schema written by :mod:`power_switcher.detection.build_profiles`
is::

    {"version": 2, "grams": {"th": 0.03, ...}, "words": {"the": 0.012, ...}}

A legacy flat ``{gram: prob}`` profile is still accepted.
"""

from __future__ import annotations

import gzip
import json
import math
from pathlib import Path
from typing import Optional

# Weight applied to each n-gram length.  Longer n-grams are more discriminative
# (an impossible 5-gram is much stronger evidence than an impossible 2-gram), so
# they receive a larger weight.
_NGRAM_WEIGHTS: dict[int, float] = {2: 1.0, 3: 1.6, 4: 2.2, 5: 2.8}

# Probability floor for a gram that is absent from a language profile.
_UNKNOWN_PROB = 1e-7

# Word-boundary marker appended/prepended when extracting n-grams, matching the
# builder so profile and query grams line up.
_BOUND = "^$"


class NGramDetector:
    def __init__(self, max_n: int = 5, unknown_prob: float = _UNKNOWN_PROB):
        self.max_n = max_n
        self.unknown_prob = unknown_prob
        self._log_unknown = math.log(unknown_prob)
        self._grams: dict[str, dict[str, float]] = {}
        self._words: dict[str, dict[str, float]] = {}
        self._logwords: dict[str, dict[str, float]] = {}

    # ── loading ─────────────────────────────────────────────────────────

    def load_profile(self, lang: str, path: Path) -> None:
        path = Path(path)
        if path.suffix == ".gz" or not path.exists() and path.with_suffix(
            path.suffix + ".gz"
        ).exists():
            opener = gzip.open
            if not path.exists():
                path = path.with_suffix(path.suffix + ".gz")
        else:
            opener = open
        with opener(path, "rt", encoding="utf-8") as fh:
            data = json.load(fh)
        if isinstance(data, dict) and "grams" in data:
            self._grams[lang] = data.get("grams", {})
            words = data.get("words", {})
            self._words[lang] = words
            self._logwords[lang] = {
                w: math.log(p) for w, p in words.items() if p > 0
            }
        else:
            # Legacy flat profile: {gram: probability}
            self._grams[lang] = data
            self._words[lang] = {}
            self._logwords[lang] = {}

    def load_profile_from_dict(self, lang: str, grams: dict[str, float]) -> None:
        self._grams[lang] = grams
        self._words.setdefault(lang, {})
        self._logwords.setdefault(lang, {})

    @property
    def languages(self) -> list[str]:
        return list(self._grams)

    def has_language(self, lang: str) -> bool:
        return lang in self._grams

    # ── scoring ─────────────────────────────────────────────────────────

    def _grams_of(self, word: str):
        padded = _BOUND[0] + word + _BOUND[1]
        for n in range(2, self.max_n + 1):
            if len(padded) < n:
                break
            weight = _NGRAM_WEIGHTS.get(n, n - 1.0)
            for i in range(len(padded) - n + 1):
                yield padded[i:i + n], weight

    def score(self, word: str, lang: str) -> float:
        """Return a per-character weighted log-probability of *word* in *lang*.

        Higher (closer to zero) is better.  Returns ``-inf`` when no profile is
        loaded for the language.
        """
        profile = self._grams.get(lang)
        if profile is None:
            return -math.inf
        word = word.lower()
        if len(word) < 2:
            return -math.inf

        total = 0.0
        weight_sum = 0.0
        for gram, weight in self._grams_of(word):
            p = profile.get(gram)
            total += (self._log_unknown if p is None else math.log(p)) * weight
            weight_sum += weight
        if weight_sum == 0.0:
            return -math.inf
        return total / weight_sum

    def word_logprob(self, word: str, lang: str) -> Optional[float]:
        """Log-probability of a whole word, or ``None`` if unknown."""
        return self._logwords.get(lang, {}).get(word.lower())

    def is_known_word(self, word: str, lang: str, min_logprob: float | None = None) -> bool:
        lp = self.word_logprob(word, lang)
        if lp is None:
            return False
        if min_logprob is not None and lp < min_logprob:
            return False
        return True

    def best_language(self, word: str, languages: list[str]) -> Optional[str]:
        if len(word) < 2:
            return None
        scored = {lang: self.score(word, lang) for lang in languages
                  if self.has_language(lang)}
        if not scored:
            return None
        return max(scored, key=scored.get)

    def ranked_languages(self, word: str, languages: list[str]) -> list[tuple[str, float]]:
        return sorted(
            ((lang, self.score(word, lang)) for lang in languages
             if self.has_language(lang)),
            key=lambda kv: kv[1],
            reverse=True,
        )
