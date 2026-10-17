"""
Undo history: store conversion pairs with timestamps for flexible undo.
Supports undo by word count or time window.
"""
import time
import logging
from dataclasses import dataclass, field

logger = logging.getLogger(__name__)


@dataclass
class UndoEntry:
    original: str
    converted: str
    layout: str
    timestamp: float
    separator_keycode: int | None = None
    separator_shift: bool = False


class UndoHistory:
    def __init__(self, max_entries: int = 50, window_seconds: float = 3.0):
        self._entries: list[UndoEntry] = []
        self._max_entries = max_entries
        self._window_seconds = window_seconds

    def push(
        self,
        original: str,
        converted: str,
        layout: str,
        separator_keycode: int | None = None,
        separator_shift: bool = False,
    ):
        entry = UndoEntry(
            original=original,
            converted=converted,
            layout=layout,
            timestamp=time.time(),
            separator_keycode=separator_keycode,
            separator_shift=separator_shift,
        )
        self._entries.append(entry)
        if len(self._entries) > self._max_entries:
            self._entries.pop(0)

    def pop_last(self) -> UndoEntry | None:
        if self._entries:
            return self._entries.pop()
        return None

    def get_undo_words(self, n: int) -> list[UndoEntry]:
        """Get last N conversion entries within the time window."""
        now = time.time()
        result = []
        for entry in reversed(self._entries):
            if now - entry.timestamp > self._window_seconds:
                break
            result.append(entry)
            if len(result) >= n:
                break
        result.reverse()
        return result

    def get_undo_time(self, seconds: float | None = None) -> list[UndoEntry]:
        """Get all conversion entries within the last N seconds."""
        if seconds is None:
            seconds = self._window_seconds
        now = time.time()
        result = []
        for entry in reversed(self._entries):
            if now - entry.timestamp > seconds:
                break
            result.append(entry)
        result.reverse()
        return result

    def clear(self):
        self._entries.clear()

    def clear_old(self):
        """Remove entries older than the window."""
        now = time.time()
        self._entries = [
            e for e in self._entries
            if now - e.timestamp <= self._window_seconds
        ]

    @property
    def last(self) -> UndoEntry | None:
        return self._entries[-1] if self._entries else None

    def __len__(self) -> int:
        return len(self._entries)
