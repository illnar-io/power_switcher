import os
from dataclasses import dataclass, field
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib


@dataclass
class Config:
    hotkey_convert:    str   = "alt+pause"
    hotkey_selection:  str   = "ctrl+alt+z"
    layouts:           list[str] = field(default_factory=lambda: ["en", "ru"])
    default_layout:    str   = "en"
    auto_detect:       bool  = True
    threshold:         float = 2.5
    min_word_len:      int   = 3
    undo_window:       float = 3.0

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
        
        # Support both 'layouts' and legacy 'layout_pair'
        c.layouts = g.get("layouts", g.get("layout_pair", c.layouts))
        
        c.default_layout   = g.get("default_layout",    c.layouts[0])
        c.auto_detect      = g.get("auto_detect",       c.auto_detect)
        det = d.get("detection", {})
        c.threshold    = det.get("threshold",     c.threshold)
        c.min_word_len = det.get("min_word_len",  c.min_word_len)
        return c
