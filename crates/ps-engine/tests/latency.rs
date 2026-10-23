//! Typing-latency report (ignored by default; run in release):
//!
//! ```text
//! cargo test --release -p ps-engine --test latency -- --ignored --nocapture
//! ```
//!
//! Measures the synchronous costs that sit between a key press and the text
//! appearing: processor CPU, focus/layout backend queries, and the deliberate
//! pacing sleeps in `replace_text`.  It does not inject real keystrokes.

use std::path::PathBuf;
use std::time::Instant;

use ps_engine::{Action, Processor, ProcessorConfig, RawEvent};
use ps_focus::ActiveWindowProvider;

const KEY_LEFTSHIFT: u16 = 42;
const KEY_SPACE: u16 = 57;

fn profiles_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../power_switcher/detection/profiles")
}

fn layouts_json() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ps-core/data/layouts.json")
}

fn make_processor(td: &tempfile::TempDir, config: ProcessorConfig, focus_ttl: f64) -> Processor {
    let layout_set =
        ps_core::LayoutSet::create(&["en", "ru"], &profiles_dir(), &layouts_json(), None)
            .expect("layout set");
    Processor::new(
        layout_set,
        config,
        Some(td.path().join("snippets.json")),
        ps_core::Stats::load(td.path().join("stats.json")),
        Some(td.path().join("exceptions.json")),
        ActiveWindowProvider::with_ttl(focus_ttl),
    )
}

fn type_word(proc: &mut Processor, word: &str, now: &mut f64) {
    for ch in word.chars() {
        let (kc, shifted) = proc.layout_set.char_to_keycode(ch, "en").unwrap();
        *now += 0.02;
        if shifted {
            let _ = proc.on_event(RawEvent::key(KEY_LEFTSHIFT, 1), *now);
        }
        let _ = proc.on_event(RawEvent::key(kc, 1), *now);
        let _ = proc.on_event(RawEvent::key(kc, 0), *now + 0.005);
        if shifted {
            let _ = proc.on_event(RawEvent::key(KEY_LEFTSHIFT, 0), *now + 0.01);
        }
    }
}

fn press_space(proc: &mut Processor, now: &mut f64) -> Vec<Action> {
    *now += 0.02;
    let mut all = proc.on_event(RawEvent::key(KEY_SPACE, 1), *now);
    all.extend(proc.on_event(RawEvent::key(KEY_SPACE, 0), *now + 0.005));
    all
}

fn action_delay_ms(actions: &[Action]) -> u64 {
    actions
        .iter()
        .map(|a| match a {
            Action::Inject { delay_ms, .. } => *delay_ms,
            Action::Pause { ms } => *ms,
            _ => 0,
        })
        .sum()
}

