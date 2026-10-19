//! Multi-layout set: scoring, conversion and the conversion decision.
//!
//! Port of `power_switcher/maps/layout_set.py`.  The decision function
//! [`LayoutSet::find_best_layout`] reproduces the Python branches exactly:
//!
//! 1. exceptions / learned words block conversion;
//! 2. eligibility (length, at least one letter, no ALL-CAPS words);
//! 3. best gain over the other configured layouts;
//! 4. punctuated words convert only when the re-mapped form is a real word;
//! 5. a word that is valid in the current language needs stronger evidence.

use std::collections::HashSet;
use std::error::Error;
use std::path::{Path, PathBuf};

use crate::layouts::LayoutMaps;
use crate::ngram::{NGramDetector, Prepared};

/// Vocabulary bonus applied when a word is in a language's word list.
pub const WORD_BONUS: f64 = 2.5;
/// Scale for the word-frequency term.
pub const WORD_FREQ_ALPHA: f64 = 0.5;
/// Anchor so the frequency term stays positive (`log(p)` is negative).
pub const WORD_LOGPROB_FLOOR: f64 = -13.0;

pub const DEFAULT_THRESHOLD: f64 = 2.5;
pub const DEFAULT_MIN_WORD_LEN: usize = 3;

/// Python's `str.isupper()`: at least one cased character and every cased
/// character is uppercase.  Titlecase letters (Lt) count as cased-not-upper,
/// exactly as in CPython.
fn py_is_upper(s: &str) -> bool {
    let mut has_cased = false;
    for c in s.chars() {
        if c.is_uppercase() {
            has_cased = true;
        } else if c.is_lowercase() || unicode_titlecase(c) {
            return false;
        }
    }
    has_cased
}

/// True for Unicode titlecase letters (category Lt).  Rust's `char` has no
/// `is_titlecase`; Lt is the small, well-known set of digraph letters.
fn unicode_titlecase(c: char) -> bool {
    matches!(u32::from(c),
        0x01C5 | 0x01C8 | 0x01CB | 0x01F2 | 0x1F88..=0x1F8F | 0x1F98..=0x1F9F
        | 0x1FA8..=0x1FAF | 0x1FBC | 0x1FCC | 0x1FFC)
}

/// Python's `str.isalpha()`: non-empty and every character alphabetic.
fn py_is_alpha(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_alphabetic())
}

#[derive(Default)]
pub struct LayoutSet {
    pub layouts: Vec<String>,
    ngram: NGramDetector,
    maps: LayoutMaps,
    /// Words the user taught us never to convert.
    exceptions: HashSet<String>,
    /// Words learned automatically when a conversion is undone.
    learned: HashSet<String>,
    /// `words.json` backing both sets; `None` → in-memory only.
    words_path: Option<PathBuf>,
}

impl LayoutSet {
    /// Build a set from the exported keycode tables plus the generated
    /// n-gram profiles (one `<lang>_ngrams.json[.gz]` per layout, exactly like
    /// `LayoutSet.create` in Python).
    pub fn create(
        layouts: &[&str],
        profiles_dir: &Path,
        layouts_json: &Path,
        words_path: Option<&Path>,
    ) -> Result<Self, Box<dyn Error>> {
        let mut ngram = NGramDetector::new(
            crate::ngram::DEFAULT_MAX_N,
            crate::ngram::DEFAULT_UNKNOWN_PROB,
        );
        for lang in layouts {
            let gz = profiles_dir.join(format!("{lang}_ngrams.json.gz"));
            let plain = profiles_dir.join(format!("{lang}_ngrams.json"));
            let path = if gz.exists() { gz } else { plain };
            if path.exists() {
                if let Err(err) = ngram.load_profile(lang, &path) {
                    // A corrupt profile must not crash the daemon.
                    eprintln!("warning: failed to load profile {}: {err}", path.display());
                }
            } else {
                eprintln!("warning: no n-gram profile for layout {lang:?}");
            }
        }

        let mut maps = LayoutMaps::load(layouts_json)?;
        // Precompute every pair we may need for conversion.
        for a in layouts {
            for b in layouts {
                maps.ensure_char_map(a, b);
            }
        }

        let mut set = LayoutSet {
            layouts: layouts.iter().map(|s| s.to_string()).collect(),
            ngram,
            maps,
            exceptions: HashSet::new(),
            learned: HashSet::new(),
            words_path: words_path.map(Path::to_path_buf),
        };
        set.load_words(words_path);
        Ok(set)
    }

