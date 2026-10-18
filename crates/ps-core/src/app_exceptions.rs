//! Application exceptions: per-app mode control (off/soft/default).
//!
//! Port of `power_switcher/core/app_exceptions.py`.  Detects the focused
//! application (via the ps-focus layer) and applies the configured mode.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use log::{info, warn};

/// Known developer/IDE apps (disable auto-detect in these).
pub const DEV_APP_PATTERNS: &[&str] = &[
    "jetbrains",
    "code",
    "terminal",
    "konsole",
    "alacritty",
    "kitty",
    "wezterm",
    "foot",
    "gnome-terminal",
    "xfce4-terminal",
    "lutris",
    "steam",
    "wine",
    "dosbox",
    "virtualbox",
    "vmware",
];

/// Chromium/Electron family (may need special handling).
pub const CHROMIUM_KEYWORDS: &[&str] = &[
    "chromium", "chrome", "brave", "electron", "slack", "discord", "obsidian", "notion", "spotify",
    "vscode", "cursor",
];

/// Per-app modes, insertion-ordered (Python dict semantics: `get_mode`
/// returns the first pattern that matches as a substring).
#[derive(Debug, Default)]
pub struct ExceptionStore {
    path: PathBuf,
    exceptions: Vec<(String, String)>,
}

impl ExceptionStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let mut store = Self {
            path: path.into(),
            exceptions: Vec::new(),
        };
        store.load();
        store
    }

    fn load(&mut self) {
        let Ok(text) = std::fs::read_to_string(&self.path) else {
            return;
        };
        let data: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(err) => {
                warn!(
                    "Failed to load exceptions from {}: {err}",
                    self.path.display()
                );
                return;
            }
        };
        self.exceptions = match data {
            serde_json::Value::Object(map) => map
                .into_iter()
                .filter_map(|(k, v)| v.as_str().map(|mode| (k, mode.to_string())))
                // serde_json maps are BTreeMap (sorted by key) — Python's dict
                // keeps file order; sorted order keeps behaviour deterministic.
                .collect(),
            serde_json::Value::Array(items) => items
                .iter()
                .filter_map(|item| {
                    let name = item.get("name")?.as_str()?.to_string();
                    let mode = item.get("mode")?.as_str()?.to_string();
                    Some((name, mode))
                })
                .collect(),
            _ => Vec::new(),
        };
    }

    fn persist(&self) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let map: std::collections::BTreeMap<&String, &String> =
            self.exceptions.iter().map(|(k, v)| (k, v)).collect();
        std::fs::write(&self.path, serde_json::to_string_pretty(&map)?)
    }

    /// Return 'off', 'soft', or 'default' for an app (substring match,
    /// first configured pattern wins).
    pub fn get_mode(&self, app_name: &str) -> &str {
        let lower = app_name.to_lowercase();
        for (pattern, mode) in &self.exceptions {
            if lower.contains(&pattern.to_lowercase()) {
                return mode;
            }
        }
        "default"
    }

    pub fn is_dev_app(&self, app_name: &str) -> bool {
        let lower = app_name.to_lowercase();
        DEV_APP_PATTERNS.iter().any(|pat| lower.contains(pat))
    }

    pub fn is_chromium(&self, app_name: &str) -> bool {
        let lower = app_name.to_lowercase();
        CHROMIUM_KEYWORDS.iter().any(|kw| lower.contains(kw))
    }

    /// Add (or update in place, like a dict) an exception and persist.
    pub fn add(&mut self, name: &str, mode: &str) -> io::Result<()> {
        match self.exceptions.iter_mut().find(|(k, _)| k == name) {
            Some(entry) => entry.1 = mode.to_string(),
            None => self.exceptions.push((name.to_string(), mode.to_string())),
        }
        self.persist()?;
        info!("Exception added: {name} → {mode}");
        Ok(())
    }

    pub fn remove(&mut self, name: &str) -> io::Result<bool> {
        let before = self.exceptions.len();
        self.exceptions.retain(|(k, _)| k != name);
        if self.exceptions.len() < before {
            self.persist()?;
            info!("Exception removed: {name}");
            return Ok(true);
        }
        Ok(false)
    }

    /// Replace all exceptions (editor path).
    pub fn set_all(&mut self, exceptions: HashMap<String, String>) -> io::Result<()> {
        self.exceptions = exceptions.into_iter().collect();
        self.persist()?;
        Ok(())
    }

    /// Snapshot of all configured exceptions.
    pub fn all(&self) -> Vec<(String, String)> {
        self.exceptions.clone()
    }

    pub fn len(&self) -> usize {
        self.exceptions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.exceptions.is_empty()
    }
}

