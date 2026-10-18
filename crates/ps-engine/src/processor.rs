//! The event processor — port of `KeyProcessor`.
//!
//! See the crate docs for the action-stream design.

use std::collections::HashSet;
use std::path::PathBuf;

use log::{debug, info};

use ps_core::key_buffer::{
    is_buffer_reset_key, is_hard_separator, is_word_separator, KeyBuffer, SentenceBuffer,
    KEY_BACKSPACE, KEY_DOWN, KEY_ENTER, KEY_KPENTER, KEY_LEFT, KEY_LEFTALT, KEY_LEFTCTRL,
    KEY_LEFTSHIFT, KEY_RIGHT, KEY_RIGHTSHIFT, KEY_SPACE, KEY_TAB, KEY_UP,
};
use ps_core::{
    AntiResonanceGuard, ExceptionStore, LayoutSet, ModifierState, ShiftAction, SnippetStore, Stats,
    UndoHistory,
};
use ps_focus::ActiveWindowProvider;

/// `EV_KEY` type code (Linux input-event-codes.h).
pub const EV_KEY: u16 = 1;

/// Post-injection blackout: events within this window are dropped
/// (Python `_injecting_until = time.time() + 0.05`).
const INJECTION_COOLDOWN: f64 = 0.05;

/// One raw input event (the fields of `evdev.InputEvent` the daemon reads).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawEvent {
    pub type_: u16,
    pub code: u16,
    /// 0 = release, 1 = press, 2 = autorepeat.
    pub value: i32,
}

impl RawEvent {
    pub fn key(code: u16, value: i32) -> Self {
        Self {
            type_: EV_KEY,
            code,
            value,
        }
    }
    pub fn is_press(&self) -> bool {
        self.value == 1
    }
    pub fn is_repeat(&self) -> bool {
        self.value == 2
    }
}

/// An action the daemon must perform, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Echo of a physical event through the virtual keyboard (immediate).
    Emit { code: u16, value: i32 },
    /// Injected keystroke: write the event, then wait `delay_ms` (Python's
    /// inter-event `asyncio.sleep` pacing).
    Inject {
        code: u16,
        value: i32,
        delay_ms: u64,
    },
    /// Let the compositor settle after a layout switch (Python's 0.1 s wait).
    Pause { ms: u64 },
    /// Switch the compositor layout to this index.
    SetLayout { index: usize },
    /// Play a freedesktop sound (`paplay …/<name>.oga`).
    Sound { name: &'static str },
}

/// Construction options — every parameter of Python's `KeyProcessor.__init__`.
#[derive(Debug, Clone)]
pub struct ProcessorConfig {
    pub current_layout: String,
    pub threshold: f64,
    pub min_word_len: usize,
    // Snippets
    pub snippet_expand_space: bool,
    pub snippet_expand_enter: bool,
    pub snippet_expand_tab: bool,
    // Triggers
    pub trigger_space: bool,
    pub trigger_enter: bool,
    pub trigger_tab: bool,
    pub trigger_punctuation: bool,
    // Undo
    pub undo_words: usize,
    pub undo_seconds: f64,
    // Behavior
    pub developer_mode: bool,
    pub arrows_cancel: bool,
    pub live_fix_enabled: bool,
    // Layout-switch guards & hotkeys
    pub no_correct_after_switch: f64,
    pub lr_shift_pause: bool,
    pub manual_only_last_word: bool,
    pub sound_enabled: bool,
    pub self_learning: bool,
    // KeyBuffer capacity (Python `KeyBuffer(max_len=100)`).
    pub key_buffer_max_len: usize,
}

impl Default for ProcessorConfig {
    fn default() -> Self {
        Self {
            current_layout: "en".to_string(),
            threshold: ps_core::layout_set::DEFAULT_THRESHOLD,
            min_word_len: ps_core::layout_set::DEFAULT_MIN_WORD_LEN,
            snippet_expand_space: true,
            snippet_expand_enter: true,
            snippet_expand_tab: true,
            trigger_space: true,
            trigger_enter: true,
            trigger_tab: true,
            trigger_punctuation: true,
            undo_words: 0,
            undo_seconds: 3.0,
            developer_mode: false,
            arrows_cancel: true,
            live_fix_enabled: true,
            no_correct_after_switch: 0.0,
            lr_shift_pause: true,
            manual_only_last_word: false,
            sound_enabled: true,
            self_learning: true,
            key_buffer_max_len: 100,
        }
    }
}

