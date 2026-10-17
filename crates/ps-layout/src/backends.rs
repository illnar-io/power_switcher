//! Layout backends — a faithful port of `power_switcher/layout/backends.py`.
//!
//! Each backend answers three questions (the Python `LayoutBackend` ABC):
//! the *current* layout index, the *number* of configured layouts, and how to
//! *switch* to an index.  External tools are called with the same arguments and
//! timeouts as the Python daemon; output parsing is delegated to [`crate::parse`].

use std::path::PathBuf;
use std::time::Duration;

use log::{info, warn};

use crate::parse;
use crate::proc::{run_capture, run_status, run_stdout};

/// Tool-call timeouts, matching the Python `subprocess.run(timeout=…)` values.
const DCONF_TIMEOUT: Duration = Duration::from_secs(1);
const GDBUS_TIMEOUT: Duration = Duration::from_secs(2);
const GDBUS_PROBE_TIMEOUT: Duration = Duration::from_secs(1);
const SWAYMSG_TIMEOUT: Duration = Duration::from_secs(1);
const HYPRCTL_TIMEOUT: Duration = Duration::from_secs(1);

/// Delay between simulated Super+Space presses (GNOME backend).
pub const SUPER_SPACE_STEP_DELAY: Duration = Duration::from_millis(50);

pub trait LayoutBackend {
    /// Current layout index (0-based, order = the compositor's list).
    fn get_current_index(&mut self) -> usize;
    /// Number of configured layouts.
    fn get_layout_count(&mut self) -> usize;
    /// Switch to `index`.
    fn set_layout(&mut self, index: usize);
    /// Backend name (for logs).
    fn name(&self) -> &'static str;
}

// ── GNOME ────────────────────────────────────────────────────────────────────

/// GNOME Wayland ignores programmatic layout switches, so the backend reads
/// the desired list from dconf and *simulates* Super+Space through our virtual
/// keyboard to cycle to the target index (port of `GNOMESwitchBackend`).
pub struct GnomeBackend {
    virtual_keyboard: Option<ps_input::VirtualKeyboard>,
    #[allow(dead_code)] // parity with Python: discovered and logged, not used yet
    ibus_addr: Option<String>,
    sources: Vec<String>,
}

impl GnomeBackend {
    pub fn new() -> Self {
        let ibus_addr = Self::discover_ibus_addr();
        let sources = Self::read_sources();
        info!(
            "GNOMESwitchBackend: IBus={:?}, Sources={:?}",
            ibus_addr, sources
        );
        Self {
            virtual_keyboard: None,
            ibus_addr,
            sources,
        }
    }

    /// Python's `set_uinput` — GNOME switching injects Super+Space.
    pub fn set_virtual_keyboard(&mut self, vk: ps_input::VirtualKeyboard) {
        self.virtual_keyboard = Some(vk);
    }

    /// `is_available()` classmethod.
    pub fn is_available() -> bool {
        desktop_env().contains("GNOME")
    }

    fn read_sources() -> Vec<String> {
        if let Some(out) = run_stdout(
            "dconf",
            &["read", "/org/gnome/desktop/input-sources/sources"],
            DCONF_TIMEOUT,
        ) {
            let found = parse::parse_dconf_sources(&out);
            if !found.is_empty() {
                return found;
            }
        }
        vec!["us".to_string(), "ru".to_string()]
    }

    fn read_current(&self) -> usize {
        if let Some(out) = run_stdout(
            "dconf",
            &["read", "/org/gnome/desktop/input-sources/current"],
            DCONF_TIMEOUT,
        ) {
            return parse::parse_dconf_current(&out);
        }
        0
    }

