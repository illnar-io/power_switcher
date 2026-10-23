//! Multi-layout set: scoring, conversion and the conversion decision.
//!
//! Port of `power_switcher/maps/layout_set.py`.  The decision function
//! [`LayoutSet::find_best_layout`] reproduces the Python branches exactly:
//!
//! 1. exceptions / learned words block conversion;
//! 2. eligibility (length, at least one letter, no ALL-CAPS words);
//! 3. best gain over the other configured layouts;
//! 4. punctuated words convert only when the re-mapped form is a real word;
//! 5. a word that is valid in the current language needs stronger evidence;
//! 6. an unknown target word is accepted only when the typed text is not a
//!    keyboard run (`fghjkl`, `zxcvbnm`) — this keeps technical terms absent
//!    from the 20k vocabulary ("plugin", "раскладка") convertible;
//! 7. a word in neither vocabulary is also accepted when the typed form is
//!    implausible in the current language and the re-mapped form is plausible
//!    in the target language (Russian IT jargon typed in the EN layout:
//!    `фронтенд`, `плагин`, `сниппет`).

use std::collections::HashMap;
use std::error::Error;
use std::path::Path;

use log::debug;

use crate::layouts::LayoutMaps;
use crate::ngram::{NGramDetector, Prepared};
use crate::word_store::WordStore;

/// Vocabulary bonus applied when a word is in a language's word list.
pub const WORD_BONUS: f64 = 2.5;
/// Scale for the word-frequency term.
pub const WORD_FREQ_ALPHA: f64 = 0.5;
/// Anchor so the frequency term stays positive (`log(p)` is negative).
pub const WORD_LOGPROB_FLOOR: f64 = -13.0;
/// Extra margin when neither the typed word nor the re-mapped form is in the
/// vocabulary: n-grams alone need slightly stronger evidence (blocks `lrkf`,
/// keeps `plugin`).
pub const UNKNOWN_TARGET_MARGIN: f64 = 0.3;
/// Absolute target n-gram floor for a word that is in neither vocabulary.
/// Calibrated on the EN/RU profiles: Russian words typed in the EN layout sit
/// above it (`фронтенд` -11.1, `плагин` -11.3, `сниппет` -11.9, `бэкенд` -12.3)
/// while valid English technical terms remapped to Russian stay below -13.5.
/// shortcut: calibrated on EN/RU profiles, recalibrate if they change.
pub const UNKNOWN_TARGET_FLOOR: f64 = -12.5;
/// Minimum gain for the absolute-floor path.
pub const UNKNOWN_TARGET_MIN_GAIN: f64 = 1.5;
/// The typed form must also be implausible in the current language.  Valid
/// English words absent from the vocabulary score around -8..-12, so this keeps
/// them from flipping to a spurious Russian n-gram match.
pub const UNKNOWN_SOURCE_CEILING: f64 = -12.0;
/// `live_fix` asks for a 15.0 gain to mean "overwhelming evidence"; the
/// absolute-floor path must not lower that explicit bar.
const ABSOLUTE_FLOOR_MAX_THRESHOLD: f64 = 10.0;

pub const DEFAULT_THRESHOLD: f64 = 2.5;
pub const DEFAULT_MIN_WORD_LEN: usize = 3;

