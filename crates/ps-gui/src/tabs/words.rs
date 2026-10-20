//! Words tab — learned words and manual exceptions, stored in `words.json`.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

use ps_core::WordStore;

use crate::state::GuiState;
use crate::ui;

/// Rows are shown learned-first, matching the Python GUI.
fn entries(store: &WordStore) -> Vec<(String, &'static str)> {
    let mut rows: Vec<(String, &'static str)> = store
        .learned_words()
        .into_iter()
        .map(|word| (word, "learned"))
        .collect();
    rows.extend(
        store
            .exceptions()
            .into_iter()
            .map(|word| (word, "exception")),
    );
    rows
}

pub struct WordsTab {
    pub root: gtk::Box,
    store: Rc<RefCell<WordStore>>,
    list: gtk::ListBox,
    word: gtk::Entry,
    kind: gtk::DropDown,
    message: gtk::Label,
}

impl WordsTab {
    pub fn new(state: &GuiState) -> Self {
        let store = WordStore::load(Some(&state.words_path));

        let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
        root.set_margin_top(12);
        root.set_margin_bottom(12);
        root.set_margin_start(12);
        root.set_margin_end(12);

        let form = ui::form();
        let word = gtk::Entry::builder()
            .placeholder_text("word")
            .hexpand(true)
            .build();
        let kind = gtk::DropDown::from_strings(&["exception", "learned"]);
        let add = gtk::Button::with_label("Add");
        add.add_css_class("suggested-action");
        let add_row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        add_row.set_halign(gtk::Align::Start);
        add_row.append(&add);
        ui::row(&form, 0, "Word", &word);
        ui::row(&form, 1, "Kind", &kind);
        ui::row_full(&form, 2, &add_row);
        root.append(&form);

        let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let remove = gtk::Button::with_label("Remove selected");
        let refresh = gtk::Button::with_label("Refresh");
        toolbar.append(&remove);
        toolbar.append(&refresh);
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
            "Words in this list are never auto-converted. \"learned\" words are added \
             automatically when you undo a conversion (double-tap Shift); \"exception\" words \
             are added by hand. Saved immediately — the daemon picks changes up on restart.",
        ));

        let tab = Self {
            root,
            store: Rc::new(RefCell::new(store)),
            list,
            word,
            kind,
            message,
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

        tab.refresh();
        tab
    }

    fn clone_refs(&self) -> Self {
        Self {
            root: self.root.clone(),
            store: Rc::clone(&self.store),
            list: self.list.clone(),
            word: self.word.clone(),
            kind: self.kind.clone(),
            message: self.message.clone(),
        }
    }

    fn add(&self) {
        let word = self.word.text().trim().to_lowercase();
        if word.chars().count() < 2 {
            self.message
                .set_text("A word must contain at least two characters.");
            return;
        }
        let learned = self.kind.selected() == 1;
        let changed = {
            let mut store = self.store.borrow_mut();
            if learned {
                store.add_learned(&word)
            } else {
                store.add_exception(&word)
            }
        };
        if !changed {
            self.message
                .set_text(&format!("\"{word}\" is already in the list."));
            return;
        }
        self.message.set_text("");
        self.word.set_text("");
        self.refresh();
    }

    fn remove_selected(&self) {
        let Some(row) = self.list.selected_row() else {
            self.message.set_text("Select a word to remove.");
            return;
        };
        let index = row.index() as usize;
        let word = entries(&self.store.borrow())
            .get(index)
            .map(|(word, _)| word.clone());
        let Some(word) = word else { return };
        if self.store.borrow_mut().remove(&word) {
            self.message.set_text("");
            self.refresh();
        }
    }

    fn refresh(&self) {
        while let Some(row) = self.list.row_at_index(0) {
            self.list.remove(&row);
        }
        let rows = entries(&self.store.borrow());
        if rows.is_empty() {
            self.list.append(&ui::placeholder(
                "No words yet — undo a conversion to teach the detector, or add one above.",
            ));
            return;
        }
        for (word, kind) in rows {
            self.list.append(&ui::list_row(&format!(
                "<b>{}</b>   <small>{}</small>",
                ui::esc(&word),
                ui::esc(kind)
            )));
        }
    }
}
