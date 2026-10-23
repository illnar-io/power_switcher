//! Keycode tables and layout→layout character maps.
//!
//! The tables are exported from `power_switcher/maps/keycode_map.py` by
//! `tools/export_parity_data.py` into `data/layouts.json` — an **ordered array**
//! of `[keycode, lo, hi]` triples per layout.  Row order matters:
//! `build_char_map` walks layout *a*'s rows in insertion order, so a later row
//! wins when two keycodes produce the same character.

use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use serde::Deserialize;

/// `[keycode, unshifted, shifted]` triples, in the Python dict's insertion
/// order.
pub type KeyTable = Vec<(u32, String, String)>;

#[derive(Deserialize)]
struct LayoutsFile(HashMap<String, KeyTable>);

fn first_char(s: &str) -> Option<char> {
    s.chars().next()
}

#[derive(Default)]
pub struct LayoutMaps {
    tables: HashMap<String, KeyTable>,
    by_kc: HashMap<String, HashMap<u32, (char, char)>>,
    char_maps: HashMap<(String, String), HashMap<char, char>>,
    /// char → (keycode, shifted), one per layout, built lazily
    /// (Python `keycode_map._REVERSE_MAPS`).
    reverse_maps: HashMap<String, HashMap<char, (u32, bool)>>,
}

impl LayoutMaps {
    pub fn load(path: &Path) -> Result<Self, Box<dyn Error>> {
        let file: LayoutsFile = serde_json::from_reader(BufReader::new(File::open(path)?))?;
        let mut maps = LayoutMaps::default();
        for (name, table) in file.0 {
            let mut by_kc = HashMap::with_capacity(table.len());
            for (kc, lo, hi) in &table {
                if let (Some(lo), Some(hi)) = (first_char(lo), first_char(hi)) {
                    by_kc.insert(*kc, (lo, hi));
                }
            }
            maps.tables.insert(name.clone(), table);
            maps.by_kc.insert(name, by_kc);
        }
        Ok(maps)
    }

    pub fn layout_names(&self) -> Vec<&str> {
        self.tables.keys().map(|s| s.as_str()).collect()
    }

    /// Lower-cased unshifted letters available in a layout, used as the
    /// alphabet for spelling-correction candidates.
    pub fn alphabet(&self, layout: &str) -> Vec<char> {
        let Some(table) = self.tables.get(layout) else {
            return Vec::new();
        };
        let mut chars: Vec<char> = table
            .iter()
            .filter_map(|(_, lo, _)| first_char(lo))
            .filter(|c| c.is_alphabetic())
            .filter_map(|c| c.to_lowercase().next())
            .collect();
        chars.sort_unstable();
        chars.dedup();
        chars
    }

    /// Precompute the conversion map for a layout pair (idempotent).
    pub fn ensure_char_map(&mut self, from: &str, to: &str) {
        let key = (from.to_string(), to.to_string());
        if self.char_maps.contains_key(&key) {
            return;
        }
        let map = match (self.tables.get(from), self.tables.get(to)) {
            (Some(a), Some(_)) => {
                let b_by_kc = &self.by_kc[to];
                let mut m: HashMap<char, char> = HashMap::new();
                for (kc, a_lo, a_hi) in a {
                    if let Some(&(b_lo, b_hi)) = b_by_kc.get(kc) {
                        if let (Some(al), Some(bl)) = (first_char(a_lo), Some(b_lo)) {
                            m.insert(al, bl);
                        }
                        if let (Some(ah), Some(bh)) = (first_char(a_hi), Some(b_hi)) {
                            m.insert(ah, bh);
                        }
                    }
                }
                m
            }
            // Unknown layout: Python's convert() catches the ValueError and
            // falls back to an identity map.
            _ => HashMap::new(),
        };
        self.char_maps.insert(key, map);
    }

    /// Character map for a pair, building it on first use.
    pub fn char_map(&mut self, from: &str, to: &str) -> &HashMap<char, char> {
        self.ensure_char_map(from, to);
        self.char_maps
            .get(&(from.to_string(), to.to_string()))
            .unwrap()
    }

