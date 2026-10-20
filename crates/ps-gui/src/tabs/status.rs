//! Status tab — daemon state, current layout, counters and paths.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gtk::prelude::*;

use ps_core::Stats;

use crate::state::GuiState;
use crate::ui;

/// The service unit shipped with the source tree.
const UNIT_TEMPLATE: &str = include_str!("../../../../systemd/power_switcher.service");
const UNIT_NAME: &str = "power_switcher.service";

pub struct StatusTab {
    pub root: gtk::ScrolledWindow,
    window: gtk::Window,
    stats_path: PathBuf,
    daemon: gtk::Label,
    layout: gtk::Label,
    counters: Vec<gtk::Label>,
    paths: gtk::Label,
    message: gtk::Label,
}

/// Counter captions, in display order (paired with the value labels).
const COUNTERS: [&str; 7] = [
    "Automatic conversions",
    "Manual conversions",
    "Total conversions",
    "Snippet expansions",
    "Undos",
    "Characters retyped",
    "Tracking for",
];

fn output(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if text.is_empty() {
        Some(String::from_utf8_lossy(&out.stderr).trim().to_string())
    } else {
        Some(text)
    }
}

/// systemd unit first, then a process scan (the daemon is often started from
/// a terminal during development).
fn daemon_state() -> String {
    if let Some(state) = output("systemctl", &["--user", "is-active", UNIT_NAME]) {
        if state == "active" {
            return "running (systemd unit power_switcher.service)".to_string();
        }
    }
    for pattern in ["[p]s daemon", "[p]ower_switcher daemon"] {
        if let Some(procs) = output("pgrep", &["-af", pattern]) {
            if let Some(first) = procs.lines().next() {
                return format!("running (not under systemd): {first}");
            }
        }
    }
    "not running".to_string()
}

/// Is the user unit installed (`systemctl --user cat` succeeds)?
fn unit_installed() -> bool {
    Command::new("systemctl")
        .args(["--user", "cat", UNIT_NAME])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

fn config_home() -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        let path = PathBuf::from(xdg);
        if path.is_absolute() {
            return Some(path);
        }
    }
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config"))
}

/// `ps` for `ExecStart`: the unit-usable path plus the file to verify.
/// Prefers the installed binary, then the one next to this GUI.
fn ps_binary() -> Option<(String, PathBuf)> {
    if let Some(home) = std::env::var_os("HOME") {
        let installed = PathBuf::from(home).join(".local/bin/ps");
        if installed.is_file() {
            return Some(("%h/.local/bin/ps".to_string(), installed));
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join("ps");
            if sibling.is_file() {
                return Some((sibling.display().to_string(), sibling));
            }
        }
    }
    None
}

/// Binary the installed unit will execute (parsed from `systemctl show`).
fn unit_exec_binary() -> Option<PathBuf> {
    let text = output(
        "systemctl",
        &["--user", "show", "-p", "ExecStart", "--value", UNIT_NAME],
    )?;
    let rest = text.split("path=").nth(1)?;
    let path = rest.split(';').next()?.trim();
    (!path.is_empty()).then(|| PathBuf::from(path))
}

