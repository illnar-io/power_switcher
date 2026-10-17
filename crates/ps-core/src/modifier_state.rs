//! Modifier state tracking with Caramba-style double-tap Shift gestures.
//!
//! Port of `power_switcher/core/modifier_state.py`.  `update` takes an
//! explicit `now` timestamp for determinism; the daemon passes its own clock
//! (there is a `*_now` convenience for the system clock).
//!
//! Gestures (timings identical to the Python version):
//! * quick tap of Shift (< 150 ms) → [`ShiftAction::SingleShift`] — toggle layout;
//! * double tap (two quick taps, gap < 300 ms) → [`ShiftAction::DoubleShift`] —
//!   convert the last word;
//! * pressing any other key cancels pending taps.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::key_buffer::{
    KEY_LEFTALT, KEY_LEFTCTRL, KEY_LEFTMETA, KEY_LEFTSHIFT, KEY_RIGHTALT, KEY_RIGHTCTRL,
    KEY_RIGHTMETA, KEY_RIGHTSHIFT,
};

/// Maximum gap between release and the next press to count as a double tap.
const DOUBLE_TAP_INTERVAL: f64 = 0.3;
/// A tap is a press+release shorter than this.
const TAP_MAX_HOLD: f64 = 0.15;

pub const SHIFT_KEYS: [u16; 2] = [KEY_LEFTSHIFT, KEY_RIGHTSHIFT];
pub const CTRL_KEYS: [u16; 2] = [KEY_LEFTCTRL, KEY_RIGHTCTRL];
pub const ALT_KEYS: [u16; 2] = [KEY_LEFTALT, KEY_RIGHTALT];
pub const META_KEYS: [u16; 2] = [KEY_LEFTMETA, KEY_RIGHTMETA];

pub fn is_shift_key(kc: u16) -> bool {
    SHIFT_KEYS.contains(&kc)
}

/// Caramba-style gesture detected on Shift release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShiftAction {
    /// Single quick tap: toggle the layout.
    SingleShift,
    /// Double tap: convert the last word.
    DoubleShift,
}

fn system_now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

#[derive(Debug, Default, Clone)]
pub struct ModifierState {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
    /// Timing for Caramba-style logic.
    pub last_shift_press_ts: f64,
    pub last_shift_release_ts: f64,
    pub shift_tap_count: u32,
}

impl ModifierState {
    /// Update state from the system clock.
    pub fn update_now(&mut self, keycode: u16, pressed: bool) -> Option<ShiftAction> {
        self.update(keycode, pressed, system_now())
    }

    /// Update state and return a Caramba action if one was detected:
    /// * [`ShiftAction::DoubleShift`] — convert last word;
    /// * [`ShiftAction::SingleShift`] — toggle layout.
    pub fn update(&mut self, keycode: u16, pressed: bool, now: f64) -> Option<ShiftAction> {
        let mut action = None;

        if is_shift_key(keycode) {
            if pressed {
                // Caramba: detect double press
                if now - self.last_shift_release_ts < DOUBLE_TAP_INTERVAL {
                    self.shift_tap_count += 1;
                } else {
                    self.shift_tap_count = 1;
                }
                self.last_shift_press_ts = now;
                self.shift = true;
            } else {
                let hold_duration = now - self.last_shift_press_ts;
                self.last_shift_release_ts = now;
                self.shift = false;

                // Heuristic: a quick tap (< 150 ms) with no other keys pressed
                // (checked by KeyProcessor) may be a toggle or convert.
                if hold_duration < TAP_MAX_HOLD {
                    if self.shift_tap_count == 2 {
                        action = Some(ShiftAction::DoubleShift);
                        self.shift_tap_count = 0;
                    } else if self.shift_tap_count == 1 {
                        // KeyProcessor decides whether it was a "clean" tap.
                        action = Some(ShiftAction::SingleShift);
                    }
                }
            }
        } else if CTRL_KEYS.contains(&keycode) {
            self.ctrl = pressed;
        } else if ALT_KEYS.contains(&keycode) {
            self.alt = pressed;
        } else if META_KEYS.contains(&keycode) {
            self.meta = pressed;
        }

        // Any other key press resets the shift tap count.
        if pressed && !is_shift_key(keycode) {
            self.shift_tap_count = 0;
        }

        action
    }

