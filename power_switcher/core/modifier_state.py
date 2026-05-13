from dataclasses import dataclass

from evdev import ecodes as e


@dataclass
class ModifierState:
    shift: bool = False
    ctrl:  bool = False
    alt:   bool = False
    meta:  bool = False

    def update(self, keycode: int, pressed: bool):
        if keycode in (e.KEY_LEFTSHIFT, e.KEY_RIGHTSHIFT):   self.shift = pressed
        elif keycode in (e.KEY_LEFTCTRL,  e.KEY_RIGHTCTRL):  self.ctrl  = pressed
        elif keycode in (e.KEY_LEFTALT,   e.KEY_RIGHTALT):   self.alt   = pressed
        elif keycode in (e.KEY_LEFTMETA,  e.KEY_RIGHTMETA):  self.meta  = pressed

    @property
    def is_plain(self) -> bool:
        """True if no modifiers except possibly Shift."""
        return not (self.ctrl or self.alt or self.meta)