/// The conversion state machine.
pub struct Processor {
    config: ProcessorConfig,
    pub layout_set: LayoutSet,
    /// Current layout name (index into `layout_set.layouts`).
    layout: String,
    snippets: SnippetStore,
    undo: UndoHistory,
    anti_resonance: AntiResonanceGuard,
    pub stats: Stats,
    exceptions: ExceptionStore,
    focus: ActiveWindowProvider,

    // Buffer state
    buf: KeyBuffer,
    sentence: SentenceBuffer,
    mods: ModifierState,
    pressed: HashSet<u16>,

    // Injection state
    injecting: bool,
    queued_events: Vec<RawEvent>,
    injecting_until: f64,

    // Live fix state
    live_fix_last: String,
    word_edited: bool,
    #[allow(dead_code)] // parity with Python (bookkeeping only)
    last_real_key_at: f64,

    // App context
    front_app_class: String,
    front_app_mode: String,

    // Pause (L+R Shift) and layout-change guard
    auto_paused: bool,
    lr_shift_consumed: bool,
    shift_held: HashSet<u16>,
    layout_changed_at: f64,
}

impl Processor {
    /// Build the processor.  `snippets_path` / `stats` / `exceptions_path` map
    /// to the Python constructor's optional stores; `focus` is the
    /// active-window provider (usually `ActiveWindowProvider::new()`).
    pub fn new(
        mut layout_set: LayoutSet,
        config: ProcessorConfig,
        snippets_path: Option<PathBuf>,
        stats: Stats,
        exceptions_path: Option<PathBuf>,
        focus: ActiveWindowProvider,
    ) -> Self {
        // Snippet canonicalisation needs the RU→EN char map (Python's
        // `_canonical` builds `build_char_map("ru", "en")`).
        let ru_to_en = layout_set.char_map_clone("ru", "en");
        let snippets = SnippetStore::new(
            snippets_path.unwrap_or_else(default_snippets_path),
            ru_to_en,
        );
        let undo = UndoHistory::new(50, config.undo_seconds);
        let exceptions = match &exceptions_path {
            Some(path) => ExceptionStore::new(path),
            None => ExceptionStore::new(ps_core::app_exceptions::default_exceptions_path()),
        };
        Self {
            layout: config.current_layout.clone(),
            layout_set,
            snippets,
            undo,
            anti_resonance: AntiResonanceGuard::default(),
            stats,
            exceptions,
            focus,
            buf: KeyBuffer::new(config.key_buffer_max_len),
            sentence: SentenceBuffer::new(),
            mods: ModifierState::default(),
            pressed: HashSet::new(),
            injecting: false,
            queued_events: Vec::new(),
            injecting_until: 0.0,
            live_fix_last: String::new(),
            word_edited: false,
            last_real_key_at: 0.0,
            front_app_class: String::new(),
            front_app_mode: "default".to_string(),
            auto_paused: false,
            lr_shift_consumed: false,
            shift_held: HashSet::new(),
            layout_changed_at: 0.0,
            config,
        }
    }

    // ── state accessors (daemon/initial-sync/debug) ─────────────────────

    pub fn current_layout(&self) -> &str {
        &self.layout
    }

    /// Initial layout sync (`run()` after `backend.get_current_index()`).
    pub fn set_layout_state(&mut self, layout: &str) {
        if self.layout_set.layouts.iter().any(|l| l == layout) {
            self.layout = layout.to_string();
        }
    }

    pub fn auto_paused(&self) -> bool {
        self.auto_paused
    }

    /// Start paused (`ps daemon --no-auto` — hotkey-only mode).
    /// NOTE: Python's `--no-auto` flag only changed the startup banner and was
    /// never applied to the processor (upstream bug); the Rust daemon applies it.
    pub fn set_auto_paused(&mut self, paused: bool) {
        self.auto_paused = paused;
    }

    /// Number of loaded snippets (startup banner).
    pub fn snippets_len(&self) -> usize {
        self.snippets.len()
    }

