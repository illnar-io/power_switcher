//! TOML configuration — port of `power_switcher/config.py`.
//!
//! Same sections, same keys, same defaults.  Missing file → defaults
//! (Python: `if not p.exists(): return cls()`).

use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct Config {
    pub hotkey_convert: String,
    pub hotkey_selection: String,
    pub layouts: Vec<String>,
    pub default_layout: String,
    #[allow(dead_code)] // parity with Python (auto_detect is not consulted yet)
    pub auto_detect: bool,
    pub threshold: f64,
    pub min_word_len: usize,
    #[allow(dead_code)] // parity with Python (undo_window is vestigial there too)
    pub undo_window: f64,

    // Snippets
    pub snippet_expand_space: bool,
    pub snippet_expand_enter: bool,
    pub snippet_expand_tab: bool,

    // Boundary triggers for auto-detection
    pub trigger_space: bool,
    pub trigger_enter: bool,
    pub trigger_tab: bool,
    pub trigger_punctuation: bool,

    // Behavior
    pub developer_mode: bool,
    pub arrows_cancel: bool,
    pub live_fix_enabled: bool,

    // Layout-switch guards & hotkeys
    pub no_correct_after_switch: f64,
    pub lr_shift_pause: bool,
    pub manual_only_last_word: bool,
    pub sound_enabled: bool,
    pub self_learning: bool,

    // Undo
    pub undo_words: usize,
    pub undo_seconds: f64,

    // Paths ("" → ~/.config/power_switcher/<name>.json)
    pub snippets_file: String,
    pub exceptions_file: String,
    pub words_file: String,
    pub stats_file: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey_convert: "alt+pause".to_string(),
            hotkey_selection: "ctrl+alt+z".to_string(),
            layouts: vec!["en".to_string(), "ru".to_string()],
            default_layout: "en".to_string(),
            auto_detect: true,
            threshold: 2.5,
            min_word_len: 3,
            undo_window: 3.0,
            snippet_expand_space: true,
            snippet_expand_enter: true,
            snippet_expand_tab: true,
            trigger_space: true,
            trigger_enter: true,
            trigger_tab: true,
            trigger_punctuation: true,
            developer_mode: false,
            arrows_cancel: true,
            live_fix_enabled: true,
            no_correct_after_switch: 1.5,
            lr_shift_pause: true,
            manual_only_last_word: false,
            sound_enabled: true,
            self_learning: true,
            undo_words: 0,
            undo_seconds: 3.0,
            snippets_file: String::new(),
            exceptions_file: String::new(),
            words_file: String::new(),
            stats_file: String::new(),
        }
    }
}

// ── TOML schema ──────────────────────────────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
struct ConfigFile {
    #[serde(default)]
    general: GeneralSection,
    #[serde(default)]
    detection: DetectionSection,
    #[serde(default)]
    triggers: TriggersSection,
    #[serde(default)]
    undo: UndoSection,
}

