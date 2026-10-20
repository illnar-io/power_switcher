//! Exceptions tab — per-application modes, stored in `exceptions.json`.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

use ps_core::ExceptionStore;

use crate::state::GuiState;
use crate::ui;

const MODES: [&str; 3] = ["off", "soft", "default (remove)"];

pub struct ExceptionsTab {
    pub root: gtk::Box,
    store: Rc<RefCell<ExceptionStore>>,
    list: gtk::ListBox,
    app: gtk::Entry,
    mode: gtk::DropDown,
    message: gtk::Label,
    count: gtk::Label,
}

impl ExceptionsTab {
    pub fn new(state: &GuiState) -> Self {
        let store = ExceptionStore::new(&state.exceptions_path);

        let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
        root.set_margin_top(12);
        root.set_margin_bottom(12);
        root.set_margin_start(12);
        root.set_margin_end(12);

        let form = ui::form();
        let app = gtk::Entry::builder()
            .placeholder_text("code, firefox, org.gnome.Terminal …")
            .hexpand(true)
            .build();
        let mode = gtk::DropDown::from_strings(&MODES);
        let apply = gtk::Button::with_label("Add / update");
        apply.add_css_class("suggested-action");
        let apply_row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        apply_row.set_halign(gtk::Align::Start);
        apply_row.append(&apply);
        ui::row(&form, 0, "Application", &app);
        ui::row(&form, 1, "Mode", &mode);
        ui::row_full(&form, 2, &apply_row);
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
            "The application is matched as a case-insensitive substring of the focused window's \
             app id/class. \"off\" disables auto-detect, \"soft\" only converts on the manual \
             hotkey. On GNOME, per-app focus detection needs the bundled shell extension. Saved \
             immediately — the daemon picks changes up on restart.",
        ));

        let tab = Self {
            root,
            store: Rc::new(RefCell::new(store)),
            list,
            app,
            mode,
            message,
            count,
        };

        {
            let tab = tab.clone_refs();
            apply.connect_clicked(move |_| tab.apply());
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
            app: self.app.clone(),
            mode: self.mode.clone(),
            message: self.message.clone(),
            count: self.count.clone(),
        }
    }

    fn apply(&self) {
        let name = self.app.text().trim().to_string();
        if name.is_empty() {
            self.message.set_text("An application name is required.");
            return;
        }
        let mode_index = self.mode.selected() as usize;
        let result = match mode_index {
            0 | 1 => self.store.borrow_mut().add(&name, MODES[mode_index]),
            2 => self.store.borrow_mut().remove(&name).map(|_| ()),
            _ => {
                self.message.set_text("Choose a mode first.");
                return;
            }
        };
        match result {
            Ok(()) => {
                self.message.set_text("");
                self.app.set_text("");
                self.refresh();
            }
            Err(err) => self.message.set_text(&format!("Could not save: {err}")),
        }
    }

    fn remove_selected(&self) {
        let Some(row) = self.list.selected_row() else {
            self.message.set_text("Select an application to remove.");
            return;
        };
        let index = row.index() as usize;
        let name = self
            .store
            .borrow()
            .all()
            .get(index)
            .map(|(name, _)| name.clone());
        let Some(name) = name else { return };
        match self.store.borrow_mut().remove(&name) {
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
        self.count
            .set_text(&format!("{} exception(s)", store.len()));
        if store.is_empty() {
            self.list.append(&ui::placeholder(
                "No per-app exceptions — auto-detect is active everywhere.",
            ));
            return;
        }
        for (name, mode) in store.all() {
            self.list.append(&ui::list_row(&format!(
                "<b>{}</b>   →   {}",
                ui::esc(&name),
                ui::esc(&mode)
            )));
        }
    }
}