    pub fn front_app_class(&self) -> &str {
        &self.front_app_class
    }

    /// Refresh per-app context lazily (Python `_refresh_front_app`).
    pub fn refresh_front_app(&mut self, now: f64) {
        let cls = self.focus.get_class_at(now, false);
        if cls != self.front_app_class {
            debug!("Focus changed: {:?}", cls);
        }
        self.front_app_class = cls.clone();
        self.front_app_mode = self.exceptions.get_mode(&cls).to_string();
    }

    // ── event handling ──────────────────────────────────────────────────

    /// Process one raw event at time `now`; returns the ordered action stream.
    pub fn on_event(&mut self, ev: RawEvent, now: f64) -> Vec<Action> {
        if ev.type_ != EV_KEY {
            return vec![Action::Emit {
                code: ev.code,
                value: ev.value,
            }];
        }

        if self.injecting {
            self.queued_events.push(ev);
            return Vec::new();
        }

        if now < self.injecting_until {
            return Vec::new();
        }

        let kc = ev.code;
        let pressed = ev.is_press();
        let repeat = ev.is_repeat();

        if repeat {
            return vec![Action::Emit {
                code: ev.code,
                value: ev.value,
            }];
        }

        // Modifier handling
        let action = self.mods.update(kc, pressed, now);
        if is_shift(kc) {
            let mut actions = vec![Action::Emit {
                code: ev.code,
                value: ev.value,
            }];

            if pressed {
                self.shift_held.insert(kc);
                // Both Shift keys held together → pause/resume auto-detect.
                if self.config.lr_shift_pause && self.shift_held.len() == 2 {
                    self.lr_shift_consumed = true;
                    self.mods.reset_shift_taps();
                    self.toggle_auto(&mut actions);
                }
                return actions;
            }

            // Release
            self.shift_held.remove(&kc);
            if self.lr_shift_consumed {
                if self.shift_held.is_empty() {
                    self.lr_shift_consumed = false;
                }
                return actions;
            }
            match action {
                Some(ShiftAction::DoubleShift) => {
                    let conv = self.do_manual_convert(now);
                    actions.extend(conv);
                }
                Some(ShiftAction::SingleShift) if self.pressed.is_empty() => {
                    let layouts = self.layout_set.layouts.clone();
                    if kc == KEY_LEFTSHIFT && !layouts.is_empty() {
                        actions.extend(self.set_layout(&layouts[0], now));
                        self.sentence.clear();
                        self.sound("audio-volume-low", &mut actions);
                    } else if kc == KEY_RIGHTSHIFT && layouts.len() > 1 {
                        actions.extend(self.set_layout(&layouts[1], now));
                        self.sentence.clear();
                        self.sound("audio-volume-low", &mut actions);
                    }
                }
                _ => {}
            }
            return actions;
        }

        if pressed {
            self.pressed.insert(kc);
        } else {
            self.pressed.remove(&kc);
        }

        // Pass through to compositor
        let mut actions = vec![Action::Emit {
            code: ev.code,
            value: ev.value,
        }];

        if pressed {
            self.update_buf(kc, now, &mut actions);
        }
        actions
    }

    fn toggle_auto(&mut self, actions: &mut Vec<Action>) {
        self.auto_paused = !self.auto_paused;
        let state = if self.auto_paused { "PAUSED" } else { "ACTIVE" };
        info!("AUDIT: auto-detect {state} (L+R Shift)");
        self.sound(
            if self.auto_paused {
                "audio-volume-low"
            } else {
                "message-new-instant"
            },
            actions,
        );
    }

    fn sound(&self, name: &'static str, actions: &mut Vec<Action>) {
        if self.config.sound_enabled {
            actions.push(Action::Sound { name });
        }
    }

    /// Python `_set_layout`: switch via the backend, update state, stamp the
    /// guard clock.  Unknown layout → no-op (Python ValueError).
    fn set_layout(&mut self, layout: &str, now: f64) -> Vec<Action> {
        match self.layout_set.layouts.iter().position(|l| l == layout) {
            Some(index) => {
                self.layout = layout.to_string();
                self.layout_changed_at = now;
                info!("AUDIT: set_layout({layout})");
                vec![Action::SetLayout { index }]
            }
            None => Vec::new(),
        }
    }