    /// Load `words.json` (exceptions + learned words).  Missing or corrupt
    /// files are ignored, matching the Python loader.
    pub fn load_words(&mut self, path: Option<&Path>) {
        let Some(path) = path else { return };
        let Ok(text) = std::fs::read_to_string(path) else {
            return;
        };
        let Ok(data) = serde_json::from_str::<serde_json::Value>(&text) else {
            return;
        };
        if let Some(items) = data.get("exceptions").and_then(|v| v.as_array()) {
            for it in items {
                if let Some(w) = it.as_str() {
                    if w.chars().count() >= 2 {
                        self.exceptions.insert(w.to_lowercase());
                    }
                }
            }
        }
        if let Some(items) = data.get("learned_words").and_then(|v| v.as_array()) {
            for it in items {
                if let Some(w) = it.as_str() {
                    if w.chars().count() >= 2 {
                        self.learned.insert(w.to_lowercase());
                    }
                }
            }
        }
    }

    pub fn add_exception(&mut self, word: &str) {
        if word.chars().count() < 2 {
            return;
        }
        self.exceptions.insert(word.to_lowercase());
        self.save_words();
    }

    pub fn add_learned(&mut self, word: &str) {
        if word.chars().count() < 2 {
            return;
        }
        self.learned.insert(word.to_lowercase());
        self.save_words();
    }

    pub fn remove_exception(&mut self, word: &str) -> bool {
        if self.exceptions.remove(&word.to_lowercase()) {
            self.save_words();
            true
        } else {
            false
        }
    }

    pub fn remove_learned(&mut self, word: &str) -> bool {
        if self.learned.remove(&word.to_lowercase()) {
            self.save_words();
            true
        } else {
            false
        }
    }

    pub fn exceptions(&self) -> Vec<String> {
        let mut v: Vec<String> = self.exceptions.iter().cloned().collect();
        v.sort();
        v
    }

    pub fn learned_words(&self) -> Vec<String> {
        let mut v: Vec<String> = self.learned.iter().cloned().collect();
        v.sort();
        v
    }

    /// Persist `words.json`, exactly like Python's `LayoutSet._save_state`
    /// (`{"exceptions": [...], "learned_words": [...]}`, sorted, pretty).
    /// Write failures are logged and ignored — learning must never break typing.
    pub fn save_words(&self) {
        let Some(path) = &self.words_path else { return };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let payload = serde_json::json!({
            "exceptions": self.exceptions(),
            "learned_words": self.learned_words(),
        });
        match serde_json::to_string_pretty(&payload) {
            Ok(text) => {
                if let Err(err) = std::fs::write(path, text) {
                    eprintln!("warning: failed to save {}: {err}", path.display());
                }
            }
            Err(err) => eprintln!("warning: failed to serialise words: {err}"),
        }
    }

    pub fn is_exception(&self, word: &str) -> bool {
        let w = word.to_lowercase();
        self.exceptions.contains(&w) || self.learned.contains(&w)
    }

    pub fn is_known_word(&self, word: &str, lang: &str) -> bool {
        self.ngram.is_known_word(word, lang)
    }

    /// Eligibility gate, port of `LayoutSet._eligible`.
    pub fn is_eligible(word: &str, min_word_len: usize) -> bool {
        let n = word.chars().count();
        if n < min_word_len {
            return false;
        }
        // At least one letter; digits/punctuation are allowed alongside.
        if !word.chars().any(|c| c.is_alphabetic()) {
            return false;
        }
        // Abbreviations typed in caps are never converted.
        if py_is_upper(word) && n > 1 {
            return false;
        }
        true
    }

    fn word_term_prepared(&self, prepared: &Prepared, lang: &str) -> f64 {
        match self.ngram.word_logprob_prepared(prepared, lang) {
            None => 0.0,
            Some(lp) => WORD_BONUS + WORD_FREQ_ALPHA * (lp - WORD_LOGPROB_FLOOR),
        }
    }

