//! Key and sentence buffers: the state of what the user is currently typing.
//!
//! Port of `power_switcher/core/key_buffer.py`, plus the Linux keycode
//! constants it depends on (previously `evdev.ecodes`).

use std::collections::VecDeque;

// ── Linux keycode constants (linux/input-event-codes.h) ─────────────────────

pub const KEY_ESC: u16 = 1;
pub const KEY_TAB: u16 = 15;
pub const KEY_BACKSPACE: u16 = 14;
pub const KEY_ENTER: u16 = 28;
pub const KEY_SPACE: u16 = 57;
pub const KEY_KPENTER: u16 = 96;

pub const KEY_LEFTSHIFT: u16 = 42;
pub const KEY_RIGHTSHIFT: u16 = 54;
pub const KEY_LEFTCTRL: u16 = 29;
pub const KEY_RIGHTCTRL: u16 = 97;
pub const KEY_LEFTALT: u16 = 56;
pub const KEY_RIGHTALT: u16 = 100;
pub const KEY_LEFTMETA: u16 = 125;
pub const KEY_RIGHTMETA: u16 = 126;

pub const KEY_COMMA: u16 = 51;
pub const KEY_DOT: u16 = 52;
pub const KEY_SEMICOLON: u16 = 39;
pub const KEY_SLASH: u16 = 53;
pub const KEY_BACKSLASH: u16 = 43;
pub const KEY_LEFTBRACE: u16 = 26;
pub const KEY_RIGHTBRACE: u16 = 27;
pub const KEY_APOSTROPHE: u16 = 40;
pub const KEY_GRAVE: u16 = 41;

pub const KEY_UP: u16 = 103;
pub const KEY_DOWN: u16 = 108;
pub const KEY_LEFT: u16 = 105;
pub const KEY_RIGHT: u16 = 106;
pub const KEY_HOME: u16 = 102;
pub const KEY_END: u16 = 107;
pub const KEY_PAGEUP: u16 = 104;
pub const KEY_PAGEDOWN: u16 = 109;

/// Keys that may end a word (hard separators plus punctuation keys).
/// NOTE: a keycode from this set is only a *boundary* when it does not produce
/// a letter in the active layout — see `LayoutSet::ambiguous_separator`.
pub const WORD_SEPARATORS: &[u16] = &[
    KEY_SPACE,
    KEY_ENTER,
    KEY_KPENTER,
    KEY_TAB,
    KEY_COMMA,
    KEY_DOT,
    KEY_SEMICOLON,
    KEY_SLASH,
    KEY_BACKSLASH,
    KEY_LEFTBRACE,
    KEY_RIGHTBRACE,
    KEY_APOSTROPHE,
    KEY_GRAVE,
];

/// Navigation keys that invalidate the typed context.
pub const BUFFER_RESET_KEYS: &[u16] = &[
    KEY_ESC,
    KEY_UP,
    KEY_DOWN,
    KEY_LEFT,
    KEY_RIGHT,
    KEY_HOME,
    KEY_END,
    KEY_PAGEUP,
    KEY_PAGEDOWN,
];

pub fn is_word_separator(kc: u16) -> bool {
    WORD_SEPARATORS.contains(&kc)
}

pub fn is_buffer_reset_key(kc: u16) -> bool {
    BUFFER_RESET_KEYS.contains(&kc)
}

/// Hard separators always end a word (Space/Enter/KP-Enter/Tab).
pub fn is_hard_separator(kc: u16) -> bool {
    kc == KEY_SPACE || kc == KEY_ENTER || kc == KEY_KPENTER || kc == KEY_TAB
}

// ── KeyBuffer ────────────────────────────────────────────────────────────────

/// The word currently being typed, as `(keycode, char)` pairs.
#[derive(Debug, Default)]
pub struct KeyBuffer {
    max_len: usize,
    word: VecDeque<(u16, char)>,
}

impl KeyBuffer {
    pub fn new(max_len: usize) -> Self {
        Self {
            max_len,
            word: VecDeque::new(),
        }
    }

    pub fn add(&mut self, keycode: u16, ch: char) {
        if self.word.len() < self.max_len {
            self.word.push_back((keycode, ch));
        }
    }

    pub fn backspace(&mut self) {
        self.word.pop_back();
    }

    /// Return the buffered text and clear the buffer.
    pub fn flush(&mut self) -> String {
        let text = self.text();
        self.word.clear();
        text
    }

    pub fn clear(&mut self) {
        self.word.clear();
    }

    pub fn text(&self) -> String {
        self.word.iter().map(|(_, c)| c).collect()
    }

    /// Alias for [`Self::text`] — the word currently being typed.
    pub fn current_word(&self) -> String {
        self.text()
    }

    pub fn len(&self) -> usize {
        self.word.len()
    }

    pub fn is_empty(&self) -> bool {
        self.word.is_empty()
    }

    pub fn keycodes(&self) -> Vec<u16> {
        self.word.iter().map(|(kc, _)| *kc).collect()
    }
}

// ── SentenceBuffer ───────────────────────────────────────────────────────────

/// Tracks the current sentence (multiple words with separators).
/// Supports soft reset (preserve current word, clear history) and hard reset.
#[derive(Debug, Default, Clone)]
pub struct SentenceBuffer {
    parts: Vec<String>,
    group_history: Vec<String>,
}