    // ── buffer management ───────────────────────────────────────────────

    fn is_auto_trigger(&self, kc: u16) -> bool {
        if kc == KEY_SPACE {
            return self.config.trigger_space;
        }
        if kc == KEY_ENTER || kc == KEY_KPENTER {
            return self.config.trigger_enter;
        }
        if kc == KEY_TAB {
            return self.config.trigger_tab;
        }
        false
    }

    fn is_snippet_trigger(&self, kc: u16) -> bool {
        if kc == KEY_SPACE {
            return self.config.snippet_expand_space;
        }
        if kc == KEY_ENTER || kc == KEY_KPENTER {
            return self.config.snippet_expand_enter;
        }
        if kc == KEY_TAB {
            return self.config.snippet_expand_tab;
        }
        false
    }

    fn should_skip_auto(&self) -> bool {
        if self.auto_paused {
            return true;
        }
        if self.front_app_mode == "off" {
            return true;
        }
        if self.config.developer_mode && self.exceptions.is_dev_app(&self.front_app_class) {
            return true;
        }
        false
    }

    /// Python `_update_buf`.
    fn update_buf(&mut self, kc: u16, now: f64, actions: &mut Vec<Action>) {
        // Backspace
        if kc == KEY_BACKSPACE {
            self.live_fix_last = String::new();
            self.word_edited = true;
            self.buf.backspace();
            self.sentence.pop();
            return;
        }

        // Navigation keys — clear context
        if is_buffer_reset_key(kc) {
            self.live_fix_last = String::new();
            self.word_edited = false;
            self.buf.clear();
            self.sentence.clear();
            self.undo.clear_old_at(now);
            if self.config.arrows_cancel && matches!(kc, KEY_UP | KEY_DOWN | KEY_LEFT | KEY_RIGHT) {
                self.anti_resonance.reset();
            }
            return;
        }

        // Arrow keys clear buffer (configurable)
        if self.config.arrows_cancel && matches!(kc, KEY_UP | KEY_DOWN | KEY_LEFT | KEY_RIGHT) {
            self.live_fix_last = String::new();
            self.word_edited = false;
            self.buf.clear();
            self.sentence.clear();
            self.anti_resonance.reset();
            return;
        }

        // Shortcuts (Ctrl/Alt combos) — clear context
        if !self.mods.is_plain() {
            self.live_fix_last = String::new();
            self.word_edited = false;
            self.buf.clear();
            self.sentence.clear();
            return;
        }

        // Word boundary (Space / Enter / Tab / punctuation).
        // A keycode is only a boundary if it does *not* produce a letter in the
        // active layout and is punctuation in *every* configured layout
        // (Python `_ambiguous_separator` — see ps-core).
        let char = self
            .layout_set
            .keycode_to_char(kc, &self.layout, self.mods.shift);
        let hard_sep = is_hard_separator(kc);
        let is_boundary = hard_sep
            || (is_word_separator(kc)
                && !char.is_some_and(|c| c.is_alphabetic())
                && !self.layout_set.ambiguous_separator(kc));

        if is_boundary {
            self.live_fix_last = String::new();
            self.word_edited = false;

            // Record separator in sentence
            if kc == KEY_SPACE {
                self.sentence.append(" ");
            } else if kc == KEY_ENTER || kc == KEY_KPENTER {
                self.sentence.append("\n");
            } else if let Some(c) = char {
                self.sentence.append(&c.to_string());
            } else {
                self.sentence.clear();
            }

            let word = self.buf.flush();
            if word.is_empty() {
                return;
            }

            // Snippet expansion (check BEFORE auto-detect)
            if self.is_snippet_trigger(kc) && !self.injecting {
                if let Some(expansion) = self.snippets.expansion(&word) {
                    self.expand_snippet(&word, &expansion, kc, now, actions);
                    return;
                }
            }

            // Password detection
            if ps_core::text::is_password(&word) {
                actions.extend(self.set_layout("en", now));
                return;
            }

            // Auto-detection.  Space/Enter/Tab are gated by the trigger
            // options; other punctuation is gated by trigger_punctuation.
            let sep_shift = self.mods.shift;
            if hard_sep && self.is_auto_trigger(kc) {
                self.maybe_auto(&word, Some(kc), false, now, actions);
            } else if !hard_sep && self.config.trigger_punctuation {
                self.maybe_auto(&word, Some(kc), sep_shift, now, actions);
            }
            return;
        }

        // Regular character
        if let Some(c) = char {
            self.buf.add(kc, c);
            self.sentence.append(&c.to_string());
            self.last_real_key_at = now;

            // Mid-word eager conversion (very high confidence only).
            // Divergence: Python used asyncio.create_task (racy); here the
            // conversion is inlined, which cannot interleave with typing.
            let word = self.buf.text();
            if self.config.live_fix_enabled
                && word.chars().count() >= 4
                && !ps_core::text::is_password(&word)
                && !self.word_edited
            {
                let target = self
                    .layout_set
                    .find_best_layout(&word, &self.layout, 15.0, 4);
                if let Some(target) = target {
                    let converted = self.layout_set.convert(&word, &self.layout, &target);
                    if self.anti_resonance.allow(&word, &converted, now) {
                        info!(
                            "AUDIT: Mid-word converting '{}' ({}) -> {}",
                            word, self.layout, target
                        );
                        self.replace_text(
                            &word,
                            &converted,
                            Some(&target),
                            None,
                            false,
                            false,
                            now,
                            actions,
                        );
                    }
                }
            }
        }
    }

