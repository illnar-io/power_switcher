//! “Report a problem” dialog: categorise the issue, point user-fixable ones at
//! the FAQ, and otherwise copy a ready-to-paste report and open the GitHub
//! issue form.

use std::path::Path;
use std::process::Command;

use gtk::prelude::*;

use ps_daemon::Config;

use crate::ui;

const ISSUE_URL: &str =
    "https://github.com/illnar-io/power_switcher/issues/new?template=bug_report.yml";
const FAQ_URL: &str = "https://github.com/illnar-io/power_switcher/blob/main/docs/FAQ.md";

/// One entry of the problem picker.  `anchor` is the FAQ heading for problems
/// the user can fix themselves; `None` means it needs a bug report.
pub struct Category {
    pub label: &'static str,
    pub anchor: Option<&'static str>,
    pub hint: &'static str,
}

pub const CATEGORIES: &[Category] = &[
    Category {
        label: "Nothing is converted",
        anchor: Some("nothing-is-converted"),
        hint: "Usually the service, the keyboard permissions or a per-app mode.",
    },
    Category {
        label: "Wrong word converted / text turns to garbage",
        anchor: Some("wrong-conversion"),
        hint: "Usually the detection threshold, learned words or an app exception.",
    },
    Category {
        label: "Keyboard input stops or keys get stuck",
        anchor: Some("keyboard-stops-responding"),
        hint: "Usually the daemon crashed or lost its virtual keyboard.",
    },
    Category {
        label: "Tray icon is missing",
        anchor: Some("missing-tray-icon"),
        hint: "GNOME needs the AppIndicator extension to show tray items.",
    },
    Category {
        label: "Settings window does not open",
        anchor: Some("settings-window-problems"),
        hint: "Usually a missing `power-switcher-gui` binary or GTK 4.10+.",
    },
    Category {
        label: "Daemon crashes or will not start",
        anchor: None,
        hint: "There is no setting that fixes a crash — send the report.",
    },
    Category {
        label: "Something else",
        anchor: None,
        hint: "Describe what you expected and what happened instead.",
    },
];

/// Open the dialog.  The report is generated once, so the preview and the
/// clipboard always match.
pub fn dialog(parent: &gtk::Window, config: &Config, config_path: &Path) {
    let report = build_report(config, config_path);

    let window = gtk::Window::builder()
        .title("Report a problem")
        .transient_for(parent)
        .modal(true)
        .default_width(760)
        .default_height(640)
        .build();

    let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
    content.set_margin_top(16);
    content.set_margin_bottom(16);
    content.set_margin_start(18);
    content.set_margin_end(18);

    content.append(&ui::hint(
        "Pick what is wrong. User-fixable problems link to the FAQ; the rest open the GitHub \
         issue form with the report below already in your clipboard. Enable Debug logging in \
         Settings → Diagnostics and restart the daemon before reproducing.",
    ));

    let labels: Vec<&str> = CATEGORIES.iter().map(|c| c.label).collect();
    let dropdown = gtk::DropDown::from_strings(&labels);
    content.append(&dropdown);

    let guidance = ui::hint("");
    content.append(&guidance);

    let preview = gtk::TextView::builder()
        .editable(false)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::WordChar)
        .top_margin(8)
        .bottom_margin(8)
        .left_margin(8)
        .right_margin(8)
        .build();
    preview.buffer().set_text(&report);
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .min_content_height(260)
        .vexpand(true)
        .child(&preview)
        .build();
    content.append(&scroller);
    content.append(&ui::hint(
        "The report lists the environment, settings and the last 100 journal lines. It never \
         contains typed text or snippet contents, but it does show file paths — review it \
         before posting.",
    ));

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    buttons.set_margin_top(6);
    let faq = gtk::Button::with_label("Open FAQ");
    let copy = gtk::Button::with_label("Copy report");
    let issue = gtk::Button::with_label("Copy report and open GitHub issue");
    issue.add_css_class("suggested-action");
    let close = gtk::Button::with_label("Close");
    buttons.append(&faq);
    buttons.append(&copy);
    buttons.append(&issue);
    buttons.append(&close);
    content.append(&buttons);

    let message = gtk::Label::new(None);
    message.set_halign(gtk::Align::Start);
    message.set_wrap(true);
    message.set_selectable(true);
    content.append(&message);

    let update = {
        let guidance = guidance.clone();
        let faq = faq.clone();
        move |selected: u32| {
            let category = &CATEGORIES[selected as usize];
            guidance.set_text(category.hint);
            faq.set_visible(category.anchor.is_some());
        }
    };
    update(dropdown.selected());
    {
        let guidance = guidance.clone();
        let faq = faq.clone();
        dropdown.connect_selected_notify(move |dropdown| {
            let category = &CATEGORIES[dropdown.selected() as usize];
            guidance.set_text(category.hint);
            faq.set_visible(category.anchor.is_some());
        });
    }

    {
        let message = message.clone();
        let dropdown = dropdown.clone();
        faq.connect_clicked(move |_| {
            let Some(anchor) = CATEGORIES[dropdown.selected() as usize].anchor else {
                return;
            };
            open_uri(&format!("{FAQ_URL}#{anchor}"), &message);
        });
    }
    {
        let report = report.clone();
        let message = message.clone();
        copy.connect_clicked(move |_| {
            if copy_report(&report) {
                message.set_text("Report copied — paste it anywhere you like.");
            } else {
                message.set_text("No clipboard available; select the text above and copy it.");
            }
        });
    }
    {
        let report = report.clone();
        let message = message.clone();
        issue.connect_clicked(move |_| {
            let category = &CATEGORIES[dropdown.selected() as usize];
            let copied = copy_report(&report);
            let title = format!("[{}] ", category.label);
            let uri = format!("{ISSUE_URL}&title={}", url_encode(title.trim()));
            if copied {
                message.set_text(
                    "Report copied. Paste it into the “Diagnostics” field of the issue form.",
                );
            }
            open_uri(&uri, &message);
        });
    }
    {
        let window = window.clone();
        close.connect_clicked(move |_| window.close());
    }

    window.set_child(Some(&content));
    window.present();
}