    pub fn ngram_score_prepared(&self, prepared: &Prepared, lang: &str) -> f64 {
        self.ngram.score_prepared(prepared, lang)
    }

    pub fn decision_score_prepared(&self, prepared: &Prepared, lang: &str) -> f64 {
        self.ngram.score_prepared(prepared, lang) + self.word_term_prepared(prepared, lang)
    }

    /// Composite score: n-gram plausibility plus a vocabulary-frequency bonus.
    pub fn decision_score(&self, word: &str, lang: &str) -> f64 {
        let prepared = Prepared::new(word);
        self.decision_score_prepared(&prepared, lang)
    }

    /// N-gram score only (as `LayoutSet.score_word`).
    pub fn score_word(&self, word: &str, lang: &str) -> f64 {
        self.ngram.score(word, lang)
    }

    /// Re-map a word between two configured layouts.
    pub fn convert(&mut self, word: &str, from: &str, to: &str) -> String {
        self.maps.convert(word, from, to)
    }

    /// Character produced by `kc` in `layout` (port of `LayoutSet.keycode_to_char`;
    /// unknown keycode/layout → `None` where Python returns `""`).
    pub fn keycode_to_char(&self, kc: u16, layout: &str, shifted: bool) -> Option<char> {
        self.maps.keycode_to_char(u32::from(kc), layout, shifted)
    }

    /// (keycode, shifted) to type `ch` in `layout` — port of
    /// `LayoutSet.char_to_keycode`; `None` when the character is not on the
    /// layout.
    pub fn char_to_keycode(&mut self, ch: char, layout: &str) -> Option<(u16, bool)> {
        self.maps
            .char_to_keycode(ch, layout)
            .map(|(kc, shifted)| (kc as u16, shifted))
    }

    /// Clone of the layout→layout character map (Python `build_char_map`) —
    /// used for snippet canonicalisation.  Unknown layouts yield an empty
    /// (identity) map, matching Python's `except ValueError: {}`.
    pub fn char_map_clone(
        &mut self,
        from: &str,
        to: &str,
    ) -> std::collections::HashMap<char, char> {
        self.maps.char_map(from, to).clone()
    }

    /// True when `kc` produces a letter in some configured layout.
    ///
    /// The comma key is ',' in English but 'б' in Russian; the semicolon key
    /// is ';' but 'ж'.  Treating such a key as a word separator would chop a
    /// word typed in the wrong layout in half, so they never end a word.
    /// Port of `KeyProcessor._ambiguous_separator` (it only depends on the
    /// layout tables, so it lives here).
    pub fn ambiguous_separator(&self, kc: u16) -> bool {
        self.layouts.iter().any(|lay| {
            self.maps
                .keycode_to_char(u32::from(kc), lay, false)
                .is_some_and(|c| c.is_alphabetic())
        })
    }

    /// Heuristic: strings that look like passwords are never converted.
    /// Port of `LayoutSet.is_password`; see [`crate::text::is_password`].
    pub fn is_password(&self, word: &str) -> bool {
        crate::text::is_password(word)
    }

