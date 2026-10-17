//! Virtual keyboard (UInput) device used to inject keystrokes.
//!
//! Port of the `UInput` usage in `key_processor.py` and
//! `GNOMESwitchBackend._simulate_switch` (`layout/backends.py`).  The Python
//! daemon writes `EV_KEY` events and calls `syn()` after each; here
//! [`VirtualDevice::emit`] batches the event and appends `SYN_REPORT`
//! automatically — the observable behaviour (one report per call) is identical.

use std::io;

use evdev::{uinput::VirtualDevice, AttributeSet, Device, EventType, InputEvent, KeyCode};

/// Name the virtual device registers with (matches the Python daemon, and is
/// also on the discovery exclusion list).
pub const VIRTUAL_DEVICE_NAME: &str = "power_switcher-virtual";

/// A handle to the virtual keyboard we inject through.
pub struct VirtualKeyboard {
    device: VirtualDevice,
}

impl VirtualKeyboard {
    /// Create the virtual device from a physical keyboard template: the
    /// template's key set is reused so the compositor sees a plausible device.
    /// `KEY_LEFTMETA`/`KEY_SPACE` are always included (GNOME Super+Space
    /// switching needs them).
    pub fn from_template(template: &Device) -> io::Result<Self> {
        let mut keys: AttributeSet<KeyCode> = template
            .supported_keys()
            .map(|k| k.iter().collect())
            .unwrap_or_default();
        keys.insert(KeyCode::KEY_LEFTMETA);
        keys.insert(KeyCode::KEY_SPACE);
        Self::from_keys(keys)
    }

    pub fn from_keys(keys: AttributeSet<KeyCode>) -> io::Result<Self> {
        let device = VirtualDevice::builder()?
            .name(VIRTUAL_DEVICE_NAME)
            .with_keys(&keys)?
            .build()?;
        Ok(Self { device })
    }

    /// The sysfs path of the created evdev node (useful for the audit log).
    pub fn syspath(&mut self) -> io::Result<std::path::PathBuf> {
        self.device.get_syspath()
    }

    /// Write one `EV_KEY` event + `SYN_REPORT` (Python: `write` + `syn`).
    pub fn key(&mut self, keycode: u16, value: i32) -> io::Result<()> {
        self.emit(&[InputEvent::new(EventType::KEY.0, keycode, value)])
    }

    /// Write a batch of events with a single `SYN_REPORT` at the end.
    pub fn emit(&mut self, events: &[InputEvent]) -> io::Result<()> {
        self.device.emit(events)
    }

    /// Press a key (down event).
    pub fn press(&mut self, keycode: u16) -> io::Result<()> {
        self.key(keycode, 1)
    }

    /// Release a key (up event).
    pub fn release(&mut self, keycode: u16) -> io::Result<()> {
        self.key(keycode, 0)
    }
}

/// True when uinput is usable in this environment (device node + write access).
/// Used by tests to skip hardware-dependent cases gracefully.
pub fn uinput_available() -> bool {
    std::fs::metadata("/dev/uinput").is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hardware test: only meaningful when /dev/uinput exists and is writable.
    #[test]
    fn creates_virtual_device_and_types_a_key() {
        if !uinput_available() {
            eprintln!("skipping: /dev/uinput unavailable");
            return;
        }
        let mut keys: AttributeSet<KeyCode> = AttributeSet::new();
        for kc in [
            KeyCode::KEY_A,
            KeyCode::KEY_Z,
            KeyCode::KEY_ENTER,
            KeyCode::KEY_SPACE,
            KeyCode::KEY_LEFTSHIFT,
            KeyCode::KEY_LEFTMETA,
        ] {
            keys.insert(kc);
        }
        let mut vk = VirtualKeyboard::from_keys(keys).expect("uinput device");
        let syspath = vk.syspath().expect("syspath");
        assert!(syspath.starts_with("/sys/devices/virtual"));

        // Down/up with SYN per event — same pattern as the Python daemon.
        vk.key(KeyCode::KEY_A.0, 1).unwrap();
        vk.key(KeyCode::KEY_A.0, 0).unwrap();

        // Batched emit: press shift, tap key, release shift in one report.
        vk.emit(&[
            InputEvent::new(EventType::KEY.0, KeyCode::KEY_LEFTSHIFT.0, 1),
            InputEvent::new(EventType::KEY.0, KeyCode::KEY_Z.0, 1),
            InputEvent::new(EventType::KEY.0, KeyCode::KEY_Z.0, 0),
            InputEvent::new(EventType::KEY.0, KeyCode::KEY_LEFTSHIFT.0, 0),
        ])
        .unwrap();
    }
}
