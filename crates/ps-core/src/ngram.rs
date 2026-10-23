//! Character n-gram language model.
//!
//! Port of `power_switcher/detection/ngram_detector.py`.  A word is scored
//! against each language by summing weighted log-probabilities of its character
//! n-grams (n = 2..5), normalised by the total weight.  The profile schema is
//! the one produced by `power_switcher.detection.build_profiles`:
//!
//! ```json
//! {"version": 2, "grams": {"th": 0.03}, "words": {"the": 0.012}}
//! ```

use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use serde::Deserialize;

pub const DEFAULT_MAX_N: usize = 5;
pub const DEFAULT_UNKNOWN_PROB: f64 = 1e-7;

/// Weight per n-gram length; longer n-grams are more discriminative.
/// Mirrors `_NGRAM_WEIGHTS` (with the same `n - 1.0` fallback).
pub fn weight_for(n: usize) -> f64 {
    match n {
        2 => 1.0,
        3 => 1.6,
        4 => 2.2,
        5 => 2.8,
        _ => (n - 1) as f64,
    }
}

#[derive(Deserialize)]
struct ProfileFile {
    #[serde(default)]
    grams: HashMap<String, f64>,
    #[serde(default)]
    words: HashMap<String, f64>,
}

/// A word prepared once for scoring: the lowercased form plus the
/// boundary-padded character vector (`^word$`), matching `_BOUND = "^$"`.
pub struct Prepared {
    lowered: String,
    padded: Vec<char>,
}

impl Prepared {
    pub fn new(word: &str) -> Self {
        // Python lowers first and measures the *lowered* word's length.
        let lowered: String = word.to_lowercase();
        let mut padded: Vec<char> = Vec::with_capacity(lowered.chars().count() + 2);
        padded.push('^');
        padded.extend(lowered.chars());
        padded.push('$');
        Self { lowered, padded }
    }

    pub fn lowered(&self) -> &str {
        &self.lowered
    }

    /// Number of characters in the lowercased word.
    pub fn char_len(&self) -> usize {
        self.padded.len().saturating_sub(2)
    }
}

#[derive(Default)]
pub struct NGramDetector {
    max_n: usize,
    unknown_prob: f64,
    log_unknown: f64,
    grams: HashMap<String, HashMap<String, f64>>,
    logwords: HashMap<String, HashMap<String, f64>>,
    /// Median word probability per language; candidates below it are not
    /// common enough to autocorrect towards.
    word_cutoff: HashMap<String, f64>,
}

impl NGramDetector {
    pub fn new(max_n: usize, unknown_prob: f64) -> Self {
        Self {
            max_n,
            unknown_prob,
            log_unknown: unknown_prob.ln(),
            grams: HashMap::new(),
            logwords: HashMap::new(),
            word_cutoff: HashMap::new(),
        }
    }

    /// Load a `<lang>_ngrams.json[.gz]` profile.
    pub fn load_profile(&mut self, lang: &str, path: &Path) -> Result<(), Box<dyn Error>> {
        let file = File::open(path)?;
        let bytes = if path.extension().and_then(|e| e.to_str()) == Some("gz") {
            let mut out = Vec::new();
            BufReader::new(file).take(u64::MAX).read_to_end(&mut out)?;
            let mut decoder = flate2::read::GzDecoder::new(out.as_slice());
            let mut plain = Vec::new();
            decoder.read_to_end(&mut plain)?;
            plain
        } else {
            let mut plain = Vec::new();
            BufReader::new(file)
                .take(u64::MAX)
                .read_to_end(&mut plain)?;
            plain
        };
        let data: ProfileFile = serde_json::from_slice(&bytes)?;

        // Median word probability: the correction candidate must be at least
        // this common, which keeps technical terms out of the autocorrect path.
        let mut probs: Vec<f64> = data.words.values().copied().filter(|p| *p > 0.0).collect();
        probs.sort_by(f64::total_cmp);
        let cutoff = probs.get(probs.len() / 2).copied().unwrap_or(1.0);
        self.word_cutoff.insert(lang.to_string(), cutoff);

        let mut logwords = HashMap::with_capacity(data.words.len());
        for (w, p) in data.words {
            if p > 0.0 {
                logwords.insert(w, p.ln());
            }
        }
        self.grams.insert(lang.to_string(), data.grams);
        self.logwords.insert(lang.to_string(), logwords);
        Ok(())
    }