/// Quote a path for systemd's `ExecStart=` when it contains whitespace.
fn systemd_path(path: &str) -> String {
    if path.contains(char::is_whitespace) {
        format!("\"{}\"", path.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        path.to_string()
    }
}

fn current_layout() -> String {
    match output("g3kb-switch", &["-p"]) {
        Some(id) if !id.is_empty() => format!("{id} (g3kb-switch)"),
        _ => "unknown (g3kb-switch not available)".to_string(),
    }
}

fn value_label() -> gtk::Label {
    let label = gtk::Label::new(None);
    label.set_halign(gtk::Align::Start);
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.set_selectable(true);
    label
}

impl StatusTab {
    pub fn new(state: &GuiState, window: &gtk::Window) -> Self {
        let (root, content) = ui::page();

        content.append(&ui::section("Daemon"));
        let daemon = value_label();
        content.append(&daemon);
        let layout = value_label();
        content.append(&layout);
        content.append(&ui::hint(
            "The layout is read from the compositor; manual Super+Space switches are picked \
             up by the daemon within 250 ms.",
        ));

        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        buttons.set_margin_top(6);
        let refresh = gtk::Button::with_label("Refresh");
        let restart = gtk::Button::with_label("Restart daemon");
        restart.add_css_class("suggested-action");
        let install = gtk::Button::with_label("Install service");
        let reset = gtk::Button::with_label("Reset statistics");
        reset.add_css_class("destructive-action");
        buttons.append(&refresh);
        buttons.append(&restart);
        buttons.append(&install);
        buttons.append(&reset);
        content.append(&buttons);
        content.append(&ui::hint(
            "Restart uses the systemd user unit power_switcher.service. If it is not installed \
             yet, “Install service” writes ~/.config/systemd/user/power_switcher.service, enables \
             it at login and starts it (stop a manually started daemon first).",
        ));

        content.append(&ui::section("Statistics"));
        let counters_grid = ui::form();
        let mut counters = Vec::with_capacity(COUNTERS.len());
        for (line, caption) in COUNTERS.iter().enumerate() {
            let value = gtk::Label::new(None);
            value.set_halign(gtk::Align::Start);
            value.set_selectable(true);
            ui::row(&counters_grid, line as i32, caption, &value);
            counters.push(value);
        }
        content.append(&counters_grid);

        content.append(&ui::section("Files"));
        let paths = value_label();
        paths.set_opacity(0.75);
        content.append(&paths);

        let message = gtk::Label::new(None);
        message.set_halign(gtk::Align::Start);
        message.set_wrap(true);
        content.append(&message);

        let tab = Self {
            root,
            window: window.clone(),
            stats_path: state.stats_path.clone(),
            daemon,
            layout,
            counters,
            paths,
            message,
        };

        {
            let tab = tab.clone_refs();
            refresh.connect_clicked(move |_| tab.refresh());
        }
        {
            let tab = tab.clone_refs();
            restart.connect_clicked(move |_| tab.restart_daemon());
        }
        {
            let tab = tab.clone_refs();
            install.connect_clicked(move |_| tab.confirm_install_service());
        }
        {
            let tab = tab.clone_refs();
            reset.connect_clicked(move |_| tab.confirm_reset());
        }

        tab.set_paths(state);
        tab.refresh();
        tab
    }

    fn clone_refs(&self) -> Self {
        Self {
            root: self.root.clone(),
            window: self.window.clone(),
            stats_path: self.stats_path.clone(),
            daemon: self.daemon.clone(),
            layout: self.layout.clone(),
            counters: self.counters.clone(),
            paths: self.paths.clone(),
            message: self.message.clone(),
        }
    }

    fn set_paths(&self, state: &GuiState) {
        self.paths.set_text(&format!(
            "Config:     {}\nSnippets:   {}\nExceptions: {}\nWords:      {}\nStats:      {}",
            state.config_path.display(),
            state.snippets_path.display(),
            state.exceptions_path.display(),
            state.words_path.display(),
            state.stats_path.display(),
        ));
    }

    fn refresh(&self) {
        self.daemon
            .set_text(&format!("State:   {}", daemon_state()));
        self.layout
            .set_text(&format!("Layout:  {}", current_layout()));

        let stats = Stats::load(&self.stats_path);
        let days = if stats.first_seen() > 0.0 {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0);
            (now - stats.first_seen()) / 86_400.0
        } else {
            0.0
        };
        let values = [
            stats.auto_conversions().to_string(),
            stats.manual_conversions().to_string(),
            stats.total_conversions().to_string(),
            stats.snippet_expansions().to_string(),
            stats.undos().to_string(),
            stats.chars_retyped().to_string(),
            format!("{days:.1} days"),
        ];
        for (label, value) in self.counters.iter().zip(values) {
            label.set_text(&value);
        }
    }

    fn restart_daemon(&self) {
        if !unit_installed() {
            self.message.set_text(&format!(
                "{UNIT_NAME} is not installed yet — click “Install service” first, or run:\n\
                 cp systemd/{UNIT_NAME} ~/.config/systemd/user/ && systemctl --user enable --now {UNIT_NAME}"
            ));
            return;
        }
        if let Some(bin) = unit_exec_binary() {
            if !bin.is_file() {
                self.message.set_text(&format!(
                    "The service points at {}, which does not exist (systemd exits 203/EXEC).\n\
                     Click “Install service” to rewrite the unit for the binary next to this GUI, \
                     or install `ps` at that path.",
                    bin.display()
                ));
                return;
            }
        }
        match Command::new("systemctl")
            .args(["--user", "restart", UNIT_NAME])
            .output()
        {
            Ok(out) if out.status.success() => {
                self.message
                    .set_text("Daemon restart requested — checking whether it stayed up…");
                let tab = self.clone_refs();
                gtk::glib::timeout_add_local_once(Duration::from_millis(1200), move || {
                    tab.verify_started();
                });
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
                self.message.set_text(&format!(
                    "Could not restart {UNIT_NAME}: {stderr}\n\
                     Start the daemon manually with `ps daemon` to see its output."
                ));
            }
            Err(err) => self
                .message
                .set_text(&format!("systemctl is unavailable: {err}")),
        }
    }

    /// After a restart, surface a failed start instead of silently claiming
    /// success (e.g. 203/EXEC for a missing binary).
    fn verify_started(&self) {
        self.refresh();
        let state = output("systemctl", &["--user", "is-active", UNIT_NAME]).unwrap_or_default();
        if state == "active" {
            self.message.set_text("Daemon restarted.");
            return;
        }
        let detail = output(
            "systemctl",
            &["--user", "status", "--no-pager", "--lines=0", UNIT_NAME],
        )
        .unwrap_or_default();
        let summary: Vec<&str> = detail
            .lines()
            .filter(|line| {
                line.contains("Active:") || line.contains("Process:") || line.contains("status=203")
            })
            .collect();
        let summary = if summary.is_empty() {
            detail
        } else {
            summary.join("\n")
        };
        self.message.set_text(&format!(
            "The service did not stay running{}:\n{summary}\n\
             If it exited with 203/EXEC, its ExecStart binary is missing — click “Install service”.",
            if state.is_empty() {
                String::new()
            } else {
                format!(" ({state})")
            }
        ));
    }

    fn confirm_install_service(&self) {
        let dialog = gtk::AlertDialog::builder()
            .modal(true)
            .message("Install the systemd user service?")
            .detail(
                "This writes ~/.config/systemd/user/power_switcher.service, enables it at login \
                 and starts the daemon now. Stop any daemon started manually first — two \
                 instances cannot grab the same keyboard.",
            )
            .buttons(["Cancel", "Install and start"])
            .cancel_button(0)
            .default_button(0)
            .build();
        let tab = self.clone_refs();
        dialog.choose(
            Some(&self.window),
            gtk::gio::Cancellable::NONE,
            move |result| {
                if result == Ok(1) {
                    tab.install_service();
                }
            },
        );
    }

    fn install_service(&self) {
        let Some(config_home) = config_home() else {
            self.message
                .set_text("Cannot locate the config directory (HOME is unset).");
            return;
        };
        let Some((exec, _)) = ps_binary() else {
            self.message.set_text(
                "Cannot find the `ps` binary next to ps-gui or in ~/.local/bin.\n\
                 Build it with `cargo build --release -p ps-daemon -p ps-gui` (or install `ps` \
                 into ~/.local/bin) and try again.",
            );
            return;
        };
        let unit_dir = config_home.join("systemd/user");
        let unit_path = unit_dir.join(UNIT_NAME);
        let unit = UNIT_TEMPLATE.replace(
            "ExecStart=%h/.local/bin/ps daemon",
            &format!("ExecStart={} daemon", systemd_path(&exec)),
        );

        if let Err(err) = fs::create_dir_all(&unit_dir).and_then(|_| fs::write(&unit_path, unit)) {
            self.message
                .set_text(&format!("Could not write {}: {err}", unit_path.display()));
            return;
        }

        let steps: [(&[&str], &str); 2] = [
            (
                &["--user", "daemon-reload"],
                "systemctl --user daemon-reload",
            ),
            (
                &["--user", "enable", "--now", UNIT_NAME],
                "systemctl --user enable --now",
            ),
        ];
        for (args, what) in steps {
            match Command::new("systemctl").args(args).output() {
                Ok(out) if out.status.success() => {}
                Ok(out) => {
                    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
                    self.message.set_text(&format!(
                        "Installed {} but `{what}` failed: {stderr}",
                        unit_path.display()
                    ));
                    return;
                }
                Err(err) => {
                    self.message
                        .set_text(&format!("systemctl is unavailable: {err}"));
                    return;
                }
            }
        }

        self.message.set_text(&format!(
            "Service installed at {} — checking whether the daemon started…",
            unit_path.display()
        ));
        let tab = self.clone_refs();
        gtk::glib::timeout_add_local_once(Duration::from_millis(1200), move || {
            tab.verify_started();
        });
    }

    fn confirm_reset(&self) {
        let dialog = gtk::AlertDialog::builder()
            .modal(true)
            .message("Reset statistics?")
            .detail("Conversion counters, snippet expansions, undos and retyped characters will be set back to zero. This cannot be undone. Restart the daemon afterwards so its in-memory counters do not overwrite the reset.")
            .buttons(["Cancel", "Reset"])
            .cancel_button(0)
            .default_button(0)
            .build();
        let tab = self.clone_refs();
        dialog.choose(
            Some(&self.window),
            gtk::gio::Cancellable::NONE,
            move |result| {
                if result == Ok(1) {
                    let mut stats = Stats::load(&tab.stats_path);
                    stats.reset();
                    tab.message.set_text(
                        "Statistics reset — restart the daemon so it does not overwrite them.",
                    );
                    tab.refresh();
                }
            },
        );
    }
}
