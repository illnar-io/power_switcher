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
    def current_word(self) -> str:
        """Alias for text — the word currently being typed."""
        return self.text

    @property
    def length(self) -> int:
        return len(self.word)

    @property
    def keycodes(self) -> list[int]:
        return [kc for kc, _ in self.word]


@dataclass
class SentenceBuffer:
    """
    Tracks the current sentence (multiple words with separators).
    Supports soft reset (preserve current word, clear history) and hard reset.
    """
    _parts: list[str] = field(default_factory=list)
    _group_history: list[str] = field(default_factory=list)

    @property
    def text(self) -> str:
        return "".join(self._parts)

    @property
    def current_word(self) -> str:
        """The current word being typed (last contiguous run of non-separator chars)."""
        if not self._parts:
            return ""
        last = self._parts[-1]
        if last in (" ", "\n", "\t"):
            return ""
        return last

    def append(self, char: str):
        self._parts.append(char)

    def pop(self):
        if self._parts:
            self._parts.pop()

    def clear(self):
        self._parts.clear()
        self._group_history.clear()

    def soft_reset(self):
        """
        Soft context reset: clear word history but keep the current word.
        Used on app switch (cursor didn't move).
        """
        word = self.current_word
        self._parts.clear()
        self._group_history.clear()
        if word:
            self._parts = list(word)

    def invalidate_group_history(self):
        self._group_history.clear()