#[test]
#[ignore = "latency report; run with --release --ignored --nocapture"]
fn typing_latency_report() {
    let td = tempfile::tempdir().unwrap();
    let mut now = 100.0;

    // ── processor CPU (focus provider parked so backend calls are excluded) ──
    let mut off = make_processor(
        &td,
        ProcessorConfig {
            spell_check: false,
            self_learning: false,
            trigger_space: false,
            trigger_enter: false,
            trigger_tab: false,
            trigger_punctuation: false,
            ..ProcessorConfig::default()
        },
        3600.0,
    );
    let t = Instant::now();
    for _ in 0..500 {
        type_word(&mut off, "hello", &mut now);
        press_space(&mut off, &mut now);
    }
    println!(
        "plain pass-through (no detection): {:.1} µs/keystroke",
        t.elapsed().as_secs_f64() * 1e6 / (500.0 * 7.0)
    );

    let mut on = make_processor(&td, ProcessorConfig::default(), 3600.0);
    let t = Instant::now();
    for _ in 0..500 {
        type_word(&mut on, "hello", &mut now);
        press_space(&mut on, &mut now);
    }
    println!(
        "full app, keystrokes:              {:.1} µs/keystroke",
        t.elapsed().as_secs_f64() * 1e6 / (500.0 * 7.0)
    );

    // Word boundaries: known word vs unknown word (autocorrect scan).
    let mut known = Vec::new();
    for i in 0..300 {
        type_word(&mut on, "hello", &mut now);
        let t = Instant::now();
        press_space(&mut on, &mut now);
        if i > 10 {
            known.push(t.elapsed().as_secs_f64() * 1e6);
        }
    }
    known.sort_by(f64::total_cmp);
    println!(
        "word boundary, known word:         {:.1} µs (p95 {:.1} µs)",
        known.iter().sum::<f64>() / known.len() as f64,
        known[known.len() * 95 / 100]
    );

    let mut unknown = Vec::new();
    for i in 0..100 {
        let word = format!("blorp{}", (b'a' + (i % 26) as u8) as char);
        type_word(&mut on, &word, &mut now);
        let t = Instant::now();
        press_space(&mut on, &mut now);
        unknown.push(t.elapsed().as_secs_f64() * 1e6);
    }
    unknown.sort_by(f64::total_cmp);
    println!(
        "word boundary, unknown word:       {:.1} µs (p95 {:.1} µs)",
        unknown.iter().sum::<f64>() / unknown.len() as f64,
        unknown[unknown.len() * 95 / 100]
    );

    let mut typo = Vec::new();
    for _ in 0..50 {
        type_word(&mut on, "wprld", &mut now);
        let t = Instant::now();
        press_space(&mut on, &mut now);
        typo.push(t.elapsed().as_secs_f64() * 1e6);
    }
    typo.sort_by(f64::total_cmp);
    println!(
        "word boundary, autocorrect fires:  {:.1} µs (p95 {:.1} µs)",
        typo.iter().sum::<f64>() / typo.len() as f64,
        typo[typo.len() * 95 / 100]
    );

    // ── deliberate injection pacing (the dominant delay) ──
    let td = tempfile::tempdir().unwrap();
    let mut proc = make_processor(&td, ProcessorConfig::default(), 3600.0);
    type_word(&mut proc, "wprld", &mut now);
    let actions = press_space(&mut proc, &mut now);
    println!(
        "autocorrect injection:             {} actions, {} ms of sleeps",
        actions.len(),
        action_delay_ms(&actions)
    );

    let td = tempfile::tempdir().unwrap();
    let mut proc = make_processor(&td, ProcessorConfig::default(), 3600.0);
    type_word(&mut proc, "ghbdtn", &mut now);
    let actions = press_space(&mut proc, &mut now);
    println!(
        "layout-convert injection:          {} actions, {} ms of sleeps",
        actions.len(),
        action_delay_ms(&actions)
    );

    assert!(known[known.len() / 2] < 500.0);
    assert!(unknown[unknown.len() / 2] < 5000.0);
}

#[test]
#[ignore = "latency report; run with --release --ignored --nocapture"]
fn backend_latency_report() {
    // Focus query: the same gdbus/g3kb-switch calls the daemon makes.
    let mut focus = ActiveWindowProvider::with_ttl(0.0);
    println!("focus strategies: {:?}", focus.strategies());
    println!("focus value: {:?}", focus.get_class_at(0.0, true));
    let mut times: Vec<f64> = (0..20)
        .map(|_| {
            let t = Instant::now();
            let _ = focus.get_class_at(0.0, true);
            t.elapsed().as_secs_f64() * 1e6
        })
        .collect();
    times.sort_by(f64::total_cmp);
    println!(
        "focus query (daemon: once/s):      {:.2} ms (p95 {:.2} ms)",
        times.iter().sum::<f64>() / times.len() as f64 / 1000.0,
        times[times.len() * 95 / 100] / 1000.0
    );

    let mut times: Vec<f64> = (0..20)
        .map(|_| {
            let t = Instant::now();
            let _ = std::process::Command::new("g3kb-switch").arg("-p").output();
            t.elapsed().as_secs_f64() * 1e6
        })
        .collect();
    times.sort_by(f64::total_cmp);
    println!(
        "layout poll (daemon: every 250ms): {:.2} ms (p95 {:.2} ms)",
        times.iter().sum::<f64>() / times.len() as f64 / 1000.0,
        times[times.len() * 95 / 100] / 1000.0
    );
}
