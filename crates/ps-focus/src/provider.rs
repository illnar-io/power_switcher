//! Active-window provider — port of `focus.ActiveWindowProvider`.
//!
//! The compositor is detected once at construction; each poll tries a small
//! ordered list of strategies.  Empty string means "unknown" — callers should
//! treat that as the default (never-disabled) mode.
//!
//! Like the other ps-* ports, the interesting methods take an explicit `now`
//! timestamp; `get_class()` uses the system clock for the daemon call sites.

use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use log::debug;

use crate::parse;

/// Cache lifetime (Python: `ActiveWindowProvider(ttl=1.0)`).
pub const DEFAULT_TTL: f64 = 1.0;

/// Subprocess timeout for every strategy call (Python `_run(timeout=1.0)`).
const RUN_TIMEOUT: Duration = Duration::from_secs(1);

/// Poll interval while waiting for a strategy subprocess.  A 4 ms `gdbus` call
/// must not become a 10 ms stall on the word-boundary decision path.
const POLL_INTERVAL: Duration = Duration::from_millis(1);

/// D-Bus coordinates of the companion GNOME Shell extension
/// (`gnome-extension/`).
pub const GNOME_EXT_DEST: &str = "org.power_switcher.Focus";
pub const GNOME_EXT_PATH: &str = "/org/power_switcher/Focus";
pub const GNOME_EXT_IFACE: &str = "org.power_switcher.Focus";

/// Ordered detection strategies.  The list is built once from the environment
/// (like Python's `_build_strategies`); each entry's tool must exist on `$PATH`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Strategy {
    Sway,
    Hyprland,
    GnomeExtension,
    Kdotool,
    KwinDbus,
    Xdotool,
}

impl Strategy {
    /// The external tool this strategy needs (Python's `add` name→tool map).
    fn tool(self) -> &'static str {
        match self {
            Strategy::Sway => "swaymsg",
            Strategy::Hyprland => "hyprctl",
            Strategy::GnomeExtension => "gdbus",
            Strategy::Kdotool => "kdotool",
            Strategy::KwinDbus => "qdbus",
            Strategy::Xdotool => "xdotool",
        }
    }

    fn name(self) -> &'static str {
        // Python deduplicates by `func.__name__`.
        match self {
            Strategy::Sway => "_from_sway",
            Strategy::Hyprland => "_from_hyprland",
            Strategy::GnomeExtension => "_from_gnome_extension",
            Strategy::Kdotool => "_from_kdotool",
            Strategy::KwinDbus => "_from_kwin_dbus",
            Strategy::Xdotool => "_from_xdotool",
        }
    }
}

/// Best-effort provider for the focused window's WM class.
pub struct ActiveWindowProvider {
    ttl: f64,
    cached: String,
    cached_at: f64,
    strategies: Vec<Strategy>,
}

impl ActiveWindowProvider {
    pub fn new() -> Self {
        Self::with_ttl(DEFAULT_TTL)
    }

    pub fn with_ttl(ttl: f64) -> Self {
        Self {
            ttl,
            cached: String::new(),
            cached_at: 0.0,
            strategies: Self::build_strategies(),
        }
    }

    /// Strategy selection (Python `_build_strategies`): env-var fast paths,
    /// then DE-specific entries, then the always-available fallbacks —
    /// deduplicated by name, keeping first occurrence, each requiring its tool.
    pub fn build_strategies() -> Vec<Strategy> {
        let desktop = std::env::var("XDG_CURRENT_DESKTOP")
            .unwrap_or_default()
            .to_uppercase();

        // A strategy is only registered when its tool exists (Python resolves
        // `_TOOLS` once so missing tools never cost a failed spawn per poll).
        let available = |s: Strategy| which(s.tool()).is_some();

        let mut order: Vec<Strategy> = Vec::new();
        let mut add = |s: Strategy| {
            if available(s) {
                order.push(s);
            }
        };

        if std::env::var_os("SWAYSOCK").is_some() {
            add(Strategy::Sway);
        }
        if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
            add(Strategy::Hyprland);
        }
        if ["GNOME", "UBUNTU", "POP", "UNITY"]
            .iter()
            .any(|x| desktop.contains(x))
        {
            add(Strategy::GnomeExtension);
        }
        if ["KDE", "PLASMA"].iter().any(|x| desktop.contains(x)) {
            add(Strategy::Kdotool);
            add(Strategy::KwinDbus);
        }
        // Always-available fallbacks, in rough order of reliability.
        add(Strategy::Sway);
        add(Strategy::Hyprland);
        add(Strategy::GnomeExtension);
        add(Strategy::Kdotool);
        add(Strategy::Xdotool);