    /// Return the layout `word` should be re-typed in, or `None`.
    ///
    /// Exact port of `LayoutSet.find_best_layout`.
    pub fn find_best_layout(
        &mut self,
        word: &str,
        current_layout: &str,
        threshold: f64,
        min_word_len: usize,
    ) -> Option<String> {
        if self.is_exception(word) {
            return None;
        }
        if !Self::is_eligible(word, min_word_len) {
            return None;
        }

        let prepared = Prepared::new(word);
        let current_score = self.decision_score_prepared(&prepared, current_layout);
        let current_known = self
            .ngram
            .word_logprob_prepared(&prepared, current_layout)
            .is_some();

        let mut best: Option<&str> = None;
        let mut best_gain = 0.0f64;
        let mut best_converted = String::new();

        for other in &self.layouts {
            if other == current_layout || !self.ngram.has_language(other) {
                continue;
            }
            let converted = self.maps.convert(word, current_layout, other);
            let cp = Prepared::new(&converted);
            let gain = self.decision_score_prepared(&cp, other) - current_score;
            if gain > best_gain {
                best_gain = gain;
                best = Some(other);
                best_converted = converted;
            }
        }

        let best = best?;

        // Words containing characters that are punctuation here but letters in
        // another layout (',' is 'б') only convert when the re-mapped form is a
        // real word of the target language — protects "don't" / "v1.2".
        if !py_is_alpha(word) {
            if self.is_known_word(&best_converted, best) && best_gain >= threshold.max(2.5) {
                return Some(best.to_string());
            }
            return None;
        }

        // A valid word of the current language needs much stronger evidence.
        if current_known {
            let required = (threshold + 1.5).max(3.5);
            let target_known = self.is_known_word(&best_converted, best);
            if !(target_known && best_gain >= required) {
                return None;
            }
        } else if best_gain < threshold {
            return None;
        }

        Some(best.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest_dir() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
    }

    fn test_set() -> LayoutSet {
        LayoutSet::create(
            &["en", "ru"],
            &manifest_dir().join("../../power_switcher/detection/profiles"),
            &manifest_dir().join("data/layouts.json"),
            None,
        )
        .unwrap()
    }

    #[test]
    fn eligibility_matches_python() {
        assert!(!LayoutSet::is_eligible("ab", 3)); // too short
        assert!(!LayoutSet::is_eligible("123", 3)); // no letters
        assert!(!LayoutSet::is_eligible("HTTP", 3)); // caps
        assert!(LayoutSet::is_eligible("Hello", 3));
        assert!(LayoutSet::is_eligible("hf,jnf", 3)); // letters + punctuation
        assert!(LayoutSet::is_eligible("v1.2", 3));
    }

    #[test]
    fn caps_detection_matches_python() {
        assert!(py_is_upper("HTTP"));
        assert!(py_is_upper("ПРИВЕТ"));
        assert!(py_is_upper("A"));
        assert!(!py_is_upper("Hello"));
        assert!(!py_is_upper("don't")); // apostrophe is uncased, but 'o' is lower
        assert!(!py_is_upper("v1.2"));
    }

    #[test]
    fn wrong_layout_russian_is_detected() {
        let mut ls = test_set();
        assert_eq!(
            ls.find_best_layout("hf,jnf", "en", DEFAULT_THRESHOLD, DEFAULT_MIN_WORD_LEN),
            Some("ru".into())
        );
        assert_eq!(
            ls.find_best_layout("ghbdtn", "en", DEFAULT_THRESHOLD, DEFAULT_MIN_WORD_LEN),
            Some("ru".into())
        );
    }

    #[test]
    fn real_english_words_stay() {
        let mut ls = test_set();
        assert_eq!(
            ls.find_best_layout("hello", "en", DEFAULT_THRESHOLD, DEFAULT_MIN_WORD_LEN),
            None
        );
        assert_eq!(
            ls.find_best_layout("don't", "en", DEFAULT_THRESHOLD, DEFAULT_MIN_WORD_LEN),
            None
        );
        assert_eq!(
            ls.find_best_layout("v1.2", "en", DEFAULT_THRESHOLD, DEFAULT_MIN_WORD_LEN),
            None
        );
        assert_eq!(
            ls.find_best_layout("HTTP", "en", DEFAULT_THRESHOLD, DEFAULT_MIN_WORD_LEN),
            None
        );
    }

    #[test]
    fn exceptions_and_learned_words_block_conversion() {
        let mut ls = test_set();
        ls.add_exception("hf,jnf");
        assert_eq!(
            ls.find_best_layout("hf,jnf", "en", DEFAULT_THRESHOLD, DEFAULT_MIN_WORD_LEN),
            None
        );
        ls.add_learned("ghbdtn");
        assert_eq!(
            ls.find_best_layout("ghbdtn", "en", DEFAULT_THRESHOLD, DEFAULT_MIN_WORD_LEN),
            None
        );
    }

    #[test]
    fn ambiguous_separator_with_en_ru() {
        let ls = test_set();
        use crate::key_buffer::{KEY_COMMA, KEY_SEMICOLON, KEY_SLASH, KEY_SPACE};
        // ',' is 'б' in RU → letter in some layout → ambiguous, never a boundary.
        assert!(ls.ambiguous_separator(KEY_COMMA));
        // ';' is 'ж' in RU.
        assert!(ls.ambiguous_separator(KEY_SEMICOLON));
        // '/' is '.', ',' in RU → punctuation in every layout → real boundary.
        assert!(!ls.ambiguous_separator(KEY_SLASH));
        // Not in any table.
        assert!(!ls.ambiguous_separator(KEY_SPACE));
    }

    #[test]
    fn ambiguous_separator_en_only_is_not_ambiguous() {
        let ls = LayoutSet::create(
            &["en"],
            &manifest_dir().join("../../power_switcher/detection/profiles"),
            &manifest_dir().join("data/layouts.json"),
            None,
        )
        .unwrap();
        use crate::key_buffer::{KEY_COMMA, KEY_SEMICOLON};
        // In EN only, ',' and ';' are punctuation → they end words.
        assert!(!ls.ambiguous_separator(KEY_COMMA));
        assert!(!ls.ambiguous_separator(KEY_SEMICOLON));
    }

    #[test]
    fn keycode_to_char_on_layout_set() {
        let ls = test_set();
        assert_eq!(ls.keycode_to_char(30, "en", false), Some('a'));
        assert_eq!(ls.keycode_to_char(30, "ru", true), Some('Ф'));
        assert_eq!(ls.keycode_to_char(999, "en", false), None);
    }

    #[test]
    fn password_heuristic_on_layout_set() {
        let ls = test_set();
        assert!(ls.is_password("Aa1!xyz"));
        assert!(!ls.is_password("приветик"));
    }

    #[test]
    fn learned_word_file_loads() {
        let dir = std::env::temp_dir().join("ps_core_test_words.json");
        std::fs::write(
            &dir,
            r#"{"exceptions": ["nj;t"], "learned_words": ["k.lb"]}"#,
        )
        .unwrap();
        let mut ls = LayoutSet::create(
            &["en", "ru"],
            &manifest_dir().join("../../power_switcher/detection/profiles"),
            &manifest_dir().join("data/layouts.json"),
            Some(&dir),
        )
        .unwrap();
        assert_eq!(
            ls.find_best_layout("nj;t", "en", DEFAULT_THRESHOLD, DEFAULT_MIN_WORD_LEN),
            None
        );
        assert_eq!(
            ls.find_best_layout("k.lb", "en", DEFAULT_THRESHOLD, DEFAULT_MIN_WORD_LEN),
            None
        );
    }

    #[test]
    fn learned_words_are_persisted() {
        // Regression: Rust kept learned words in memory only, so a word the
        // user reverted came back on the next daemon start (Python persists).
        let dir = std::env::temp_dir().join(format!(
            "ps_core_learned_persist_{}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&dir);
        let mut ls = LayoutSet::create(
            &["en", "ru"],
            &manifest_dir().join("../../power_switcher/detection/profiles"),
            &manifest_dir().join("data/layouts.json"),
            Some(&dir),
        )
        .unwrap();
        ls.add_learned("ghbdtn");
        ls.add_exception("nj;t");

        let raw = std::fs::read_to_string(&dir).unwrap();
        let data: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(data["learned_words"], serde_json::json!(["ghbdtn"]));
        assert_eq!(data["exceptions"], serde_json::json!(["nj;t"]));

        // Reloading picks them up, so conversion is blocked after a restart.
        let mut reloaded = LayoutSet::create(
            &["en", "ru"],
            &manifest_dir().join("../../power_switcher/detection/profiles"),
            &manifest_dir().join("data/layouts.json"),
            Some(&dir),
        )
        .unwrap();
        assert!(reloaded.is_exception("ghbdtn"));
        assert!(reloaded.is_exception("nj;t"));
        assert!(reloaded.remove_learned("ghbdtn"));
        assert!(!reloaded.is_exception("ghbdtn"));
        assert!(!reloaded.remove_learned("ghbdtn"));
        let _ = std::fs::remove_file(&dir);
    }
}
