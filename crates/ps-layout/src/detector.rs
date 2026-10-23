//! Backend auto-detection — port of `power_switcher/layout/detector.py`.
//!
//! Order: Sway → Hyprland → GNOME (desktop match) → KDE (desktop match),
//! then the same list again as fallback, else an error.

use log::{info, warn};

use crate::backends::{GnomeBackend, HyprlandBackend, KdeBackend, LayoutBackend, SwayBackend};

/// Detect the DE and return the best available layout backend.
pub fn detect_layout_backend() -> Result<Box<dyn LayoutBackend + Send>, String> {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_uppercase();
    let session = std::env::var("XDG_SESSION_TYPE")
        .unwrap_or_default()
        .to_lowercase();
    info!("XDG_CURRENT_DESKTOP={desktop} XDG_SESSION_TYPE={session}");

    if !session.is_empty() && session != "wayland" {
        warn!("XDG_SESSION_TYPE={session:?} is not Wayland. PowerSwitcher might fail.");
    }

    // Sway
    if SwayBackend::is_available() {
        return Ok(Box::new(SwayBackend));
    }

    // Hyprland
    if HyprlandBackend::is_available() {
        return Ok(Box::new(HyprlandBackend));
    }

    // GNOME
    if ["GNOME", "UBUNTU", "UNITY", "POP"]
        .iter()
        .any(|x| desktop.contains(x))
        && GnomeBackend::is_available()
    {
        return Ok(Box::new(GnomeBackend::new()));
    }

    // KDE Plasma
    if ["KDE", "PLASMA"].iter().any(|x| desktop.contains(x)) && KdeBackend::is_available() {
        return Ok(Box::new(KdeBackend));
    }

    // Fallbacks
    if SwayBackend::is_available() {
        return Ok(Box::new(SwayBackend));
    }
    if HyprlandBackend::is_available() {
        return Ok(Box::new(HyprlandBackend));
    }
    if GnomeBackend::is_available() {
        return Ok(Box::new(GnomeBackend::new()));
    }
    if KdeBackend::is_available() {
        return Ok(Box::new(KdeBackend));
    }

    Err(format!(
        "No supported layout backend found.\n\
         Supported: GNOME, KDE Plasma, Sway, Hyprland.\n\
         XDG_CURRENT_DESKTOP={}",
        std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "(unset)".to_string())
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// The detector reads process-global environment variables; concurrent
    /// tests would otherwise observe each other's `set_var` calls.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn sway_env_wins() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // In an environment with SWAYSOCK set, detection must return Sway
        // regardless of XDG_CURRENT_DESKTOP (matches the Python order).
        std::env::set_var("SWAYSOCK", "/run/user/1000/sway-ipc.sock");
        std::env::remove_var("HYPRLAND_INSTANCE_SIGNATURE");
        let backend = detect_layout_backend().expect("sway detected");
        assert_eq!(backend.name(), "sway");
        std::env::remove_var("SWAYSOCK");
    }

    #[test]
    fn hyprland_env_wins_when_no_sway() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("SWAYSOCK");
        std::env::set_var("HYPRLAND_INSTANCE_SIGNATURE", "sig_123");
        let backend = detect_layout_backend().expect("hyprland detected");
        assert_eq!(backend.name(), "hyprland");
        std::env::remove_var("HYPRLAND_INSTANCE_SIGNATURE");
    }

    #[test]
    fn no_backend_is_an_error() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // Clean environment, no GNOME/KDE tools: detection must fail with the
        // Python error message shape.
        std::env::remove_var("SWAYSOCK");
        std::env::remove_var("HYPRLAND_INSTANCE_SIGNATURE");
        std::env::set_var("XDG_CURRENT_DESKTOP", "rocket");
        if GnomeBackend::is_available() || KdeBackend::is_available() {
            eprintln!("skipping: GNOME/KDE tooling present on this machine");
            return;
        }
        let err = match detect_layout_backend() {
            Err(err) => err,
            Ok(_) => panic!("expected no backend"),
        };
        assert!(err.contains("No supported layout backend found"));
        assert!(err.contains("XDG_CURRENT_DESKTOP=rocket"));
    }
}
