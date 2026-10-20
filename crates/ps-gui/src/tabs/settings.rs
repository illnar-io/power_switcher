//! Settings tab — edits `config.toml`.

use gtk::prelude::*;

use ps_daemon::Config;

use crate::state::parse_layouts;
use crate::ui;

pub struct SettingsTab {
    pub root: gtk::ScrolledWindow,
    layouts: gtk::Entry,
    default_layout: gtk::Entry,
    threshold: gtk::SpinButton,
    min_word_len: gtk::SpinButton,
    auto_detect: gtk::Switch,
    developer_mode: gtk::Switch,

    trigger_space: gtk::Switch,
    trigger_enter: gtk::Switch,
    trigger_tab: gtk::Switch,
    trigger_punctuation: gtk::Switch,
    snippet_space: gtk::Switch,
    snippet_enter: gtk::Switch,
    snippet_tab: gtk::Switch,

    arrows_cancel: gtk::Switch,
    live_fix_enabled: gtk::Switch,
    lr_shift_pause: gtk::Switch,
    shift_tap_switch_layout: gtk::Switch,
    manual_only_last_word: gtk::Switch,
    self_learning: gtk::Switch,
    sound_enabled: gtk::Switch,
    no_correct_after_switch: gtk::SpinButton,

    undo_words: gtk::SpinButton,
    undo_seconds: gtk::SpinButton,

    hotkey_convert: gtk::Entry,
    hotkey_selection: gtk::Entry,
}

fn switch(active: bool) -> gtk::Switch {
    let sw = gtk::Switch::builder().active(active).build();
    sw.set_halign(gtk::Align::Start);
    sw.set_valign(gtk::Align::Center);
    sw
}

fn spin(min: f64, max: f64, step: f64, digits: u32, value: f64) -> gtk::SpinButton {
    let spin = gtk::SpinButton::with_range(min, max, step);
    spin.set_digits(digits);
    spin.set_value(value);
    spin.set_halign(gtk::Align::Start);
    spin
}

impl SettingsTab {
    pub fn new(config: &Config) -> Self {
        let (root, content) = ui::page();

        // ── Layouts ─────────────────────────────────────────────────────
        content.append(&ui::section("Layouts"));
        let grid = ui::form();
        let layouts = gtk::Entry::builder()
            .text(config.layouts.join(", "))
            .width_chars(30)
            .hexpand(true)
            .build();
        let default_layout = gtk::Entry::builder()
            .text(&config.default_layout)
            .width_chars(12)
            .build();
        ui::row(&grid, 0, "Layouts (comma-separated)", &layouts);
        ui::row(&grid, 1, "Default layout", &default_layout);
        content.append(&grid);
        content.append(&ui::hint(
            "Every layout needs an n-gram profile. Supported: en, ru, ua, de, fr, es, it, pl.",
        ));

        // ── Detection ───────────────────────────────────────────────────
        content.append(&ui::section("Detection"));
        let grid = ui::form();
        let threshold = spin(0.5, 10.0, 0.1, 1, config.threshold);
        let min_word_len = spin(1.0, 20.0, 1.0, 0, config.min_word_len as f64);
        let auto_detect = switch(config.auto_detect);
        let developer_mode = switch(config.developer_mode);
        ui::row(&grid, 0, "Threshold", &threshold);
        ui::row(&grid, 1, "Minimum word length", &min_word_len);
        ui::row(&grid, 2, "Auto-detect", &auto_detect);
        ui::row(
            &grid,
            3,
            "Developer mode (never auto-convert)",
            &developer_mode,
        );
        content.append(&grid);
        content.append(&ui::hint(
            "Lower thresholds convert more eagerly; a known word of the current language is only \
             converted with a much larger margin.",
        ));

        // ── Triggers ────────────────────────────────────────────────────
        content.append(&ui::section("Triggers"));
        let grid = ui::form();
        let trigger_space = switch(config.trigger_space);
        let trigger_enter = switch(config.trigger_enter);
        let trigger_tab = switch(config.trigger_tab);
        let trigger_punctuation = switch(config.trigger_punctuation);
        let snippet_space = switch(config.snippet_expand_space);
        let snippet_enter = switch(config.snippet_expand_enter);
        let snippet_tab = switch(config.snippet_expand_tab);
        ui::row(&grid, 0, "Space checks the word", &trigger_space);
        ui::row(&grid, 1, "Enter checks the word", &trigger_enter);
        ui::row(&grid, 2, "Tab checks the word", &trigger_tab);
        ui::row(
            &grid,
            3,
            "Punctuation checks the word",
            &trigger_punctuation,
        );
        ui::row(&grid, 4, "Space expands snippets", &snippet_space);
        ui::row(&grid, 5, "Enter expands snippets", &snippet_enter);
        ui::row(&grid, 6, "Tab expands snippets", &snippet_tab);
        content.append(&grid);

        // ── Behaviour ───────────────────────────────────────────────────
        content.append(&ui::section("Behaviour"));
        let grid = ui::form();
        let arrows_cancel = switch(config.arrows_cancel);
        let live_fix_enabled = switch(config.live_fix_enabled);
        let lr_shift_pause = switch(config.lr_shift_pause);
        let shift_tap_switch_layout = switch(config.shift_tap_switch_layout);
        let manual_only_last_word = switch(config.manual_only_last_word);
        let self_learning = switch(config.self_learning);
        let sound_enabled = switch(config.sound_enabled);
        let no_correct_after_switch = spin(0.0, 10.0, 0.1, 1, config.no_correct_after_switch);
        ui::row(&grid, 0, "Arrow keys cancel detection", &arrows_cancel);
        ui::row(&grid, 1, "Live (mid-word) fix", &live_fix_enabled);
        ui::row(&grid, 2, "L+R Shift pauses auto-detect", &lr_shift_pause);
        ui::row(
            &grid,
            3,
            "Lone Shift tap switches layout (legacy)",
            &shift_tap_switch_layout,
        );
        ui::row(
            &grid,
            4,
            "Hotkey converts only the last word",
            &manual_only_last_word,
        );
        ui::row(
            &grid,
            5,
            "Learn the word when a conversion is undone",
            &self_learning,
        );
        ui::row(&grid, 6, "Sound feedback", &sound_enabled);
        ui::row(
            &grid,
            7,
            "Ignore auto-detect after a layout switch (s)",
            &no_correct_after_switch,
        );
        content.append(&grid);
        content.append(&ui::hint(
            "Leave the lone-Shift gesture off: your desktop already switches layouts with Super+Space.",
        ));

        // ── Undo ────────────────────────────────────────────────────────
        content.append(&ui::section("Undo"));
        let grid = ui::form();
        let undo_words = spin(0.0, 50.0, 1.0, 0, config.undo_words as f64);
        let undo_seconds = spin(0.0, 60.0, 0.5, 1, config.undo_seconds);
        ui::row(&grid, 0, "Undo words (0 = single word)", &undo_words);
        ui::row(&grid, 1, "Undo window (seconds)", &undo_seconds);
        content.append(&grid);

        // ── Hotkeys ─────────────────────────────────────────────────────
        content.append(&ui::section("Hotkeys"));
        let grid = ui::form();
        let hotkey_convert = gtk::Entry::builder()
            .text(&config.hotkey_convert)
            .width_chars(20)
            .build();
        let hotkey_selection = gtk::Entry::builder()
            .text(&config.hotkey_selection)
            .width_chars(20)
            .build();
        ui::row(&grid, 0, "Manual convert", &hotkey_convert);
        ui::row(&grid, 1, "Convert selection", &hotkey_selection);
        content.append(&grid);
        content.append(&ui::hint(
            "Manual convert: `ctrl+alt+z` (default, works on laptops), `pause`/`break` \
             (Caramba, needs a full keyboard), or `off`. Double-tap Shift always works too. \
             `alt+shift` converts the selection — that needs clipboard access and is not \
             available on Wayland yet.",
        ));
        content.append(&ui::hint(
            "Saving rewrites config.toml (comments are not preserved). Restart the daemon from \
             the Status tab to apply changes; changing the layout list rebuilds the detector on \
             restart.",
        ));

        Self {
            root,
            layouts,
            default_layout,
            threshold,
            min_word_len,
            auto_detect,
            developer_mode,
            trigger_space,
            trigger_enter,
            trigger_tab,
            trigger_punctuation,
            snippet_space,
            snippet_enter,
            snippet_tab,
            arrows_cancel,
            live_fix_enabled,
            lr_shift_pause,
            shift_tap_switch_layout,
            manual_only_last_word,
            self_learning,
            sound_enabled,
            no_correct_after_switch,
            undo_words,
            undo_seconds,
            hotkey_convert,
            hotkey_selection,
        }
    }