    /// Search the IBus bus files (newest first) for an address containing a
    /// `path=` parameter.
    fn discover_ibus_addr() -> Option<String> {
        let mut files: Vec<PathBuf> = Vec::new();
        for dir in ["~/.cache/ibus/bus/", "~/.config/ibus/bus/"] {
            let dir = shellexpand_home(dir);
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    files.push(entry.path());
                }
            }
        }
        if files.is_empty() {
            return None;
        }
        files.sort_by_cached_key(|p| {
            std::cmp::Reverse(
                p.metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_nanos())
                    .unwrap_or(0),
            )
        });
        for f in files {
            if let Ok(content) = std::fs::read_to_string(&f) {
                for line in content.lines() {
                    if let Some(addr) = line.split_once("IBUS_ADDRESS=") {
                        let addr = addr.1.trim();
                        if addr.contains("path=") {
                            return Some(addr.to_string());
                        }
                    }
                }
            }
        }
        None
    }

    /// One simulated Super+Space through the virtual keyboard
    /// (Python `_simulate_switch`, same inter-event delays).
    fn simulate_switch(&mut self) {
        use ps_input::keycodes as kc;
        let Some(vk) = self.virtual_keyboard.as_mut() else {
            return;
        };
        let delay = |ms| std::thread::sleep(Duration::from_millis(ms));
        let _ = vk.key(kc::KEY_LEFTMETA, 1);
        delay(10);
        let _ = vk.key(kc::KEY_SPACE, 1);
        delay(20);
        let _ = vk.key(kc::KEY_SPACE, 0);
        delay(10);
        let _ = vk.key(kc::KEY_LEFTMETA, 0);
    }
}

impl Default for GnomeBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutBackend for GnomeBackend {
    fn get_current_index(&mut self) -> usize {
        self.read_current()
    }

    fn get_layout_count(&mut self) -> usize {
        self.sources.len()
    }

    fn set_layout(&mut self, index: usize) {
        if index >= self.sources.len() {
            return;
        }
        let current = self.get_current_index();
        if current == index {
            return;
        }
        // Steps forward in the cycle to reach `index`.
        let steps =
            (index + self.sources.len() - current % self.sources.len()) % self.sources.len();
        info!("GNOMESwitchBackend: Simulating {steps}x Super+Space to reach layout index {index}");
        for _ in 0..steps {
            self.simulate_switch();
            std::thread::sleep(SUPER_SPACE_STEP_DELAY);
        }
    }

    fn name(&self) -> &'static str {
        "gnome"
    }
}

// ── KDE ──────────────────────────────────────────────────────────────────────

/// KDE Plasma layout control over the `org.kde.KeyboardLayouts` D-Bus API,
/// called through `gdbus` (port of `KDELayoutBackend`).
pub struct KdeBackend;

impl KdeBackend {
    const DEST: &'static str = "org.kde.keyboard";
    const PATH: &'static str = "/Layouts";
    const IFACE: &'static str = "org.kde.KeyboardLayouts";

    pub fn is_available() -> bool {
        run_status(
            "gdbus",
            &[
                "introspect",
                "--session",
                "--dest",
                Self::DEST,
                "--object-path",
                Self::PATH,
            ],
            GDBUS_PROBE_TIMEOUT,
        )
        .unwrap_or(false)
    }

    fn gdbus(&self, method: &str, args: &[&str]) -> String {
        let mut call_args = vec![
            "call",
            "--session",
            "--dest",
            Self::DEST,
            "--object-path",
            Self::PATH,
            "--method",
        ];
        let method = format!("{}.{}", Self::IFACE, method);
        call_args.push(&method);
        call_args.extend_from_slice(args);
        run_stdout("gdbus", &call_args, GDBUS_TIMEOUT).unwrap_or_default()
        // Python: `.stdout.strip()` — we keep run_stdout's full output; the
        // parsers tolerate surrounding whitespace.
    }
}

impl LayoutBackend for KdeBackend {
    fn get_current_index(&mut self) -> usize {
        parse::parse_gdbus_layout(&self.gdbus("getLayout", &[]))
    }

    fn get_layout_count(&mut self) -> usize {
        parse::count_gdbus_layouts(&self.gdbus("getLayoutsList", &[]))
    }

    fn set_layout(&mut self, index: usize) {
        let idx = index.to_string();
        let _ = self.gdbus("setLayout", &[&idx]);
    }

    fn name(&self) -> &'static str {
        "kde"
    }
}

// ── Sway ─────────────────────────────────────────────────────────────────────