    pub fn has_language(&self, lang: &str) -> bool {
        self.grams.contains_key(lang)
    }

    /// Probability assigned to n-grams absent from a profile.
    pub fn unknown_prob(&self) -> f64 {
        self.unknown_prob
    }

    /// Maximum n-gram length scored.
    pub fn max_n(&self) -> usize {
        self.max_n
    }

    pub fn languages(&self) -> Vec<&str> {
        self.grams.keys().map(|s| s.as_str()).collect()
    }

    /// Per-character weighted log-probability of `word` in `lang`.
    /// Higher (closer to zero) is better; `-inf` when no profile is loaded or
    /// the lowered word has fewer than two characters.
    pub fn score(&self, word: &str, lang: &str) -> f64 {
        let prepared = Prepared::new(word);
        self.score_prepared(&prepared, lang)
    }

    /// Scoring on an already-prepared word (avoids re-lowering per layout).
    pub fn score_prepared(&self, prepared: &Prepared, lang: &str) -> f64 {
        let Some(profile) = self.grams.get(lang) else {
            return f64::NEG_INFINITY;
        };
        if prepared.char_len() < 2 {
            return f64::NEG_INFINITY;
        }

        let padded = &prepared.padded;
        let mut total = 0.0f64;
        let mut weight_sum = 0.0f64;
        let mut gram = String::with_capacity(self.max_n);

        for n in 2..=self.max_n {
            if padded.len() < n {
                break;
            }
            let weight = weight_for(n);
            for i in 0..=(padded.len() - n) {
                gram.clear();
                gram.extend(padded[i..i + n].iter());
                let log_p = match profile.get(gram.as_str()) {
                    Some(&p) => p.ln(),
                    None => self.log_unknown,
                };
                total += log_p * weight;
                weight_sum += weight;
            }
        }

        if weight_sum == 0.0 {
            f64::NEG_INFINITY
        } else {
            total / weight_sum
        }
    }

    /// Log-probability of a whole word, or `None` if unknown.
    pub fn word_logprob(&self, word: &str, lang: &str) -> Option<f64> {
        let lowered = word.to_lowercase();
        self.logwords.get(lang)?.get(&lowered).copied()
    }

    pub fn word_logprob_prepared(&self, prepared: &Prepared, lang: &str) -> Option<f64> {
        self.logwords.get(lang)?.get(prepared.lowered()).copied()
    }

    pub fn is_known_word(&self, word: &str, lang: &str) -> bool {
        self.word_logprob(word, lang).is_some()
    }

    /// Median word probability of a language's vocabulary (the minimum for an
    /// autocorrect candidate).  `None` when the profile is not loaded.
    pub fn word_cutoff(&self, lang: &str) -> Option<f64> {
        self.word_cutoff.get(lang).copied()
    }

    /// Highest-scoring language for `word` among `languages`.
    pub fn best_language(&self, word: &str, languages: &[&str]) -> Option<String> {
        if word.to_lowercase().chars().count() < 2 {
            return None;
        }
        let prepared = Prepared::new(word);
        let mut best: Option<(&str, f64)> = None;
        for lang in languages {
            if !self.has_language(lang) {
                continue;
            }
            let s = self.score_prepared(&prepared, lang);
            match best {
                Some((_, bs)) if bs >= s => {}
                _ => best = Some((lang, s)),
            }
        }
        best.map(|(l, _)| l.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weights_match_python() {
        assert_eq!(weight_for(2), 1.0);
        assert_eq!(weight_for(3), 1.6);
        assert_eq!(weight_for(4), 2.2);
        assert_eq!(weight_for(5), 2.8);
        assert_eq!(weight_for(6), 5.0);
    }

    #[test]
    fn short_words_score_negative_infinity() {
        let d = NGramDetector::new(5, 1e-7);
        assert_eq!(d.score("a", "en"), f64::NEG_INFINITY);
        assert_eq!(d.score("", "en"), f64::NEG_INFINITY);
        assert_eq!(d.score("hello", "en"), f64::NEG_INFINITY); // no profile
    }

    #[test]
    fn prepared_lowering_matches_python() {
        let p = Prepared::new("Hello");
        assert_eq!(p.lowered(), "hello");
        assert_eq!(p.char_len(), 5);
        let p2 = Prepared::new("ЙЦУКЕН");
        assert_eq!(p2.lowered(), "йцукен");
    }
}