    // ── auto-detection ──────────────────────────────────────────────────

    /// Python `_maybe_auto`.
    fn maybe_auto(
        &mut self,
        word: &str,
        separator_kc: Option<u16>,
        separator_shift: bool,
        now: f64,
        actions: &mut Vec<Action>,
    ) {
        // Refresh per-app context lazily, only when a decision is about to be
        // made (the provider caches, so this is cheap).
        self.refresh_front_app(now);

        if self.should_skip_auto() {
            return;
        }

        // "Don't correct after layout change": grace period after any
        // manual/programmatic layout switch.
        if self.config.no_correct_after_switch > 0.0
            && now - self.layout_changed_at < self.config.no_correct_after_switch
        {
            return;
        }

        // Double caps fix
        if let Some(fix) = ps_core::text::check_double_caps(word) {
            self.replace_text(
                word,
                &fix,
                None,
                separator_kc,
                separator_shift,
                false,
                now,
                actions,
            );
            self.stats
                .record_auto_at(now, ps_core::text::changed_char_count(word, &fix) as i64);
            return;
        }

        let target = self.layout_set.find_best_layout(
            word,
            &self.layout,
            self.config.threshold,
            self.config.min_word_len,
        );
        if let Some(target) = target {
            let converted = self.layout_set.convert(word, &self.layout, &target);
            if !self.anti_resonance.allow(word, &converted, now) {
                return;
            }
            info!(
                "AUDIT: Converting '{}' ({}) -> {}",
                word, self.layout, target
            );
            self.replace_text(
                word,
                &converted,
                Some(&target),
                separator_kc,
                separator_shift,
                false,
                now,
                actions,
            );
            self.stats.record_auto_at(
                now,
                ps_core::text::changed_char_count(word, &converted) as i64,
            );
            debug!("auto-detected {:?} → {:?}", word, converted);
        }
    }

    // ── snippet expansion ───────────────────────────────────────────────

    /// Python `_expand_snippet`.
    fn expand_snippet(
        &mut self,
        trigger: &str,
        expansion: &str,
        separator_kc: u16,
        now: f64,
        actions: &mut Vec<Action>,
    ) {
        info!("AUDIT: Snippet '{trigger}' -> '{expansion}'");

        self.injecting = true;
        self.queued_events.clear();
        // Erase trigger + the separator key that was already sent.
        let erase_count = trigger.chars().count() + 1;
        for _ in 0..erase_count {
            actions.push(inject(KEY_BACKSPACE, 1, 5));
            actions.push(inject(KEY_BACKSPACE, 0, 5));
        }

        // Type the expansion using keycodes in the current layout.
        for ch in expansion.chars() {
            if let Some((kc, shifted)) = self.layout_set.char_to_keycode(ch, &self.layout) {
                if shifted {
                    actions.push(inject(KEY_LEFTSHIFT, 1, 5));
                }
                actions.push(inject(kc, 1, 8));
                actions.push(inject(kc, 0, 8));
                if shifted {
                    actions.push(inject(KEY_LEFTSHIFT, 0, 5));
                }
            }
        }

        // Re-send the separator (space/enter/tab).
        actions.push(inject(separator_kc, 1, 10));
        actions.push(inject(separator_kc, 0, 0));

        self.buf.clear();
        self.sentence.clear();
        self.sound("message-new-instant", actions);
        self.stats.record_snippet_at(now);

        self.end_injection(now, actions);
    }

