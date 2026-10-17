#!/usr/bin/env python3
"""Export parity data from the Python engine for the Rust ps-core tests.

Generates three artefacts (committed to the repo so Rust tests are hermetic):

1. ``crates/ps-core/data/layouts.json``
   The keycode tables as an *ordered array* of ``[keycode, lo, hi]`` triples per
   layout.  Order matters: ``build_char_map`` iterates the Python dict in
   insertion order, so later keycodes win on character collisions.

2. ``crates/ps-core/tests/data/parity_scores.json``
   Exact ``score`` / ``decision_score`` values for a sample of words, so numeric
   drift between Python's ``math.log`` and Rust's ``f64::ln`` is caught early.

3. ``crates/ps-core/tests/data/parity_en_ru.json``
   Golden ``find_best_layout`` decisions for the EN↔RU corpus (both directions
   plus plain words and hand-crafted edge cases).

Run from the repo root:  PYTHONPATH=. python3 tools/export_parity_data.py
"""
from __future__ import annotations

import gzip
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from power_switcher.maps.keycode_map import LAYOUTS  # noqa: E402
from power_switcher.maps.layout_set import LayoutSet  # noqa: E402

PROFILES = ROOT / "power_switcher" / "detection" / "profiles"
FREQ = ROOT / "data" / "frequency"
OUT_CORE = ROOT / "crates" / "ps-core"
OUT_DATA = OUT_CORE / "data"
OUT_TESTS = OUT_CORE / "tests" / "data"

N_WORDS = 8000
MIN_LEN, MAX_LEN = 3, 14


def load_words(lang: str, n: int = N_WORDS) -> list[str]:
    fname = {"ua": "uk"}.get(lang, lang)
    words: list[str] = []
    with gzip.open(FREQ / f"{fname}_frequency.txt.gz", "rt", encoding="utf-8") as f:
        for line in f:
            parts = line.split()
            if not parts:
                continue
            w = parts[0].lower()
            if w.isalpha() and MIN_LEN <= len(w) <= MAX_LEN:
                words.append(w)
            if len(words) >= n:
                break
    return words


def export_layouts() -> None:
    OUT_DATA.mkdir(parents=True, exist_ok=True)
    data = {
        name: [[kc, lo, hi] for kc, (lo, hi) in table.items()]
        for name, table in LAYOUTS.items()
    }
    (OUT_DATA / "layouts.json").write_text(
        json.dumps(data, ensure_ascii=False), encoding="utf-8"
    )
    print(f"layouts.json: {sum(len(v) for v in data.values())} keys across {len(data)} layouts")


def build_ls(layouts: list[str]) -> LayoutSet:
    return LayoutSet.create(
        layouts, profiles_dir=PROFILES, words_path=Path("/nonexistent/words.json")
    )


def export_scores() -> None:
    """Score-level parity: n-gram score and composite decision score."""
    ls2 = build_ls(["en", "ru"])
    ls5 = build_ls(["en", "ru", "ua", "de", "fr"])

    en_words = load_words("en", 120)
    ru_words = load_words("ru", 120)

    cases: list[dict] = []
    for w in en_words[:60] + ru_words[:60]:
        for lang in ("en", "ru"):
            cases.append({
                "word": w, "lang": lang,
                "score": ls2.score_word(w, lang),
                "decision": ls2.decision_score(w, lang),
            })
    # Converted (wrong-layout) forms: what the user actually types.
    for w in ru_words[:60]:
        t = ls2.convert(w, "ru", "en")
        if t != w:
            cases.append({
                "word": t, "lang": "en",
                "score": ls2.score_word(t, "en"),
                "decision": ls2.decision_score(t, "en"),
            })
    for w in en_words[:60]:
        t = ls2.convert(w, "en", "ru")
        if t != w:
            cases.append({
                "word": t, "lang": "ru",
                "score": ls2.score_word(t, "ru"),
                "decision": ls2.decision_score(t, "ru"),
            })
    # A few through the 5-layout set to pin multi-language scoring.
    for w in en_words[:20] + ru_words[:20]:
        for lang in ("en", "ru", "ua", "de", "fr"):
            cases.append({
                "word": w, "lang": lang, "set": 5,
                "score": ls5.score_word(w, lang),
                "decision": ls5.decision_score(w, lang),
            })

    OUT_TESTS.mkdir(parents=True, exist_ok=True)
    (OUT_TESTS / "parity_scores.json").write_text(
        json.dumps(cases, ensure_ascii=False), encoding="utf-8"
    )
    print(f"parity_scores.json: {len(cases)} cases")


def export_decisions() -> None:
    ls = build_ls(["en", "ru"])  # the product default / EN↔RU focus

    ru_words = load_words("ru")
    en_words = load_words("en")

    records: list[dict] = []

    def add(word: str, cur: str, note: str = "") -> None:
        records.append({
            "word": word, "cur": cur,
            "want": ls.find_best_layout(word, cur),
            **({"note": note} if note else {}),
        })

    # 1. Russian intent typed on the EN layout (Latin buffer → convert to RU).
    for w in ru_words:
        t = ls.convert(w, "ru", "en")
        if t == w:
            continue
        add(t, "en")

    # 2. English intent typed on the RU layout (Cyrillic buffer → convert to EN).
    for w in en_words:
        t = ls.convert(w, "en", "ru")
        if t == w:
            continue
        add(t, "ru")

    # 3. Plain words in their own layout — must stay untouched (FP check).
    for w in en_words:
        add(w, "en")
    for w in ru_words:
        add(w, "ru")

    # 4. Hand-crafted edge cases (verified behaviour from the roadmap).
    edge = [
        # wrong-layout Russian typed on EN (punctuated forms included)
        ("hf,jnf", "en"), ("nj;t", "en"), ("k.lb", "en"), (",snm", "en"),
        ("ghbdtn", "en"), ("ру", "ru"), ("вфы", "ru"), ("тест", "en"),
        # things that must never convert
        ("don't", "en"), ("it's", "en"), ("v1.2", "en"), ("abc123", "en"),
        ("a1b2c3", "en"), ("e2e", "en"), ("HTTP", "en"), ("API", "en"),
        ("SQL", "en"), ("OK", "en"), ("ID", "en"), ("foo.bar", "en"),
        ("user@example", "en"), ("x-y-z", "en"), ("half-life", "en"),
        # Russian caps / abbreviations
        ("ВФЫВ", "ru"), ("ПРИВЕТ", "ru"),
        # short words below min_word_len
        ("ab", "en"), ("йц", "ru"),
    ]
    for w, cur in edge:
        add(w, cur, note="edge")

    OUT_TESTS.mkdir(parents=True, exist_ok=True)
    (OUT_TESTS / "parity_en_ru.json").write_text(
        json.dumps(records, ensure_ascii=False), encoding="utf-8"
    )
    conv = sum(1 for r in records if r["want"])
    print(f"parity_en_ru.json: {len(records)} cases ({conv} conversions, "
          f"{len(records) - conv} keep)")


def main() -> None:
    export_layouts()
    export_scores()
    export_decisions()


if __name__ == "__main__":
    main()
