import time
from dataclasses import dataclass, field
from evdev import ecodes as e

# Modifier categories
SHIFT_KEYS = {e.KEY_LEFTSHIFT, e.KEY_RIGHTSHIFT}
CTRL_KEYS = {e.KEY_LEFTCTRL, e.KEY_RIGHTCTRL}
ALT_KEYS = {e.KEY_LEFTALT, e.KEY_RIGHTALT}
META_KEYS = {e.KEY_LEFTMETA, e.KEY_RIGHTMETA}

@dataclass
class ModifierState:
    shift: bool = False
    ctrl: bool = False
    alt: bool = False
    meta: bool = False
    
    # Timing for Caramba-style logic
    last_shift_press_ts: float = 0.0
    last_shift_release_ts: float = 0.0
    shift_tap_count: int = 0
    
    def update(self, keycode: int, pressed: bool) -> str | None:
        """
        Updates state and returns a Caramba action if detected:
        - 'double_shift': Convert last word
        - 'single_shift': Toggle layout
        """
        now = time.time()
        action = None

        if keycode in SHIFT_KEYS:
            if pressed:
                # Caramba: Detect double press
                if (now - self.last_shift_release_ts) < 0.3:
                    self.shift_tap_count += 1
                else:
                    self.shift_tap_count = 1
                self.last_shift_press_ts = now
                self.shift = True
            else:
                hold_duration = now - self.last_shift_press_ts
                self.last_shift_release_ts = now
                self.shift = False
                
                # Heuristic: If it was a quick tap (< 150ms) and no other keys were pressed
                # (handled in KeyProcessor), this might be a toggle or convert.
                if hold_duration < 0.15:
                    if self.shift_tap_count == 2:
                        action = "double_shift"
                        self.shift_tap_count = 0
                    elif self.shift_tap_count == 1:
                        # We return this to KeyProcessor which will decide
                        # if it was a "clean" tap (no other keys)
                        action = "single_shift"
        
        elif keycode in CTRL_KEYS: self.ctrl = pressed
        elif keycode in ALT_KEYS:  self.alt = pressed
        elif keycode in META_KEYS: self.meta = pressed
        
        # Any other key press resets the shift tap count
        if pressed and keycode not in SHIFT_KEYS:
            self.shift_tap_count = 0
            
        return action

    @property
    def is_plain(self) -> bool:
        """True if only Shift is allowed (for typing)."""
        return not (self.ctrl or self.alt or self.meta)

    @property
    def has_modifiers(self) -> bool:
        """True if any control modifier is active."""
        return self.ctrl or self.alt or self.meta