        // Deduplicate while preserving order (Python uses the function name).
        let mut seen = std::collections::HashSet::new();
        order.retain(|s| seen.insert(s.name()));
        order
    }

    /// Cached lookup at the current system time.
    pub fn get_class(&mut self) -> String {
        self.get_class_at(system_now(), false)
    }

    /// Cached lookup with an explicit clock.
    pub fn get_class_at(&mut self, now: f64, force: bool) -> String {
        if !force && now - self.cached_at < self.ttl {
            return self.cached.clone();
        }
        // Clone the small strategy list so strategy calls can take &mut self.
        let strategies = self.strategies.clone();
        for &strategy in &strategies {
            let raw = self.run_strategy(strategy);
            let value = parse::clean(&raw);
            debug!(
                "focus strategy {} succeeded={}",
                strategy.name(),
                !value.is_empty()
            );
            if !value.is_empty() {
                self.cached = value;
                self.cached_at = now;
                return self.cached.clone();
            }
        }
        self.cached = String::new();
        self.cached_at = now;
        String::new()
    }

    /// Drop the cached class so the next poll re-queries.
    pub fn invalidate(&mut self) {
        self.cached_at = 0.0;
    }

    /// The strategy order actually registered (for tests/logs).
    pub fn strategies(&self) -> &[Strategy] {
        &self.strategies
    }

    fn run_strategy(&mut self, strategy: Strategy) -> String {
        match strategy {
            Strategy::Sway => {
                let out = run(&["swaymsg", "-t", "get_tree", "-r"]);
                if out.is_empty() {
                    return String::new();
                }
                match serde_json::from_str::<serde_json::Value>(&out) {
                    Ok(tree) => parse::parse_sway_tree(&tree),
                    Err(_) => String::new(),
                }
            }
            Strategy::Hyprland => {
                let out = run(&["hyprctl", "activewindow", "-j"]);
                if out.is_empty() {
                    String::new()
                } else {
                    parse::parse_hyprland(&out)
                }
            }
            Strategy::Kdotool => run(&["kdotool", "getactivewindow", "getwindowclassname"])
                .trim()
                .to_string(),
            Strategy::KwinDbus => {
                // KWin exposes the active window UUID; getWindowInfo needs it.
                let uuid = run(&[
                    "qdbus",
                    "org.kde.KWin",
                    "/KWin",
                    "org.kde.KWin.activeWindow",
                ])
                .trim()
                .to_string();
                if uuid.is_empty() {
                    return String::new();
                }
                let info = run(&[
                    "qdbus",
                    "org.kde.KWin",
                    "/KWin",
                    "org.kde.KWin.getWindowInfo",
                    &uuid,
                ]);
                parse::parse_key_value(&info, "resourceClass")
            }
            Strategy::Xdotool => run(&["xdotool", "getactivewindow", "getwindowclassname"])
                .trim()
                .to_string(),
            Strategy::GnomeExtension => {
                let out = run(&[
                    "gdbus",
                    "call",
                    "--session",
                    "--dest",
                    GNOME_EXT_DEST,
                    "--object-path",
                    GNOME_EXT_PATH,
                    "--method",
                    &format!("{GNOME_EXT_IFACE}.GetActiveWindow"),
                ]);
                if out.is_empty() {
                    String::new()
                } else {
                    parse::parse_gdbus_string(&out)
                }
            }
        }
    }
}

impl Default for ActiveWindowProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// Run a command and return its stdout, or `""` on any failure — including a
/// non-zero exit (Python `_run` only accepts `returncode == 0`) or the
/// 1-second timeout expiring (Python's `TimeoutExpired` is swallowed).
fn run(args: &[&str]) -> String {
    run_timeout(args, RUN_TIMEOUT)
}

