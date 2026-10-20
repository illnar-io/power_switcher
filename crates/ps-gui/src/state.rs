//! Shared state for the settings window: paths plus the loaded `Config`.
//!
//! The GUI deliberately has **no daemon IPC**: it edits the same files the CLI
//! and the daemon use (`config.toml`, `snippets.json`, `exceptions.json`,
//! `words.json`, `stats.json`).  This keeps every front-end interchangeable.

use std::path::{Path, PathBuf};

use ps_daemon::config::{default_config_dir, Config};

/// Layout ids with shipped keycode tables and n-gram profiles.
pub const SUPPORTED_LAYOUTS: &[&str] = &["en", "ru", "ua", "de", "fr", "es", "it", "pl"];

/// Everything the tabs need to read and write.
pub struct GuiState {
    pub config_path: PathBuf,
    pub config: Config,
    pub snippets_path: PathBuf,
    pub exceptions_path: PathBuf,
    pub words_path: PathBuf,
    pub stats_path: PathBuf,
}

impl GuiState {
    /// Load `config.toml` (`PS_CONFIG`, else `~/.config/power_switcher/`).
    pub fn load() -> Result<Self, String> {
        let config_path = config_path();
        let config = Config::load(&config_path)?;
        let snippets_path = config
            .resolve_path("snippets", "snippets.json")
            .ok_or("cannot resolve snippets path")?;
        let exceptions_path = config
            .resolve_path("exceptions", "exceptions.json")
            .ok_or("cannot resolve exceptions path")?;
        let words_path = config
            .resolve_path("words", "words.json")
            .ok_or("cannot resolve words path")?;
        let stats_path = config
            .resolve_path("stats", "stats.json")
            .ok_or("cannot resolve stats path")?;
        Ok(Self {
            config_path,
            config,
            snippets_path,
            exceptions_path,
            words_path,
            stats_path,
        })
    }

    /// Write `config.toml` atomically (temp file + rename).
    pub fn save_config(&self, config: &Config) -> Result<(), String> {
        write_atomic(&self.config_path, &config.to_toml())
    }
}

/// `PS_CONFIG`, else `~/.config/power_switcher/config.toml`.
pub fn config_path() -> PathBuf {
    if let Some(path) = std::env::var_os("PS_CONFIG") {
        return PathBuf::from(path);
    }
    default_config_dir().join("config.toml")
}

/// Parse the comma/space-separated layout list from the settings entry and
/// validate it against the layouts the daemon ships profiles for.
pub fn parse_layouts(raw: &str) -> Result<Vec<String>, String> {
    let mut layouts: Vec<String> = Vec::new();
    for part in raw.split([',', ' ', '\t', ';']) {
        let id = part.trim().to_lowercase();
        if id.is_empty() {
            continue;
        }
        if !SUPPORTED_LAYOUTS.contains(&id.as_str()) {
            return Err(format!(
                "Unsupported layout \"{id}\". Supported layouts: {}",
                SUPPORTED_LAYOUTS.join(", ")
            ));
        }
        if !layouts.contains(&id) {
            layouts.push(id);
        }
    }
    if layouts.is_empty() {
        return Err("At least one layout is required.".to_string());
    }
    Ok(layouts)
}

/// Write `text` to `path` via a sibling temp file so a crash cannot leave a
/// half-written config behind.
pub fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
    }
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config.toml");
    let tmp = path.with_file_name(format!(".{file_name}.tmp"));
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("{}: {e}", path.display())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts_are_normalised_and_deduplicated() {
        let layouts = parse_layouts(" EN, ru ;en, de ").unwrap();
        assert_eq!(layouts, vec!["en", "ru", "de"]);
    }

    #[test]
    fn layouts_reject_unknown_and_empty_input() {
        assert!(parse_layouts("en, xx").unwrap_err().contains("Unsupported"));
        assert!(parse_layouts(" , ").unwrap_err().contains("At least one"));
    }

    #[test]
    fn atomic_write_creates_parents_and_replaces() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("config.toml");
        write_atomic(&path, "one").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "one");
        write_atomic(&path, "two").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "two");
        assert!(!dir.path().join("nested").join(".config.toml.tmp").exists());
    }

    #[test]
    fn saved_config_reloads_identically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let state = GuiState {
            config_path: path.clone(),
            config: Config::default(),
            snippets_path: dir.path().join("snippets.json"),
            exceptions_path: dir.path().join("exceptions.json"),
            words_path: dir.path().join("words.json"),
            stats_path: dir.path().join("stats.json"),
        };
        let mut config = state.config.clone();
        config.layouts = vec!["ru".into(), "en".into()];
        config.default_layout = "ru".into();
        config.threshold = 4.0;
        state.save_config(&config).unwrap();
        assert_eq!(Config::load(&path).unwrap(), config);
    }
}
