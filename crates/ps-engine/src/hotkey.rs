//! Manual-convert hotkey parsing.
//!
//! Caramba Switcher binds Pause/Break, but laptops rarely have that key.  The
//! config value is therefore a small, explicit grammar:
//!
//! * `off` / `none` / empty — no extra hotkey (double-tap Shift still works);
//! * `double_shift` — only the built-in double-Shift gesture;
//! * `pause` / `break` / `pause/break` — the Caramba default;
//! * a modifier+key combo such as `ctrl+alt+z`, `alt+grave`, `ctrl+shift+f12`.
//!
//! Modifiers: `ctrl`/`control`, `alt`, `shift`, `super`/`meta`/`win`.
//! Keys: `a`–`z`, `0`–`9`, `f1`–`f12`, `space`, `enter`, `tab`, `backspace`,
//! `insert`, `delete`, `home`, `end`, `pageup`, `pagedown`, `pause`, `break`,
//! `grave`/`` ` ``, `minus`/`-`, `equal`/`=`, `comma`/`,`, `dot`/`.`,
//! `slash`/`/`, `backslash`/`\`, `semicolon`/`;`, `apostrophe`/`'`,
//! `leftbrace`/`[`, `rightbrace`/`]`.

use ps_core::modifier_state::{ALT_KEYS, CTRL_KEYS, META_KEYS, SHIFT_KEYS};

pub const KEY_PAUSE: u16 = 119;

const KEY_INSERT: u16 = 110;
const KEY_DELETE: u16 = 111;
const KEY_SCROLLLOCK: u16 = 70;
const KEY_MINUS: u16 = 12;
const KEY_EQUAL: u16 = 13;

/// A parsed manual-convert hotkey: exact modifier set + non-modifier key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hotkey {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
    pub key: u16,
}

impl Hotkey {
    /// True when `kc` is the hotkey key and the currently held modifiers match
    /// the configured combination exactly (no extra modifiers held).
    pub fn matches(&self, kc: u16, ctrl: bool, alt: bool, shift: bool, meta: bool) -> bool {
        kc == self.key
            && ctrl == self.ctrl
            && alt == self.alt
            && shift == self.shift
            && meta == self.meta
    }
}

/// Parse a `hotkey_convert` value.
///
/// `Ok(None)` means "no extra hotkey"; `Err` carries a human-readable reason
/// for the warning log (the caller treats it as disabled).
pub fn parse(spec: &str) -> Result<Option<Hotkey>, String> {
    let spec = spec.trim().to_ascii_lowercase();
    match spec.as_str() {
        "" | "off" | "none" | "disabled" | "double_shift" | "double-shift" => return Ok(None),
        _ => {}
    }

    let mut ctrl = false;
    let mut alt = false;
    let mut shift = false;
    let mut meta = false;
    let mut key = None;

    for raw in spec.split('+') {
        let token = raw.trim();
        if token.is_empty() {
            return Err(format!("empty part in hotkey {spec:?}"));
        }
        match token {
            "ctrl" | "control" | "ctl" => ctrl = true,
            "alt" | "option" | "opt" => alt = true,
            "shift" => shift = true,
            "super" | "meta" | "win" | "cmd" | "command" => meta = true,
            _ => {
                if key.is_some() {
                    return Err(format!("more than one key in hotkey {spec:?}"));
                }
                key = Some(
                    parse_key(token)
                        .ok_or_else(|| format!("unknown key {token:?} in hotkey {spec:?}"))?,
                );
            }
        }
    }

    let key = key.ok_or_else(|| format!("hotkey {spec:?} has no key"))?;
    if is_modifier_key(key) {
        return Err(format!("hotkey {spec:?} must not be a modifier key"));
    }
    Ok(Some(Hotkey {
        ctrl,
        alt,
        shift,
        meta,
        key,
    }))
}

fn is_modifier_key(kc: u16) -> bool {
    SHIFT_KEYS.contains(&kc)
        || CTRL_KEYS.contains(&kc)
        || ALT_KEYS.contains(&kc)
        || META_KEYS.contains(&kc)
}