impl SentenceBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn text(&self) -> String {
        self.parts.concat()
    }

    /// The current word: the last contiguous run of non-separator characters.
    pub fn current_word(&self) -> String {
        match self.parts.last() {
            None => String::new(),
            Some(last) => {
                if last == " " || last == "\n" || last == "\t" {
                    String::new()
                } else {
                    last.clone()
                }
            }
        }
    }

    pub fn append(&mut self, part: &str) {
        self.parts.push(part.to_string());
    }

    pub fn pop(&mut self) {
        self.parts.pop();
    }

    pub fn clear(&mut self) {
        self.parts.clear();
        self.group_history.clear();
    }

    /// Soft context reset: clear word history but keep the current word.
    /// Used on app switch (cursor didn't move).  The word is re-split into
    /// per-character parts (matching Python's `list(word)`), so a later
    /// [`pop`](Self::pop) removes a single character, not the whole word.
    pub fn soft_reset(&mut self) {
        let word = self.current_word();
        self.parts.clear();
        self.group_history.clear();
        if !word.is_empty() {
            self.parts = word.chars().map(|c| c.to_string()).collect();
        }
    }

    pub fn invalidate_group_history(&mut self) {
        self.group_history.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_text() {
        let mut buf = KeyBuffer::new(100);
        buf.add(30, 'a');
        buf.add(48, 'b');
        assert_eq!(buf.text(), "ab");
    }

    #[test]
    fn backspace() {
        let mut buf = KeyBuffer::new(100);
        buf.add(30, 'a');
        buf.add(48, 'b');
        buf.backspace();
        assert_eq!(buf.text(), "a");
    }

    #[test]
    fn flush() {
        let mut buf = KeyBuffer::new(100);
        buf.add(30, 'a');
        buf.add(48, 'b');
        let result = buf.flush();
        assert_eq!(result, "ab");
        assert_eq!(buf.text(), "");
    }

    #[test]
    fn clear() {
        let mut buf = KeyBuffer::new(100);
        buf.add(30, 'a');
        buf.add(48, 'b');
        buf.clear();
        assert_eq!(buf.text(), "");
    }

    #[test]
    fn max_len() {
        let mut buf = KeyBuffer::new(2);
        buf.add(30, 'a');
        buf.add(48, 'b');
        buf.add(46, 'c');
        assert_eq!(buf.text(), "ab");
        assert_eq!(buf.len(), 2);
    }

    #[test]
    fn keycodes() {
        let mut buf = KeyBuffer::new(100);
        buf.add(30, 'a');
        buf.add(48, 'b');
        assert_eq!(buf.keycodes(), vec![30, 48]);
    }

    #[test]
    fn keycodes_tracked_on_backspace() {
        let mut buf = KeyBuffer::new(100);
        buf.add(30, 'a');
        buf.add(48, 'b');
        buf.backspace();
        assert_eq!(buf.keycodes(), vec![30]);
    }

    #[test]
    fn sentence_current_word() {
        let mut s = SentenceBuffer::new();
        assert_eq!(s.current_word(), "");
        s.append("hello");
        s.append(" ");
        assert_eq!(s.current_word(), "");
        s.append("мир");
        assert_eq!(s.current_word(), "мир");
        assert_eq!(s.text(), "hello мир");
    }

    #[test]
    fn sentence_soft_reset_keeps_word() {
        let mut s = SentenceBuffer::new();
        s.append("hello");
        s.append(" ");
        s.append("мир");
        s.soft_reset();
        assert_eq!(s.text(), "мир");
        // Python quirk (faithfully reproduced): after soft_reset the parts are
        // per-character, so a second soft_reset keeps only the *last* char
        // (`word = current_word` = parts[-1] = "р").
        s.soft_reset();
        assert_eq!(s.text(), "р");
    }

    #[test]
    fn sentence_soft_reset_splits_word_into_chars() {
        // Python: soft_reset does `parts = list(word)` — pop() must then
        // remove a single character, not the whole word, and current_word
        // (parts[-1]) is the last character.
        let mut s = SentenceBuffer::new();
        s.append("привет");
        s.soft_reset();
        s.pop();
        assert_eq!(s.text(), "приве");
        assert_eq!(s.current_word(), "е");
    }

    #[test]
    fn sentence_pop_and_clear() {
        let mut s = SentenceBuffer::new();
        s.append("ab");
        s.append(" ");
        s.pop();
        assert_eq!(s.text(), "ab");
        s.clear();
        assert_eq!(s.text(), "");
        assert_eq!(s.current_word(), "");
    }

    #[test]
    fn separator_constants_match_evdev() {
        // Pinned against `evdev.ecodes` so the port cannot drift.
        assert_eq!(KEY_SPACE, 57);
        assert_eq!(KEY_ENTER, 28);
        assert_eq!(KEY_KPENTER, 96);
        assert_eq!(KEY_TAB, 15);
        assert_eq!(KEY_COMMA, 51);
        assert_eq!(KEY_DOT, 52);
        assert_eq!(KEY_SEMICOLON, 39);
        assert_eq!(KEY_SLASH, 53);
        assert_eq!(KEY_BACKSLASH, 43);
        assert_eq!(KEY_LEFTBRACE, 26);
        assert_eq!(KEY_RIGHTBRACE, 27);
        assert_eq!(KEY_APOSTROPHE, 40);
        assert_eq!(KEY_GRAVE, 41);
        assert_eq!(KEY_ESC, 1);
        assert_eq!(KEY_UP, 103);
        assert_eq!(KEY_DOWN, 108);
        assert_eq!(KEY_LEFT, 105);
        assert_eq!(KEY_RIGHT, 106);
        assert_eq!(KEY_HOME, 102);
        assert_eq!(KEY_END, 107);
        assert_eq!(KEY_PAGEUP, 104);
        assert_eq!(KEY_PAGEDOWN, 109);
        assert_eq!(KEY_BACKSPACE, 14);
    }
}