    // ── text replacement ────────────────────────────────────────────────

    /// Python `_replace_text`.
    #[allow(clippy::too_many_arguments)]
    fn replace_text(
        &mut self,
        old_word: &str,
        new_word: &str,
        target_layout: Option<&str>,
        separator_kc: Option<u16>,
        separator_shift: bool,
        is_undo: bool,
        now: f64,
        actions: &mut Vec<Action>,
    ) {
        self.injecting = true;
        self.queued_events.clear();

        // Release all modifiers to prevent ⌥⌫ / ⇧⌫ (no pacing in Python).
        for code in [KEY_LEFTSHIFT, KEY_RIGHTSHIFT, KEY_LEFTCTRL, KEY_LEFTALT] {
            actions.push(inject(code, 0, 0));
        }

        // Erase the original word (+ the separator, if one was typed).
        let backspaces = old_word.chars().count() + usize::from(separator_kc.is_some());
        for _ in 0..backspaces {
            actions.push(inject(KEY_BACKSPACE, 1, 5));
            actions.push(inject(KEY_BACKSPACE, 0, 5));
        }

        // Switch layout if needed (before typing).
        let old_layout = self.layout.clone();
        let layout_for_typing: String = match target_layout {
            Some(target) if *target != self.layout => {
                actions.extend(self.set_layout(target, now));
                actions.push(Action::Pause { ms: 100 });
                target.to_string()
            }
            _ => self.layout.clone(),
        };

        // Type the converted text using keycodes.
        for ch in new_word.chars() {
            if let Some((kc, shifted)) = self.layout_set.char_to_keycode(ch, &layout_for_typing) {
                if shifted {
                    actions.push(inject(KEY_LEFTSHIFT, 1, 5));
                }
                actions.push(inject(kc, 1, 10));
                actions.push(inject(kc, 0, 10));
                if shifted {
                    actions.push(inject(KEY_LEFTSHIFT, 0, 5));
                }
            }
        }

        // Re-send separator if needed (preserving Shift for shifted
        // punctuation such as '?' or ':').
        if let Some(sep) = separator_kc {
            if separator_shift {
                actions.push(inject(KEY_LEFTSHIFT, 1, 5));
            }
            actions.push(inject(sep, 1, 10));
            actions.push(inject(sep, 0, 0));
            if separator_shift {
                actions.push(inject(KEY_LEFTSHIFT, 0, 5));
            }
        }

        self.buf.clear();
        if !is_undo {
            self.undo.push_full_at(
                now,
                old_word,
                new_word,
                &old_layout,
                separator_kc,
                separator_shift,
            );
            self.sentence.clear();
            for ch in new_word.chars() {
                self.sentence.append(&ch.to_string());
            }
            if let Some(sep) = separator_kc {
                let sep_char =
                    self.layout_set
                        .keycode_to_char(sep, &layout_for_typing, separator_shift);
                match sep_char {
                    Some(c) => self.sentence.append(&c.to_string()),
                    None if sep == KEY_SPACE => self.sentence.append(" "),
                    None if sep == KEY_ENTER || sep == KEY_KPENTER => self.sentence.append("\n"),
                    None => {}
                }
            }
        } else {
            self.sentence.clear();
        }

        self.sound("message-new-instant", actions);
        self.end_injection(now, actions);
    }

    /// Python `_replace_text`'s `finally` block: replay queued physical
    /// events, then start the 50 ms blackout.
    fn end_injection(&mut self, now: f64, actions: &mut Vec<Action>) {
        let to_replay: Vec<RawEvent> = self.queued_events.drain(..).collect();
        self.injecting = false;
        self.injecting_until = 0.0;
        for ev in to_replay {
            if ev.type_ == EV_KEY {
                actions.extend(self.on_event(ev, now));
            }
        }
        self.injecting_until = now + INJECTION_COOLDOWN;
    }

