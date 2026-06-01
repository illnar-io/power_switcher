"""
Run once to generate multi-gram frequency profiles.
Usage: python -m power_switcher.detection.build_profiles
"""
import json
from collections import Counter
from pathlib import Path

from ..maps.layout_set import BIGRAMS_FALLBACK

def build_ngram_profile(text: str, max_n: int = 5) -> dict[str, float]:
    text = text.lower()
    ngrams = Counter()
    total = Counter()
    
    for n in range(2, max_n + 1):
        for i in range(len(text) - n + 1):
            gram = text[i:i + n]
            # Only count if it's purely letters or spaces (no punctuation)
            if all(c.isalpha() or c.isspace() for c in gram):
                ngrams[gram] += 1
                total[n] += 1
                
    result = {}
    for gram, count in ngrams.items():
        n = len(gram)
        if total[n] > 0:
            result[gram] = count / total[n]
    return result

def main():
    # Correct path to the package root and then to data/
    root_dir = Path(__file__).parents[2]
    out_dir = Path(__file__).parent / "profiles"
    out_dir.mkdir(exist_ok=True)
    data_dir = root_dir / "data"

    print(f"Searching for corpora in: {data_dir}")

    for lang in ["en", "ru", "ua", "de"]:
        corpus_path = data_dir / f"{lang}_corpus.txt"
        if corpus_path.exists():
            text = corpus_path.read_text(encoding="utf-8", errors="ignore")
            profile = build_ngram_profile(text)
            print(f"Built {lang} profile from corpus: {len(profile)} grams")
        else:
            print(f"WARNING: {corpus_path} not found, using fallback")
            profile = BIGRAMS_FALLBACK.get(lang, {})

        out_path = out_dir / f"{lang}_ngrams.json"
        with open(out_path, "w", encoding="utf-8") as f:
            json.dump(profile, f, ensure_ascii=False, indent=2)

if __name__ == "__main__":
    main()
