//! PowerSwitcher input layer (Rust port, phase 2).
//!
//! Linux input plumbing: evdev device discovery and grabbing, and the UInput
//! virtual keyboard used for direct injection (the "Direct Injection" design —
//! no external tools like `ydotool`).
//!
//! Layer map (Python → Rust):
//! * `KeyProcessor._find_keyboards` → [`device::find_keyboards`]
//! * `UInput.from_device(...)` + `write/syn` → [`virtual_keyboard::VirtualKeyboard`]
//! * `GNOMESwitchBackend._simulate_switch` (Super+Space) → [`virtual_keyboard::VirtualKeyboard`]

pub mod device;
pub mod virtual_keyboard;

pub use device::{find_keyboards, is_physical_keyboard, keyboard_decision};
pub use virtual_keyboard::{uinput_available, VirtualKeyboard, VIRTUAL_DEVICE_NAME};

/// Linux keycodes the daemon logic depends on (mirrors `evdev.ecodes` in
/// Python; kept here so ps-layout/ps-focus need not depend on the `evdev`
/// crate's enums).
pub mod keycodes {
    pub const KEY_ESC: u16 = 1;
    pub const KEY_BACKSPACE: u16 = 14;
    pub const KEY_TAB: u16 = 15;
    pub const KEY_ENTER: u16 = 28;
    pub const KEY_LEFTCTRL: u16 = 29;
    pub const KEY_LEFTSHIFT: u16 = 42;
    pub const KEY_RIGHTSHIFT: u16 = 54;
    pub const KEY_LEFTALT: u16 = 56;
    pub const KEY_SPACE: u16 = 57;
    pub const KEY_CAPSLOCK: u16 = 58;
    pub const KEY_COMMA: u16 = 51;
    pub const KEY_DOT: u16 = 52;
    pub const KEY_SEMICOLON: u16 = 39;
    pub const KEY_SLASH: u16 = 53;
    pub const KEY_APOSTROPHE: u16 = 40;
    pub const KEY_GRAVE: u16 = 41;
    pub const KEY_LEFTBRACE: u16 = 26;
    pub const KEY_RIGHTBRACE: u16 = 27;
    pub const KEY_BACKSLASH: u16 = 43;
    pub const KEY_UP: u16 = 103;
    pub const KEY_LEFT: u16 = 105;
    pub const KEY_RIGHT: u16 = 106;
    pub const KEY_DOWN: u16 = 108;
    pub const KEY_HOME: u16 = 102;
    pub const KEY_END: u16 = 107;
    pub const KEY_PAGEUP: u16 = 104;
    pub const KEY_PAGEDOWN: u16 = 109;
    pub const KEY_LEFTMETA: u16 = 125;
    pub const KEY_KPENTER: u16 = 96;

    /// `EV_KEY` type code.
    pub const EV_KEY: u16 = 1;
    /// `EV_SYN` type code.
    pub const EV_SYN: u16 = 0;
}
