//! `ps-gui` — the PowerSwitcher settings window (GTK4).
//!
//! Five tabs mirroring the Python tkinter GUI: Settings, Snippets,
//! Applications (per-app modes), Words, Status.  The GUI has no daemon IPC:
//! it edits the same files the daemon and the CLI use.

mod state;
mod tabs;
mod ui;

use std::rc::Rc;

use gtk::prelude::*;
use gtk::{glib, Application, ApplicationWindow, HeaderBar, Notebook};

fn main() -> glib::ExitCode {
    let app = Application::builder()
        .application_id("org.power_switcher.Settings")
        .build();
    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &Application) {
    let state = match state::GuiState::load() {
        Ok(state) => state,
        Err(err) => {
            error_window(app, &err);
            return;
        }
    };

    let window = ApplicationWindow::builder()
        .application(app)
        .title("PowerSwitcher")
        .default_width(900)
        .default_height(720)
        .build();

    let settings = Rc::new(tabs::settings::SettingsTab::new(&state.config));
    let snippets = tabs::snippets::SnippetsTab::new(&state);
    let exceptions = tabs::exceptions::ExceptionsTab::new(&state);
    let words = tabs::words::WordsTab::new(&state);
    let status = tabs::status::StatusTab::new(&state, window.upcast_ref());

    let notebook = Notebook::new();
    notebook.append_page(&settings.root, Some(&page_label("Settings")));
    notebook.append_page(&snippets.root, Some(&page_label("Snippets")));
    notebook.append_page(&exceptions.root, Some(&page_label("Applications")));
    notebook.append_page(&words.root, Some(&page_label("Words")));
    notebook.append_page(&status.root, Some(&page_label("Status")));
    window.set_child(Some(&notebook));

    let feedback = gtk::Label::new(None);
    feedback.set_opacity(0.7);
    let save = gtk::Button::with_label("Save");
    save.add_css_class("suggested-action");
    save.set_tooltip_text(Some(
        "Write config.toml. Snippets, applications and words are saved immediately.",
    ));

    let header = HeaderBar::new();
    header.set_title_widget(Some(&title_widget(&state.config_path)));
    header.pack_end(&save);
    header.pack_end(&feedback);
    window.set_titlebar(Some(&header));

    let state = Rc::new(state);
    {
        let state = Rc::clone(&state);
        let settings = Rc::clone(&settings);
        let feedback = feedback.clone();
        let window = window.clone();
        save.connect_clicked(move |_| {
            let mut config = state.config.clone();
            if let Err(err) = settings.apply(&mut config) {
                show_error(&window, "Check the settings", &err);
                return;
            }
            match state.save_config(&config) {
                Ok(()) => {
                    feedback.set_text("Saved — restart the daemon to apply");
                    let clear = feedback.clone();
                    glib::timeout_add_seconds_local_once(4, move || clear.set_text(""));
                }
                Err(err) => show_error(&window, "Could not save config.toml", &err),
            }
        });
    }

    window.present();
}

fn page_label(text: &str) -> gtk::Label {
    gtk::Label::new(Some(text))
}

fn title_widget(config_path: &std::path::Path) -> gtk::Box {
    let title = gtk::Label::new(None);
    title.set_markup("<b>PowerSwitcher</b>");
    let subtitle = gtk::Label::new(Some(&config_path.display().to_string()));
    subtitle.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    subtitle.set_opacity(0.55);
    let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
    widget.append(&title);
    widget.append(&subtitle);
    widget
}

fn show_error(parent: &impl IsA<gtk::Window>, message: &str, detail: &str) {
    let dialog = gtk::AlertDialog::builder()
        .modal(true)
        .message(message)
        .detail(detail)
        .build();
    dialog.show(Some(parent));
}

fn error_window(app: &Application, message: &str) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("PowerSwitcher")
        .default_width(560)
        .build();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_top(24);
    content.set_margin_bottom(24);
    content.set_margin_start(24);
    content.set_margin_end(24);

    let title = gtk::Label::new(None);
    title.set_markup("<b>PowerSwitcher could not start</b>");
    title.set_halign(gtk::Align::Start);
    let detail = gtk::Label::new(Some(message));
    detail.set_wrap(true);
    detail.set_xalign(0.0);
    detail.set_selectable(true);

    content.append(&title);
    content.append(&detail);
    window.set_child(Some(&content));
    window.present();
}
