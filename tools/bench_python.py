#!/usr/bin/env python3
"""Benchmark the Python engine over the golden parity corpus.

Mirror of `crates/ps-core/examples/bench.rs` so the Rust vs Python latency
comparison uses identical inputs.

    PYTHONPATH=. python3 tools/bench_python.py
"""
from __future__ import annotations

import json
import statistics
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from power_switcher.maps.layout_set import LayoutSet  # noqa: E402

CORPUS = ROOT / "crates" / "ps-core" / "tests" / "data" / "parity_en_ru.json"
PROFILES = ROOT / "power_switcher" / "detection" / "profiles"


def main() -> None:
    ls = LayoutSet.create(["en", "ru"], profiles_dir=PROFILES,
                          words_path=Path("/nonexistent/words.json"))
    cases = json.loads(CORPUS.read_text(encoding="utf-8"))
    print(f"corpus: {len(cases)} cases, layouts {ls.layouts}")

    rounds = 3
    durations: list[float] = []

    for c in cases:  # warm-up
        ls.find_best_layout(c["word"], c["cur"])

    start = time.perf_counter()
    for _ in range(rounds):
        for c in cases:
            t0 = time.perf_counter()
            ls.find_best_layout(c["word"], c["cur"])
            durations.append(time.perf_counter() - t0)
    wall = time.perf_counter() - start

    n = len(durations)
    us = sorted(d * 1e6 for d in durations)
    pct = lambda p: us[int(p / 100 * (n - 1))]
    mean = statistics.fmean(us)
    print("find_best_layout over "
          f"{n} calls ({rounds} rounds):")
    print(f"  mean {mean:8.1f} µs   p50 {pct(50):8.1f} µs   p90 {pct(90):8.1f} µs"
          f"   p99 {pct(99):8.1f} µs   max {us[-1]:8.1f} µs")
    print(f"  throughput: {n / wall:.0f} decisions/s   (wall {wall:.2f}s for {n} calls)")


if __name__ == "__main__":
    main()
