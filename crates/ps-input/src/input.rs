//! PowerSwitcher input: evdev device enumeration, keyboard grabbing, UInput virtual device.
//!
//! Port of the keyboard-discovery and event-replay logic from
//! `power_switcher/core/key_processor.py` and `power_switcher/layout/backends.py`.
//!
//! Uses the `evdev` crate for Linux input-event interface access.
//! UInput device creation follows the same pattern as the Python daemon,
//! but the daemon should NOT be run unsupervised — it grabs real keyboards.

use evdev::InputDevice;
use evdev::UInput;
use evdev::list_devices;
use evdev::ecodes as e;
use std::collections::HashSet;
use std::path::Path;
use log::{info, warn};

/// Filter used by `_find_keyboards` to select only physical keyboards.
///
/// Matches the Python daemon's selection criteria:
/// * Name must NOT contain any of: "virtual", "ydotool", "uinput", "mouse", "touchpad", "power_switcher"
/// * Must support EV_KEY event type
/// * Must have KEY_A, KEY_Z, and KEY_ENTER in its capability bits
pub fn is_physical_keyboard(dev: &InputDevice) -> bool {
    let name = dev.name().to_lowercase();
    // Exclude virtual/device names
    let excluded: &[&str] = &[
        "virtual", "ydotool", "uinput", "mouse", "touchpad", "power_switcher",
    ];
    if excluded.iter().any(|&s| name.contains(s)) {
        return false;
    }
    // Must support EV_KEY
    let caps = dev.capabilities();
    if !caps.contains(evdev::InputEventType::EV_KEY) {
        return false;
    }
    let key_caps = caps.get(evdev::InputEventType::EV_KEY).unwrap_or(&HashSet::new());
    key_caps.contains(&e.KEY_A) && key_caps.contains(&e.KEY_Z) && key_caps.contains(&e.KEY_ENTER)
}

/// Enumerate physical keyboards attached to the system, matching the
/// Python daemon's `_find_keyboards` logic.
pub fn find_keyboards() -> Vec<InputDevice> {
    let mut result = Vec::new();
    if let Ok(devices) = list_devices() {
        for dev_path in devices {
            if let Ok(dev) = InputDevice::open(&dev_path) {
                if is_physical_keyboard(&dev) {
                    result.push(dev);
                }
            }
        }
    }
    result
}

/// Create a UInput virtual device from a list of physical keyboards.
///
/// The first keyboard's capabilities are used as the basis for the virtual device,
/// so the injected events are compatible with the compositor's expectations.
pub fn create_uinput(keyboards: &[InputDevice]) -> anyhow::Result<UInput> {
    if keyboards.is_empty() {
        anyhow::bail!("No keyboards provided for UInput creation");
    }
    // Use the first keyboard's capabilities as the basis.
    // We include all key types that the first keyboard supports.
    let first = &keyboards[0];
    let capabilities = first.capabilities();

    // Build a list of supported keycodes for UInput
    let mut keycodes = Vec::new();
    if let Some(key_caps) = capabilities.get(evdev::InputEventType::EV_KEY) {
        for code in key_caps.iter() {
            // Only include keys that have a defined keycode value
            if let Ok(kc) = evdev::Key::try_from(*code) {
                keycodes.push(kc);
            }
        }
    }

    UInput::new(&keycodes, Some("power_switcher-virtual"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_is_physical_keyboard_excludes_virtual() {
        // Just verify the logic compiles and the function exists.
        // Real device testing requires /dev/input/eventX.
    }

    #[test]
    fn find_keyboards_requires_input_access() {
        // This test would need /dev/input present; we just verify the function
        // compiles and returns a result type.
        let _ = find_keyboards;
    }
}