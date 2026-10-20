//! Small GTK helpers shared by the tabs (no libadwaita: GTK 4 alone).

use gtk::prelude::*;

/// A vertically scrolling page with a padded content box.
pub fn page() -> (gtk::ScrolledWindow, gtk::Box) {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    content.set_margin_start(18);
    content.set_margin_end(18);

    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .build();
    scroll.set_child(Some(&content));
    (scroll, content)
}

/// Bold section heading.
pub fn section(title: &str) -> gtk::Label {
    let label = gtk::Label::new(None);
    label.set_markup(&format!("<b>{}</b>", gtk::glib::markup_escape_text(title)));
    label.set_halign(gtk::Align::Start);
    label.set_margin_top(10);
    label
}

/// Dimmed explanatory text (wraps).
pub fn hint(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_halign(gtk::Align::Start);
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.set_opacity(0.65);
    label
}

/// Two-column settings form.
pub fn form() -> gtk::Grid {
    let grid = gtk::Grid::builder()
        .column_spacing(18)
        .row_spacing(8)
        .build();
    grid.set_halign(gtk::Align::Start);
    grid
}

/// Attach `control` next to a left-aligned caption.
pub fn row(grid: &gtk::Grid, line: i32, caption: &str, control: &impl IsA<gtk::Widget>) {
    let label = gtk::Label::new(Some(caption));
    label.set_halign(gtk::Align::Start);
    label.set_xalign(0.0);
    grid.attach(&label, 0, line, 1, 1);
    grid.attach(control, 1, line, 1, 1);
}

/// Attach a widget across both form columns.
pub fn row_full(grid: &gtk::Grid, line: i32, widget: &impl IsA<gtk::Widget>) {
    grid.attach(widget, 0, line, 2, 1);
}

/// A list row whose text is Pango markup.
pub fn list_row(markup: &str) -> gtk::ListBoxRow {
    let label = gtk::Label::new(None);
    label.set_markup(markup);
    label.set_halign(gtk::Align::Start);
    label.set_xalign(0.0);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label.set_margin_top(6);
    label.set_margin_bottom(6);
    label.set_margin_start(8);
    label.set_margin_end(8);

    let row = gtk::ListBoxRow::new();
    row.set_child(Some(&label));
    row
}

/// Empty-state placeholder row.
pub fn placeholder(text: &str) -> gtk::ListBoxRow {
    let label = gtk::Label::new(Some(text));
    label.set_halign(gtk::Align::Start);
    label.set_margin_top(10);
    label.set_margin_bottom(10);
    label.set_margin_start(8);
    label.set_opacity(0.55);
    let row = gtk::ListBoxRow::new();
    row.set_child(Some(&label));
    row.set_activatable(false);
    row.set_selectable(false);
    row
}

/// Escape a value for use inside Pango markup.
pub fn esc(text: &str) -> String {
    gtk::glib::markup_escape_text(text).to_string()
}
