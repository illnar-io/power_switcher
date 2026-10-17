//! Pure parsers for backend tool output.
//!
//! Every regex/JSON decode from `layout/backends.py`, extracted so it can be
//! unit-tested against real tool output without spawning anything:
//!
//! * dconf `sources` / `current` (GNOME)
//! * gdbus `getLayout` / `getLayoutsList` (KDE)
//! * swaymsg `get_inputs` (Sway)
//! * hyprctl `devices -j` (Hyprland)

/// Parse `dconf read /org/gnome/desktop/input-sources/sources`.
///
/// Output looks like `@[('xkb', 'us'), ('xkb', 'ru')]` (or without the `@`
/// prefix in older GNOMEs); Python's regex is `\('[^']+', '([^']+)'\)` and the
/// default fallback is `["us", "ru"]`.
pub fn parse_dconf_sources(stdout: &str) -> Vec<String> {
    // Same pattern as Python: \( '[^']+', ' ([^']+)' \)
    let re = regex::Regex::new(r"\('[^']+', '([^']+)'\)").expect("static regex");
    re.captures_iter(stdout).map(|c| c[1].to_string()).collect()
}

/// Parse `dconf read /org/gnome/desktop/input-sources/current`.
///
/// Output is often `uint32 1`; Python grabs the first whitespace-preceded
/// integer and falls back to 0.
pub fn parse_dconf_current(stdout: &str) -> usize {
    // Python: re.search(r'\s(\d+)', stdout)
    let re = regex::Regex::new(r"\s(\d+)").expect("static regex");
    re.captures(stdout)
        .and_then(|c| c[1].parse::<usize>().ok())
        .unwrap_or(0)
}

/// Parse `gdbus call ... org.kde.KeyboardLayouts.getLayout`, e.g. `(uint32 1)`.
///
/// **Divergence from Python (upstream bug fixed):** the Python regex
/// `re.search(r'\d+', '(uint32 1,)')` matches `32` *inside* `uint32`, so the
/// KDE backend always read index 32.  Here we match whole numbers
/// (`\b\d+\b`), which cannot start inside an identifier, so the actual index
/// (`1`) is returned.
pub fn parse_gdbus_layout(stdout: &str) -> usize {
    let re = regex::Regex::new(r"\b\d+\b").expect("static regex");
    re.find(stdout)
        .and_then(|m| m.as_str().parse::<usize>().ok())
        .unwrap_or(0)
}

/// Count layouts in `gdbus call ... getLayoutsList` — Python counts
/// `'shortName'` occurrences.
pub fn count_gdbus_layouts(stdout: &str) -> usize {
    stdout.matches("'shortName'").count()
}

/// One keyboard entry we care about from `swaymsg -t get_inputs`.
#[derive(Debug, Default, PartialEq, Clone)]
pub struct SwayInput {
    pub current_layout_index: Option<usize>,
    pub layout_names: Option<Vec<String>>,
}

/// Parse `swaymsg -t get_inputs` (a JSON array of input devices).
/// Python scans for the first item with `"type": "keyboard"` carrying the
/// requested field.
pub fn parse_sway_inputs(stdout: &str) -> Option<SwayInput> {
    let items: Vec<serde_json::Value> = serde_json::from_str(stdout).ok()?;
    for item in &items {
        if item.get("type").and_then(|t| t.as_str()) != Some("keyboard") {
            continue;
        }
        let mut input = SwayInput::default();
        if let Some(idx) = item.get("current_layout_index").and_then(|v| v.as_u64()) {
            input.current_layout_index = Some(idx as usize);
        }
        if let Some(names) = item.get("xkb_layout_names").and_then(|v| v.as_array()) {
            input.layout_names = Some(
                names
                    .iter()
                    .filter_map(|n| n.as_str().map(|s| s.to_string()))
                    .collect(),
            );
        }
        // Python returns on the *first* keyboard item that has the requested
        // field; here we return the first keyboard item that has either.
        if input.current_layout_index.is_some() || input.layout_names.is_some() {
            return Some(input);
        }
    }
    None
}