    /// Forget pending shift taps (used after an L+R Shift hotkey).
    pub fn reset_shift_taps(&mut self) {
        self.shift_tap_count = 0;
        self.last_shift_release_ts = 0.0;
        self.last_shift_press_ts = 0.0;
    }

    /// True if only Shift is allowed (for typing).
    pub fn is_plain(&self) -> bool {
        !(self.ctrl || self.alt || self.meta)
    }

    /// True if any control modifier is active.
    pub fn has_modifiers(&self) -> bool {
        self.ctrl || self.alt || self.meta
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quick_tap_is_single_shift() {
        let mut m = ModifierState::default();
        assert_eq!(m.update(KEY_LEFTSHIFT, true, 0.0), None);
        assert!(m.shift);
        assert_eq!(
            m.update(KEY_LEFTSHIFT, false, 0.05),
            Some(ShiftAction::SingleShift)
        );
        assert!(!m.shift);
    }

    #[test]
    fn double_tap_is_double_shift() {
        let mut m = ModifierState::default();
        m.update(KEY_RIGHTSHIFT, true, 0.0);
        m.update(KEY_RIGHTSHIFT, false, 0.05); // tap 1 → SingleShift, count stays 1
                                               // Next press within the 300 ms interval → count becomes 2.
        m.update(KEY_RIGHTSHIFT, true, 0.1);
        assert_eq!(m.shift_tap_count, 2);
        assert_eq!(
            m.update(KEY_RIGHTSHIFT, false, 0.15),
            Some(ShiftAction::DoubleShift)
        );
        // Count is reset after a double shift.
        assert_eq!(m.shift_tap_count, 0);
    }

    #[test]
    fn long_hold_produces_no_action() {
        let mut m = ModifierState::default();
        m.update(KEY_LEFTSHIFT, true, 0.0);
        assert_eq!(m.update(KEY_LEFTSHIFT, false, 1.0), None);
    }

    #[test]
    fn slow_second_press_is_not_double() {
        let mut m = ModifierState::default();
        m.update(KEY_LEFTSHIFT, true, 0.0);
        m.update(KEY_LEFTSHIFT, false, 0.05);
        // Second press after the 300 ms interval → count restarts at 1.
        m.update(KEY_LEFTSHIFT, true, 1.0);
        assert_eq!(m.shift_tap_count, 1);
        assert_eq!(
            m.update(KEY_LEFTSHIFT, false, 1.05),
            Some(ShiftAction::SingleShift)
        );
    }

    #[test]
    fn other_key_press_cancels_tap() {
        let mut m = ModifierState::default();
        m.update(KEY_LEFTSHIFT, true, 0.0);
        assert_eq!(m.shift_tap_count, 1);
        // Pressing a letter resets the tap count.
        m.update(30, true, 0.05);
        assert_eq!(m.shift_tap_count, 0);
        // Releasing shift then is just a hold, not a tap gesture.
        assert_eq!(m.update(KEY_LEFTSHIFT, false, 0.1), None);
    }

    #[test]
    fn ctrl_alt_meta_tracking() {
        let mut m = ModifierState::default();
        assert!(m.is_plain());
        assert!(!m.has_modifiers());
        m.update(KEY_LEFTCTRL, true, 0.0);
        assert!(!m.is_plain());
        assert!(m.has_modifiers());
        m.update(KEY_LEFTCTRL, false, 0.1);
        m.update(KEY_LEFTALT, true, 0.2);
        assert!(m.alt);
        assert!(m.has_modifiers());
        m.update(KEY_LEFTALT, false, 0.3);
        m.update(KEY_LEFTMETA, true, 0.4);
        assert!(m.meta);
        assert!(m.has_modifiers());
        m.update(KEY_LEFTMETA, false, 0.5);
        assert!(m.is_plain());
    }

    #[test]
    fn reset_shift_taps_clears_state() {
        let mut m = ModifierState::default();
        m.update(KEY_LEFTSHIFT, true, 1.0);
        m.update(KEY_LEFTSHIFT, false, 1.05);
        m.reset_shift_taps();
        assert_eq!(m.shift_tap_count, 0);
        assert_eq!(m.last_shift_press_ts, 0.0);
        assert_eq!(m.last_shift_release_ts, 0.0);
    }

    #[test]
    fn release_without_press_does_nothing() {
        let mut m = ModifierState::default();
        // last_shift_press_ts = 0 → hold_duration is huge → no action.
        assert_eq!(m.update(KEY_LEFTSHIFT, false, 100.0), None);
    }
}