    // ── manual convert (double-shift) ───────────────────────────────────

    /// Python `_do_manual_convert`.
    fn do_manual_convert(&mut self, now: f64) -> Vec<Action> {
        let mut actions = Vec::new();
        let text = self.sentence.text();
        if text.trim().is_empty() {
            // Empty buffer — try undo
            if self.config.undo_words > 0 {
                let entries = self.undo.get_undo_words_at(now, self.config.undo_words);
                if !entries.is_empty() {
                    self.undo_entries(&entries, now, &mut actions);
                    return actions;
                }
            } else if self.undo.last().is_some() {
                if let Some(entry) = self.undo.pop_last() {
                    self.learn_from_undo(&entry);
                    self.replace_text(
                        &entry.converted,
                        &entry.original,
                        Some(&entry.layout),
                        entry.separator_keycode,
                        entry.separator_shift,
                        true,
                        now,
                        &mut actions,
                    );
                    self.stats.record_undo_at(now);
                    return actions;
                }
            }
            return actions;
        }

        // Manual (re)conversion of what is on screen.
        let (prefix, text, suffix) = if self.config.manual_only_last_word {
            ps_core::text::split_last_word(&text)
        } else {
            (String::new(), text, String::new())
        };

        let last_word = text.split_whitespace().last().map(|w| w.to_string());
        let target = last_word.and_then(|w| {
            self.layout_set
                .find_best_layout(&w, &self.layout, -100.0, 1)
        });
        let target = match target {
            Some(t) => t,
            None => {
                let idx = self
                    .layout_set
                    .layouts
                    .iter()
                    .position(|l| *l == self.layout)
                    .unwrap_or(0);
                let n = self.layout_set.layouts.len();
                self.layout_set.layouts[(idx + 1) % n].clone()
            }
        };

        let converted = self.layout_set.convert(&text, &self.layout, &target);
        // When only the last word is converted we must retype the whole
        // sentence prefix so the cursor lands in the right place.
        if self.config.manual_only_last_word {
            let old = format!("{prefix}{text}{suffix}");
            let new = format!("{prefix}{converted}{suffix}");
            self.replace_text(
                &old,
                &new,
                Some(&target),
                None,
                false,
                false,
                now,
                &mut actions,
            );
        } else {
            self.replace_text(
                &text,
                &converted,
                Some(&target),
                None,
                false,
                false,
                now,
                &mut actions,
            );
        }
        let changed = ps_core::text::changed_char_count(&text, &converted) as i64;
        self.stats.record_manual_at(now, changed);
        actions
    }

    /// Python `_learn_from_undo`.
    fn learn_from_undo(&mut self, entry: &ps_core::UndoEntry) {
        if !self.config.self_learning {
            return;
        }
        let original = &entry.original;
        // Only learn plain alphabetic words (never URLs, code, etc.).
        if !original.is_empty()
            && original.chars().all(|c| c.is_alphabetic())
            && original.chars().count() >= 2
        {
            self.layout_set.add_learned(original);
            info!("AUDIT: learned word '{original}' (user reverted)");
            info!("Learned word: {:?}", original);
        }
    }

    /// Python `_undo_entries` — undo multiple entries, reverse order.
    fn undo_entries(
        &mut self,
        entries: &[ps_core::UndoEntry],
        now: f64,
        actions: &mut Vec<Action>,
    ) {
        if entries.is_empty() {
            return;
        }
        for entry in entries.iter().rev() {
            self.learn_from_undo(entry);
            self.replace_text(
                &entry.converted,
                &entry.original,
                Some(&entry.layout),
                entry.separator_keycode,
                entry.separator_shift,
                true,
                now,
                actions,
            );
        }
        self.stats.record_undo_at(now);
    }
}

fn inject(code: u16, value: i32, delay_ms: u64) -> Action {
    Action::Inject {
        code,
        value,
        delay_ms,
    }
}

fn is_shift(kc: u16) -> bool {
    kc == KEY_LEFTSHIFT || kc == KEY_RIGHTSHIFT
}

fn default_snippets_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home)
        .join(".config")
        .join("power_switcher")
        .join("snippets.json")
}
