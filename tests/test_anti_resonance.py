import time
from power_switcher.core.anti_resonance import AntiResonanceGuard


def test_allow_first_conversion():
    g = AntiResonanceGuard()
    assert g.allow("hello", "привет") is True


def test_detect_oscillation():
    g = AntiResonanceGuard(freeze_seconds=0.1)
    assert g.allow("hello", "привет") is True
    # "привет" → "hello" is the reverse of the last conversion
    assert g.allow("привет", "hello") is False


def test_freeze_expires():
    g = AntiResonanceGuard(freeze_seconds=0.05)
    g.allow("a", "b")
    g.allow("b", "a")  # oscillation → frozen
    assert g.is_frozen is True
    time.sleep(0.1)
    assert g.is_frozen is False
    assert g.allow("c", "d") is True


def test_detect_repeated_conversion():
    g = AntiResonanceGuard(freeze_seconds=0.1)
    assert g.allow("hello", "привет") is True
    assert g.allow("hello", "привет") is False  # same conversion repeated


def test_reset():
    g = AntiResonanceGuard()
    g.allow("a", "b")
    g.reset()
    assert g.is_frozen is False
    assert len(g._history) == 0


def test_different_words_allowed():
    g = AntiResonanceGuard()
    assert g.allow("hello", "привет") is True
    assert g.allow("world", "мир") is True
