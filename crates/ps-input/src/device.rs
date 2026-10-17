//! Keyboard discovery and filtering.
//!
//! Port of `KeyProcessor._find_keyboards` (`power_switcher/core/key_processor.py`):
//! only *physical* keyboards are grabbed — virtual devices, mice, touchpads and
//! our own UInput device are ignored based on their names and capabilities.

use evdev::{Device, EventType, KeyCode};
use log::{debug, warn};

/// Substrings that disqualify a device (Python's exclusion list).
const EXCLUDED_NAME_PARTS: &[&str] = &[
    "virtual",
    "ydotool",
    "uinput",
    "mouse",
    "touchpad",
    "power_switcher",
];

/// A device is a real keyboard when:
/// 1. its name contains none of [`EXCLUDED_NAME_PARTS`],
/// 2. it supports `EV_KEY`,
/// 3. its key set includes `KEY_A`, `KEY_Z` and `KEY_ENTER`.
///
/// Pure decision function over the device properties — unit-testable without
/// `/dev/input`.  A device with no name is rejected (conservative: Python's
/// `dev.name.lower()` would throw and skip such a device).
pub fn keyboard_decision(name: Option<&str>, has_ev_key: bool, keys: &[KeyCode]) -> bool {
    let Some(raw) = name else {
        return false;
    };
    let name = raw.to_lowercase();
    if EXCLUDED_NAME_PARTS.iter().any(|s| name.contains(s)) {
        return false;
    }
    if !has_ev_key {
        return false;
    }
    // Python: {KEY_A, KEY_Z, KEY_ENTER}.issubset(capabilities[EV_KEY])
    [KeyCode::KEY_A, KeyCode::KEY_Z, KeyCode::KEY_ENTER]
        .iter()
        .all(|k| keys.contains(k))
}

/// Convenience wrapper applying [`keyboard_decision`] to a real device.
pub fn is_physical_keyboard(dev: &Device) -> bool {
    let has_ev_key = dev.supported_events().contains(EventType::KEY);
    let keys: Vec<KeyCode> = dev
        .supported_keys()
        .map(|k| k.iter().collect())
        .unwrap_or_default();
    keyboard_decision(dev.name(), has_ev_key, &keys)
}

/// Enumerate physical keyboards (port of `_find_keyboards`).
///
/// Devices that fail to open are skipped (Python catches every exception in
/// the loop).
pub fn find_keyboards() -> Vec<Device> {
    let mut result = Vec::new();
    for (path, dev) in evdev::enumerate() {
        if is_physical_keyboard(&dev) {
            result.push(dev);
        } else {
            debug!("skipping {}", path.display());
        }
    }
    if result.is_empty() {
        warn!("no physical keyboards found (insufficient permissions? try the `input` group)");
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The standard key set of a plain PC keyboard (QWERTY letters + Enter).
    fn pc_keys() -> Vec<KeyCode> {
        vec![
            KeyCode::KEY_A,
            KeyCode::KEY_Z,
            KeyCode::KEY_ENTER,
            KeyCode::KEY_SPACE,
            KeyCode::KEY_LEFTSHIFT,
        ]
    }

    #[test]
    fn accepts_plain_keyboard() {
        assert!(keyboard_decision(
            Some("AT Translated Set 2 keyboard"),
            true,
            &pc_keys()
        ));
    }

    #[test]
    fn rejects_virtual_devices() {
        for name in [
            "power_switcher-virtual",
            "Virtual Box Keyboard",
            "ydotool virtual device",
            "uinput-gen",
        ] {
            assert!(
                !keyboard_decision(Some(name), true, &pc_keys()),
                "name={name}"
            );
        }
    }

    #[test]
    fn rejects_mice_and_touchpads() {
        for name in ["Logitech Mouse", "Synaptics Touchpad"] {
            assert!(
                !keyboard_decision(Some(name), true, &pc_keys()),
                "name={name}"
            );
        }
    }

    #[test]
    fn rejects_missing_ev_key() {
        assert!(!keyboard_decision(Some("Some Pad"), false, &pc_keys()));
    }

    #[test]
    fn rejects_partial_key_set() {
        // A device with letters but no KEY_Z (e.g. a numpad-ish gadget).
        let partial = vec![KeyCode::KEY_A, KeyCode::KEY_ENTER];
        assert!(!keyboard_decision(Some("Odd Gadget"), true, &partial));
    }

    #[test]
    fn rejects_empty_key_set() {
        assert!(!keyboard_decision(
            Some("AT Translated Set 2 keyboard"),
            true,
            &[]
        ));
    }

    #[test]
    fn rejects_missing_name() {
        // Python lowercases dev.name — None/empty names fail the subset check
        // only via keys; a full key set with no name is still a keyboard in
        // Python (name.lower() on None would crash, but evdev always has a
        // name); we treat missing names conservatively as non-keyboard.
        assert!(!keyboard_decision(None, true, &pc_keys()));
    }
}