    /// Character produced by a keycode in a layout (`shifted` selects the
    /// upper row).  `None` when the keycode or the layout is unknown — Python's
    /// `keycode_to_char` returns `""` in those cases.
    pub fn keycode_to_char(&self, kc: u32, layout: &str, shifted: bool) -> Option<char> {
        self.by_kc
            .get(layout)
            .and_then(|m| m.get(&kc))
            .map(|&(lo, hi)| if shifted { hi } else { lo })
    }

    /// (keycode, shifted) needed to type `ch` in `layout`, or `None` when the
    /// character is not on the layout.  Port of `keycode_map.char_to_keycode`:
    /// the table is walked in insertion order and later rows win, exactly like
    /// `build_reverse_map`.
    pub fn char_to_keycode(&mut self, ch: char, layout: &str) -> Option<(u32, bool)> {
        if !self.reverse_maps.contains_key(layout) {
            let mut m: HashMap<char, (u32, bool)> = HashMap::new();
            if let Some(table) = self.tables.get(layout) {
                for (kc, lo, hi) in table {
                    if let Some(lo) = first_char(lo) {
                        m.insert(lo, (*kc, false));
                    }
                    if let Some(hi) = first_char(hi) {
                        m.insert(hi, (*kc, true));
                    }
                }
            }
            self.reverse_maps.insert(layout.to_string(), m);
        }
        self.reverse_maps
            .get(layout)
            .and_then(|m| m.get(&ch))
            .copied()
    }

    /// Re-map every character of `word` from one layout to another.
    /// Characters missing from the map pass through unchanged.
    pub fn convert(&mut self, word: &str, from: &str, to: &str) -> String {
        if from == to {
            return word.to_string();
        }
        let map = self.char_map(from, to);
        word.chars()
            .map(|c| map.get(&c).copied().unwrap_or(c))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest_dir() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
    }

    fn maps() -> LayoutMaps {
        LayoutMaps::load(&manifest_dir().join("data/layouts.json")).unwrap()
    }

    #[test]
    fn en_to_ru_matches_python() {
        let mut m = maps();
        // Same examples verified against the Python engine.
        assert_eq!(m.convert("hello", "en", "ru"), "руддщ");
        assert_eq!(m.convert("руддщ", "ru", "en"), "hello");
        assert_eq!(m.convert("hf,jnf", "en", "ru"), "работа");
        assert_eq!(m.convert("ghbdtn", "en", "ru"), "привет");
    }

    #[test]
    fn unknown_layout_is_identity() {
        let mut m = maps();
        assert_eq!(m.convert("abc", "en", "xx"), "abc");
        assert_eq!(m.convert("abc", "en", "en"), "abc");
    }

    #[test]
    fn punctuation_remaps_like_python() {
        let mut m = maps();
        // In RU layout the comma key is 'б', dot key is 'ю', semicolon 'ж'.
        assert_eq!(m.convert(",", "en", "ru"), "б");
        assert_eq!(m.convert(".", "en", "ru"), "ю");
        assert_eq!(m.convert(";", "en", "ru"), "ж");
    }

    #[test]
    fn keycode_to_char_lookup() {
        let m = maps();
        // KEY_A = 30: en 'a'/'A', ru 'ф'/'Ф'.
        assert_eq!(m.keycode_to_char(30, "en", false), Some('a'));
        assert_eq!(m.keycode_to_char(30, "en", true), Some('A'));
        assert_eq!(m.keycode_to_char(30, "ru", false), Some('ф'));
        assert_eq!(m.keycode_to_char(30, "ru", true), Some('Ф'));
        // Unknown keycode / layout → None (Python returns "").
        assert_eq!(m.keycode_to_char(999, "en", false), None);
        assert_eq!(m.keycode_to_char(30, "xx", false), None);
        // KEY_SLASH = 53: en '/'/'?', ru '.'/','.
        assert_eq!(m.keycode_to_char(53, "ru", false), Some('.'));
        assert_eq!(m.keycode_to_char(53, "ru", true), Some(','));
    }
}
