//! Snippet auto-expansion: trigger → expansion on word boundary.
//!
//! Layout-independent matching (Keyboop/Caramba pattern): triggers are matched
//! by their *canonical form* — the keystrokes interpreted as if typed in the EN
//! layout, lowercased — so `!test`, `!TEST` and `!еуые` (the RU-layout spelling
//! of the same physical keys) all hit the same snippet.
//!
//! Port of `power_switcher/core/snippets.py`.  The store takes an explicit
//! `path` (no `~/.config` default in the library — the CLI provides it) and a
//! prebuilt RU→EN character map from [`crate::layouts::LayoutMaps`].
//!
//! Persistence differences from Python (deliberate):
//! * Mutating methods return `io::Result` instead of raising through the CLI;
//!   the in-memory state is updated first, exactly like Python's ordering.
//! * When loading a JSON array, non-array items, short items and non-string
//!   values are skipped (Python would only crash later in `_canonical`).

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use log::{info, warn};

/// Layout-independent canonical form: interpret keystrokes as if in the EN
/// layout, then lowercase.  So `!test`, `!TEST`, `!еуые` all become the same
/// canonical key.
pub fn canonical_with(s: &str, ru_to_en: &HashMap<char, char>) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let mapped = ru_to_en.get(&c).copied().unwrap_or(c);
        out.extend(mapped.to_lowercase());
    }
    out
}

/// Ordered snippet pairs with layout-independent matching.
#[derive(Debug)]
pub struct SnippetStore {
    path: PathBuf,
    pairs: Vec<(String, String)>,
    index: HashMap<String, String>,
    ru_to_en: HashMap<char, char>,
}

impl SnippetStore {
    /// Open (and load, if present) the snippet store at `path`.  `ru_to_en` is
    /// the RU→EN character map, normally `maps.char_map("ru", "en").clone()`.
    pub fn new(path: impl Into<PathBuf>, ru_to_en: HashMap<char, char>) -> Self {
        let mut store = Self {
            path: path.into(),
            pairs: Vec::new(),
            index: HashMap::new(),
            ru_to_en,
        };
        store.load();
        store
    }

    /// Canonical form of `s` under this store's RU→EN map.
    pub fn canonical(&self, s: &str) -> String {
        canonical_with(s, &self.ru_to_en)
    }

