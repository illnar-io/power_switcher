"""
Build character n-gram and word-frequency profiles used by the layout detector.

Data source
-----------
Word-frequency lists (``word<TAB>count`` or ``word count``) for each supported
language.  These lists were extracted from the reference application
``ref/Lang Switcher.app`` (``Contents/Resources/<lang>_frequency.txt``) and are
kept, gzipped, under ``data/frequency/`` so the repository is self-contained.

The builder turns those word lists into two compact artifacts per language:

``<lang>_ngrams.json``
    ``{"version": 2, "grams": {"<gram>": <probability>, ...},
       "words": {"<word>": <log probability>, ...}}``

    ``grams`` holds character n-grams for ``n`` in 2..5 with their weighted
    relative frequency.  ``words`` holds the most frequent whole words, used
    for the high-confidence word-level checks in :mod:`ngram_detector`.

Usage
-----
    python -m power_switcher.detection.build_profiles            # all languages
    python -m power_switcher.detection.build_profiles --lang en ru
"""

from __future__ import annotations

import argparse
import gzip
import json
import math
import sys
from collections import Counter
from pathlib import Path

# Layout codes we ship profiles for.  Keys are the codes used by
# ``power_switcher.maps.keycode_map`` (so ``ua`` for Ukrainian); values are the
# code of the source frequency list in the reference data.
LAYOUT_SOURCES = {
    "en": "en",
    "ru": "ru",
    "ua": "uk",   # Ukrainian layout maps to the reference "uk" list
    "de": "de",
    "fr": "fr",
    "pl": "pl",
    "it": "it",
    "es": "es",
}

DEFAULT_LANGS = list(LAYOUT_SOURCES)

# Maximum number of grams kept per n-gram length (0 == keep everything).
GRAM_CAPS: dict[int, int] = {2: 0, 3: 40_000, 4: 25_000, 5: 12_000}

# Number of most frequent words kept for word-level scoring.
WORD_CAP = 20_000

# Smallest probability a gram may receive (floor for unseen grams is applied
# in the detector, not here).
_PROB_EPS = 1e-12


def _open_text(path: Path):
    if path.suffix == ".gz":
        return gzip.open(path, "rt", encoding="utf-8", errors="ignore")
    return open(path, "r", encoding="utf-8", errors="ignore")


def load_frequency(path: Path) -> list[tuple[str, float]]:
    """Read a ``word count`` / ``word<TAB>count`` frequency list."""
    out: list[tuple[str, float]] = []
    with _open_text(path) as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            # Accept both whitespace and tab separators.  Some lists use a
            # trailing count, some do not.
            parts = line.rsplit(None, 1)
            if len(parts) == 2:
                word, count = parts
                try:
                    freq = float(count)
                except ValueError:
                    word, freq = line, 1.0
            else:
                word, freq = parts[0], 1.0
            word = word.strip().lower()
            if not word:
                continue
            out.append((word, freq))
    return out


def find_frequency_file(lang: str, search_dirs: list[Path]) -> Path | None:
    for d in search_dirs:
        for name in (f"{lang}_frequency.txt.gz", f"{lang}_frequency.txt"):
            p = d / name
            if p.exists():
                return p
    return None


def build_ngrams(
    words: list[tuple[str, float]],
    max_n: int = 5,
    caps: dict[int, int] | None = None,
) -> tuple[dict[str, float], dict[str, float]]:
    """Return ``(gram_probabilities, word_log_probabilities)``.

    Character n-grams are weighted by the frequency of the word they occur in,
    so that n-grams from common words dominate the profile.  Only alphabetic
    words contribute (digits/punctuation are ignored) because the detector only
    ever scores pure-letter words.
    """
    caps = caps or GRAM_CAPS
    counts: dict[int, Counter[str]] = {n: Counter() for n in range(1, max_n + 1)}
    word_counts: Counter[str] = Counter()
    total_weight = 0.0

    for word, freq in words:
        if not word.isalpha():
            # Keep the word for the word list only if it is a single token of
            # letters (hyphenated words still contribute their letter n-grams).
            cleaned = "".join(ch for ch in word if ch.isalpha())
            if not cleaned:
                continue
            letters = cleaned
        else:
            letters = word
        if len(letters) < 1:
            continue

        weight = math.log1p(freq)  # dampen extreme head-word dominance
        total_weight += weight
        word_counts[letters] += weight

        padded = "^" + letters + "$"
        for n in range(1, max_n + 1):
            gram_counter = counts[n]
            for i in range(len(padded) - n + 1):
                gram_counter[padded[i:i + n]] += weight

    grams: dict[str, float] = {}
    for n in range(2, max_n + 1):
        counter = counts[n]
        if not counter:
            continue
        total = sum(counter.values())
        items = counter.most_common()
        cap = caps.get(n, 0)
        if cap:
            items = items[:cap]
        for gram, c in items:
            p = c / total
            if p >= _PROB_EPS:
                grams[gram] = p

    # Whole-word probabilities (normalised, log-scaled).
    word_total = sum(word_counts.values()) or 1.0
    top_words = word_counts.most_common(WORD_CAP)
    word_probs = {w: c / word_total for w, c in top_words}

    return grams, word_probs


def build_language(lang: str, search_dirs: list[Path], out_dir: Path,
                   max_n: int = 5) -> dict:
    source_lang = LAYOUT_SOURCES.get(lang, lang)
    src = find_frequency_file(source_lang, search_dirs)
    if src is None:
        raise FileNotFoundError(
            f"No frequency list for '{source_lang}' in "
            f"{[str(d) for d in search_dirs]}"
        )
    words = load_frequency(src)
    grams, word_probs = build_ngrams(words, max_n=max_n)
    payload = {
        "version": 2,
        "language": lang,
        "source": src.name,
        "word_count": len(words),
        "grams": grams,
        "words": word_probs,
    }
    out_dir.mkdir(parents=True, exist_ok=True)
    out_path = out_dir / f"{lang}_ngrams.json.gz"
    # ``mtime=0`` keeps the gzip output byte-for-byte reproducible.
    with gzip.GzipFile(out_path, "wb", compresslevel=9, mtime=0) as gz:
        gz.write(json.dumps(
            payload, ensure_ascii=False, separators=(",", ":")
        ).encode("utf-8"))
    size = out_path.stat().st_size
    print(
        f"{lang:3s}: {len(words):6d} words -> {len(grams):6d} grams, "
        f"{len(word_probs):6d} words  ({size/1024:.0f} KiB)"
    )
    return {"lang": lang, "grams": len(grams), "words": len(word_probs),
            "bytes": size}


def _search_dirs() -> list[Path]:
    root = Path(__file__).parents[2]
    return [
        root / "data" / "frequency",
        # Also accept the raw reference dump if present.
        root / "ref" / "Lang Switcher.app" / "Contents" / "Resources",
    ]


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--lang", nargs="*", default=DEFAULT_LANGS,
                        help="Languages to build (default: all supported).")
    parser.add_argument("--max-n", type=int, default=5)
    parser.add_argument("--out", default=None,
                        help="Output directory (default: package profiles dir).")
    args = parser.parse_args(argv)

    out_dir = Path(args.out) if args.out else Path(__file__).parent / "profiles"
    dirs = _search_dirs()

    ok = 0
    for lang in args.lang:
        try:
            build_language(lang, dirs, out_dir, max_n=args.max_n)
            ok += 1
        except FileNotFoundError as exc:
            print(f"skip {lang}: {exc}", file=sys.stderr)
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
