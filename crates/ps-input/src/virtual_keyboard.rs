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

    /// Enumerate the `/dev/input/eventX` nodes the kernel created for this
    /// device — where the compositor (and tests) read our injected events.
    pub fn dev_nodes(&mut self) -> io::Result<evdev::uinput::DevNodesBlocking> {
        self.device.enumerate_dev_nodes_blocking()
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

    /// End-to-end injection loopback: emit an event through the virtual
    /// keyboard and read it back from the /dev/input node the kernel created
    /// for it.  This is exactly what the compositor consumes — the Direct
    /// Injection path delivering real input events.  Uses `KEY_FN` (no visible
    /// effect in applications) to stay harmless on a live desktop.
    #[test]
    fn injected_events_reach_the_evdev_node() {
        if !uinput_available() {
            eprintln!("skipping: /dev/uinput unavailable");
            return;
        }
        let mut keys: AttributeSet<KeyCode> = AttributeSet::new();
        keys.insert(KeyCode::KEY_FN);
        let mut vk = VirtualKeyboard::from_keys(keys).expect("uinput device");

        // Find the /dev/input/eventX node the kernel created for this device.
        let node_path = vk
            .dev_nodes()
            .expect("dev nodes")
            .find_map(|n| n.ok())
            .expect("at least one dev node");

        // Open it as a reader (what any compositor/client would do).
        // Retry briefly: udev applies the input-group permissions to a freshly
        // created node asynchronously, so an immediate open can race it.
        let mut reader = None;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            match Device::open(&node_path) {
                Ok(d) => {
                    reader = Some(d);
                    break;
                }
                Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                Err(err) => panic!("open {node_path:?} failed: {err}"),
            }
        }
        let mut reader = reader.expect("device node became openable");

        // Emit FN down/up and read them back (with a bounded wait).
        vk.key(KeyCode::KEY_FN.0, 1).unwrap();
        vk.key(KeyCode::KEY_FN.0, 0).unwrap();

        let read_deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut seen = Vec::new();
        while seen.len() < 2 && std::time::Instant::now() < read_deadline {
            match reader.fetch_events() {
                Ok(events) => {
                    for ev in events {
                        if ev.event_type() == EventType::KEY {
                            seen.push((ev.code(), ev.value()));
                        }
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(err) => panic!("read failed: {err}"),
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(
            seen,
            vec![(KeyCode::KEY_FN.0, 1), (KeyCode::KEY_FN.0, 0)],
            "injected events must arrive on the device node"
        );
    }

    /// Non-key events are forwarded with their original type.  Regression:
    /// the first live daemon run re-emitted EV_MSC/SCAN as EV_KEY, which typed
    /// phantom characters ("3#").  The kernel must accept the passthrough even
    /// though the device only declared key capabilities.
    #[test]
    fn non_key_passthrough_is_accepted() {
        if !uinput_available() {
            eprintln!("skipping: /dev/uinput unavailable");
            return;
        }
        let mut keys: AttributeSet<KeyCode> = AttributeSet::new();
        keys.insert(KeyCode::KEY_FN);
        let mut vk = VirtualKeyboard::from_keys(keys).expect("uinput device");
        // EV_MSC (4) / MSC_SCAN (4) with a scancode — same shape as the live run.
        vk.emit(&[InputEvent::new(4, 4, 42)])
            .expect("MSC passthrough must be accepted");
    }
}
