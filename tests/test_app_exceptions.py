import tempfile
from pathlib import Path
from power_switcher.core.app_exceptions import ExceptionStore


def test_default_mode():
    store = ExceptionStore()
    assert store.get_mode("unknown_app") == "default"


def test_add_and_get():
    with tempfile.TemporaryDirectory() as td:
        path = Path(td) / "exceptions.json"
        store = ExceptionStore(path=path)
        store.add("code", "off")
        assert store.get_mode("com.microsoft.VSCode") == "off"
        assert store.get_mode("code-oss") == "off"


def test_remove():
    with tempfile.TemporaryDirectory() as td:
        path = Path(td) / "exceptions.json"
        store = ExceptionStore(path=path)
        store.add("vscode", "off")
        assert store.remove("vscode") is True
        assert store.get_mode("vscode") == "default"
        assert store.remove("vscode") is False


def test_is_dev_app():
    store = ExceptionStore()
    assert store.is_dev_app("Visual Studio Code") is True
    assert store.is_dev_app("jetbrains-idea") is True
    assert store.is_dev_app("Firefox") is False


def test_is_chromium():
    store = ExceptionStore()
    assert store.is_chromium("Google Chrome") is True
    assert store.is_chromium("Chromium") is True
    assert store.is_chromium("Firefox") is False


def test_persistence():
    with tempfile.TemporaryDirectory() as td:
        path = Path(td) / "exceptions.json"
        store1 = ExceptionStore(path=path)
        store1.add("app1", "off")
        store1.add("app2", "soft")

        store2 = ExceptionStore(path=path)
        assert store2.get_mode("app1") == "off"
        assert store2.get_mode("app2") == "soft"


def test_set_all():
    with tempfile.TemporaryDirectory() as td:
        path = Path(td) / "exceptions.json"
        store = ExceptionStore(path=path)
        store.add("old", "off")
        store.set_all({"new1": "off", "new2": "soft"})
        assert store.get_mode("old") == "default"
        assert store.get_mode("new1") == "off"
        assert store.get_mode("new2") == "soft"


def test_all():
    with tempfile.TemporaryDirectory() as td:
        path = Path(td) / "exceptions.json"
        store = ExceptionStore(path=path)
        store.add("a", "off")
        store.add("b", "soft")
        result = store.all()
        assert result == {"a": "off", "b": "soft"}