    /// Copy the widget values into `config`, validating user input.
    pub fn apply(&self, config: &mut Config) -> Result<(), String> {
        let layouts = parse_layouts(&self.layouts.text())?;
        let default_layout = self.default_layout.text().trim().to_lowercase();
        if default_layout.is_empty() {
            return Err("The default layout must not be empty.".to_string());
        }
        if !layouts.contains(&default_layout) {
            return Err(format!(
                "The default layout \"{default_layout}\" is not in the layout list ({}).",
                layouts.join(", ")
            ));
        }

        config.layouts = layouts;
        config.default_layout = default_layout;
        config.threshold = self.threshold.value();
        config.min_word_len = self.min_word_len.value() as usize;
        config.auto_detect = self.auto_detect.is_active();
        config.developer_mode = self.developer_mode.is_active();

        config.trigger_space = self.trigger_space.is_active();
        config.trigger_enter = self.trigger_enter.is_active();
        config.trigger_tab = self.trigger_tab.is_active();
        config.trigger_punctuation = self.trigger_punctuation.is_active();
        config.snippet_expand_space = self.snippet_space.is_active();
        config.snippet_expand_enter = self.snippet_enter.is_active();
        config.snippet_expand_tab = self.snippet_tab.is_active();

        config.arrows_cancel = self.arrows_cancel.is_active();
        config.live_fix_enabled = self.live_fix_enabled.is_active();
        config.lr_shift_pause = self.lr_shift_pause.is_active();
        config.shift_tap_switch_layout = self.shift_tap_switch_layout.is_active();
        config.manual_only_last_word = self.manual_only_last_word.is_active();
        config.self_learning = self.self_learning.is_active();
        config.sound_enabled = self.sound_enabled.is_active();
        config.no_correct_after_switch = self.no_correct_after_switch.value();

        config.undo_words = self.undo_words.value() as usize;
        config.undo_seconds = self.undo_seconds.value();

        config.hotkey_convert = self.hotkey_convert.text().trim().to_string();
        config.hotkey_selection = self.hotkey_selection.text().trim().to_string();
        Ok(())
    }
}
