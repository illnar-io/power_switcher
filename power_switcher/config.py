import os
from dataclasses import dataclass, field
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib


@dataclass
class Config:
    # Manual convert: double-tap Shift always works.  Pause/Break is Caramba's
    # key; the configurable default is a laptop-friendly combo.
    hotkey_convert:    str   = "ctrl+alt+z"
    hotkey_selection:  str   = "alt+shift"
    layouts:           list[str] = field(default_factory=lambda: ["en", "ru"])
    default_layout:    str   = "en"
    auto_detect:       bool  = True
    threshold:         float = 2.5
    min_word_len:      int   = 3
    undo_window:       float = 3.0

    # Snippets
    snippet_expand_space: bool = True
    snippet_expand_enter: bool = True
    snippet_expand_tab:   bool = True

    # Boundary triggers for auto-detection
    trigger_space: bool = True
    trigger_enter: bool = True
    trigger_tab:   bool = True
    trigger_punctuation: bool = True

    # Behavior
    developer_mode:    bool  = False
    arrows_cancel:     bool  = True
    live_fix_enabled:  bool  = True

    # Layout-switch guards & hotkeys
    no_correct_after_switch: float = 1.5
    lr_shift_pause:      bool  = True
    manual_only_last_word: bool = False
    sound_enabled:       bool  = True
    self_learning:       bool  = True
    # Legacy Caramba gesture: a lone Shift tap switches the layout.
    # Off by default — desktops already bind Super+Space for that.
    shift_tap_switch_layout: bool = False

    # Undo
    undo_words:    int   = 0
    undo_seconds:  float = 3.0

    # Paths
    snippets_file: str = ""
    exceptions_file: str = ""
    words_file: str = ""
    stats_file: str = ""

    @classmethod
    def load(cls, path: str) -> "Config":
        p = Path(path)
        if not p.exists():
            return cls()
        with open(p, "rb") as f:
            d = tomllib.load(f)
        c = cls()
        g = d.get("general", {})
        c.hotkey_convert   = g.get("hotkey_convert",   c.hotkey_convert)
        c.hotkey_selection = g.get("hotkey_selection", c.hotkey_selection)
        # Old reserved placeholders (never wired to a key) → Caramba defaults.
        if c.hotkey_convert == "alt+pause":
            c.hotkey_convert = cls.hotkey_convert
        if c.hotkey_selection == "ctrl+alt+z":
            c.hotkey_selection = cls.hotkey_selection

        # Support both 'layouts' and legacy 'layout_pair'
        c.layouts = g.get("layouts", g.get("layout_pair", c.layouts))

        c.default_layout   = g.get("default_layout",    c.layouts[0])
        c.auto_detect      = g.get("auto_detect",       c.auto_detect)
        c.developer_mode   = g.get("developer_mode",    c.developer_mode)
        c.arrows_cancel    = g.get("arrows_cancel",     c.arrows_cancel)
        c.live_fix_enabled = g.get("live_fix_enabled",  c.live_fix_enabled)
        c.lr_shift_pause   = g.get("lr_shift_pause",    c.lr_shift_pause)
        c.no_correct_after_switch = g.get(
            "no_correct_after_switch", c.no_correct_after_switch
        )
        c.manual_only_last_word = g.get(
            "manual_only_last_word", c.manual_only_last_word
        )
        c.sound_enabled    = g.get("sound_enabled",     c.sound_enabled)
        c.self_learning    = g.get("self_learning",     c.self_learning)
        c.shift_tap_switch_layout = g.get(
            "shift_tap_switch_layout", c.shift_tap_switch_layout
        )
        c.snippets_file    = g.get("snippets_file",     c.snippets_file)
        c.exceptions_file  = g.get("exceptions_file",   c.exceptions_file)
        c.words_file       = g.get("words_file",        c.words_file)
        c.stats_file       = g.get("stats_file",        c.stats_file)

        det = d.get("detection", {})
        c.threshold    = det.get("threshold",     c.threshold)
        c.min_word_len = det.get("min_word_len",  c.min_word_len)

        trig = d.get("triggers", {})
        c.trigger_space = trig.get("space", c.trigger_space)
        c.trigger_enter = trig.get("enter", c.trigger_enter)
        c.trigger_tab   = trig.get("tab",   c.trigger_tab)
        c.trigger_punctuation = trig.get("punctuation", c.trigger_punctuation)
        c.snippet_expand_space = trig.get("snippet_space", c.snippet_expand_space)
        c.snippet_expand_enter = trig.get("snippet_enter", c.snippet_expand_enter)
        c.snippet_expand_tab   = trig.get("snippet_tab",   c.snippet_expand_tab)

        und = d.get("undo", {})
        c.undo_words   = und.get("words",   c.undo_words)
        c.undo_seconds = und.get("seconds", c.undo_seconds)

        return c
