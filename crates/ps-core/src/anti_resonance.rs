//! Anti-resonance guard: detect conversion oscillation.
//!
//! If word A → B and later B → A within a short window, auto-conversion is
//! frozen for N seconds.  Port of `power_switcher/core/anti_resonance.py`.
//! Methods take an explicit `now` timestamp for determinism; the daemon passes
//! its own clock.

use std::time::{SystemTime, UNIX_EPOCH};

use log::warn;

pub const DEFAULT_FREEZE_SECONDS: f64 = 3.0;
pub const DEFAULT_HISTORY_SIZE: usize = 10;

fn system_now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// (word, produced, timestamp)
type HistoryEntry = (String, String, f64);

#[derive(Debug)]
pub struct AntiResonanceGuard {
    freeze_seconds: f64,
    history_size: usize,
    history: Vec<HistoryEntry>,
    frozen_until: f64,
}

impl Default for AntiResonanceGuard {
    fn default() -> Self {
        Self::new(DEFAULT_FREEZE_SECONDS, DEFAULT_HISTORY_SIZE)
    }
}

impl AntiResonanceGuard {
    pub fn new(freeze_seconds: f64, history_size: usize) -> Self {
        Self {
            freeze_seconds,
            history_size,
            history: Vec::new(),
            frozen_until: 0.0,
        }
    }

    /// Check if this conversion is allowed at the current system time.
    pub fn allow_now(&mut self, word: &str, produced: &str) -> bool {
        self.allow(word, produced, system_now())
    }

    /// Check if this conversion is allowed at an explicit timestamp.
    ///
    /// Returns false while a freeze is active (oscillation detected).  Records
    /// the conversion in history when allowed.
    pub fn allow(&mut self, word: &str, produced: &str, now: f64) -> bool {
        if now < self.frozen_until {
            return false;
        }

        for (prev_word, prev_produced, prev_time) in &self.history {
            // A → B and later B → A: the conversion is being undone by itself.
            if prev_produced == word && now - prev_time < self.freeze_seconds {
                warn!(
                    "Anti-resonance: conversion oscillation detected ({} → {} → {}), \
                     freezing auto-conversion for {}s",
                    prev_word, prev_produced, word, self.freeze_seconds
                );
                self.frozen_until = now + self.freeze_seconds;
                self.history.clear();
                return false;
            }
            // Identical conversion repeated shortly after itself.
            if word == prev_word
                && produced == prev_produced
                && now - prev_time < self.freeze_seconds
            {
                warn!(
                    "Anti-resonance: conversion loop detected ({} → {}), \
                     freezing auto-conversion for {}s",
                    word, produced, self.freeze_seconds
                );
                self.frozen_until = now + self.freeze_seconds;
                self.history.clear();
                return false;
            }
        }

        self.history
            .push((word.to_string(), produced.to_string(), now));
        if self.history.len() > self.history_size {
            self.history.remove(0);
        }
        true
    }

    /// Drop all history and cancel any active freeze.
    pub fn reset(&mut self) {
        self.history.clear();
        self.frozen_until = 0.0;
    }

    /// Whether a freeze is active at the current system time.
    pub fn is_frozen(&self) -> bool {
        self.is_frozen_at(system_now())
    }

    pub fn is_frozen_at(&self, now: f64) -> bool {
        now < self.frozen_until
    }

    /// When the current freeze expires (0.0 when not frozen).
    pub fn frozen_until(&self) -> f64 {
        self.frozen_until
    }

    pub fn history_len(&self) -> usize {
        self.history.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_normal_conversions() {
        let mut g = AntiResonanceGuard::default();
        assert!(g.allow("hello", "привет", 0.0));
        assert!(g.allow("world", "мир", 1.0));
        assert_eq!(g.history_len(), 2);
    }

    #[test]
    fn detects_oscillation_a_b_a() {
        let mut g = AntiResonanceGuard::default();
        // A → B
        assert!(g.allow("hello", "привет", 0.0));
        // Later B → A: the produced word is converted back.
        assert!(!g.allow("привет", "hello", 1.0));
        assert!(g.is_frozen_at(1.0));
        // Freeze window: everything blocked.
        assert!(!g.allow("a", "b", 2.0));
        // After the freeze expires, conversions are allowed again.
        assert!(g.allow("a", "b", 5.0));
    }

    #[test]
    fn detects_identical_loop() {
        let mut g = AntiResonanceGuard::default();
        assert!(g.allow("hello", "привет", 0.0));
        // Same conversion repeated within the window → loop.
        assert!(!g.allow("hello", "привет", 1.0));
        assert!(g.is_frozen_at(1.0));
    }

    #[test]
    fn old_history_does_not_trigger() {
        let mut g = AntiResonanceGuard::default();
        assert!(g.allow("hello", "привет", 0.0));
        // Same conversion again, but outside the freeze window → allowed.
        assert!(g.allow("hello", "привет", 10.0));
    }

    #[test]
    fn reset_clears_freeze() {
        let mut g = AntiResonanceGuard::default();
        assert!(g.allow("hello", "привет", 0.0));
        assert!(!g.allow("привет", "hello", 1.0));
        g.reset();
        assert!(!g.is_frozen_at(1.0));
        assert_eq!(g.history_len(), 0);
        assert!(g.allow("hello", "привет", 1.0));
    }

    #[test]
    fn history_size_limit() {
        let mut g = AntiResonanceGuard::new(3.0, 3);
        for i in 0..5 {
            g.allow(&format!("w{i}"), &format!("p{i}"), i as f64);
        }
        assert_eq!(g.history_len(), 3);
    }

    #[test]
    fn freeze_expires_naturally() {
        let mut g = AntiResonanceGuard::default();
        assert!(g.allow("a", "b", 0.0));
        assert!(!g.allow("b", "a", 1.0));
        // frozen_until = 1.0 + 3.0 = 4.0
        assert!(g.is_frozen_at(3.999));
        assert!(!g.is_frozen_at(4.0));
        assert!(g.allow("c", "d", 4.0));
    }
}