fn copy_report(text: &str) -> bool {
    let Some(display) = gtk::gdk::Display::default() else {
        return false;
    };
    display.clipboard().set_text(text);
    true
}

fn open_uri(uri: &str, message: &gtk::Label) {
    if let Err(err) =
        gtk::gio::AppInfo::launch_default_for_uri(uri, None::<&gtk::gio::AppLaunchContext>)
    {
        message.set_text(&format!("{err}\nOpen this link manually:\n{uri}"));
    }
}

/// Diagnostics pasted into the issue form.  Deliberately excludes typed words,
/// snippet contents and any file contents that could contain them.
pub fn build_report(config: &Config, config_path: &Path) -> String {
    let journal = command(
        "journalctl",
        &["--user", "-u", "power_switcher", "-n", "100", "--no-pager"],
    )
    .unwrap_or_else(|| "journal unavailable".to_string());

    format!(
        "PowerSwitcher {}\n\
         OS: {}\n\
         Kernel: {}\n\
         Session: {}\n\
         Desktop: {}\n\
         GNOME: {}\n\
         Daemon: {}\n\
         Debug logging: {}\n\
         Layouts: {} (default {})\n\
         Threshold: {} (min word length {})\n\
         Live fix: {} | autocorrect: {} | self-learning: {}\n\
         Config: {}\n\
         \n\
         Journal (last 100 lines):\n{}\n",
        env!("CARGO_PKG_VERSION"),
        os_release(),
        read("/proc/sys/kernel/osrelease").unwrap_or_else(|| "unknown".to_string()),
        std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "unknown".to_string()),
        std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "unknown".to_string()),
        command("gnome-shell", &["--version"]).unwrap_or_else(|| "n/a".to_string()),
        command(
            "systemctl",
            &["--user", "is-active", "power_switcher.service"]
        )
        .unwrap_or_else(|| "unknown".to_string()),
        if config.debug_log { "on" } else { "off" },
        config.layouts.join(", "),
        config.default_layout,
        config.threshold,
        config.min_word_len,
        on_off(config.live_fix_enabled),
        on_off(config.spell_check),
        on_off(config.self_learning),
        config_path.display(),
        journal,
    )
}

fn on_off(value: bool) -> &'static str {
    if value {
        "on"
    } else {
        "off"
    }
}

fn read(path: &str) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

fn os_release() -> String {
    let text = read("/etc/os-release").unwrap_or_default();
    text.lines()
        .find_map(|line| line.strip_prefix("PRETTY_NAME="))
        .map(|name| name.trim_matches('"').to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn command(bin: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(bin).args(args).output().ok()?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let text = if stdout.is_empty() {
        String::from_utf8_lossy(&out.stderr).trim().to_string()
    } else {
        stdout
    };
    (!text.is_empty()).then_some(text)
}

/// Minimal percent-encoding for the issue title (no dependency needed).
fn url_encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            b' ' => "%20".to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAQ: &str = include_str!("../../../docs/FAQ.md");
    const TEMPLATE: &str = include_str!("../../../.github/ISSUE_TEMPLATE/bug_report.yml");

    /// GitHub's anchor rules for the headings we actually use.
    fn slug(heading: &str) -> String {
        heading
            .to_lowercase()
            .chars()
            .filter_map(|c| match c {
                'a'..='z' | '0'..='9' | '-' => Some(c),
                ' ' => Some('-'),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn faq_anchors_exist() {
        let anchors: Vec<String> = FAQ
            .lines()
            .filter_map(|line| line.strip_prefix("## "))
            .map(slug)
            .collect();
        for category in CATEGORIES {
            if let Some(anchor) = category.anchor {
                assert!(
                    anchors.iter().any(|a| a == anchor),
                    "docs/FAQ.md has no ## heading for #{anchor}"
                );
            }
        }
    }

    #[test]
    fn issue_template_accepts_the_report() {
        assert!(TEMPLATE.contains("id: diagnostics"));
        assert!(TEMPLATE.contains("https://github.com/illnar-io/power_switcher"));
    }

    #[test]
    fn report_contains_environment_and_settings_but_no_text() {
        let report = build_report(&Config::default(), Path::new("/tmp/config.toml"));
        assert!(report.starts_with(&format!("PowerSwitcher {}", env!("CARGO_PKG_VERSION"))));
        assert!(report.contains("Layouts: en, ru (default en)"));
        assert!(report.contains("Debug logging: off"));
        assert!(report.contains("Config: /tmp/config.toml"));
        assert!(report.contains("Journal (last 100 lines):"));
    }

    #[test]
    fn debug_toggle_is_reported() {
        let config = Config {
            debug_log: true,
            ..Config::default()
        };
        assert!(build_report(&config, Path::new("c.toml")).contains("Debug logging: on"));
    }

    #[test]
    fn url_encoding_is_query_safe() {
        assert_eq!(
            url_encode("[Tray icon] missing"),
            "%5BTray%20icon%5D%20missing"
        );
        assert_eq!(url_encode("a+b&c=d"), "a%2Bb%26c%3Dd");
    }
}
