//! Undo history: store conversion pairs with timestamps for flexible undo.
//!
//! Port of `power_switcher/core/undo_history.py`.  Supports undo by word count
//! or time window.  Unlike the Python version (which reads `time.time()`
//! internally), the interesting methods take an explicit `now` timestamp so the
//! engine stays deterministic and testable; the daemon passes its own clock.

use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;

/// One performed conversion, kept so it can be undone.
#[derive(Debug, Clone, Deserialize)]
pub struct UndoEntry {
    pub original: String,
    pub converted: String,
    pub layout: String,
    pub timestamp: f64,
    #[serde(default)]
    pub separator_keycode: Option<u16>,
    #[serde(default)]
    pub separator_shift: bool,
}

pub const DEFAULT_MAX_ENTRIES: usize = 50;
pub const DEFAULT_WINDOW_SECONDS: f64 = 3.0;

fn system_now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

#[derive(Debug)]
pub struct UndoHistory {
    entries: VecDeque<UndoEntry>,
    max_entries: usize,
    window_seconds: f64,
}

impl Default for UndoHistory {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_ENTRIES, DEFAULT_WINDOW_SECONDS)
    }
}

impl UndoHistory {
    pub fn new(max_entries: usize, window_seconds: f64) -> Self {
        Self {
            entries: VecDeque::new(),
            max_entries,
            window_seconds,
        }
    }

    /// Record a conversion at the current system time (no separator).
    pub fn push(&mut self, original: &str, converted: &str, layout: &str) {
        self.push_full_at(system_now(), original, converted, layout, None, false);
    }

    /// Record a conversion at an explicit timestamp (no separator).
    pub fn push_at(&mut self, now: f64, original: &str, converted: &str, layout: &str) {
        self.push_full_at(now, original, converted, layout, None, false);
    }

    /// Record a conversion (with separator details) at the current time.
    pub fn push_with_separator(
        &mut self,
        original: &str,
        converted: &str,
        layout: &str,
        separator_keycode: Option<u16>,
        separator_shift: bool,
    ) {
        self.push_full_at(
            system_now(),
            original,
            converted,
            layout,
            separator_keycode,
            separator_shift,
        );
    }

    /// Record a conversion (with separator details) at an explicit timestamp.
    pub fn push_full_at(
        &mut self,
        now: f64,
        original: &str,
        converted: &str,
        layout: &str,
        separator_keycode: Option<u16>,
        separator_shift: bool,
    ) {
        self.entries.push_back(UndoEntry {
            original: original.to_string(),
            converted: converted.to_string(),
            layout: layout.to_string(),
            timestamp: now,
            separator_keycode,
            separator_shift,
        });
        if self.entries.len() > self.max_entries {
            self.entries.pop_front();
        }
    }

    /// Pop the most recent entry (single-word undo).
    pub fn pop_last(&mut self) -> Option<UndoEntry> {
        self.entries.pop_back()
    }

    /// Last `n` conversion entries within the time window, oldest first.
    pub fn get_undo_words(&self, n: usize) -> Vec<UndoEntry> {
        self.get_undo_words_at(system_now(), n)
    }

    pub fn get_undo_words_at(&self, now: f64, n: usize) -> Vec<UndoEntry> {
        let mut result = Vec::new();
        for entry in self.entries.iter().rev() {
            if now - entry.timestamp > self.window_seconds {
                break;
            }
            result.push(entry.clone());
            if result.len() >= n {
                break;
            }
        }
        result.reverse();
        result
    }

    /// All conversion entries within the last `seconds` (default: the window),
    /// oldest first.
    pub fn get_undo_time(&self, seconds: Option<f64>) -> Vec<UndoEntry> {
        self.get_undo_time_at(system_now(), seconds)
    }

