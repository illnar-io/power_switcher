"""End-to-end tests of the KeyProcessor decision logic.

Real keyboards and UInput devices are unavailable in CI, so the event loop is
driven directly with a recording fake UInput and a fake layout backend.
"""
import asyncio
import tempfile
import time
from pathlib import Path
from types import SimpleNamespace

import pytest
from evdev import ecodes as e

from power_switcher.core.key_processor import KeyProcessor
from power_switcher.core.stats import Stats
from power_switcher.maps.layout_set import LayoutSet

PROFILES_DIR = Path(__file__).parents[1] / "power_switcher" / "detection" / "profiles"


class FakeUInput:
    def __init__(self):
        self.events = []

    def write_event(self, ev):
        self.events.append((ev.type, ev.code, ev.value))

    def write(self, etype, code, value):
        self.events.append((etype, code, value))

    def syn(self):
        pass

    def close(self):
        pass

    def key_codes(self, value=1):
        return [c for (t, c, v) in self.events if t == e.EV_KEY and v == value]

    def backspaces(self):
        return self.key_codes(1).count(e.KEY_BACKSPACE)


class FakeBackend:
    def __init__(self):
        self.index = 0
        self.calls = []

    def set_layout(self, index):
        self.index = index
        self.calls.append(index)

    def get_current_index(self):
        return self.index


def _event(code, value):
    return SimpleNamespace(type=e.EV_KEY, code=code, value=value)


@pytest.fixture
def tmp_state(tmp_path):
    return {
        "snippets": tmp_path / "snippets.json",
        "exceptions": tmp_path / "exceptions.json",
        "words": tmp_path / "words.json",
        "stats": tmp_path / "stats.json",
    }


def make_processor(tmp_state, **overrides):
    layout_set = LayoutSet.create(["en", "ru"], PROFILES_DIR, words_path=tmp_state["words"])
    kwargs = dict(
        layout_backend=FakeBackend(),
        layout_set=layout_set,
        current_layout="en",
        snippets_path=tmp_state["snippets"],
        exceptions_path=tmp_state["exceptions"],
        stats=Stats.load(tmp_state["stats"]),
        sound_enabled=False,
    )
    kwargs.update(overrides)
    proc = KeyProcessor(**kwargs)
    proc._ui = FakeUInput()
    return proc


# ── L+R Shift pause hotkey ──────────────────────────────────────────────


def test_lr_shift_pauses_auto(tmp_state):
    proc = make_processor(tmp_state)

    async def scenario():
        await proc._on_event(_event(e.KEY_LEFTSHIFT, 1))
        await proc._on_event(_event(e.KEY_RIGHTSHIFT, 1))
        await proc._on_event(_event(e.KEY_RIGHTSHIFT, 0))
        await proc._on_event(_event(e.KEY_LEFTSHIFT, 0))

    assert proc._auto_paused is False
    asyncio.run(scenario())
    assert proc._auto_paused is True
    assert proc._should_skip_auto() is True

    # Toggling again resumes.
    asyncio.run(scenario())
    assert proc._auto_paused is False
    assert proc._should_skip_auto() is False


def test_lr_shift_does_not_trigger_manual_convert(tmp_state):
    proc = make_processor(tmp_state)
    called = []

    async def fake_convert():
        called.append(1)

    proc._do_manual_convert = fake_convert  # type: ignore

    async def scenario():
        await proc._on_event(_event(e.KEY_LEFTSHIFT, 1))
        await proc._on_event(_event(e.KEY_RIGHTSHIFT, 1))
        await proc._on_event(_event(e.KEY_RIGHTSHIFT, 0))
        await proc._on_event(_event(e.KEY_LEFTSHIFT, 0))
        assert called == []  # L+R Shift must not count as a double tap
        # A further quick double-tap of one shift must still be detected.
        await proc._on_event(_event(e.KEY_LEFTSHIFT, 1))
        await proc._on_event(_event(e.KEY_LEFTSHIFT, 0))
        await proc._on_event(_event(e.KEY_LEFTSHIFT, 1))
        await proc._on_event(_event(e.KEY_LEFTSHIFT, 0))

    asyncio.run(scenario())
    assert called == [1]


