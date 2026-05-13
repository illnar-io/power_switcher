from power_switcher.core.key_buffer import KeyBuffer


def test_add_and_text():
    buf = KeyBuffer()
    buf.add(30, "a")
    buf.add(48, "b")
    assert buf.text == "ab"


def test_backspace():
    buf = KeyBuffer()
    buf.add(30, "a")
    buf.add(48, "b")
    buf.backspace()
    assert buf.text == "a"


def test_flush():
    buf = KeyBuffer()
    buf.add(30, "a")
    buf.add(48, "b")
    result = buf.flush()
    assert result == "ab"
    assert buf.text == ""


def test_clear():
    buf = KeyBuffer()
    buf.add(30, "a")
    buf.add(48, "b")
    buf.clear()
    assert buf.text == ""


def test_max_len():
    buf = KeyBuffer(max_len=2)
    buf.add(30, "a")
    buf.add(48, "b")
    buf.add(46, "c")
    assert buf.text == "ab"
    assert len(buf.word) == 2


def test_keycodes():
    buf = KeyBuffer()
    buf.add(30, "a")
    buf.add(48, "b")
    assert buf.keycodes == [30, 48]


def test_keycodes_tracked_on_backspace():
    buf = KeyBuffer()
    buf.add(30, "a")
    buf.add(48, "b")
    buf.backspace()
    assert buf.keycodes == [30]
