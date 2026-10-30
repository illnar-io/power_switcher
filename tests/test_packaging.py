import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DESKTOP = ROOT / "packaging" / "power-switcher.desktop"
GUI = ROOT / "crates" / "ps-gui" / "src" / "main.rs"
ICONS = ROOT / "assets" / "icon" / "hicolor"


def _desktop():
    return {
        key.strip(): value.strip()
        for key, _, value in (
            line.partition("=")
            for line in DESKTOP.read_text().splitlines()
            if "=" in line and not line.startswith("[")
        )
    }


def test_desktop_entry_matches_gtk_app_id():
    # Without this GNOME Shell cannot link the window to the entry, and the
    # dock shows the window without an icon.
    app_id = re.search(r'application_id\("([^"]+)"\)', GUI.read_text()).group(1)
    assert _desktop()["StartupWMClass"] == app_id


def test_desktop_icon_exists():
    icon = _desktop()["Icon"]
    assert (ROOT / "packaging" / f"{icon}.svg").is_file()
    for size in (16, 22, 24, 32, 48, 64, 128, 256, 512):
        assert (ICONS / f"{size}x{size}" / "apps" / f"{icon}.png").is_file()


def test_symbolic_icon_follows_theme():
    svg = (ICONS / "symbolic" / "apps" / "power-switcher-symbolic.svg").read_text()
    assert "currentColor" in svg
    assert not re.search(r'fill="#', svg)
