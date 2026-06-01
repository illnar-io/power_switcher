import json
import math
from pathlib import Path
from typing import Optional


class NGramDetector:
    def __init__(self, smoothing: float = 1e-9):
        # We store n-grams of lengths 2, 3, 4, 5
        self._profiles: dict[str, dict[str, float]] = {}
        self._smoothing = smoothing

    def load_profile(self, lang: str, path: Path):
        with open(path, encoding="utf-8") as f:
            self._profiles[lang] = json.load(f)

    def load_profile_from_dict(self, lang: str, ngrams: dict[str, float]):
        self._profiles[lang] = ngrams

    def score(self, word: str, lang: str) -> float:
        """
        Caramba-inspired weighted scoring.
        Calculates log-probability sum for all n-grams (len 2-5).
        Larger n-grams have more weight implicitly (they are rarer).
        """
        if lang not in self._profiles or len(word) < 2:
            return -math.inf
            
        profile = self._profiles[lang]
        word = word.lower()
        total_score = 0.0
        count = 0
        
        # Check all n-gram lengths from 2 to 5
        for n in range(2, 6):
            if len(word) < n:
                break
            
            # Weight for this n-gram length (Caramba clone style)
            weight = n - 1 
            
            for i in range(len(word) - n + 1):
                ngram = word[i:i + n]
                freq = profile.get(ngram, self._smoothing)
                total_score += math.log(freq) * weight
                count += 1
                
        return total_score / count if count > 0 else -math.inf

    def best_language(self, word: str, languages: list[str]) -> Optional[str]:
        if len(word) < 2:
            return None
        scored = {lang: self.score(word, lang) for lang in languages}
        return max(scored, key=scored.get)
