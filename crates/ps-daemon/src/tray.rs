//! Top-bar tray (StatusNotifierItem) for the daemon.
//!
//! GNOME has no tray of its own; the AppIndicator extension hosts SNI items.
//! The tray lives in the daemon process, so it appears and disappears with the
//! service. Uses ksni blocking API with tokio runtime.
//!
//! Use `disable_dbus_name(true)` to use the unique bus name instead of a
//! well-known name, which works better in some environments (flatpak, etc.).

use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ksni::blocking::{Handle, TrayMethods};
use log::{info, warn};

#[derive(Debug)]
pub(crate) struct PowerSwitcherTray {
    stop: Arc<AtomicBool>,
}

impl ksni::Tray for PowerSwitcherTray {
    const MENU_ON_ACTIVATE: bool = true;

    fn id(&self) -> String {
        "power_switcher".into()
    }

    fn title(&self) -> String {
        "PowerSwitcher".into()
    }

    fn icon_name(&self) -> String {
        "power-switcher-symbolic".into()
    }

    fn category(&self) -> ksni::Category {
        ksni::Category::SystemServices
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        vec![
            StandardItem {
                label: "Settings".into(),
                activate: Box::new(|_| open_settings()),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Restart daemon".into(),
                activate: Box::new(|_| restart_daemon()),
                ..Default::default()
            }
            .into(),
            ksni::MenuItem::Separator,
            StandardItem {
                label: "Exit".into(),
                icon_name: "application-exit".into(),
                activate: Box::new(|tray: &mut Self| exit_app(&tray.stop)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Start the tray in the background. Returns a Handle that must be kept alive
/// for the tray to stay registered.
pub(crate) fn spawn(stop: Arc<AtomicBool>) -> Option<Handle<PowerSwitcherTray>> {
    let tray = PowerSwitcherTray { stop };
    match tray
        .disable_dbus_name(true)
        .assume_sni_available(true)
        .spawn()
    {
        Ok(handle) => {
            info!("tray: StatusNotifierItem registered");
            Some(handle)
        }
        Err(err) => {
            warn!("tray unavailable: {err}");
            None
        }
    }
}

fn open_settings() {
    // systemd-run gives the GUI its own unit: children of the daemon share its
    // service cgroup and would be killed by a daemon restart.
    let Ok(exe) = std::env::current_exe() else {
        warn!("tray: cannot resolve the ps binary for Settings");
        return;
    };
    if let Err(err) = Command::new("systemd-run")
        .args([
            "--user",
            "--collect",
            "--quiet",
            "--unit=power_switcher_gui",
            exe.to_string_lossy().as_ref(),
            "gui",
        ])
        .spawn()
    {
        warn!("tray: failed to open settings: {err}");
    }
}

fn restart_daemon() {
    if let Err(err) = Command::new("systemctl")
        .args(["--user", "restart", "--no-block", "power_switcher"])
        .spawn()
    {
        warn!("tray: failed to restart the daemon: {err}");
    }
}

fn exit_app(stop: &Arc<AtomicBool>) {
    // Exiting through `stop` keeps the regular cleanup (stats save).
    let _ = Command::new("systemctl")
        .args(["--user", "stop", "power_switcher_gui.service"])
        .output();
    stop.store(true, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    #[test]
    fn menu_has_settings_and_restart() {
        let labels: Vec<String> = ksni::Tray::menu(&super::PowerSwitcherTray {
            stop: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
        .into_iter()
        .filter_map(|item| match item {
            ksni::MenuItem::Standard(item) => Some(item.label),
            _ => None,
        })
        .collect();
        assert_eq!(labels, ["Settings", "Restart daemon", "Exit"]);
    }
}