def test_single_shift_toggles_layout(tmp_state):
    proc = make_processor(tmp_state, current_layout="ru")

    async def scenario():
        await proc._on_event(_event(e.KEY_LEFTSHIFT, 1))
        await proc._on_event(_event(e.KEY_LEFTSHIFT, 0))

    asyncio.run(scenario())
    assert proc.layout_backend.index == 0  # layouts[0] == "en"


# ── "don't correct after layout change" guard ───────────────────────────


def test_no_correct_after_switch_guard(tmp_state):
    proc = make_processor(tmp_state, no_correct_after_switch=30.0)
    proc._layout_changed_at = time.time()
    asyncio.run(proc._maybe_auto("ghbdtn", separator_kc=e.KEY_SPACE))
    assert proc._ui.backspaces() == 0


def test_correction_happens_once_guard_expires(tmp_state):
    proc = make_processor(tmp_state, no_correct_after_switch=0.01)
    proc._layout_changed_at = time.time()
    time.sleep(0.02)
    asyncio.run(proc._maybe_auto("ghbdtn", separator_kc=e.KEY_SPACE))
    assert proc._ui.backspaces() == len("ghbdtn") + 1


# ── auto detection + stats ──────────────────────────────────────────────


def test_auto_conversion_records_stats(tmp_state):
    proc = make_processor(tmp_state)
    asyncio.run(proc._maybe_auto("ghbdtn", separator_kc=e.KEY_SPACE))
    assert proc.layout_backend.index == 1  # switched to ru
    assert proc.stats.auto_conversions == 1


def test_correct_word_is_not_converted(tmp_state):
    proc = make_processor(tmp_state)
    asyncio.run(proc._maybe_auto("hello", separator_kc=e.KEY_SPACE))
    assert proc._ui.backspaces() == 0
    assert proc.stats.auto_conversions == 0


# ── learned words ───────────────────────────────────────────────────────


def test_undo_learns_word(tmp_state):
    proc = make_processor(tmp_state)
    proc._undo.push("ghbdtn", "привет", "ru", e.KEY_SPACE)
    asyncio.run(proc._do_manual_convert())
    assert proc.layout_set.is_exception("ghbdtn")
    assert proc.stats.undos == 1
    # A learned word is never auto-converted again.
    asyncio.run(proc._maybe_auto("ghbdtn", separator_kc=e.KEY_SPACE))
    assert proc.stats.auto_conversions == 0


def test_self_learning_disabled(tmp_state):
    proc = make_processor(tmp_state, self_learning=False)
    proc._undo.push("ghbdtn", "привет", "ru", e.KEY_SPACE)
    asyncio.run(proc._do_manual_convert())
    assert not proc.layout_set.is_exception("ghbdtn")


def test_learned_word_persists(tmp_state):
    proc = make_processor(tmp_state)
    proc._undo.push("ghbdtn", "привет", "ru", e.KEY_SPACE)
    asyncio.run(proc._do_manual_convert())
    reloaded = LayoutSet.create(["en", "ru"], PROFILES_DIR, words_path=tmp_state["words"])
    assert reloaded.is_exception("ghbdtn")


# ── helpers ─────────────────────────────────────────────────────────────


@pytest.mark.parametrize("text,expected", [
    ("hello мир", ("hello ", "мир", "")),
    ("hello мир  ", ("hello ", "мир", "  ")),
    ("мир", ("", "мир", "")),
    ("a\tb", ("a\t", "b", "")),
    ("trailing ", ("", "trailing", " ")),
])
def test_split_last_word(text, expected):
    prefix, word, suffix = KeyProcessor._split_last_word(text)
    assert (prefix, word, suffix) == expected
    assert prefix + word + suffix == text


def test_changed_char_count():
    assert KeyProcessor._changed_char_count("abc", "abc") == 0
    assert KeyProcessor._changed_char_count("abc", "abd") == 1
    assert KeyProcessor._changed_char_count("abc", "abcd") == 1
    assert KeyProcessor._changed_char_count("ghbdtn", "привет") == 6


# ── app exceptions ──────────────────────────────────────────────────────


def test_disabled_app_skips_auto(tmp_state, monkeypatch):
    proc = make_processor(tmp_state)
    proc._exceptions.add("terminal", "off")
    monkeypatch.setattr(
        "power_switcher.core.key_processor.get_active_window_class",
        lambda force=False: "org.gnome.Terminal",
    )
    asyncio.run(proc._maybe_auto("ghbdtn", separator_kc=e.KEY_SPACE))
    assert proc._ui.backspaces() == 0