fn run_timeout(args: &[&str], timeout: Duration) -> String {
    use std::io::Read;
    use std::process::{Child, Command, Stdio};

    let program = args[0];
    let rest = &args[1..];
    let mut child: Child = match Command::new(program)
        .args(rest)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return String::new(),
    };

    let mut stdout_pipe = child.stdout.take().expect("piped stdout");
    let reader = std::thread::spawn(move || {
        let mut buf = String::new();
        let _ = stdout_pipe.read_to_string(&mut buf);
        buf
    });

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => {
                return reader.join().unwrap_or_default();
            }
            Ok(Some(_)) => {
                // Non-zero exit: Python returns "" (but drains the thread).
                let _ = reader.join();
                return String::new();
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = reader.join();
                    return String::new();
                }
                std::thread::sleep(POLL_INTERVAL);
            }
            Err(_) => return String::new(),
        }
    }
}

/// Minimal `shutil.which`: find `tool` on `$PATH` and executable.
pub fn which(tool: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|dir| dir.join(tool))
        .find(|p| p.is_file())
}

/// True when the companion GNOME extension answers on the session bus.
pub fn extension_available() -> bool {
    let mut provider = ActiveWindowProvider::with_ttl(0.0);
    !parse::clean(&provider.run_strategy(Strategy::GnomeExtension)).is_empty()
}

fn system_now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strategy_order_respects_env() {
        // Single test to avoid parallel env-var races (env is process-global).
        std::env::remove_var("SWAYSOCK");
        std::env::remove_var("HYPRLAND_INSTANCE_SIGNATURE");
        std::env::set_var("XDG_CURRENT_DESKTOP", "KDE");
        let s = ActiveWindowProvider::build_strategies();
        // Every registered strategy's tool exists on this machine.
        for strategy in &s {
            assert!(
                which(strategy.tool()).is_some(),
                "{strategy:?} without tool"
            );
        }
        // No duplicates (Python dedups by function name).
        let mut seen = std::collections::HashSet::new();
        assert!(s.iter().all(|x| seen.insert(x.name())));

        // Sway env fast path — only when swaymsg is actually installed
        // (strategies without their tool are never registered).
        std::env::set_var("SWAYSOCK", "/tmp/sway.sock");
        std::env::set_var("XDG_CURRENT_DESKTOP", "");
        let s = ActiveWindowProvider::build_strategies();
        if which("swaymsg").is_some() {
            assert_eq!(s.first().map(|x| x.name()), Some("_from_sway"));
        } else {
            assert!(
                !s.contains(&Strategy::Sway),
                "swaymsg missing but Sway registered"
            );
        }

        // KDE desktop registers kwin_dbus when qdbus exists (kdotool may not).
        std::env::set_var("XDG_CURRENT_DESKTOP", "KDE");
        let s = ActiveWindowProvider::build_strategies();
        assert!(s.contains(&Strategy::KwinDbus) == which("qdbus").is_some());

        std::env::remove_var("SWAYSOCK");
    }

    #[test]
    fn ttl_cache_hits_and_forces() {
        let mut p = ActiveWindowProvider::with_ttl(1.0);
        // Deterministic: no real strategy may answer on a live desktop.
        p.strategies.clear();
        // Simulate a populated cache by driving get_class_at through a fake
        // strategy result: instead, test the cache arithmetic directly.
        p.cached = "firefox".to_string();
        p.cached_at = 10.0;
        // Within the TTL: cached value, no subprocess attempt.
        assert_eq!(p.get_class_at(10.5, false), "firefox");
        assert_eq!(p.get_class_at(10.99, false), "firefox");
        // Expired: strategies run (all fail here) → "" and cache cleared.
        assert_eq!(p.get_class_at(11.0, false), "");
        assert_eq!(p.cached, "");
        // force bypasses the TTL even when fresh.
        p.cached = "kitty".to_string();
        p.cached_at = 10.0;
        assert_eq!(p.get_class_at(10.5, true), "");
    }

    #[test]
    fn invalidate_forces_refresh() {
        let mut p = ActiveWindowProvider::with_ttl(5.0);
        p.cached = "x".to_string();
        p.cached_at = 100.0;
        p.invalidate();
        assert_eq!(p.cached_at, 0.0);
    }

    #[test]
    fn run_requires_success_and_honors_timeout() {
        // Non-zero exit → "" (Python `_run` returncode check).
        assert_eq!(run(&["false"]), "");
        // Missing binary → "".
        assert_eq!(run(&["definitely-not-a-binary-xyz"]), "");
        // Timeout: hanging process killed quickly, "" returned.
        let start = std::time::Instant::now();
        assert_eq!(run_timeout(&["sleep", "5"], Duration::from_millis(150)), "");
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}
