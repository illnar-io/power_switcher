import tempfile
from pathlib import Path
from power_switcher.core.snippets import SnippetStore, _canonical


def test_canonical_layout_independent():
    assert _canonical("!test") == _canonical("!еуые")
    assert _canonical("!TEST") == _canonical("!test")


def test_canonical_lowercase():
    assert _canonical("Hello") == "hello"
    assert _canonical("WORLD") == "world"


def test_add_and_match():
    with tempfile.TemporaryDirectory() as td:
        path = Path(td) / "snippets.json"
        store = SnippetStore(path=path)
        store.add("!ee", "user@example.com")
        assert store.expansion("!ee") == "user@example.com"
        assert store.expansion("!EE") == "user@example.com"
        # When typing !ee while in Russian layout, physical keys produce !уу
        assert store.expansion("!уу") == "user@example.com"


def test_no_match():
    with tempfile.TemporaryDirectory() as td:
        path = Path(td) / "snippets.json"
        store = SnippetStore(path=path)
        store.add("!ee", "user@example.com")
        assert store.expansion("something") is None


def test_remove():
    with tempfile.TemporaryDirectory() as td:
        path = Path(td) / "snippets.json"
        store = SnippetStore(path=path)
        store.add("!ee", "user@example.com")
        assert store.remove("!ee") is True
        assert store.expansion("!ee") is None
        assert store.remove("!ee") is False


def test_persistence():
    with tempfile.TemporaryDirectory() as td:
        path = Path(td) / "snippets.json"
        store1 = SnippetStore(path=path)
        store1.add("!ee", "user@example.com")
        store1.add("btw", "by the way")

        store2 = SnippetStore(path=path)
        assert len(store2) == 2
        assert store2.expansion("!ee") == "user@example.com"
        assert store2.expansion("btw") == "by the way"


def test_set_all():
    with tempfile.TemporaryDirectory() as td:
        path = Path(td) / "snippets.json"
        store = SnippetStore(path=path)
        store.add("!old", "old expansion")
        store.set_all([("!new1", "exp1"), ("!new2", "exp2")])
        assert len(store) == 2
        assert store.expansion("!old") is None
        assert store.expansion("!new1") == "exp1"


def test_pairs():
    with tempfile.TemporaryDirectory() as td:
        path = Path(td) / "snippets.json"
        store = SnippetStore(path=path)
        store.add("a", "1")
        store.add("b", "2")
        pairs = store.pairs()
        assert len(pairs) == 2
        assert pairs[0] == ("a", "1")
        assert pairs[1] == ("b", "2")


def test_layout_independent_matching():
    with tempfile.TemporaryDirectory() as td:
        path = Path(td) / "snippets.json"
        store = SnippetStore(path=path)
        store.add("!ghbdtn", "привет")
        # "ghbdtn" typed in EN layout maps to "привет" in RU
        # But the trigger "!ghbdtn" should match regardless of current layout
        assert store.expansion("!ghbdtn") == "привет"
