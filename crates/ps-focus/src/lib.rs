//! PowerSwitcher focus layer (Rust port, phase 2).
//!
//! Active-window detection across Wayland compositors and X11 — a faithful
//! port of `power_switcher/focus.py`.  Wayland deliberately hides the focused
//! window from unprivileged clients, so each compositor is queried through its
//! own interface:
//!
//! | Compositor  | Mechanism |
//! |---|---|
//! | Sway        | `swaymsg -t get_tree -r` (`focused` node → `app_id`) |
//! | Hyprland    | `hyprctl activewindow -j` (`class`) |
//! | KDE Plasma  | `kdotool`, then `qdbus org.kde.KWin` |
//! | X11/XWayland| `xdotool getactivewindow getwindowclassname` |
//! | GNOME       | companion extension `org.power_switcher.Focus` (see `gnome-extension/`) |
//!
//! Results are cached for a short TTL (`DEFAULT_TTL`) so the per-keystroke
//! decision path never blocks on a subprocess spawn per word.

pub mod parse;
pub mod provider;

pub use provider::{
    extension_available, ActiveWindowProvider, Strategy, DEFAULT_TTL, GNOME_EXT_DEST,
    GNOME_EXT_IFACE, GNOME_EXT_PATH,
};