    pub fn get_undo_time_at(&self, now: f64, seconds: Option<f64>) -> Vec<UndoEntry> {
        let seconds = seconds.unwrap_or(self.window_seconds);
        let mut result = Vec::new();
        for entry in self.entries.iter().rev() {
            if now - entry.timestamp > seconds {
                break;
            }
            result.push(entry.clone());
        }
        result.reverse();
        result
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Remove entries older than the window (current system time).
    pub fn clear_old(&mut self) {
        self.clear_old_at(system_now());
    }

    pub fn clear_old_at(&mut self, now: f64) {
        self.entries
            .retain(|e| now - e.timestamp <= self.window_seconds);
    }

    /// Most recent entry, if any.
    pub fn last(&self) -> Option<&UndoEntry> {
        self.entries.back()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Test/inspection helper: chronological entry list.
    #[cfg(test)]
    pub(crate) fn entries(&self) -> &VecDeque<UndoEntry> {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_last() {
        let mut h = UndoHistory::default();
        h.push("hello", "привет", "en");
        let last = h.last().unwrap();
        assert_eq!(last.original, "hello");
        assert_eq!(last.converted, "привет");
    }

    #[test]
    fn pop_last() {
        let mut h = UndoHistory::default();
        h.push("hello", "привет", "en");
        let entry = h.pop_last().unwrap();
        assert_eq!(entry.original, "hello");
        assert!(h.last().is_none());
    }

    #[test]
    fn get_undo_words() {
        let mut h = UndoHistory::default();
        h.push_at(1.0, "a", "b", "en");
        h.push_at(1.1, "c", "d", "en");
        h.push_at(1.2, "e", "f", "en");
        let entries = h.get_undo_words_at(1.2, 2);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].original, "c");
        assert_eq!(entries[1].original, "e");
    }

    #[test]
    fn get_undo_time_window() {
        let mut h = UndoHistory::new(50, 1.0);
        h.push_at(0.0, "a", "b", "en");
        h.push_at(0.05, "c", "d", "en");
        // Window of 0.1s ending at t=0.06 excludes nothing → both present.
        assert_eq!(h.get_undo_time_at(0.06, Some(0.1)).len(), 2);
        // Window of 0.02s at t=0.06 excludes the first entry.
        let entries = h.get_undo_time_at(0.06, Some(0.02));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].original, "c");
    }

    #[test]
    fn get_undo_time_default_window() {
        let mut h = UndoHistory::new(50, 3.0);
        h.push_at(10.0, "a", "b", "en");
        h.push_at(11.0, "c", "d", "en");
        // seconds=None → use window_seconds (3.0)
        assert_eq!(h.get_undo_time_at(11.0, None).len(), 2);
        assert_eq!(h.get_undo_time_at(20.0, None).len(), 0);
    }

    #[test]
    fn clear() {
        let mut h = UndoHistory::default();
        h.push_at(1.0, "a", "b", "en");
        h.clear();
        assert_eq!(h.len(), 0);
        assert!(h.last().is_none());
    }

    #[test]
    fn max_entries() {
        let mut h = UndoHistory::new(3, 3.0);
        for i in 0..5 {
            h.push_at(1.0 + i as f64, &format!("a{i}"), &format!("b{i}"), "en");
        }
        assert_eq!(h.len(), 3);
        assert_eq!(h.entries().front().unwrap().original, "a2");
    }

    #[test]
    fn clear_old_drops_expired() {
        let mut h = UndoHistory::new(50, 1.0);
        h.push_at(0.0, "old", "x", "en");
        h.push_at(2.5, "new", "y", "en");
        h.clear_old_at(3.0);
        assert_eq!(h.len(), 1);
        assert_eq!(h.last().unwrap().original, "new");
    }

    #[test]
    fn separator_keycode() {
        let mut h = UndoHistory::default();
        h.push_full_at(1.0, "hello", "привет", "en", Some(49), false);
        let last = h.last().unwrap();
        assert_eq!(last.separator_keycode, Some(49));
        assert!(!last.separator_shift);
    }

    #[test]
    fn separator_shift_round_trip() {
        let mut h = UndoHistory::default();
        h.push_full_at(1.0, "nj", "те", "en", Some(39), true);
        let last = h.last().unwrap();
        assert_eq!(last.separator_keycode, Some(39));
        assert!(last.separator_shift);
    }
}
