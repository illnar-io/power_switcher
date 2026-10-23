//! `words.json` — user word exceptions and words learned from undo.
//!
//! Port of the persistence half of `power_switcher/maps/layout_set.py`
//! (`_load_state` / `_save_state`).  The file is the contract shared with the
//! Python CLI (`python -m power_switcher words …`) and the GUI:
//!
//! ```json
//! {"exceptions": ["nj;t"], "learned_words": ["k.lb"]}
//! ```
//!
//! Words are lower-cased and kept sorted, exactly like Python.  A bare JSON
//! list is accepted as a legacy exceptions-only file (Python does the same).
//! Write failures are logged and ignored — learning must never break typing.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use log::warn;

use crate::fsutil::{read_private, write_private};

/// Persisted set of words that must never be auto-converted.
#[derive(Debug, Default, Clone)]
pub struct WordStore {
    /// `None` → in-memory only (unit tests, embedded use).
    path: Option<PathBuf>,
    exceptions: BTreeSet<String>,
    learned: BTreeSet<String>,
}

impl WordStore {
    /// Empty, in-memory store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Load the store at `path` (missing/corrupt files are ignored, matching
    /// the Python loader).  `None` keeps it in memory only.
    pub fn load(path: Option<&Path>) -> Self {
        let mut store = Self {
            path: path.map(Path::to_path_buf),
            ..Self::default()
        };
        store.reload();
        store
    }

    /// Point the store at a new file and load it.
    pub fn set_path(&mut self, path: Option<PathBuf>) {
        self.path = path;
        self.reload();
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Re-read the backing file, replacing both sets.
    pub fn reload(&mut self) {
        self.exceptions.clear();
        self.learned.clear();
        let Some(path) = &self.path else { return };
        let Ok(text) = read_private(path) else {
            return;
        };
        let data: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(err) => {
                warn!("Failed to load words from {}: {err}", path.display());
                return;
            }
        };
        match data {
            serde_json::Value::Object(map) => {
                self.load_set(&map, "exceptions", true);
                self.load_set(&map, "learned_words", false);
            }
            // Legacy: a plain list of word exceptions.
            serde_json::Value::Array(items) => {
                for word in items.iter().filter_map(|v| v.as_str()) {
                    self.insert_exception(word);
                }
            }
            _ => {}
        }
    }

    fn load_set(
        &mut self,
        map: &serde_json::Map<String, serde_json::Value>,
        key: &str,
        exceptions: bool,
    ) {
        let Some(items) = map.get(key).and_then(|v| v.as_array()) else {
            return;
        };
        for word in items.iter().filter_map(|v| v.as_str()) {
            if exceptions {
                self.insert_exception(word);
            } else {
                self.insert_learned(word);
            }
        }
    }

    fn insert_exception(&mut self, word: &str) {
        if word.chars().count() >= 2 {
            self.exceptions.insert(word.to_lowercase());
        }
    }

    fn insert_learned(&mut self, word: &str) {
        if word.chars().count() >= 2 {
            self.learned.insert(word.to_lowercase());
        }
    }

    /// Persist `words.json` (sorted, pretty, UTF-8 preserved).
    pub fn save(&self) {
        let Some(path) = &self.path else { return };
        let payload = serde_json::json!({
            "exceptions": self.exceptions(),
            "learned_words": self.learned_words(),
        });
        match serde_json::to_string_pretty(&payload) {
            Ok(text) => {
                if let Err(err) = write_private(path, text.as_bytes()) {
                    warn!("failed to save {}: {err}", path.display());
                }
            }
            Err(err) => warn!("failed to serialise words: {err}"),
        }
    }

    /// Add (or keep) a manual exception; returns whether the set changed.
    pub fn add_exception(&mut self, word: &str) -> bool {
        let before = self.exceptions.len();
        self.insert_exception(word);
        let changed = self.exceptions.len() != before;
        if changed {
            self.save();
        }
        changed
    }

