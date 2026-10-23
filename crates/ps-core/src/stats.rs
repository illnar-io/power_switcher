//! Lightweight, persistent conversion statistics.
//!
//! Port of `power_switcher/core/stats.py`.  Counters are kept in
//! `stats.json` and are updated in memory; writes are debounced so a fast
//! typist does not thrash the disk.  Time-dependent methods take an explicit
//! `now` (the daemon passes its own clock) with `*_now` conveniences.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use log::warn;
use serde::{Deserialize, Serialize};

use crate::fsutil::{read_private, write_private};

/// Minimum seconds between automatic writes (Python debounce).
const SAVE_DEBOUNCE: f64 = 5.0;

fn system_now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// The persisted payload (every field except the path and dirty flags).
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct StatsData {
    auto_conversions: u64,
    manual_conversions: u64,
    snippet_expansions: u64,
    undos: u64,
    chars_retyped: u64,
    first_seen: f64,
    last_conversion: f64,
}

/// Conversion counters (all monotonic over the lifetime of the install).
#[derive(Debug)]
pub struct Stats {
    path: PathBuf,
    data: StatsData,
    dirty: bool,
    last_save: f64,
}

impl Stats {
    /// Load from `path` (missing or corrupt file → zeroed counters), stamping
    /// `first_seen` when unset.
    pub fn load(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let mut data = StatsData::default();
        if let Ok(text) = read_private(&path) {
            match serde_json::from_str::<StatsData>(&text) {
                Ok(loaded) => data = loaded,
                Err(err) => warn!("Failed to load stats from {}: {err}", path.display()),
            }
        }
        if data.first_seen == 0.0 {
            data.first_seen = system_now();
        }
        Self {
            path,
            data,
            dirty: false,
            last_save: 0.0,
        }
    }

    /// Save at the current system time.
    pub fn save(&mut self, force: bool) {
        let _ = self.save_at(system_now(), force);
    }

    /// Debounced save with an explicit clock: skipped when not dirty, or (when
    /// not forced) within [`SAVE_DEBOUNCE`] of the previous write.
    pub fn save_at(&mut self, now: f64, force: bool) -> io::Result<()> {
        if !self.dirty && !force {
            return Ok(());
        }
        if !force && now - self.last_save < SAVE_DEBOUNCE {
            return Ok(());
        }
        self.last_save = now;
        self.dirty = false;
        persist(&self.path, &self.data)
    }

    /// Reset all counters (persisted immediately).
    pub fn reset(&mut self) {
        self.reset_at(system_now());
    }

    pub fn reset_at(&mut self, now: f64) {
        self.data = StatsData {
            first_seen: now,
            ..StatsData::default()
        };
        self.dirty = true;
        let _ = self.save_at(now, true);
    }

    pub fn record_auto(&mut self, changed_chars: i64) {
        self.record_auto_at(system_now(), changed_chars);
    }

    pub fn record_auto_at(&mut self, now: f64, changed_chars: i64) {
        self.data.auto_conversions += 1;
        self.data.chars_retyped += changed_chars.max(0) as u64;
        self.data.last_conversion = now;
        self.dirty = true;
        let _ = self.save_at(now, false);
    }

    pub fn record_manual(&mut self, changed_chars: i64) {
        self.record_manual_at(system_now(), changed_chars);
    }

    pub fn record_manual_at(&mut self, now: f64, changed_chars: i64) {
        self.data.manual_conversions += 1;
        self.data.chars_retyped += changed_chars.max(0) as u64;
        self.data.last_conversion = now;
        self.dirty = true;
        let _ = self.save_at(now, false);
    }

    pub fn record_snippet(&mut self) {
        self.record_snippet_at(system_now());
    }

    pub fn record_snippet_at(&mut self, now: f64) {
        self.data.snippet_expansions += 1;
        self.dirty = true;
        let _ = self.save_at(now, false);
    }

    pub fn record_undo(&mut self) {
        self.record_undo_at(system_now());
    }

    pub fn record_undo_at(&mut self, now: f64) {
        self.data.undos += 1;
        self.dirty = true;
        let _ = self.save_at(now, false);
    }

    /// Auto + manual conversions.
    pub fn total_conversions(&self) -> u64 {
        self.data.auto_conversions + self.data.manual_conversions
    }

    pub fn auto_conversions(&self) -> u64 {
        self.data.auto_conversions
    }
    pub fn manual_conversions(&self) -> u64 {
        self.data.manual_conversions
    }
    pub fn snippet_expansions(&self) -> u64 {
        self.data.snippet_expansions
    }
    pub fn undos(&self) -> u64 {
        self.data.undos
    }
    pub fn chars_retyped(&self) -> u64 {
        self.data.chars_retyped
    }
    pub fn first_seen(&self) -> f64 {
        self.data.first_seen
    }
    pub fn last_conversion(&self) -> f64 {
        self.data.last_conversion
    }
}

fn persist(path: &Path, data: &StatsData) -> io::Result<()> {
    write_private(path, serde_json::to_string_pretty(data)?.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_stats_start_at_zero() {
        let dir = tempfile::tempdir().unwrap();
        let s = Stats::load(dir.path().join("stats.json"));
        assert_eq!(s.total_conversions(), 0);
        assert!(s.first_seen() > 0.0);
    }

    #[test]
    fn recording_and_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stats.json");
        let now = 1000.0;
        let mut s = Stats::load(&path);
        s.record_auto_at(now, 4);
        s.record_auto_at(now, 2);
        s.record_manual_at(now, 3);
        s.record_snippet_at(now);
        s.record_undo_at(now);
        s.save_at(now, true).unwrap();

        let again = Stats::load(&path);
        assert_eq!(again.auto_conversions(), 2);
        assert_eq!(again.manual_conversions(), 1);
        assert_eq!(again.total_conversions(), 3);
        assert_eq!(again.chars_retyped(), 9);
        assert_eq!(again.snippet_expansions(), 1);
        assert_eq!(again.undos(), 1);
    }

    #[test]
    fn writes_are_debounced_but_force_writes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stats.json");
        let mut s = Stats::load(&path);
        // First record saves immediately (last_save = 0).
        s.record_auto_at(100.0, 1);
        assert!(path.exists());
        let first = std::fs::read_to_string(&path).unwrap();
        // Second record within 5 s: debounced, file unchanged.
        s.record_auto_at(102.0, 1);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), first);
        // Forced save always writes.
        s.save_at(103.0, true).unwrap();
        assert_ne!(std::fs::read_to_string(&path).unwrap(), first);
        // After the debounce window an ordinary save writes again.
        s.record_auto_at(110.0, 1);
        assert_ne!(std::fs::read_to_string(&path).unwrap(), first);
    }

    #[test]
    fn reset() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stats.json");
        let mut s = Stats::load(&path);
        s.record_auto_at(1.0, 5);
        s.reset_at(2.0);
        assert_eq!(s.total_conversions(), 0);
        assert_eq!(Stats::load(&path).total_conversions(), 0);
        // first_seen is re-stamped by reset.
        assert_eq!(Stats::load(&path).first_seen(), 2.0);
    }

    #[test]
    fn corrupt_file_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stats.json");
        std::fs::write(&path, "{ this is not json").unwrap();
        let s = Stats::load(&path);
        assert_eq!(s.total_conversions(), 0);
    }

    #[test]
    fn negative_changed_chars_clamped() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Stats::load(dir.path().join("stats.json"));
        s.record_auto_at(1.0, -3);
        assert_eq!(s.chars_retyped(), 0);
    }
}
