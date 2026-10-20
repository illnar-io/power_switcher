//! Snippets tab — abbreviation → expansion, stored in `snippets.json`.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

use ps_core::SnippetStore;
use ps_daemon::paths::layouts_json;

use crate::state::GuiState;
use crate::ui;

pub struct SnippetsTab {
    pub root: gtk::Box,
    store: Rc<RefCell<SnippetStore>>,
    list: gtk::ListBox,
    trigger: gtk::Entry,
    expansion: gtk::Entry,
    message: gtk::Label,
    count: gtk::Label,
}

impl SnippetsTab {
    pub fn new(state: &GuiState) -> Self {
        // Canonical matching needs the exported keycode map; without it the
        // store still works, only layout-independent duplicate detection is
        // lost.
        let ru_to_en = ps_core::snippets::ru_to_en_map(&layouts_json()).unwrap_or_default();
        let store = SnippetStore::new(&state.snippets_path, ru_to_en);

        let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
        root.set_margin_top(12);
        root.set_margin_bottom(12);
        root.set_margin_start(12);
        root.set_margin_end(12);

        let form = ui::form();
        let trigger = gtk::Entry::builder()
            .placeholder_text("!ee")
            .width_chars(16)
            .build();
        let expansion = gtk::Entry::builder()
            .placeholder_text("user@example.com")
            .hexpand(true)
            .build();
        let add = gtk::Button::with_label("Add");
        add.add_css_class("suggested-action");
        let add_row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        add_row.set_halign(gtk::Align::Start);
        add_row.append(&add);
        ui::row(&form, 0, "Abbreviation", &trigger);
        ui::row(&form, 1, "Expands to", &expansion);
        ui::row_full(&form, 2, &add_row);
        root.append(&form);

        let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let remove = gtk::Button::with_label("Remove selected");
        let refresh = gtk::Button::with_label("Refresh");
        let count = gtk::Label::new(None);
        count.set_opacity(0.6);
        toolbar.append(&remove);
        toolbar.append(&refresh);
        toolbar.append(&count);
        root.append(&toolbar);

        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::Single);
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .vexpand(true)
            .has_frame(true)
            .build();
        scroll.set_child(Some(&list));
        root.append(&scroll);

        let message = gtk::Label::new(None);
        message.set_halign(gtk::Align::Start);
        message.set_wrap(true);
        message.set_opacity(0.85);
        root.append(&message);
        root.append(&ui::hint(
            "Snippets match regardless of keyboard layout; the expansion is typed for you when \
             the abbreviation is followed by a trigger (Space, Enter or Tab). Saved immediately — \
             the daemon picks changes up on restart.",
        ));

        let tab = Self {
            root,
            store: Rc::new(RefCell::new(store)),
            list,
            trigger,
            expansion,
            message,
            count,
        };

        {
            let tab = tab.clone_refs();
            add.connect_clicked(move |_| tab.add());
        }
        {
            let tab = tab.clone_refs();
            remove.connect_clicked(move |_| tab.remove_selected());
        }
        {
            let tab = tab.clone_refs();
            refresh.connect_clicked(move |_| tab.refresh());
        }
        {
            let expansion = tab.expansion.clone();
            tab.trigger.connect_activate(move |_| {
                expansion.grab_focus();
            });
        }

        tab.refresh();
        tab
    }

    /// Cheap handle for signal closures (widgets are reference counted).
    fn clone_refs(&self) -> Self {
        Self {
            root: self.root.clone(),
            store: Rc::clone(&self.store),
            list: self.list.clone(),
            trigger: self.trigger.clone(),
            expansion: self.expansion.clone(),
            message: self.message.clone(),
            count: self.count.clone(),
        }
    }

    fn add(&self) {
        let trigger = self.trigger.text().trim().to_string();
        let expansion = self.expansion.text().to_string();
        if trigger.is_empty() || expansion.is_empty() {
            self.message
                .set_text("Both an abbreviation and an expansion are required.");
            return;
        }
        match self.store.borrow_mut().add(&trigger, &expansion) {
            Ok(()) => {
                self.message.set_text("");
                self.trigger.set_text("");
                self.expansion.set_text("");
                self.trigger.grab_focus();
                self.refresh();
            }
            Err(err) => self.message.set_text(&format!("Could not save: {err}")),
        }
    }

    fn remove_selected(&self) {
        let Some(row) = self.list.selected_row() else {
            self.message.set_text("Select a snippet to remove.");
            return;
        };
        let index = row.index();
        let trigger = self
            .store
            .borrow()
            .pairs()
            .get(index as usize)
            .map(|(trigger, _)| trigger.clone());
        let Some(trigger) = trigger else { return };
        match self.store.borrow_mut().remove(&trigger) {
            Ok(_) => {
                self.message.set_text("");
                self.refresh();
            }
            Err(err) => self.message.set_text(&format!("Could not save: {err}")),
        }
    }

    fn refresh(&self) {
        while let Some(row) = self.list.row_at_index(0) {
            self.list.remove(&row);
        }
        let store = self.store.borrow();
        self.count.set_text(&format!("{} snippet(s)", store.len()));
        if store.is_empty() {
            self.list.append(&ui::placeholder(
                "No snippets yet — add one above, e.g. !ee → user@example.com",
            ));
            return;
        }
        for (trigger, expansion) in store.pairs() {
            self.list.append(&ui::list_row(&format!(
                "<b>{}</b>   →   {}",
                ui::esc(trigger),
                ui::esc(expansion)
            )));
        }
    }
}