    /// Add (or keep) a learned word; returns whether the set changed.
    pub fn add_learned(&mut self, word: &str) -> bool {
        let before = self.learned.len();
        self.insert_learned(word);
        let changed = self.learned.len() != before;
        if changed {
            self.save();
        }
        changed
    }

    pub fn remove_exception(&mut self, word: &str) -> bool {
        if self.exceptions.remove(&word.to_lowercase()) {
            self.save();
            true
        } else {
            false
        }
    }

    pub fn remove_learned(&mut self, word: &str) -> bool {
        if self.learned.remove(&word.to_lowercase()) {
            self.save();
            true
        } else {
            false
        }
    }

    /// Remove a word from either set; returns whether anything was removed.
    pub fn remove(&mut self, word: &str) -> bool {
        let exception = self.remove_exception(word);
        let learned = self.remove_learned(word);
        exception || learned
    }

    pub fn exceptions(&self) -> Vec<String> {
        self.exceptions.iter().cloned().collect()
    }

    pub fn learned_words(&self) -> Vec<String> {
        self.learned.iter().cloned().collect()
    }

    /// True when the word is a manual exception or a learned word.
    pub fn is_exception(&self, word: &str) -> bool {
        let w = word.to_lowercase();
        self.exceptions.contains(&w) || self.learned.contains(&w)
    }

    pub fn len(&self) -> usize {
        self.exceptions.len() + self.learned.len()
    }

    pub fn is_empty(&self) -> bool {
        self.exceptions.is_empty() && self.learned.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_remove_and_persist() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("words.json");
        let mut store = WordStore::load(Some(&path));
        assert!(store.add_exception("NJ;T")); // lower-cased, punctuation kept
        assert!(store.add_learned("k.lb"));
        assert!(!store.add_learned("k.lb"), "duplicate is a no-op");

        let reloaded = WordStore::load(Some(&path));
        assert_eq!(reloaded.exceptions(), vec!["nj;t"]);
        assert_eq!(reloaded.learned_words(), vec!["k.lb"]);
        assert!(reloaded.is_exception("Nj;t"));
        assert_eq!(reloaded.len(), 2);
    }

    #[test]
    fn remove_from_either_set() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("words.json");
        let mut store = WordStore::load(Some(&path));
        store.add_exception("foo");
        store.add_learned("bar");
        assert!(store.remove("foo"));
        assert!(store.remove("bar"));
        assert!(!store.remove("bar"));
        assert!(store.is_empty());
    }

    #[test]
    fn one_character_words_are_ignored() {
        let mut store = WordStore::new();
        assert!(!store.add_exception("я"));
        assert!(!store.add_learned("a"));
        assert!(store.is_empty());
    }

    #[test]
    fn missing_and_corrupt_files_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nope.json");
        assert!(WordStore::load(Some(&missing)).is_empty());

        let corrupt = dir.path().join("corrupt.json");
        std::fs::write(&corrupt, "{ not json").unwrap();
        assert!(WordStore::load(Some(&corrupt)).is_empty());
    }

    #[test]
    fn legacy_list_is_exceptions_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("words.json");
        std::fs::write(&path, r#"["nj;t", "k.lb"]"#).unwrap();
        let store = WordStore::load(Some(&path));
        assert_eq!(store.exceptions(), vec!["k.lb", "nj;t"]);
        assert!(store.learned_words().is_empty());
    }

    #[test]
    fn python_file_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("words.json");
        std::fs::write(
            &path,
            r#"{"exceptions": ["nj;t"], "learned_words": ["ghbdtn"]}"#,
        )
        .unwrap();
        let mut store = WordStore::load(Some(&path));
        assert!(store.is_exception("ghbdtn"));
        store.add_exception("макбук");
        let data: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(data["exceptions"], serde_json::json!(["nj;t", "макбук"]));
        assert_eq!(data["learned_words"], serde_json::json!(["ghbdtn"]));
    }
}
