"""Tests for active-window detection (parsers + provider caching)."""
import json

import pytest

from power_switcher import focus
from power_switcher.focus import (
    ActiveWindowProvider,
    _clean,
    _parse_gdbus_string,
    _parse_hyprland,
    _parse_key_value,
    _parse_sway_tree,
)


@pytest.mark.parametrize("raw,expected", [
    ("(null)", ""),
    ("", ""),
    ("null", ""),
    ("None", ""),
    ("unknown", ""),
    ("kitty", "kitty"),
    ("org.gnome.Terminal", "org.gnome.Terminal"),
    ("  firefox  ", "firefox"),
    ("XGetClassHint[WM_CLASS] failed (code=1)", ""),
    ("Error: something", ""),
    ("foo\nbar", "foo"),
])
def test_clean(raw, expected):
    assert _clean(raw) == expected


def test_parse_sway_tree_focused_con():
    tree = {
        "type": "root",
        "nodes": [
            {
                "type": "output",
                "nodes": [
                    {
                        "type": "workspace",
                        "nodes": [
                            {
                                "type": "con",
                                "focused": False,
                                "app_id": "firefox",
                            },
                            {
                                "type": "con",
                                "focused": True,
                                "app_id": "org.gnome.Terminal",
                            },
                        ],
                    }
                ],
            }
        ],
    }
    assert _parse_sway_tree(tree) == "org.gnome.Terminal"


def test_parse_sway_tree_xwayland_class_fallback():
    tree = {
        "type": "con",
        "focused": True,
        "app_id": None,
        "window_properties": {"class": "Steam"},
    }
    assert _parse_sway_tree(tree) == "Steam"


def test_parse_sway_tree_floating():
    tree = {
        "type": "root",
        "nodes": [],
        "floating_nodes": [
            {"type": "floating_con", "focused": True, "app_id": "mpv"},
        ],
    }
    assert _parse_sway_tree(tree) == "mpv"


def test_parse_sway_tree_nothing_focused():
    tree = {"type": "root", "nodes": [{"type": "con", "focused": False}]}
    assert _parse_sway_tree(tree) == ""


def test_parse_hyprland():
    assert _parse_hyprland(json.dumps({"class": "kitty"})) == "kitty"
    assert _parse_hyprland("{}") == ""
    assert _parse_hyprland("not json") == ""


def test_parse_gdbus_string():
    assert _parse_gdbus_string("('org.gnome.Terminal',)") == "org.gnome.Terminal"
    assert _parse_gdbus_string("('',)") == ""
    assert _parse_gdbus_string("()") == ""


def test_parse_key_value():
    out = "caption: Some window\nresourceClass: konsole\nresourceName: konsole\n"
    assert _parse_key_value(out, "resourceClass") == "konsole"
    assert _parse_key_value(out, "missing") == ""


def test_provider_caches_until_ttl(monkeypatch):
    provider = ActiveWindowProvider(ttl=10.0)
    calls = []

    def fake():
        calls.append(1)
        return "term"

    provider._strategies = [fake]
    assert provider.get_class() == "term"
    assert provider.get_class() == "term"
    assert len(calls) == 1
    # force=True must bypass the cache.
    assert provider.get_class(force=True) == "term"
    assert len(calls) == 2


def test_provider_falls_through_failing_strategies():
    provider = ActiveWindowProvider(ttl=0.0)

    def boom():
        raise RuntimeError("no compositor")

    def good():
        return "kitty"

    provider._strategies = [boom, good]
    assert provider.get_class() == "kitty"


def test_provider_returns_empty_when_all_fail():
    provider = ActiveWindowProvider(ttl=0.0)
    provider._strategies = [lambda: "", lambda: ""]
    assert provider.get_class() == ""


def test_get_active_window_class_uses_default_provider(monkeypatch):
    provider = ActiveWindowProvider(ttl=0.0)
    provider._strategies = [lambda: "my-app"]
    monkeypatch.setattr(focus, "_default_provider", provider)
    assert focus.get_active_window_class() == "my-app"
