//! Status tab — daemon state, current layout, counters and paths.

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gtk::prelude::*;

use ps_core::Stats;

use crate::state::GuiState;
use crate::ui;

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
    if let Some(state) = output("systemctl", &["user", "is-active", "power_switcher"]) {
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
        let reset = gtk::Button::with_label("Reset statistics");
        reset.add_css_class("destructive-action");
        buttons.append(&refresh);
        buttons.append(&restart);
        buttons.append(&reset);
        content.append(&buttons);

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
        match Command::new("systemctl")
            .args(["user", "restart", "power_switcher"])
            .output()
        {
            Ok(out) if out.status.success() => {
                self.message
                    .set_text("Daemon restart requested — refreshing shortly.");
                let tab = self.clone_refs();
                gtk::glib::timeout_add_local_once(Duration::from_millis(900), move || {
                    tab.refresh();
                });
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
                self.message.set_text(&format!(
                    "Could not restart power_switcher.service: {stderr}\n\
                     Start the daemon manually with `ps daemon` or install the systemd user unit."
                ));
            }
            Err(err) => self
                .message
                .set_text(&format!("systemctl is unavailable: {err}")),
        }
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
