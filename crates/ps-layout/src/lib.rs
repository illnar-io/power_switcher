//! PowerSwitcher layout layer (Rust port, phase 2).
//!
//! Compositor-specific keyboard-layout switching, a faithful port of
//! `power_switcher/layout/backends.py` + `layout/detector.py`:
//!
//! * [`backends::LayoutBackend`] trait ← Python `LayoutBackend` ABC
//! * [`backends::GnomeBackend`] ← `GNOMESwitchBackend` (dconf state + Super+Space
//!   injection through the virtual keyboard)
//! * [`backends::KdeBackend`] ← `KDELayoutBackend` (gdbus → org.kde.KeyboardLayouts)
//! * [`backends::SwayBackend`] ← `SwayLayoutBackend` (swaymsg)
//! * [`backends::HyprlandBackend`] ← `HyprlandLayoutBackend` (hyprctl)
//! * [`detector::detect_layout_backend`] ← `detector.detect_layout_backend`
//!
//! The external tools are invoked with the same arguments and timeouts as the
//! Python daemon; response parsing lives in [`parse`] as pure, unit-tested
//! functions.

pub mod backends;
pub mod detector;
pub mod parse;
pub mod proc;

pub use backends::{
    GnomeBackend, HyprlandBackend, KdeBackend, LayoutBackend, SwayBackend, SUPER_SPACE_STEP_DELAY,
};
pub use detector::detect_layout_backend;