/// Sway/i3 layout control over `swaymsg` (port of `SwayLayoutBackend`).
pub struct SwayBackend;

impl SwayBackend {
    pub fn is_available() -> bool {
        std::env::var_os("SWAYSOCK").is_some()
    }

    fn query_inputs(&self) -> Option<parse::SwayInput> {
        let out = run_stdout("swaymsg", &["-t", "get_inputs"], SWAYMSG_TIMEOUT)?;
        parse::parse_sway_inputs(&out)
    }
}

impl LayoutBackend for SwayBackend {
    fn get_current_index(&mut self) -> usize {
        self.query_inputs()
            .and_then(|i| i.current_layout_index)
            .unwrap_or(0)
    }

    fn get_layout_count(&mut self) -> usize {
        self.query_inputs()
            .and_then(|i| i.layout_names)
            .map(|n| n.len())
            .unwrap_or(2)
    }

    fn set_layout(&mut self, index: usize) {
        let idx = index.to_string();
        if let Err(err) = std::process::Command::new("swaymsg")
            .args(["input", "type:keyboard", "xkb_switch_layout", &idx])
            .status()
        {
            warn!("swaymsg xkb_switch_layout failed: {err}");
        }
    }

    fn name(&self) -> &'static str {
        "sway"
    }
}

// ── Hyprland ─────────────────────────────────────────────────────────────────

/// Hyprland layout control over `hyprctl` (port of `HyprlandLayoutBackend`).
pub struct HyprlandBackend;

impl HyprlandBackend {
    pub fn is_available() -> bool {
        std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some()
    }
}

impl LayoutBackend for HyprlandBackend {
    fn get_current_index(&mut self) -> usize {
        match run_capture("hyprctl", &["devices", "-j"], HYPRCTL_TIMEOUT) {
            Some(captured) => parse::parse_hyprland_devices(&captured.stdout).unwrap_or(0),
            None => 0,
        }
    }

    fn get_layout_count(&mut self) -> usize {
        2
    }

    fn set_layout(&mut self, index: usize) {
        let idx = index.to_string();
        if let Err(err) = std::process::Command::new("hyprctl")
            .args(["switchxkblayout", "all", &idx])
            .status()
        {
            warn!("hyprctl switchxkblayout failed: {err}");
        }
    }

    fn name(&self) -> &'static str {
        "hyprland"
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

/// `XDG_CURRENT_DESKTOP` upper-cased (Python: `os.environ.get(...).upper()`).
fn desktop_env() -> String {
    std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_uppercase()
}

/// Expand a leading `~/` the way Python's `Path.expanduser()` does.
fn shellexpand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gnome_steps_calculation_matches_python() {
        // Python: steps = (index - current) % len(sources)
        let sources = 4;
        for current in 0..sources {
            for index in 0..sources {
                let expected = (index + sources - current % sources) % sources;
                // Forward-only cycling: 1→3 takes 2 presses, 3→1 takes 2.
                let py =
                    ((index as isize - current as isize).rem_euclid(sources as isize)) as usize;
                assert_eq!(expected, py, "current={current} index={index}");
            }
        }
    }

    #[test]
    fn shellexpand_expands_home() {
        let expanded = shellexpand_home("~/.cache/ibus/bus/");
        let home = std::env::var("HOME").unwrap();
        assert_eq!(expanded, PathBuf::from(home).join(".cache/ibus/bus"));
        assert_eq!(shellexpand_home("/abs/path"), PathBuf::from("/abs/path"));
    }

    #[test]
    fn gnome_backend_defaults_without_dconf() {
        // On a machine without dconf the backend must fall back to us/ru.
        let backend = GnomeBackend::new();
        if which_dconf().is_none() {
            let mut b = backend;
            assert_eq!(b.get_layout_count(), 2);
            assert_eq!(b.get_current_index(), 0);
        }
    }

    fn which_dconf() -> Option<std::path::PathBuf> {
        std::env::var_os("PATH").and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|p| p.join("dconf"))
                .find(|p| p.exists())
        })
    }
}