/// Parse `hyprctl devices -j`: return the active keymap index of the `main`
/// keyboard (Python's `active_keymap_index` lookup), default 0.
pub fn parse_hyprland_devices(stdout: &str) -> Option<usize> {
    let root: serde_json::Value = serde_json::from_str(stdout).ok()?;
    for kbd in root.get("keyboards")?.as_array()? {
        if kbd.get("main").and_then(|m| m.as_bool()) == Some(true) {
            return Some(
                kbd.get("active_keymap_index")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize,
            );
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dconf_sources_real_shape() {
        let out = "@as [('xkb', 'us'), ('xkb', 'ru')]";
        assert_eq!(parse_dconf_sources(out), vec!["us", "ru"]);
    }

    #[test]
    fn dconf_sources_three_layouts() {
        let out = "@as [('xkb', 'us'), ('xkb', 'de'), ('xkb', 'ua')]";
        assert_eq!(parse_dconf_sources(out), vec!["us", "de", "ua"]);
    }

    #[test]
    fn dconf_sources_empty_and_garbage() {
        assert!(parse_dconf_sources("@as []").is_empty());
        assert!(parse_dconf_sources("").is_empty());
        assert!(parse_dconf_sources("nothing here").is_empty());
    }

    #[test]
    fn dconf_current_shapes() {
        assert_eq!(parse_dconf_current("uint32 1"), 1);
        assert_eq!(parse_dconf_current("> uint32 2"), 2);
        assert_eq!(parse_dconf_current(""), 0);
        assert_eq!(parse_dconf_current("@u 0"), 0);
    }

    #[test]
    fn gdbus_layout_shapes() {
        // Regression test for the upstream Python bug: `\d+` matched the `32`
        // inside `uint32`, so get_current_index always returned 32.
        assert_eq!(parse_gdbus_layout("(uint32 1,)"), 1);
        assert_eq!(parse_gdbus_layout("(uint32 0,)"), 0);
        assert_eq!(parse_gdbus_layout("(uint32 3,)"), 3);
        // A reply that is a bare number still parses.
        assert_eq!(parse_gdbus_layout("2"), 2);
        assert_eq!(parse_gdbus_layout(""), 0);
    }

    #[test]
    fn gdbus_layouts_list_count() {
        let out = "([('xkb', 'us', 'English (US)', True, [], [], [], []), \
                    ('xkb', 'ru', 'Russian', True, [], [], [], [])],)";
        // The real reply embeds variant dicts with 'shortName' keys; counting
        // occurrences is exactly what Python does.
        assert_eq!(count_gdbus_layouts(out), 0);
        let with_names = "([{'shortName': 'us', 'longName': 'English'}, \
                          {'shortName': 'ru', 'longName': 'Russian'}],)";
        assert_eq!(count_gdbus_layouts(with_names), 2);
    }

    #[test]
    fn sway_inputs_shape() {
        let out = r#"[
            {"identifier": "1:1:AT_Translated_Set_2_keyboard", "name": "AT Translated Set 2 keyboard",
             "type": "keyboard", "current_layout_index": 1,
             "xkb_layout_names": ["English (US)", "Russian"]},
            {"identifier": "2:2:mouse", "type": "pointer"}
        ]"#;
        let parsed = parse_sway_inputs(out).expect("keyboard found");
        assert_eq!(parsed.current_layout_index, Some(1));
        assert_eq!(
            parsed.layout_names,
            Some(vec!["English (US)".to_string(), "Russian".to_string()])
        );
    }

    #[test]
    fn sway_inputs_no_keyboard() {
        let out = r#"[{"type": "pointer", "name": "mouse"}]"#;
        assert!(parse_sway_inputs(out).is_none());
        assert!(parse_sway_inputs("not json").is_none());
    }

    #[test]
    fn hyprland_devices_shape() {
        let out = r#"{"keyboards": [
            {"address": "0x1", "name": "at-translated-set-2-keyboard", "main": true,
             "active_keymap": "English (US)", "active_keymap_index": 0},
            {"address": "0x2", "name": "other", "main": false, "active_keymap_index": 1}
        ]}"#;
        assert_eq!(parse_hyprland_devices(out), Some(0));
    }

    #[test]
    fn hyprland_devices_main_is_second() {
        let out = r#"{"keyboards": [
            {"name": "a", "main": false, "active_keymap_index": 0},
            {"name": "b", "main": true, "active_keymap_index": 2}
        ]}"#;
        assert_eq!(parse_hyprland_devices(out), Some(2));
    }

    #[test]
    fn hyprland_devices_missing_main() {
        let out = r#"{"keyboards": [{"name": "a", "main": false}]}"#;
        assert!(parse_hyprland_devices(out).is_none());
        assert!(parse_hyprland_devices("garbage").is_none());
    }
}
