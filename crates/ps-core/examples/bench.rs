//! Phase-0/1 exit criterion: p99 `find_best_layout` < 1 ms.
//!
//! Replays the golden EN↔RU corpus through the engine and reports the
//! per-call latency distribution.
//!
//!     cargo run --release -p ps-core --example bench

use std::path::PathBuf;
use std::time::Instant;

use ps_core::layout_set::{LayoutSet, DEFAULT_MIN_WORD_LEN, DEFAULT_THRESHOLD};
use serde::Deserialize;

#[derive(Deserialize)]
struct DecisionCase {
    word: String,
    cur: String,
    #[serde(default)]
    #[allow(dead_code)]
    want: Option<String>,
}

fn main() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.join("../..").canonicalize().unwrap();
    let mut ls = LayoutSet::create(
        &["en", "ru"],
        &root.join("power_switcher/detection/profiles"),
        &manifest.join("data/layouts.json"),
        None,
    )
    .expect("load set");

    let cases: Vec<DecisionCase> = serde_json::from_str(
        &std::fs::read_to_string(manifest.join("tests/data/parity_en_ru.json")).unwrap(),
    )
    .unwrap();
    println!("corpus: {} cases, layouts {:?}", cases.len(), ls.layouts);

    let rounds = 3;
    let mut durations_ns: Vec<u128> = Vec::with_capacity(cases.len() * rounds);

    // Warm-up (page in profiles, branch predictors, allocator).
    for c in &cases {
        let _ = ls.find_best_layout(&c.word, &c.cur, DEFAULT_THRESHOLD, DEFAULT_MIN_WORD_LEN);
    }

    let start = Instant::now();
    for _ in 0..rounds {
        for c in &cases {
            let t0 = Instant::now();
            let _ = ls.find_best_layout(&c.word, &c.cur, DEFAULT_THRESHOLD, DEFAULT_MIN_WORD_LEN);
            durations_ns.push(t0.elapsed().as_nanos());
        }
    }
    let wall = start.elapsed();

    durations_ns.sort_unstable();
    let n = durations_ns.len();
    let pct = |p: f64| durations_ns[((p / 100.0) * (n - 1) as f64) as usize];
    let mean = durations_ns.iter().sum::<u128>() / n as u128;

    println!("find_best_layout over {n} calls ({} rounds):", rounds);
    println!(
        "  mean {:>8.1} µs   p50 {:>8.1} µs   p90 {:>8.1} µs   p99 {:>8.1} µs   max {:>8.1} µs",
        mean as f64 / 1e3,
        pct(50.0) as f64 / 1e3,
        pct(90.0) as f64 / 1e3,
        pct(99.0) as f64 / 1e3,
        *durations_ns.last().unwrap() as f64 / 1e3,
    );
    println!(
        "  throughput: {:.0} decisions/s   (wall {:.2}s for {n} calls)",
        n as f64 / wall.as_secs_f64(),
        wall.as_secs_f64()
    );

    let p99_us = pct(99.0) as f64 / 1e3;
    let max_us = *durations_ns.last().unwrap() as f64 / 1e3;
    println!(
        "  exit criterion (p99 < 1000 µs): {}",
        if p99_us < 1000.0 && max_us < 1000.0 {
            "PASS"
        } else {
            "FAIL"
        }
    );
}
