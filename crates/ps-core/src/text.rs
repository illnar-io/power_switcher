//! Pure text helpers used by the conversion pipeline.
//!
//! Port of the static/pure helpers on `KeyProcessor`
//! (`power_switcher/core/key_processor.py`): `_split_last_word`,
//! `_changed_char_count` and `_check_double_caps`.
//!
//! Minor Unicode caveats (Python semantics preserved for Latin/Cyrillic, the
//! supported scope): Python's `str.rstrip` also trims a few control chars
//! (U+001C–U+001F) that Rust's `trim_end` keeps.

/// Split `text` into `(prefix, last_word, trailing_whitespace)`.
///
/// `prefix + last_word + suffix` reproduces the original string exactly, so
/// the caller can retype the whole line while converting only the final word.
pub fn split_last_word(text: &str) -> (String, String, String) {
    let stripped = text.trim_end();
    let suffix = &text[stripped.len()..];
    let cut = [" ", "\n", "\t"]
        .iter()
        .filter_map(|sep| stripped.rfind(sep))
        .max();
    match cut {
        Some(cut) => (
            stripped[..cut + 1].to_string(),
            stripped[cut + 1..].to_string(),
            suffix.to_string(),
        ),
        None => (String::new(), stripped.to_string(), suffix.to_string()),
    }
}

/// Number of characters actually different between two strings.
pub fn changed_char_count(old: &str, new: &str) -> usize {
    let mut count = old.chars().zip(new.chars()).filter(|(a, b)| a != b).count();
    count += old.chars().count().abs_diff(new.chars().count());
    count
}

/// Fix DOUBLE→Double style caps typos: two uppercase letters followed by a
/// lowercase one (`HELLO` typed as `HEllo` → `Hello`).  Returns the fixed word,
/// or `None` when the word doesn't match the pattern.
pub fn check_double_caps(word: &str) -> Option<String> {
    let mut chars = word.chars();
    let (Some(c0), Some(c1), Some(c2)) = (chars.next(), chars.next(), chars.next()) else {
        return None;
    };
    if c0.is_uppercase() && c1.is_uppercase() && c2.is_lowercase() {
        let mut out = String::with_capacity(word.len());
        out.push(c0);
        out.extend(c1.to_lowercase());
        // Python: `word[0] + word[1].lower() + word[2:]`
        out.push_str(&word[c0.len_utf8() + c1.len_utf8()..]);
        Some(out)
    } else {
        None
    }
}

/// Heuristic: does this word look like a password / random string that should
/// never be converted?  Port of `LayoutSet.is_password` (kept here next to its
/// siblings; `LayoutSet::is_password` delegates).
///
/// At least 3 of 4 character classes (upper, lower, digit, special) and longer
/// than 6 characters.
pub fn is_password(word: &str) -> bool {
    let char_count = word.chars().count();
    if char_count <= 6 {
        return false;
    }
    let has_upper = word.chars().any(|c| c.is_uppercase());
    let has_lower = word.chars().any(|c| c.is_lowercase());
    // Python `isdigit()` ≈ Unicode Nd (plus digit-typed No like '²');
    // Rust `is_numeric` covers Nd|Nl|No — a superset, irrelevant for the
    // Latin/Cyrillic scope of this app.
    let has_digit = word.chars().any(|c| c.is_numeric());
    let has_special = word.chars().any(|c| !(c.is_alphabetic() || c.is_numeric()));
    (has_upper as u8 + has_lower as u8 + has_digit as u8 + has_special as u8) >= 3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_last_word_cases() {
        // Ported from tests/test_key_processor.py::test_split_last_word.
        let cases: [(&str, &str, &str, &str); 5] = [
            ("hello мир", "hello ", "мир", ""),
            ("hello мир  ", "hello ", "мир", "  "),
            ("мир", "", "мир", ""),
            ("a\tb", "a\t", "b", ""),
            ("trailing ", "", "trailing", " "),
        ];
        for (text, prefix, word, suffix) in cases {
            let (p, w, s) = split_last_word(text);
            assert_eq!(
                (p.as_str(), w.as_str(), s.as_str()),
                (prefix, word, suffix),
                "text={text:?}"
            );
            // Round-trip invariant from the Python test.
            assert_eq!(format!("{p}{w}{s}"), text);
        }
    }

    #[test]
    fn changed_char_count_cases() {
        // Ported from tests/test_key_processor.py::test_changed_char_count.
        assert_eq!(changed_char_count("abc", "abc"), 0);
        assert_eq!(changed_char_count("abc", "abd"), 1);
        assert_eq!(changed_char_count("abc", "abcd"), 1);
        assert_eq!(changed_char_count("ghbdtn", "привет"), 6);
    }

    #[test]
    fn double_caps_fix() {
        assert_eq!(check_double_caps("HEllo").as_deref(), Some("Hello"));
        assert_eq!(check_double_caps("ПРивет").as_deref(), Some("Привет"));
        // Too short.
        assert_eq!(check_double_caps("AB"), None);
        // Third char not lowercase ('И' is uppercase).
        assert_eq!(check_double_caps("ПРИвет"), None);
        assert_eq!(check_double_caps("ABC"), None);
        // First/second not both uppercase.
        assert_eq!(check_double_caps("Abc"), None);
        assert_eq!(check_double_caps("abc"), None);
    }

    #[test]
    fn password_heuristic() {
        // <= 6 chars: never a password.
        assert!(!is_password("Aa1!xy"));
        // 3+ classes: password-like.
        assert!(is_password("Aa1!xyz"));
        assert!(is_password("qwerty123X")); // lower+digit+upper = 3 classes
                                            // 2 classes only: normal word / simple password.
        assert!(!is_password("приветик"));
        assert!(!is_password("abcdefgh"));
    }
}
