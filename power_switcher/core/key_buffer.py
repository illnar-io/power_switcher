from dataclasses import dataclass, field

from evdev import ecodes as e


WORD_SEPARATORS = frozenset({
    e.KEY_SPACE, e.KEY_ENTER, e.KEY_KPENTER, e.KEY_TAB,
    e.KEY_COMMA, e.KEY_DOT, e.KEY_SEMICOLON,
})

BUFFER_RESET_KEYS = frozenset({
    e.KEY_ESC, e.KEY_ENTER, e.KEY_KPENTER,
    e.KEY_UP, e.KEY_DOWN, e.KEY_LEFT, e.KEY_RIGHT,
    e.KEY_HOME, e.KEY_END, e.KEY_PAGEUP, e.KEY_PAGEDOWN,
})

MODIFIER_KEYS = frozenset({
    e.KEY_LEFTSHIFT, e.KEY_RIGHTSHIFT,
    e.KEY_LEFTCTRL,  e.KEY_RIGHTCTRL,
    e.KEY_LEFTALT,   e.KEY_RIGHTALT,
    e.KEY_LEFTMETA,  e.KEY_RIGHTMETA,
})


@dataclass
class KeyBuffer:
    word: list[tuple[int, str]] = field(default_factory=list)
    max_len: int = 50

    def add(self, keycode: int, char: str):
        if len(self.word) < self.max_len:
            self.word.append((keycode, char))

    def backspace(self):
        if self.word:
            self.word.pop()

    def flush(self) -> str:
        text = "".join(c for _, c in self.word)
        self.word.clear()
        return text

    def clear(self):
        self.word.clear()

    @property
    def text(self) -> str:
        return "".join(c for _, c in self.word)

    @property
    def keycodes(self) -> list[int]:
        return [kc for kc, _ in self.word]
