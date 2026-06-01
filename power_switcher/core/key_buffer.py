from dataclasses import dataclass, field
from collections import deque
import re
from evdev import ecodes as e

WORD_SEPARATORS = frozenset({
    e.KEY_SPACE, e.KEY_ENTER, e.KEY_KPENTER, e.KEY_TAB,
    e.KEY_COMMA, e.KEY_DOT, e.KEY_SEMICOLON, e.KEY_SLASH, e.KEY_BACKSLASH,
    e.KEY_LEFTBRACE, e.KEY_RIGHTBRACE, e.KEY_APOSTROPHE, e.KEY_GRAVE
})

BUFFER_RESET_KEYS = frozenset({
    e.KEY_ESC, e.KEY_UP, e.KEY_DOWN, e.KEY_LEFT, e.KEY_RIGHT,
    e.KEY_HOME, e.KEY_END, e.KEY_PAGEUP, e.KEY_PAGEDOWN,
})

@dataclass
class KeyBuffer:
    max_len: int = 100
    word: deque = field(init=False)

    def __post_init__(self):
        self.word = deque()

    def add(self, keycode: int, char: str):
        if len(self.word) < self.max_len:
            self.word.append((keycode, char))

    def backspace(self):
        if self.word:
            self.word.pop()

    def flush(self) -> str:
        text = self.text
        self.word.clear()
        return text

    def clear(self):
        self.word.clear()

    @property
    def text(self) -> str:
        return "".join(c for _, c in self.word)

    @property
    def length(self) -> int:
        return len(self.word)

    @property
    def keycodes(self) -> list[int]:
        return [kc for kc, _ in self.word]
