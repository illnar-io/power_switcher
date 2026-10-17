import time
from power_switcher.core.undo_history import UndoHistory


def test_push_and_last():
    h = UndoHistory()
    h.push("hello", "привет", "en")
    assert h.last is not None
    assert h.last.original == "hello"
    assert h.last.converted == "привет"


def test_pop_last():
    h = UndoHistory()
    h.push("hello", "привет", "en")
    entry = h.pop_last()
    assert entry is not None
    assert entry.original == "hello"
    assert h.last is None


def test_get_undo_words():
    h = UndoHistory()
    h.push("a", "b", "en")
    h.push("c", "d", "en")
    h.push("e", "f", "en")
    entries = h.get_undo_words(2)
    assert len(entries) == 2
    assert entries[0].original == "c"
    assert entries[1].original == "e"


def test_get_undo_time():
    h = UndoHistory(window_seconds=1.0)
    h.push("a", "b", "en")
    time.sleep(0.05)
    h.push("c", "d", "en")
    entries = h.get_undo_time(0.1)
    assert len(entries) >= 1


def test_clear():
    h = UndoHistory()
    h.push("a", "b", "en")
    h.clear()
    assert len(h) == 0
    assert h.last is None


def test_max_entries():
    h = UndoHistory(max_entries=3)
    for i in range(5):
        h.push(f"a{i}", f"b{i}", "en")
    assert len(h) == 3
    assert h._entries[0].original == "a2"


def test_separator_keycode():
    h = UndoHistory()
    h.push("hello", "привет", "en", separator_keycode=49)
    assert h.last.separator_keycode == 49