// Relative likelihoods for the single edit that turned the intended word into
// what the user typed.  Corrected typing errors are overwhelmingly single
// edits (91.8% in Tachibana 2016); substitutions dominate, and roughly a third
// of corrected errors are adjacent-key slips (Pinet & Nozari 2022; Kukich
// 1992).  A non-adjacent consonant swap is usually a different word
// (`coding` → `coming`), not a typo, so it is kept below MIN_EDIT_WEIGHT.
const EDIT_ADJACENT: f64 = 1.0;
const EDIT_VOWEL: f64 = 0.6;
const EDIT_TRANSPOSE: f64 = 0.85;
/// The typed word is missing a letter (`helo` → `hello`).
const EDIT_OMISSION: f64 = 0.8;
/// The typed word has an extra letter (`helllo` → `hello`).
const EDIT_INSERTION: f64 = 0.6;
/// Same row, two keys apart.
const EDIT_NEAR: f64 = 0.35;
const EDIT_FAR: f64 = 0.05;
/// Below this the edit is too unlikely to be a typo even when the result is a
/// common word.
const MIN_EDIT_WEIGHT: f64 = 0.1;

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
    /// Manual exceptions + words learned from undo (`words.json`).
    words: WordStore,
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

        let set = LayoutSet {
            layouts: layouts.iter().map(|s| s.to_string()).collect(),
            ngram,
            maps,
            words: WordStore::load(words_path),
        };
        Ok(set)
    }

    /// Load `words.json` (exceptions + learned words), replacing the current
    /// sets.  Missing or corrupt files are ignored, matching the Python loader.
    pub fn load_words(&mut self, path: Option<&Path>) {
        self.words.set_path(path.map(Path::to_path_buf));
    }

    pub fn add_exception(&mut self, word: &str) {
        self.words.add_exception(word);
    }

    pub fn add_learned(&mut self, word: &str) {
        self.words.add_learned(word);
    }

    pub fn remove_exception(&mut self, word: &str) -> bool {
        self.words.remove_exception(word)
    }

    pub fn remove_learned(&mut self, word: &str) -> bool {
        self.words.remove_learned(word)
    }

    pub fn exceptions(&self) -> Vec<String> {
        self.words.exceptions()
    }

    pub fn learned_words(&self) -> Vec<String> {
        self.words.learned_words()
    }

    /// Persist `words.json`, exactly like Python's `LayoutSet._save_state`
    /// (`{"exceptions": [...], "learned_words": [...]}`, sorted, pretty).
    pub fn save_words(&self) {
        self.words.save();
    }

    pub fn is_exception(&self, word: &str) -> bool {
        self.words.is_exception(word)
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

    /// Conservative single-edit autocorrect for a word in `lang`.
    ///
    /// Only alphabetic words of 4+ characters that are unknown in the
    /// vocabulary are considered.  Candidates are ranked by vocabulary
    /// frequency plus a keyboard-aware error prior: adjacent-key
    /// substitutions and transpositions are likely, non-adjacent consonant
    /// substitutions are not (they usually mean the typed word is jargon or
    /// another real word).  The replacement must be a known word at least as
    /// common as the vocabulary median, with no ambiguous second candidate
    /// (1.5× frequency margin).  Learned/exception words and keyboard runs are
    /// never touched.  Returns the corrected word with the original leading
    /// capital preserved.
    pub fn correct_spelling(
        &mut self,
        word: &str,
        lang: &str,
        sentence_start: bool,
    ) -> Option<String> {
        let chars: Vec<char> = word.to_lowercase().chars().collect();
        if !(4..=24).contains(&chars.len()) || !chars.iter().all(|c| c.is_alphabetic()) {
            return None;
        }
        // Mid-sentence capitals are names/acronyms (`Groq` → `Grow`); a
        // leading capital is only accepted at a sentence start (`Thsi` →
        // `This`).  All-caps is rejected by both branches.
        let mut first = true;
        for c in word.chars() {
            if c.is_uppercase() && (!first || !sentence_start) {
                return None;
            }
            first = false;
        }
        let lower: String = chars.iter().collect();
        if self.words.is_exception(&lower)
            || self.ngram.is_known_word(&lower, lang)
            || self.is_keyboard_run(&lower, lang)
        {
            return None;
        }

        let cutoff = self.ngram.word_cutoff(lang)?;
        let alphabet = self.maps.alphabet(lang);
        let n = chars.len();
        let mut candidates: HashMap<String, f64> = HashMap::new();
        let mut add = |candidate: String, weight: f64| {
            candidates
                .entry(candidate)
                .and_modify(|old| *old = old.max(weight))
                .or_insert(weight);
        };

        // Edits at position 0 are skipped: technical words often start with an
        // extra letter (`uinput` → `input`), and first-letter typos are rare.
        for i in 1..n {
            let mut w = chars.clone();
            w.remove(i);
            add(w.iter().collect(), EDIT_INSERTION);

            for &a in &alphabet {
                if a != chars[i] {
                    let mut w = chars.clone();
                    w[i] = a;
                    add(
                        w.iter().collect(),
                        self.substitution_weight(chars[i], a, lang),
                    );
                }
                let mut w = chars.clone();
                w.insert(i, a);
                let doubled = chars.get(i.wrapping_sub(1)) == Some(&a) || chars.get(i) == Some(&a);
                add(
                    w.iter().collect(),
                    if doubled {
                        EDIT_OMISSION + 0.1
                    } else {
                        EDIT_OMISSION
                    },
                );
            }
        }
        for &a in &alphabet {
            let mut w = chars.clone();
            w.push(a);
            add(
                w.iter().collect(),
                if chars.last() == Some(&a) {
                    EDIT_OMISSION + 0.1
                } else {
                    EDIT_OMISSION
                },
            );
        }
        for i in 0..n.saturating_sub(1) {
            if chars[i] != chars[i + 1] {
                let mut w = chars.clone();
                w.swap(i, i + 1);
                add(w.iter().collect(), EDIT_TRANSPOSE);
            }
        }

        let cutoff_log = cutoff.ln();
        let mut ranked: Vec<(f64, f64, String)> = candidates
            .into_iter()
            .filter(|(c, weight)| {
                c.chars().count() >= 3 && c != &lower && *weight >= MIN_EDIT_WEIGHT
            })
            .filter_map(|(c, weight)| {
                self.ngram
                    .word_logprob(&c, lang)
                    .filter(|logp| *logp >= cutoff_log)
                    .map(|logp| (logp + weight.ln(), weight, c))
            })
            .collect();
        ranked.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.2.cmp(&b.2)));
        let (best_score, best_weight, best_word) = ranked.first()?;
        if let Some((second_score, _, _)) = ranked.get(1) {
            if *best_score - *second_score < 1.5f64.ln() {
                debug!(
                    "autocorrect ambiguous ({} -> {} chars) weight={best_weight:.2} margin={:.2}",
                    word.chars().count(),
                    best_word.chars().count(),
                    *best_score - *second_score
                );
                return None;
            }
        }

        let mut out = String::with_capacity(best_word.len());
        for (i, c) in best_word.chars().enumerate() {
            if i == 0 && word.chars().next().is_some_and(char::is_uppercase) {
                out.extend(c.to_uppercase());
            } else {
                out.push(c);
            }
        }
        debug!(
            "autocorrect candidate ({} -> {} chars) ({lang}, weight={best_weight:.2})",
            word.chars().count(),
            out.chars().count()
        );
        Some(out)
    }

    /// Prior for a single-character substitution, based on the physical
    /// keyboard distance and the vowel/consonant class.
    fn substitution_weight(&mut self, typed: char, intended: char, lang: &str) -> f64 {
        let typed_key = self.maps.char_to_keycode(typed, lang);
        let intended_key = self.maps.char_to_keycode(intended, lang);
        if let (Some((typed_kc, _)), Some((intended_kc, _))) = (typed_key, intended_key) {
            if let (Some(a), Some(b)) = (key_position(typed_kc), key_position(intended_kc)) {
                if keys_adjacent(a, b) {
                    return EDIT_ADJACENT;
                }
                if a.0 == b.0 && a.1.abs_diff(b.1) == 2 {
                    return EDIT_NEAR;
                }
            }
        }
        if is_vowel(typed) && is_vowel(intended) {
            EDIT_VOWEL
        } else {
            EDIT_FAR
        }
    }

    /// True when consecutive characters sit on adjacent keys of one keyboard
    /// row (`fghjkl`, `zxcvbnm`, `vbnm`).  Such runs give high n-gram gains but
    /// are mashing, not words.
    pub fn is_keyboard_run(&mut self, word: &str, layout: &str) -> bool {
        let mut prev: Option<(u8, u8)> = None;
        let mut pairs = 0usize;
        let mut adjacent = 0usize;
        for ch in word.chars() {
            let Some((kc, _)) = self.maps.char_to_keycode(ch, layout) else {
                return false;
            };
            let Some(pos) = key_position(kc) else {
                return false;
            };
            if let Some(prev) = prev {
                pairs += 1;
                if prev.0 == pos.0 && prev.1.abs_diff(pos.1) == 1 {
                    adjacent += 1;
                }
            }
            prev = Some(pos);
        }
        pairs >= 2 && adjacent * 5 >= pairs * 4
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
            debug!(
                "gate: learned word/exception ({} chars)",
                word.chars().count()
            );
            return None;
        }
        if !Self::is_eligible(word, min_word_len) {
            debug!(
                "gate: ineligible ({} chars, min_word_len={min_word_len})",
                word.chars().count()
            );
            return None;
        }

        let prepared = Prepared::new(word);
        let current_score = self.decision_score_prepared(&prepared, current_layout);
        let current_known = self
            .ngram
            .word_logprob_prepared(&prepared, current_layout)
            .is_some();
        // Computed up front: `best` later borrows from `self.layouts`.
        let keyboard_run = py_is_alpha(word) && self.is_keyboard_run(word, current_layout);

        let mut best: Option<&str> = None;
        let mut best_gain = 0.0f64;
        let mut best_target_score = f64::NEG_INFINITY;
        let mut best_converted = String::new();

        for other in &self.layouts {
            if other == current_layout || !self.ngram.has_language(other) {
                continue;
            }
            let converted = self.maps.convert(word, current_layout, other);
            let cp = Prepared::new(&converted);
            let target_score = self.decision_score_prepared(&cp, other);
            let gain = target_score - current_score;
            if gain > best_gain {
                best_gain = gain;
                best_target_score = target_score;
                best = Some(other);
                best_converted = converted;
            }
        }

        let best = best?.to_string();
        let priority_pair = matches!((current_layout, best.as_str()), ("en", "ru") | ("ru", "en"));

        // Words containing characters that are punctuation here but letters in
        // another layout (',' is 'б', '.' is 'ю') only convert when the
        // re-mapped form is a real word of the target language — protects
        // "don't" / "v1.2".
        if !py_is_alpha(word) {
            let target_known = self.is_known_word(&best_converted, &best);
            if target_known && best_gain >= threshold.max(2.5) {
                return Some(best);
            }
            // A Russian word typed in the EN layout keeps ','/'.' where the
            // user meant 'б'/'ю'.  When the whole re-mapped form is letters,
            // treat it as a wrong-layout word rather than punctuation; real
            // punctuation words ("don't", "e.g.") stay below the floor.
            if best_converted.chars().all(|c| c.is_alphabetic())
                && threshold < ABSOLUTE_FLOOR_MAX_THRESHOLD
                && self.plausible_unknown_target(
                    word,
                    &best_converted,
                    &best,
                    priority_pair,
                    (current_score, best_target_score, best_gain),
                )
            {
                return Some(best);
            }
            debug!(
                "gate: punctuation ({} -> {} chars) known={target_known} gain={best_gain:.2}",
                word.chars().count(),
                best_converted.chars().count()
            );
            return None;
        }

        // A valid word of the current language needs much stronger evidence.
        if current_known {
            let required = (threshold + 1.5).max(3.5);
            let target_known = self.is_known_word(&best_converted, &best);
            if !(target_known && best_gain >= required) {
                debug!(
                    "gate: known {current_layout} word ({} -> {} chars); \
                     known_in_{best}={target_known} gain={best_gain:.2} \
                     required={required:.2}",
                    word.chars().count(),
                    best_converted.chars().count()
                );
                return None;
            }
        } else {
            let target_known = self.is_known_word(&best_converted, &best);
            if target_known {
                if best_gain < threshold {
                    debug!(
                        "gate: unknown in {current_layout} ({} -> {} chars); \
                         known_in_{best} but gain={best_gain:.2} \
                         threshold={threshold:.2}",
                        word.chars().count(),
                        best_converted.chars().count()
                    );
                    return None;
                }
            } else if keyboard_run
                || (best_gain < threshold + UNKNOWN_TARGET_MARGIN
                    && (threshold >= ABSOLUTE_FLOOR_MAX_THRESHOLD
                        || !self.plausible_unknown_target(
                            word,
                            &best_converted,
                            &best,
                            priority_pair,
                            (current_score, best_target_score, best_gain),
                        )))
            {
                // The 20k vocabulary misses technical terms, names and rare
                // inflections.  With no vocabulary hit on either side, n-gram
                // evidence decides — but keyboard runs ("fghjkl",
                // "zxcvbnm", "vbn") produce high n-gram gains for adjacent
                // keys and must never flip the layout.
                debug!(
                    "gate: unknown in {current_layout} ({} -> {} chars); \
                     unknown_in_{best} gain={best_gain:.2} \
                     target={best_target_score:.2} source={current_score:.2} \
                     threshold={threshold:.2} keyboard_run={keyboard_run}",
                    word.chars().count(),
                    best_converted.chars().count()
                );
                return None;
            }
        }

        Some(best)
    }

    /// Accept a word that is in neither vocabulary when the typed form is
    /// implausible in the current language and the re-mapped form is plausible
    /// in the target language.
    ///
    /// Russian IT jargon absent from the 20k list (`фронтенд`, `плагин`,
    /// `сниппет`) has too small a relative gain, but the absolute scores are
    /// decisive.  A length floor and the target keyboard-run check keep
    /// mashing (`lrkf` → `дкла`) out.
    fn plausible_unknown_target(
        &mut self,
        typed: &str,
        converted: &str,
        target_layout: &str,
        priority_pair: bool,
        (source_score, target_score, gain): (f64, f64, f64),
    ) -> bool {
        // The absolute floors are calibrated on the EN/RU profiles; the other
        // six layouts keep the old relative-gain gate.
        priority_pair
            && source_score <= UNKNOWN_SOURCE_CEILING
            && target_score >= UNKNOWN_TARGET_FLOOR
            && gain >= UNKNOWN_TARGET_MIN_GAIN
            // Wrong-layout Russian maps Russian vowels to Latin consonants
            // (`а`→f, `о`→j), so the typed form cannot be required to contain
            // a vowel.  The length floor keeps short mashing (`lrkf` → `дкла`)
            // out; the target must still look pronounceable.
            && typed.chars().count() >= 5
            && converted.chars().any(is_vowel)
            && !self.is_keyboard_run(converted, target_layout)
    }
}