# ── punctuation keys that are letters in another layout ─────────────────


def _type(proc, keycodes):
    for kc in keycodes:
        asyncio.run(proc._update_buf(kc))


def test_ambiguous_separator_keeps_ru_word_intact(tmp_state):
    proc = make_processor(tmp_state, current_layout="ru", live_fix_enabled=False)
    # "работа": р=H а=F б=COMMA о=J т=N а=F.  The comma key must not split it.
    _type(proc, (e.KEY_H, e.KEY_F, e.KEY_COMMA, e.KEY_J, e.KEY_N, e.KEY_F))
    assert proc._buf.text == "работа"


def test_ambiguous_separator_keeps_wrong_layout_word_intact(tmp_state):
    proc = make_processor(tmp_state, current_layout="en", live_fix_enabled=False)
    # "работа" typed in the English layout is "hf,jnf".
    _type(proc, (e.KEY_H, e.KEY_F, e.KEY_COMMA, e.KEY_J, e.KEY_N, e.KEY_F))
    assert proc._buf.text == "hf,jnf"


def test_wrong_layout_word_with_punctuation_key_is_converted(tmp_state):
    proc = make_processor(tmp_state, current_layout="en", live_fix_enabled=False)
    _type(proc, (e.KEY_H, e.KEY_F, e.KEY_COMMA, e.KEY_J, e.KEY_N, e.KEY_F))
    asyncio.run(proc._update_buf(e.KEY_SPACE))
    assert proc.layout_backend.index == 1  # switched to ru
    assert proc.stats.auto_conversions == 1
    assert proc._ui.backspaces() == len("hf,jnf") + 1


def test_local_apostrophe_word_not_converted(tmp_state):
    proc = make_processor(tmp_state, current_layout="en", live_fix_enabled=False)
    # "don't" must be left alone even though the apostrophe key is 'э' in RU.
    _type(proc, (e.KEY_D, e.KEY_J, e.KEY_N, e.KEY_APOSTROPHE, e.KEY_T))
    asyncio.run(proc._update_buf(e.KEY_SPACE))
    assert proc._ui.backspaces() == 0
    assert proc.stats.auto_conversions == 0


# ── punctuation-triggered auto-detection ────────────────────────────────


def test_punctuation_triggers_detection_by_default(tmp_state):
    proc = make_processor(tmp_state, current_layout="en", live_fix_enabled=False)
    # KEY_SLASH is '/' in EN and '.' in RU → an unambiguous separator.
    _type(proc, (e.KEY_G, e.KEY_H, e.KEY_B, e.KEY_D, e.KEY_T, e.KEY_N))
    asyncio.run(proc._update_buf(e.KEY_SLASH))
    assert proc.stats.auto_conversions == 1
    assert proc._ui.backspaces() == len("ghbdtn") + 1


def test_punctuation_trigger_can_be_disabled(tmp_state):
    proc = make_processor(
        tmp_state, current_layout="en", live_fix_enabled=False,
        trigger_punctuation=False,
    )
    _type(proc, (e.KEY_G, e.KEY_H, e.KEY_B, e.KEY_D, e.KEY_T, e.KEY_N))
    asyncio.run(proc._update_buf(e.KEY_SLASH))
    assert proc.stats.auto_conversions == 0
    assert proc._ui.backspaces() == 0


# ── separator shift preservation ────────────────────────────────────────


def test_separator_shift_is_resent(tmp_state):
    proc = make_processor(tmp_state)
    asyncio.run(proc._replace_text(
        "ghbdtn", "привет", "ru",
        separator_kc=e.KEY_SLASH, separator_shift=True,
    ))
    presses = proc._ui.key_codes(1)
    assert e.KEY_SLASH in presses
    # A Shift press must precede the shifted separator.
    assert e.KEY_LEFTSHIFT in presses
    assert presses.index(e.KEY_LEFTSHIFT) < presses.index(e.KEY_SLASH)


def test_separator_without_shift_does_not_press_shift(tmp_state):
    proc = make_processor(tmp_state)
    asyncio.run(proc._replace_text(
        "ghbdtn", "привет", "ru",
        separator_kc=e.KEY_SLASH, separator_shift=False,
    ))
    assert e.KEY_LEFTSHIFT not in proc._ui.key_codes(1)
    assert e.KEY_SLASH in proc._ui.key_codes(1)