#[derive(Debug, Default, Deserialize)]
struct GeneralSection {
    hotkey_convert: Option<String>,
    hotkey_selection: Option<String>,
    layouts: Option<Vec<String>>,
    /// Legacy alias for `layouts` (Python: `g.get("layouts", g.get("layout_pair", …))`).
    layout_pair: Option<Vec<String>>,
    default_layout: Option<String>,
    auto_detect: Option<bool>,
    developer_mode: Option<bool>,
    arrows_cancel: Option<bool>,
    live_fix_enabled: Option<bool>,
    lr_shift_pause: Option<bool>,
    no_correct_after_switch: Option<f64>,
    manual_only_last_word: Option<bool>,
    sound_enabled: Option<bool>,
    self_learning: Option<bool>,
    snippets_file: Option<String>,
    exceptions_file: Option<String>,
    words_file: Option<String>,
    stats_file: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct DetectionSection {
    threshold: Option<f64>,
    min_word_len: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
struct TriggersSection {
    space: Option<bool>,
    enter: Option<bool>,
    tab: Option<bool>,
    punctuation: Option<bool>,
    snippet_space: Option<bool>,
    snippet_enter: Option<bool>,
    snippet_tab: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
struct UndoSection {
    words: Option<usize>,
    seconds: Option<f64>,
}

impl Config {
    /// Load from `path`; missing file → defaults (Python `Config.load`).
    /// A present-but-invalid file is an error (Python's `tomllib.load` raises).
    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let p = path.as_ref();
        if !p.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(p).map_err(|e| e.to_string())?;
        let file: ConfigFile = toml::from_str(&text).map_err(|e| e.to_string())?;
        let mut c = Self::default();
        let g = file.general;

        if let Some(v) = g.hotkey_convert {
            c.hotkey_convert = v;
        }
        if let Some(v) = g.hotkey_selection {
            c.hotkey_selection = v;
        }
        // Support both 'layouts' and legacy 'layout_pair'.
        if let Some(v) = g.layouts.or(g.layout_pair) {
            c.layouts = v;
        }
        // Python: default_layout falls back to the *loaded* layouts[0].
        c.default_layout = g.default_layout.unwrap_or_else(|| {
            c.layouts
                .first()
                .cloned()
                .unwrap_or_else(|| "en".to_string())
        });
        if let Some(v) = g.auto_detect {
            c.auto_detect = v;
        }
        if let Some(v) = g.developer_mode {
            c.developer_mode = v;
        }
        if let Some(v) = g.arrows_cancel {
            c.arrows_cancel = v;
        }
        if let Some(v) = g.live_fix_enabled {
            c.live_fix_enabled = v;
        }
        if let Some(v) = g.lr_shift_pause {
            c.lr_shift_pause = v;
        }
        if let Some(v) = g.no_correct_after_switch {
            c.no_correct_after_switch = v;
        }
        if let Some(v) = g.manual_only_last_word {
            c.manual_only_last_word = v;
        }
        if let Some(v) = g.sound_enabled {
            c.sound_enabled = v;
        }
        if let Some(v) = g.self_learning {
            c.self_learning = v;
        }
        if let Some(v) = g.snippets_file {
            c.snippets_file = v;
        }
        if let Some(v) = g.exceptions_file {
            c.exceptions_file = v;
        }
        if let Some(v) = g.words_file {
            c.words_file = v;
        }
        if let Some(v) = g.stats_file {
            c.stats_file = v;
        }

        let det = file.detection;
        if let Some(v) = det.threshold {
            c.threshold = v;
        }
        if let Some(v) = det.min_word_len {
            c.min_word_len = v;
        }

        let trig = file.triggers;
        if let Some(v) = trig.space {
            c.trigger_space = v;
        }
        if let Some(v) = trig.enter {
            c.trigger_enter = v;
        }
        if let Some(v) = trig.tab {
            c.trigger_tab = v;
        }
        if let Some(v) = trig.punctuation {
            c.trigger_punctuation = v;
        }
        if let Some(v) = trig.snippet_space {
            c.snippet_expand_space = v;
        }
        if let Some(v) = trig.snippet_enter {
            c.snippet_expand_enter = v;
        }
        if let Some(v) = trig.snippet_tab {
            c.snippet_expand_tab = v;
        }

        let und = file.undo;
        if let Some(v) = und.words {
            c.undo_words = v;
        }
        if let Some(v) = und.seconds {
            c.undo_seconds = v;
        }

        Ok(c)
    }

    /// Resolve a config path field: empty → the default under
    /// `~/.config/power_switcher/`, else the expanded value.
    pub fn resolve_path(&self, field: &str, default_name: &str) -> Option<PathBuf> {
        let raw = match field {
            "snippets" => &self.snippets_file,
            "exceptions" => &self.exceptions_file,
            "words" => &self.words_file,
            "stats" => &self.stats_file,
            _ => return None,
        };
        if raw.is_empty() {
            Some(default_config_dir().join(default_name))
        } else {
            Some(expand_tilde(raw))
        }
    }
}

/// `~/.config/power_switcher`
pub fn default_config_dir() -> PathBuf {
    expand_tilde("~/.config/power_switcher")
}

/// Python `Path.expanduser` for `~/…` paths.
pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_gives_defaults() {
        let c = Config::load("/nonexistent-ps/config.toml").unwrap();
        assert_eq!(c.layouts, vec!["en", "ru"]);
        assert_eq!(c.default_layout, "en");
        assert_eq!(c.threshold, 2.5);
        assert_eq!(c.no_correct_after_switch, 1.5);
        assert!(c.trigger_punctuation);
    }

    #[test]
    fn parses_full_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[general]
layouts = ["de", "fr", "ru"]
default_layout = "de"
developer_mode = true
no_correct_after_switch = 2.0
sound_enabled = false

[detection]
threshold = 3.0
min_word_len = 4

[triggers]
space = true
punctuation = false
snippet_tab = false

[undo]
words = 3
seconds = 5.0
"#,
        )
        .unwrap();
        let c = Config::load(&path).unwrap();
        assert_eq!(c.layouts, vec!["de", "fr", "ru"]);
        assert_eq!(c.default_layout, "de");
        assert!(c.developer_mode);
        assert_eq!(c.no_correct_after_switch, 2.0);
        assert!(!c.sound_enabled);
        assert_eq!(c.threshold, 3.0);
        assert_eq!(c.min_word_len, 4);
        assert!(!c.trigger_punctuation);
        assert!(!c.snippet_expand_tab);
        assert_eq!(c.undo_words, 3);
        assert_eq!(c.undo_seconds, 5.0);
    }

    #[test]
    fn default_layout_falls_back_to_loaded_layouts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[general]\nlayouts = [\"de\", \"fr\"]\n").unwrap();
        let c = Config::load(&path).unwrap();
        // Python: c.default_layout = g.get("default_layout", c.layouts[0])
        assert_eq!(c.default_layout, "de");
    }

    #[test]
    fn legacy_layout_pair_supported() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[general]\nlayout_pair = [\"en\", \"ua\"]\n").unwrap();
        let c = Config::load(&path).unwrap();
        assert_eq!(c.layouts, vec!["en", "ua"]);
    }

    #[test]
    fn corrupt_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[general\nbroken").unwrap();
        assert!(Config::load(&path).is_err());
    }

    #[test]
    fn partial_config_tolerates_missing_sections() {
        // Python: d.get("triggers", {}) — missing sections are fine.
        // Regression: the real user config only has [general] and [detection].
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "[general]\nlayouts = [\"en\", \"ru\"]\ndefault_layout = \"en\"\n\n[detection]\nthreshold = 2.5\n",
        )
        .unwrap();
        let c = Config::load(&path).unwrap();
        assert_eq!(c.layouts, vec!["en", "ru"]);
        assert!(c.trigger_punctuation, "triggers default to true");
        assert_eq!(c.undo_words, 0, "undo defaults apply");
    }

    #[test]
    fn resolve_paths() {
        let mut c = Config::default();
        assert_eq!(
            c.resolve_path("snippets", "snippets.json"),
            Some(default_config_dir().join("snippets.json"))
        );
        c.snippets_file = "/tmp/my-snippets.json".to_string();
        assert_eq!(
            c.resolve_path("snippets", "snippets.json"),
            Some(PathBuf::from("/tmp/my-snippets.json"))
        );
    }
}