    /// Load `snippets.json`.  Missing or corrupt files are ignored, matching
    /// the Python loader.  Accepts either a list of `[trigger, expansion]`
    /// pairs or a `{trigger: expansion}` map (sorted by key, like Python's
    /// `sorted(data.items())`).
    fn load(&mut self) {
        let Ok(text) = fs::read_to_string(&self.path) else {
            return;
        };
        let data: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(err) => {
                warn!(
                    "Failed to load snippets from {}: {err}",
                    self.path.display()
                );
                return;
            }
        };
        self.pairs = match data {
            serde_json::Value::Array(items) => items
                .iter()
                .filter_map(|item| {
                    let pair = item.as_array()?;
                    if pair.len() < 2 {
                        return None;
                    }
                    Some((pair[0].as_str()?.to_string(), pair[1].as_str()?.to_string()))
                })
                .collect(),
            serde_json::Value::Object(map) => {
                let mut items: Vec<(String, String)> = map
                    .iter()
                    .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                    .collect();
                // Python: `sorted(data.items())` — sort by key, code-point order
                // (byte order of UTF-8 == code-point order).
                items.sort_by(|a, b| a.0.cmp(&b.0));
                items
            }
            _ => Vec::new(),
        };
        self.rebuild_index();
    }

    fn rebuild_index(&mut self) {
        self.index.clear();
        for (trigger, expansion) in &self.pairs {
            let canon = canonical_with(trigger, &self.ru_to_en);
            if !canon.is_empty() {
                self.index.insert(canon, expansion.clone());
            }
        }
    }

    /// Write `pairs` to disk in Python's `json.dumps(..., indent=2)` shape: a
    /// pretty-printed array of two-element arrays.
    fn persist(&self) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let json = serde_json::to_string_pretty(&self.pairs)?;
        fs::write(&self.path, json)
    }

    /// Return the expansion if `typed_word` matches a snippet trigger.
    pub fn expansion(&self, typed_word: &str) -> Option<String> {
        let canon = self.canonical(typed_word);
        self.index.get(&canon).cloned()
    }

    /// All pairs in insertion order.
    pub fn pairs(&self) -> &[(String, String)] {
        &self.pairs
    }

    /// Add a snippet.  Duplicate canonical keys keep every pair but the index
    /// resolves to the newest expansion (same as Python).
    pub fn add(&mut self, trigger: &str, expansion: &str) -> io::Result<()> {
        let canon = self.canonical(trigger);
        if canon.is_empty() {
            return Ok(());
        }
        self.pairs
            .push((trigger.to_string(), expansion.to_string()));
        self.index.insert(canon, expansion.to_string());
        self.persist()?;
        info!("Snippet added: {trigger} → {expansion}");
        Ok(())
    }

    /// Remove every pair whose canonical trigger equals `trigger`'s.
    /// Returns whether anything was removed.
    pub fn remove(&mut self, trigger: &str) -> io::Result<bool> {
        let canon = self.canonical(trigger);
        let before = self.pairs.len();
        let map = &self.ru_to_en;
        self.pairs.retain(|(t, _)| canonical_with(t, map) != canon);
        let removed = self.pairs.len() < before;
        if removed {
            self.index.remove(&canon);
            self.persist()?;
            info!("Snippet removed: {trigger}");
        }
        Ok(removed)
    }

    /// Replace all snippets (editor path).  Deduplicates by canonical key:
    /// the first occurrence's position is kept, the last value wins — matching
    /// Python's dict-ordered `set_all`.
    pub fn set_all(&mut self, pairs: Vec<(String, String)>) -> io::Result<()> {
        let mut seen: HashMap<String, (String, String)> = HashMap::new();
        let mut order: Vec<String> = Vec::new();
        for (trigger, expansion) in pairs {
            let canon = self.canonical(&trigger);
            if canon.is_empty() {
                continue;
            }
            if !seen.contains_key(&canon) {
                order.push(canon.clone());
            }
            seen.insert(canon, (trigger, expansion));
        }
        self.pairs = order
            .into_iter()
            .map(|c| seen.remove(&c).expect("ordered canon present"))
            .collect();
        self.rebuild_index();
        self.persist()?;
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }
}

