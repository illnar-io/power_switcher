//! Pure parsing helpers — port of the module-level functions in `focus.py`.
//! Unit-tested against real tool output without spawning anything.

/// Values tools emit when they have no real answer (Python `_UNKNOWN_VALUES`).
const UNKNOWN_VALUES: &[&str] = &["", "(null)", "null", "none", "unknown", "n/a", "nil"];

/// Normalise a raw tool answer; return `""` when it is not a real class.
pub fn clean(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    let value = value.trim();
    if value.is_empty() {
        return String::new();
    }
    // Python: value.strip().splitlines()[0].strip()
    let value = value.lines().next().unwrap_or("").trim();
    if UNKNOWN_VALUES.contains(&value.to_lowercase().as_str()) {
        return String::new();
    }
    // xdotool can echo diagnostics on stdout.
    if value.starts_with("XGetClassHint") || value.starts_with("Error") {
        return String::new();
    }
    value.to_string()
}

/// Return the app_id / WM class of the focused node in a sway tree
/// (`swaymsg -t get_tree -r`).  Walks `nodes` + `floating_nodes`; a node wins
/// when it is `focused` and a `con`/`floating_con`; `app_id` preferred over
/// `window_properties.class`.
pub fn parse_sway_tree(tree: &serde_json::Value) -> String {
    walk(tree).unwrap_or_default()
}

fn walk(node: &serde_json::Value) -> Option<String> {
    let is_focused = node.get("focused").and_then(|f| f.as_bool()) == Some(true);
    let node_type = node.get("type").and_then(|t| t.as_str());
    if is_focused && matches!(node_type, Some("con") | Some("floating_con")) {
        if let Some(app_id) = node.get("app_id").and_then(|a| a.as_str()) {
            if !app_id.is_empty() {
                return Some(app_id.to_string());
            }
        }
        // Python: props.get("class") — may be None → propagated as "".
        return node
            .get("window_properties")
            .and_then(|p| p.get("class"))
            .and_then(|c| c.as_str())
            .map(|s| s.to_string());
    }
    for child_key in ["nodes", "floating_nodes"] {
        if let Some(children) = node.get(child_key).and_then(|c| c.as_array()) {
            for child in children {
                if let Some(found) = walk(child) {
                    return Some(found);
                }
            }
        }
    }
    None
}

/// Parse `hyprctl activewindow -j`: `class` preferred over `initialClass`.
pub fn parse_hyprland(json_text: &str) -> String {
    let Ok(data) = serde_json::from_str::<serde_json::Value>(json_text) else {
        return String::new();
    };
    match data {
        serde_json::Value::Object(_) => data
            .get("class")
            .and_then(|c| c.as_str())
            .filter(|s| !s.is_empty())
            .or_else(|| {
                data.get("initialClass")
                    .and_then(|c| c.as_str())
                    .filter(|s| !s.is_empty())
            })
            .unwrap_or_default()
            .to_string(),
        _ => String::new(),
    }
}

/// Extract the first string out of a `gdbus call` tuple reply.
///
/// Python: `re.search(r"'((?:[^'\\]|\\.)*)'", stdout)` then unescape `\'` and
/// `\"`.  Hand-rolled single-pass scanner, equivalent to the regex: find the
/// first `'`, consume runs of non-quote/non-backslash chars and `\`+char
/// escape pairs (one *character*, UTF-8 aware), stop at the closing `'`.
/// No closing quote → no match → `""` (the regex would retry later positions,
/// but any later opening quote in that case also lacks a closer).
pub fn parse_gdbus_string(stdout: &str) -> String {
    let mut chars = stdout.char_indices().peekable();
    while let Some((_, c)) = chars.next() {
        if c != '\'' {
            continue;
        }
        let mut out = String::new();
        loop {
            match chars.next() {
                Some((_, '\\')) => match chars.next() {
                    Some((_, '\'')) => out.push('\''),
                    Some((_, '"')) => out.push('"'),
                    // Other escapes keep the backslash + the character,
                    // exactly like Python's post-match replace.
                    Some((_, other)) => {
                        out.push('\\');
                        out.push(other);
                    }
                    // Trailing lone backslash: the regex cannot complete the
                    // escape pair → no match.
                    None => return String::new(),
                },
                Some((_, '\'')) => return out,
                Some((_, other)) => out.push(other),
                // Unterminated string.
                None => return String::new(),
            }
        }
    }
    String::new()
}