fn parse_key(token: &str) -> Option<u16> {
    // Letters (KEY_A = 30 … KEY_Z = 44 with a gap at Shift/Grave, so use a
    // table rather than arithmetic).
    const LETTERS: [(char, u16); 26] = [
        ('a', 30),
        ('b', 48),
        ('c', 46),
        ('d', 32),
        ('e', 18),
        ('f', 33),
        ('g', 34),
        ('h', 35),
        ('i', 23),
        ('j', 36),
        ('k', 37),
        ('l', 38),
        ('m', 50),
        ('n', 49),
        ('o', 24),
        ('p', 25),
        ('q', 16),
        ('r', 19),
        ('s', 31),
        ('t', 20),
        ('u', 22),
        ('v', 47),
        ('w', 17),
        ('x', 45),
        ('y', 21),
        ('z', 44),
    ];
    // Digits: KEY_1 = 2 … KEY_9 = 10, KEY_0 = 11.
    const DIGITS: [(char, u16); 10] = [
        ('1', 2),
        ('2', 3),
        ('3', 4),
        ('4', 5),
        ('5', 6),
        ('6', 7),
        ('7', 8),
        ('8', 9),
        ('9', 10),
        ('0', 11),
    ];

    if let Some(kc) = token.chars().next().filter(|_| token.chars().count() == 1) {
        if let Some((_, kc)) = LETTERS.iter().find(|(c, _)| *c == kc) {
            return Some(*kc);
        }
        if let Some((_, kc)) = DIGITS.iter().find(|(c, _)| *c == kc) {
            return Some(*kc);
        }
    }

    if let Some(n) = token.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
        return match n {
            1..=10 => Some(58 + u16::from(n)), // KEY_F1 = 59 … KEY_F10 = 68
            11 => Some(87),
            12 => Some(88),
            _ => None,
        };
    }

    Some(match token {
        "space" => 57,
        "enter" | "return" => 28,
        "tab" => 15,
        "backspace" | "bksp" => 14,
        "esc" | "escape" => 1,
        "insert" | "ins" => KEY_INSERT,
        "delete" | "del" => KEY_DELETE,
        "home" => 102,
        "end" => 107,
        "pageup" | "pgup" => 104,
        "pagedown" | "pgdn" => 109,
        "pause" | "break" | "pause/break" => KEY_PAUSE,
        "scrolllock" | "scroll" => KEY_SCROLLLOCK,
        "grave" | "backtick" | "`" => 41,
        "minus" | "-" => KEY_MINUS,
        "equal" | "=" => KEY_EQUAL,
        "leftbrace" | "bracketleft" | "[" => 26,
        "rightbrace" | "bracketright" | "]" => 27,
        "semicolon" | ";" => 39,
        "apostrophe" | "quote" | "'" => 40,
        "comma" | "," => 51,
        "dot" | "period" | "." => 52,
        "slash" | "/" => 53,
        "backslash" | "\\" => 43,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caramba_pause_is_supported() {
        for spec in ["pause", "break", "PAUSE/BREAK"] {
            let hk = parse(spec).unwrap().unwrap();
            assert_eq!(hk.key, KEY_PAUSE);
            assert!(!hk.ctrl && !hk.alt && !hk.shift && !hk.meta);
        }
    }

    #[test]
    fn laptop_combo_is_supported() {
        let hk = parse("Ctrl+Alt+Z").unwrap().unwrap();
        assert_eq!(
            hk,
            Hotkey {
                ctrl: true,
                alt: true,
                shift: false,
                meta: false,
                key: 44,
            }
        );
        assert!(hk.matches(44, true, true, false, false));
        assert!(!hk.matches(44, true, false, false, false)); // Alt missing
        assert!(!hk.matches(44, true, true, true, false)); // extra Shift
        assert!(!hk.matches(45, true, true, false, false)); // other key
    }

    #[test]
    fn off_is_disabled() {
        for spec in ["", "off", "none", "double_shift"] {
            assert_eq!(parse(spec).unwrap(), None, "{spec:?}");
        }
    }

    #[test]
    fn invalid_specs_are_rejected() {
        for spec in ["ctrl+alt", "ctrl+alt+z+x", "hyper+z", "ctrl+", "shift"] {
            assert!(parse(spec).is_err(), "{spec:?} should be rejected");
        }
    }

    #[test]
    fn named_keys_parse() {
        assert_eq!(parse("ctrl+grave").unwrap().unwrap().key, 41);
        assert_eq!(parse("f12").unwrap().unwrap().key, 88);
        assert_eq!(parse("ctrl+shift+space").unwrap().unwrap().key, 57);
        assert_eq!(parse("meta+enter").unwrap().unwrap().key, 28);
    }
}
