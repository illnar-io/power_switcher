"""
Run once to generate bigram frequency profiles.
Usage: python -m power_switcher.detection.build_profiles
Outputs: power_switcher/detection/profiles/<lang>_bigrams.json
"""
import json
from collections import Counter
from pathlib import Path

from ..maps.layout_pair import BIGRAMS_FALLBACK


def build_bigram_profile(text: str) -> dict[str, float]:
    text = text.lower()
    bigrams = Counter()
    total = 0
    for i in range(len(text) - 1):
        a, b = text[i], text[i + 1]
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

    for lang, fallback in sorted(BIGRAMS_FALLBACK.items()):
        corpus_path = data_dir / f"{lang}_corpus.txt"
        if corpus_path.exists():
            text = corpus_path.read_text(encoding="utf-8", errors="ignore")
            profile = build_bigram_profile(text)
        else:
            print(f"WARNING: {corpus_path} not found, using fallback for {lang}")
            profile = fallback

        out_path = out_dir / f"{lang}_bigrams.json"
        with open(out_path, "w", encoding="utf-8") as f:
            json.dump(profile, f, ensure_ascii=False, indent=2)
        print(f"Built {lang} profile: {len(profile)} bigrams → {out_path}")


if __name__ == "__main__":
    main()