/// `~/.config/power_switcher/exceptions.json` default location helper for the
/// CLI/daemon (the library itself takes explicit paths).
pub fn default_exceptions_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    Path::new(&home)
        .join(".config")
        .join("power_switcher")
        .join("exceptions.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store(dir: &tempfile::TempDir) -> ExceptionStore {
        ExceptionStore::new(dir.path().join("exceptions.json"))
    }

    #[test]
    fn default_mode() {
        let store = ExceptionStore::new("/nonexistent-ps/exceptions.json");
        assert_eq!(store.get_mode("unknown_app"), "default");
    }

    #[test]
    fn add_and_get() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = temp_store(&dir);
        store.add("code", "off").unwrap();
        assert_eq!(store.get_mode("com.microsoft.VSCode"), "off");
        assert_eq!(store.get_mode("code-oss"), "off");
    }

    #[test]
    fn remove() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = temp_store(&dir);
        store.add("vscode", "off").unwrap();
        assert!(store.remove("vscode").unwrap());
        assert_eq!(store.get_mode("vscode"), "default");
        assert!(!store.remove("vscode").unwrap());
    }

    #[test]
    fn is_dev_app() {
        let store = ExceptionStore::new("/nonexistent-ps/exceptions.json");
        assert!(store.is_dev_app("Visual Studio Code"));
        assert!(store.is_dev_app("jetbrains-idea"));
        assert!(!store.is_dev_app("Firefox"));
    }

    #[test]
    fn is_chromium() {
        let store = ExceptionStore::new("/nonexistent-ps/exceptions.json");
        assert!(store.is_chromium("Google Chrome"));
        assert!(store.is_chromium("Chromium"));
        assert!(!store.is_chromium("Firefox"));
    }

    #[test]
    fn persistence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("exceptions.json");
        let mut store1 = ExceptionStore::new(&path);
        store1.add("app1", "off").unwrap();
        store1.add("app2", "soft").unwrap();

        let store2 = ExceptionStore::new(&path);
        assert_eq!(store2.get_mode("app1"), "off");
        assert_eq!(store2.get_mode("app2"), "soft");
    }

    #[test]
    fn set_all_replaces() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = temp_store(&dir);
        store.add("old", "off").unwrap();
        let mut map = HashMap::new();
        map.insert("new1".to_string(), "off".to_string());
        map.insert("new2".to_string(), "soft".to_string());
        store.set_all(map).unwrap();
        assert_eq!(store.get_mode("old"), "default");
        assert_eq!(store.get_mode("new1"), "off");
        assert_eq!(store.get_mode("new2"), "soft");
    }

    #[test]
    fn all_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = temp_store(&dir);
        store.add("a", "off").unwrap();
        store.add("b", "soft").unwrap();
        let result = store.all();
        assert_eq!(
            result,
            vec![
                ("a".to_string(), "off".to_string()),
                ("b".to_string(), "soft".to_string())
            ]
        );
    }

    #[test]
    fn update_keeps_position_like_dict() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = temp_store(&dir);
        store.add("a", "off").unwrap();
        store.add("b", "soft").unwrap();
        store.add("a", "soft").unwrap(); // dict: value replaced, position kept
        let result = store.all();
        assert_eq!(result[0], ("a".to_string(), "soft".to_string()));
        assert_eq!(result[1], ("b".to_string(), "soft".to_string()));
    }

    #[test]
    fn load_list_format() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("exceptions.json");
        std::fs::write(
            &path,
            r#"[{"name": "term", "mode": "off"}, {"name": "web", "mode": "soft"}]"#,
        )
        .unwrap();
        let store = ExceptionStore::new(&path);
        assert_eq!(store.get_mode("gnome-terminal"), "off");
        assert_eq!(store.get_mode("webapp"), "soft");
    }

    #[test]
    fn corrupt_file_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("exceptions.json");
        std::fs::write(&path, "{ broken").unwrap();
        let store = ExceptionStore::new(&path);
        assert!(store.is_empty());
    }
}