/// Parse `key: value` lines emitted by kdotool / qdbus queryWindowInfo.
/// Python: `^\s*{key}\s*:\s*(.+?)\s*$` with MULTILINE.
pub fn parse_key_value(stdout: &str, key: &str) -> String {
    let re = regex::Regex::new(&format!(r"(?m)^\s*{}\s*:\s*(.+?)\s*$", regex::escape(key)))
        .expect("static pattern");
    re.captures(stdout)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn clean_normalises() {
        assert_eq!(clean("firefox"), "firefox");
        assert_eq!(clean("  firefox \n"), "firefox");
        assert_eq!(clean("first\nsecond"), "first");
        for unknown in ["", "(null)", "null", "None", "unknown", "n/a", "nil"] {
            assert_eq!(clean(unknown), "", "value={unknown:?}");
        }
        assert_eq!(clean("XGetClassHint: cannot open display"), "");
        assert_eq!(clean("Error: can't open display"), "");
    }

    #[test]
    fn sway_tree_finds_focused_app_id() {
        let tree = json!({
            "type": "root", "nodes": [{
                "type": "output", "nodes": [{
                    "type": "workspace", "nodes": [
                        {"type": "con", "app_id": "term", "focused": false},
                        {"type": "con", "app_id": "firefox", "focused": true}
                    ],
                    "floating_nodes": []
                }]
            }]
        });
        assert_eq!(parse_sway_tree(&tree), "firefox");
    }

    #[test]
    fn sway_tree_xwayland_uses_window_properties() {
        let tree = json!({
            "type": "root", "nodes": [{
                "type": "con", "focused": true,
                "window_properties": {"class": "Steam"}
            }]
        });
        assert_eq!(parse_sway_tree(&tree), "Steam");
    }

    #[test]
    fn sway_tree_floating_nodes_searched() {
        let tree = json!({
            "type": "root", "nodes": [{
                "type": "workspace", "nodes": [],
                "floating_nodes": [{"type": "floating_con", "app_id": "pin", "focused": true}]
            }]
        });
        assert_eq!(parse_sway_tree(&tree), "pin");
    }

    #[test]
    fn sway_tree_no_focus() {
        let tree =
            json!({"type": "root", "nodes": [{"type": "con", "app_id": "a", "focused": false}]});
        assert_eq!(parse_sway_tree(&tree), "");
        assert_eq!(parse_sway_tree(&json!({})), "");
    }

    #[test]
    fn sway_tree_focused_container_without_app_id() {
        // focused con with no app_id and no window_properties → "".
        let tree = json!({"type": "root", "nodes": [{"type": "con", "focused": true}]});
        assert_eq!(parse_sway_tree(&tree), "");
    }

    #[test]
    fn hyprland_class_and_initial_class() {
        assert_eq!(
            parse_hyprland(r#"{"class": "kitty", "initialClass": "kitty"}"#),
            "kitty"
        );
        assert_eq!(
            parse_hyprland(r#"{"initialClass": "steam_app_123"}"#),
            "steam_app_123"
        );
        assert_eq!(parse_hyprland(r#"{"class": ""}"#), "");
        assert_eq!(parse_hyprland("not json"), "");
        assert_eq!(parse_hyprland("[]"), "");
    }

    #[test]
    fn gdbus_string_extraction() {
        assert_eq!(parse_gdbus_string("(('firefox',),)"), "firefox");
        assert_eq!(parse_gdbus_string(r#"(uint32 0, '',)"#), "");
        // Python's regex matches single quotes only — double quotes are not a
        // match and yield "".
        assert_eq!(parse_gdbus_string(r#"("org.term",)"#), "");
        // Escaped quotes are unescaped exactly like Python.
        assert_eq!(parse_gdbus_string(r"('a\'b',)"), "a'b");
        assert_eq!(parse_gdbus_string(r#"('a\"b',)"#), "a\"b");
        // Other escapes keep the backslash.
        assert_eq!(parse_gdbus_string(r"('a\nb',)"), "a\\nb");
        assert_eq!(parse_gdbus_string("no strings here"), "");
        assert_eq!(parse_gdbus_string("('unterminated,)"), "");
    }

    #[test]
    fn key_value_parsing() {
        let out = "resourceClass: firefox\n\
                   resourceName: Navigator\n\
                   role: browser";
        assert_eq!(parse_key_value(out, "resourceClass"), "firefox");
        assert_eq!(parse_key_value(out, "role"), "browser");
        assert_eq!(parse_key_value(out, "missing"), "");
        // Indented keys and surrounding whitespace are handled.
        assert_eq!(
            parse_key_value("  resourceClass :  kitty  ", "resourceClass"),
            "kitty"
        );
        // Key must be a whole word (Python anchors the whole line).
        assert_eq!(
            parse_key_value("resourceClassExtra: x", "resourceClass"),
            ""
        );
    }
}
