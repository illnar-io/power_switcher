//! TOML configuration — port of `power_switcher/config.py`.
//!
//! Same sections, same keys, same defaults.  Missing file → defaults
//! (Python: `if not p.exists(): return cls()`).

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Hotkey values written by older versions; `Config::load` upgrades them.
const LEGACY_HOTKEY_CONVERT: &str = "alt+pause";
const LEGACY_HOTKEY_SELECTION: &str = "ctrl+alt+z";

#[derive(Debug, Clone, PartialEq)]
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
    /// A lone Shift tap switches the layout (legacy Caramba gesture).
    /// Off by default — the desktop binds `Super+Space` for that.
    pub shift_tap_switch_layout: bool,

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
            // Manual convert: double-tap Shift always works.  Pause/Break is
            // Caramba's key but laptops have no Pause, so the configurable
            // default is a laptop-friendly combo (Ctrl+Alt+Z).
            hotkey_convert: "ctrl+alt+z".to_string(),
            hotkey_selection: "alt+shift".to_string(),
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
            shift_tap_switch_layout: false,
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
    shift_tap_switch_layout: Option<bool>,
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
        // Migrate configs written before the Caramba defaults were adopted:
        // the old values were reserved placeholders that never triggered a
        // key, so nobody could have relied on them.
        if c.hotkey_convert == LEGACY_HOTKEY_CONVERT {
            c.hotkey_convert = Self::default().hotkey_convert;
        }
        if c.hotkey_selection == LEGACY_HOTKEY_SELECTION {
            c.hotkey_selection = Self::default().hotkey_selection;
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
        if let Some(v) = g.shift_tap_switch_layout {
            c.shift_tap_switch_layout = v;
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

    /// Serialise to the same `config.toml` layout the Python GUI writes, so
    /// either front-end can edit the file without surprising the other.
    pub fn to_toml(&self) -> String {
        let layouts = self
            .layouts
            .iter()
            .map(|l| format!("\"{}\"", escape_toml(l)))
            .collect::<Vec<_>>()
            .join(", ");

        let mut lines = vec![
            "[general]".to_string(),
            format!("layouts = [{layouts}]"),
            format!("default_layout = \"{}\"", escape_toml(&self.default_layout)),
            format!("auto_detect = {}", self.auto_detect),
            format!("developer_mode = {}", self.developer_mode),
            format!("arrows_cancel = {}", self.arrows_cancel),
            format!("live_fix_enabled = {}", self.live_fix_enabled),
            format!("lr_shift_pause = {}", self.lr_shift_pause),
            format!("shift_tap_switch_layout = {}", self.shift_tap_switch_layout),
            format!("manual_only_last_word = {}", self.manual_only_last_word),
            format!("self_learning = {}", self.self_learning),
            format!("sound_enabled = {}", self.sound_enabled),
            format!(
                "no_correct_after_switch = {}",
                fmt_float(self.no_correct_after_switch)
            ),
            format!("hotkey_convert = \"{}\"", escape_toml(&self.hotkey_convert)),
            format!(
                "hotkey_selection = \"{}\"",
                escape_toml(&self.hotkey_selection)
            ),
        ];
        // Only persist custom store paths; empty means "default location".
        for (key, value) in [
            ("snippets_file", &self.snippets_file),
            ("exceptions_file", &self.exceptions_file),
            ("words_file", &self.words_file),
            ("stats_file", &self.stats_file),
        ] {
            if !value.is_empty() {
                lines.push(format!("{key} = \"{}\"", escape_toml(value)));
            }
        }
        lines.extend([
            String::new(),
            "[detection]".to_string(),
            format!("threshold = {}", fmt_float(self.threshold)),
            format!("min_word_len = {}", self.min_word_len),
            String::new(),
            "[triggers]".to_string(),
            format!("space = {}", self.trigger_space),
            format!("enter = {}", self.trigger_enter),
            format!("tab = {}", self.trigger_tab),
            format!("punctuation = {}", self.trigger_punctuation),
            format!("snippet_space = {}", self.snippet_expand_space),
            format!("snippet_enter = {}", self.snippet_expand_enter),
            format!("snippet_tab = {}", self.snippet_expand_tab),
            String::new(),
            "[undo]".to_string(),
            format!("words = {}", self.undo_words),
            format!("seconds = {}", fmt_float(self.undo_seconds)),
        ]);
        lines.join("\n") + "\n"
    }
}

/// TOML basic-string escaping (only the two characters that matter for the
/// short values we write: layout ids and hotkey names).
fn escape_toml(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// TOML floats need a decimal point (`3` would deserialise as an integer).
fn fmt_float(v: f64) -> String {
    let s = format!("{v}");
    if s.contains('.') || s.contains('e') || s.contains('E') {
        s
    } else {
        format!("{s}.0")
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
        assert_eq!(c.hotkey_convert, "ctrl+alt+z", "laptop-friendly default");
        assert_eq!(
            c.hotkey_selection, "alt+shift",
            "Caramba's selected-text hotkey"
        );
        assert!(
            !c.shift_tap_switch_layout,
            "lone Shift tap must not switch layouts by default"
        );
    }

    #[test]
    fn migrates_legacy_hotkey_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "[general]\nhotkey_convert = \"alt+pause\"\nhotkey_selection = \"ctrl+alt+z\"\n",
        )
        .unwrap();
        let c = Config::load(&path).unwrap();
        assert_eq!(c.hotkey_convert, "ctrl+alt+z");
        assert_eq!(c.hotkey_selection, "alt+shift");
    }

    #[test]
    fn keeps_explicit_hotkeys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "[general]\nhotkey_convert = \"pause\"\nhotkey_selection = \"ctrl+shift+z\"\n",
        )
        .unwrap();
        let c = Config::load(&path).unwrap();
        assert_eq!(c.hotkey_convert, "pause", "explicit Pause must be kept");
        assert_eq!(c.hotkey_selection, "ctrl+shift+z");
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
shift_tap_switch_layout = true

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
        assert!(c.shift_tap_switch_layout);
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

    #[test]
    fn to_toml_round_trips_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let c = Config::default();
        std::fs::write(&path, c.to_toml()).unwrap();
        let loaded = Config::load(&path).unwrap();
        assert_eq!(loaded, c, "written config must reload identically");
    }

    #[test]
    fn to_toml_round_trips_custom_values() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let c = Config {
            layouts: vec!["de".into(), "fr".into(), "ru".into()],
            default_layout: "de".into(),
            threshold: 3.0, // would serialise as `3` without the .0 fix
            min_word_len: 4,
            undo_seconds: 5.0,
            no_correct_after_switch: 2.0,
            developer_mode: true,
            sound_enabled: false,
            shift_tap_switch_layout: true,
            trigger_punctuation: false,
            snippet_expand_tab: false,
            words_file: "~/custom-words.json".into(),
            ..Config::default()
        };
        std::fs::write(&path, c.to_toml()).unwrap();
        let loaded = Config::load(&path).unwrap();
        assert_eq!(loaded, c);
        assert!(std::fs::read_to_string(&path)
            .unwrap()
            .contains("threshold = 3.0"));
    }

    #[test]
    fn to_toml_preserves_python_key_order_and_sections() {
        let text = Config::default().to_toml();
        let general = text.find("[general]").unwrap();
        let detection = text.find("[detection]").unwrap();
        let triggers = text.find("[triggers]").unwrap();
        let undo = text.find("[undo]").unwrap();
        assert!(general < detection && detection < triggers && triggers < undo);
        assert!(text.contains("shift_tap_switch_layout = false"));
        assert!(text.contains("layouts = [\"en\", \"ru\"]"));
    }
}