/// Load the RU→EN character map from the exported keycode tables —
/// convenience for the CLI/daemon.
pub fn ru_to_en_map(layouts_json: &Path) -> io::Result<HashMap<char, char>> {
    let mut maps = crate::layouts::LayoutMaps::load(layouts_json)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    Ok(maps.char_map("ru", "en").clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_maps() -> HashMap<char, char> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data/layouts.json");
        ru_to_en_map(&path).unwrap()
    }

    fn temp_store(dir: &tempfile::TempDir) -> SnippetStore {
        SnippetStore::new(dir.path().join("snippets.json"), test_maps())
    }

    #[test]
    fn canonical_layout_independent() {
        let m = test_maps();
        assert_eq!(canonical_with("!test", &m), canonical_with("!еуые", &m));
        assert_eq!(canonical_with("!TEST", &m), canonical_with("!test", &m));
    }

    #[test]
    fn canonical_lowercase() {
        let m = test_maps();
        assert_eq!(canonical_with("Hello", &m), "hello");
        assert_eq!(canonical_with("WORLD", &m), "world");
    }

    #[test]
    fn add_and_match() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = temp_store(&dir);
        store.add("!ee", "user@example.com").unwrap();
        assert_eq!(store.expansion("!ee").as_deref(), Some("user@example.com"));
        assert_eq!(store.expansion("!EE").as_deref(), Some("user@example.com"));
        // Typing !ee while in Russian layout: physical keys produce !уу.
        assert_eq!(store.expansion("!уу").as_deref(), Some("user@example.com"));
    }

    #[test]
    fn no_match() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = temp_store(&dir);
        store.add("!ee", "user@example.com").unwrap();
        assert!(store.expansion("something").is_none());
    }

    #[test]
    fn remove() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = temp_store(&dir);
        store.add("!ee", "user@example.com").unwrap();
        assert!(store.remove("!ee").unwrap());
        assert!(store.expansion("!ee").is_none());
        assert!(!store.remove("!ee").unwrap());
    }

    #[test]
    fn persistence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("snippets.json");
        let mut store1 = SnippetStore::new(&path, test_maps());
        store1.add("!ee", "user@example.com").unwrap();
        store1.add("btw", "by the way").unwrap();

        let store2 = SnippetStore::new(&path, test_maps());
        assert_eq!(store2.len(), 2);
        assert_eq!(store2.expansion("!ee").as_deref(), Some("user@example.com"));
        assert_eq!(store2.expansion("btw").as_deref(), Some("by the way"));
    }

    #[test]
    fn set_all_replaces_and_dedups() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = temp_store(&dir);
        store.add("!old", "old expansion").unwrap();
        store
            .set_all(vec![
                ("!new1".to_string(), "exp1".to_string()),
                ("!new2".to_string(), "exp2".to_string()),
            ])
            .unwrap();
        assert_eq!(store.len(), 2);
        assert!(store.expansion("!old").is_none());
        assert_eq!(store.expansion("!new1").as_deref(), Some("exp1"));
    }

    #[test]
    fn pairs_in_insertion_order() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = temp_store(&dir);
        store.add("a", "1").unwrap();
        store.add("b", "2").unwrap();
        let pairs = store.pairs();
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0], ("a".to_string(), "1".to_string()));
        assert_eq!(pairs[1], ("b".to_string(), "2".to_string()));
    }

    #[test]
    fn layout_independent_matching() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = temp_store(&dir);
        store.add("!ghbdtn", "привет").unwrap();
        // "ghbdtn" typed in EN layout maps to "привет" in RU; the trigger must
        // match regardless of the current layout.
        assert_eq!(store.expansion("!ghbdtn").as_deref(), Some("привет"));
    }

    #[test]
    fn empty_trigger_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = temp_store(&dir);
        store.add("", "nothing").unwrap();
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn set_all_dedups_by_canonical_first_position_last_value() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = temp_store(&dir);
        store
            .set_all(vec![
                ("!test".to_string(), "first".to_string()),
                ("!еуые".to_string(), "second".to_string()), // same canonical as !test
                ("btw".to_string(), "by the way".to_string()),
            ])
            .unwrap();
        // Python dict semantics: the duplicate's *value* (trigger + expansion)
        // replaces the first one at its position.
        assert_eq!(store.len(), 2);
        assert_eq!(
            store.pairs()[0],
            ("!еуые".to_string(), "second".to_string())
        );
        assert_eq!(store.expansion("!test").as_deref(), Some("second"));
        assert_eq!(store.expansion("!еуые").as_deref(), Some("second"));
    }

    #[test]
    fn load_dict_format_sorted_by_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("snippets.json");
        fs::write(&path, r#"{"btw": "by the way", "!ee": "user@example.com"}"#).unwrap();
        let store = SnippetStore::new(&path, test_maps());
        // Python: sorted(data.items()) — "!ee" (0x21) sorts before "btw".
        assert_eq!(
            store.pairs()[0],
            ("!ee".to_string(), "user@example.com".to_string())
        );
        assert_eq!(
            store.pairs()[1],
            ("btw".to_string(), "by the way".to_string())
        );
    }

    #[test]
    fn load_corrupt_file_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("snippets.json");
        fs::write(&path, "{not json!").unwrap();
        let store = SnippetStore::new(&path, test_maps());
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn load_list_skips_invalid_items() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("snippets.json");
        fs::write(
            &path,
            r#"[["!ee", "user@example.com"], ["short"], "not-a-pair", [1, 2]]"#,
        )
        .unwrap();
        let store = SnippetStore::new(&path, test_maps());
        // Non-string values ([1, 2]) and short/invalid items are skipped.
        assert_eq!(store.len(), 1);
        assert_eq!(
            store.pairs()[0],
            ("!ee".to_string(), "user@example.com".to_string())
        );
    }
}
