import json
import math
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
            bigram = word[i:i + 2]
            freq = profile.get(bigram, self._smoothing)
            scores.append(math.log(freq))
        return sum(scores) / len(scores)

    def best_language(self, word: str, languages: list[str]) -> Optional[str]:
        if len(word) < 2:
            return None
        scored = {lang: self.score(word, lang) for lang in languages}
        return max(scored, key=scored.get)