/// Physical (row, column) of a letter key, for the keyboard-run guard.
fn key_position(kc: u32) -> Option<(u8, u8)> {
    const ROWS: [&[u32]; 3] = [
        &[16, 17, 18, 19, 20, 21, 22, 23, 24, 25], // q w e r t y u i o p
        &[30, 31, 32, 33, 34, 35, 36, 37, 38],     // a s d f g h j k l
        &[44, 45, 46, 47, 48, 49, 50],             // z x c v b n m
    ];
    for (row, keys) in ROWS.iter().enumerate() {
        if let Some(col) = keys.iter().position(|&k| k == kc) {
            return Some((row as u8, col as u8));
        }
    }
    None
}

/// True when two letter keys touch (including the QWERTY stagger between
/// rows).
fn keys_adjacent(a: (u8, u8), b: (u8, u8)) -> bool {
    match a.0.abs_diff(b.0) {
        0 => a.1.abs_diff(b.1) == 1,
        1 => a.1.abs_diff(b.1) <= 1,
        _ => false,
    }
}

/// Vowel class for the languages PowerSwitcher ships profiles for.  Used only
/// as a soft prior: vowel↔vowel slips (`campare` → `compare`) are more likely
/// than a far consonant swap.
fn is_vowel(c: char) -> bool {
    let c = c.to_lowercase().next().unwrap_or(c);
    matches!(
        c,
        'a' | 'e'
            | 'i'
            | 'o'
            | 'u'
            | 'y'
            | 'а'
            | 'е'
            | 'ё'
            | 'и'
            | 'о'
            | 'у'
            | 'ы'
            | 'э'
            | 'ю'
            | 'я'
            | 'і'
            | 'ї'
            | 'є'
            | 'ä'
            | 'ö'
            | 'ü'
            | 'é'
            | 'è'
            | 'ê'
            | 'ë'
            | 'à'
            | 'â'
            | 'î'
            | 'ï'
            | 'ô'
            | 'û'
            | 'ù'
            | 'œ'
            | 'æ'
            | 'ą'
            | 'ę'
            | 'ó'
    )
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
    fn spell_correction_is_conservative() {
        let mut ls = test_set();
        assert_eq!(
            ls.correct_spelling("привт", "ru", true).as_deref(),
            Some("привет")
        );
        assert_eq!(
            ls.correct_spelling("Привт", "ru", true).as_deref(),
            Some("Привет")
        );
        assert_eq!(
            ls.correct_spelling("wprld", "en", true).as_deref(),
            Some("world")
        );
        // Transpositions and adjacent-key substitutions are the common typos.
        assert_eq!(
            ls.correct_spelling("recieve", "en", true).as_deref(),
            Some("receive")
        );
        assert_eq!(
            ls.correct_spelling("campare", "en", true).as_deref(),
            Some("compare")
        );
        // Known words, abbreviations, technical terms and ambiguous edits stay.
        assert_eq!(ls.correct_spelling("привет", "ru", true), None);
        assert_eq!(ls.correct_spelling("ЛИНУКС", "ru", true), None);
        assert_eq!(ls.correct_spelling("linux", "en", true), None);
        assert_eq!(
            ls.correct_spelling("тект", "ru", true).as_deref(),
            Some("текст")
        );
        assert_eq!(ls.correct_spelling("ghbdtn", "en", true), None);
        // An unknown word one far consonant edit from a common word is not a
        // typo (`coding` → `coming`), and a mid-sentence capital is a name
        // (`Groq` → `Grow`).
        assert_eq!(ls.correct_spelling("coding", "en", true), None);
        assert_eq!(ls.correct_spelling("Groq", "en", false), None);
        // A Russian typo must reach the autocorrect branch, not become EN gibberish.
        assert_eq!(
            ls.find_best_layout("превет", "ru", DEFAULT_THRESHOLD, DEFAULT_MIN_WORD_LEN),
            None
        );
        // Learned words are protected.
        ls.add_exception("привт");
        assert_eq!(ls.correct_spelling("привт", "ru", true), None);
    }

    #[test]
    fn gibberish_is_not_converted() {
        // Regression: n-gram noise alone used to convert keyboard mashing
        // ("hjkl" → "ролд", "lrkf" → "дкла").  The re-mapped form must be a
        // real word of the target language.
        let mut ls = test_set();
        for word in [
            "hjkl", "jkl;", "lrkf", "testtest", "dfgh", "wert", "vbn", "fghjkl", "zxcvbnm",
        ] {
            assert_eq!(
                ls.find_best_layout(word, "en", DEFAULT_THRESHOLD, DEFAULT_MIN_WORD_LEN),
                None,
                "{word:?} must not convert"
            );
        }
        for word in ["фыва", "олдж", "йцук", "ячсм"] {
            assert_eq!(
                ls.find_best_layout(word, "ru", DEFAULT_THRESHOLD, DEFAULT_MIN_WORD_LEN),
                None,
                "{word:?} must not convert"
            );
        }
    }

    #[test]
    fn keyboard_runs_are_detected() {
        let mut ls = test_set();
        for word in ["fghjkl", "zxcvbnm", "vbn", "ghjk", "asdfgh"] {
            assert!(ls.is_keyboard_run(word, "en"), "{word:?}");
        }
        for word in ["plugin", "config", "linux", "repo", "tree"] {
            assert!(!ls.is_keyboard_run(word, "en"), "{word:?}");
        }
    }

    #[test]
    fn technical_terms_missing_from_vocabulary_convert() {
        // The 20k subtitle vocabulary lacks these words, but n-gram evidence
        // and the typed text's non-run shape are enough.
        let mut ls = test_set();
        for (word, from, to) in [
            ("plugin", "en", "ru"),
            ("config", "en", "ru"),
            ("docker", "en", "ru"),
            ("клавиатура", "ru", "en"),
            ("раскладка", "ru", "en"),
            ("настройки", "ru", "en"),
        ] {
            let typed = ls.convert(word, from, to);
            assert_eq!(
                ls.find_best_layout(&typed, to, DEFAULT_THRESHOLD, DEFAULT_MIN_WORD_LEN)
                    .as_deref(),
                Some(from),
                "{word:?} typed as {typed:?} must convert back"
            );
        }
    }

    #[test]
    fn l2_russian_tech_terms_convert() {
        // Russian IT jargon absent from the 20k vocabulary.  `б`/`ю` arrive
        // as ','/'.' in the EN layout, which used to hit the punctuation gate.
        let mut ls = test_set();
        for (word, typed) in [
            ("разработка", "hfphf,jnrf"),
            ("переключение", "gthtrk.xtybt"),
            ("фронтенд", "ahjyntyl"),
            ("плагин", "gkfuby"),
            ("сниппет", "cybggtn"),
            ("браузер", ",hfepth"),
            ("бэкенд", ",'rtyl"),
        ] {
            assert_eq!(
                ls.convert(typed, "en", "ru"),
                word,
                "{typed:?} should map to {word:?}"
            );
            assert_eq!(
                ls.find_best_layout(typed, "en", DEFAULT_THRESHOLD, DEFAULT_MIN_WORD_LEN)
                    .as_deref(),
                Some("ru"),
                "{word:?} typed as {typed:?} must convert"
            );
        }
        // The same rules must not flip correctly typed English technical terms.
        for word in ["backend", "frontend", "switcher", "plugin", "typescript"] {
            assert_eq!(
                ls.find_best_layout(word, "en", DEFAULT_THRESHOLD, DEFAULT_MIN_WORD_LEN),
                None,
                "{word:?} must stay English"
            );
        }
        // Mid-word eager conversion asks for an overwhelming 15.0 gain; the
        // absolute-floor path must not lower that bar.
        assert_eq!(ls.find_best_layout("ahjyntyl", "en", 15.0, 4), None);
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
