"""Active-window detection across Wayland compositors and X11.

Wayland deliberately hides the focused window from unprivileged clients, so
each compositor has to be queried through its own interface:

==========  ==========================================================
Compositor  Mechanism
==========  ==========================================================
Sway        ``swaymsg -t get_tree`` (``focused`` node → ``app_id``)
Hyprland    ``hyprctl activewindow -j`` (``class``)
KDE Plasma  ``kdotool`` then ``qdbus org.kde.KWin``
X11/XWayland``xdotool getactivewindow getwindowclassname``
GNOME       companion extension ``org.power_switcher.Focus`` (see
            ``gnome-extension/``); GNOME exposes no public API for this
==========  ==========================================================

Results are cached for a short TTL so that the per-keystroke decision path
never blocks on spawning a subprocess for every word.
"""

from __future__ import annotations

import json
import logging
import os
import re
import shutil
import subprocess
import time
from pathlib import Path

logger = logging.getLogger(__name__)

_GNOME_EXT_DEST = "org.power_switcher.Focus"
_GNOME_EXT_PATH = "/org/power_switcher/Focus"
_GNOME_EXT_IFACE = "org.power_switcher.Focus"

# External helper binaries, resolved once.  Strategies whose tool is missing are
# never registered, so we do not pay for a failed spawn on every poll.
_TOOLS = {
    "gdbus": shutil.which("gdbus"),
    "swaymsg": shutil.which("swaymsg"),
    "hyprctl": shutil.which("hyprctl"),
    "kdotool": shutil.which("kdotool"),
    "qdbus": shutil.which("qdbus"),
    "xdotool": shutil.which("xdotool"),
}

# Wayland session detection helper (kept for callers/tests).
def _session_is_wayland() -> bool:
    return os.environ.get("XDG_SESSION_TYPE", "").lower() == "wayland"


def _run(cmd: list[str], timeout: float = 1.0) -> str:
    """Run a command and return its stdout, or '' on any failure."""
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
        if r.returncode == 0:
            return r.stdout
    except Exception:
        pass
    return ""


# Values that tools emit when they have no real answer.
_UNKNOWN_VALUES = {"", "(null)", "null", "none", "unknown", "n/a", "nil"}


def _clean(value: str) -> str:
    """Normalise a raw tool answer; return '' when it is not a real class."""
    if not value:
        return ""
    value = value.strip().splitlines()[0].strip() if value.strip() else ""
    if value.lower() in _UNKNOWN_VALUES:
        return ""
    # xdotool can echo diagnostics on stdout.
    if value.startswith("XGetClassHint") or value.startswith("Error"):
        return ""
    return value


# ── per-compositor parsers (pure functions, unit-testable) ──────────────


def _parse_sway_tree(tree: dict) -> str:
    """Return the app_id / WM class of the focused node in a sway tree."""
    def walk(node: dict) -> str | None:
        if node.get("focused") and node.get("type") in ("con", "floating_con"):
            app_id = node.get("app_id")
            if app_id:
                return app_id
            props = node.get("window_properties") or {}
            return props.get("class")
        for child in node.get("nodes", []) + node.get("floating_nodes", []):
            found = walk(child)
            if found:
                return found
        return None

    return walk(tree) or ""


def _parse_hyprland(json_text: str) -> str:
    try:
        data = json.loads(json_text)
    except Exception:
        return ""
    if isinstance(data, dict):
        return data.get("class") or data.get("initialClass") or ""
    return ""


def _parse_gdbus_string(stdout: str) -> str:
    """Extract the first string out of a ``gdbus call`` tuple reply."""
    m = re.search(r"'((?:[^'\\]|\\.)*)'", stdout)
    if not m:
        return ""
    return m.group(1).replace("\\'", "'").replace('\\"', '"')


def _parse_key_value(stdout: str, key: str) -> str:
    """Parse ``key: value`` lines emitted by kdotool / qdbus queryWindowInfo."""
    m = re.search(rf"^\s*{re.escape(key)}\s*:\s*(.+?)\s*$", stdout, re.MULTILINE)
    return m.group(1) if m else ""


# ── provider ────────────────────────────────────────────────────────────


class ActiveWindowProvider:
    """Best-effort provider for the focused window's WM class.

    The compositor is detected once; each poll tries a small ordered list of
    strategies.  Empty string means "unknown" — callers should treat that as
    the default (never-disabled) mode.
    """

    def __init__(self, ttl: float = 1.0):
        self._ttl = ttl
        self._cached = ""
        self._cached_at = 0.0
        self._strategies = self._build_strategies()

    # -- strategy selection ------------------------------------------------

    def _build_strategies(self) -> list:
        desktop = os.environ.get("XDG_CURRENT_DESKTOP", "").upper()
        strategies = []

        def add(name: str, func):
            tool = {
                "_from_sway": "swaymsg",
                "_from_hyprland": "hyprctl",
                "_from_kdotool": "kdotool",
                "_from_kwin_dbus": "qdbus",
                "_from_xdotool": "xdotool",
                "_from_gnome_extension": "gdbus",
            }.get(name)
            if tool and not _TOOLS.get(tool):
                return
            strategies.append(func)

        if os.environ.get("SWAYSOCK"):
            add("_from_sway", self._from_sway)
        if os.environ.get("HYPRLAND_INSTANCE_SIGNATURE"):
            add("_from_hyprland", self._from_hyprland)

        if "GNOME" in desktop or "UBUNTU" in desktop or "POP" in desktop or "UNITY" in desktop:
            add("_from_gnome_extension", self._from_gnome_extension)

        if "KDE" in desktop or "PLASMA" in desktop:
            add("_from_kdotool", self._from_kdotool)
            add("_from_kwin_dbus", self._from_kwin_dbus)

        # Always-available fallbacks, in rough order of reliability.
        add("_from_sway", self._from_sway)
        add("_from_hyprland", self._from_hyprland)
        add("_from_gnome_extension", self._from_gnome_extension)
        add("_from_kdotool", self._from_kdotool)
        add("_from_xdotool", self._from_xdotool)

        # Deduplicate while preserving order.
        seen: set = set()
        unique = []
        for s in strategies:
            if s.__name__ not in seen:
                seen.add(s.__name__)
                unique.append(s)
        return unique

    # -- public API --------------------------------------------------------

    def get_class(self, force: bool = False) -> str:
        now = time.time()
        if not force and now - self._cached_at < self._ttl:
            return self._cached
        for strategy in self._strategies:
            try:
                value = _clean(strategy())
            except Exception as exc:  # never let detection break typing
                logger.debug("focus strategy %s failed: %s", strategy.__name__, exc)
                value = ""
            if value:
                self._cached = value
                self._cached_at = now
                return value
        self._cached = ""
        self._cached_at = now
        return ""

    def invalidate(self):
        self._cached_at = 0.0

    # -- strategies --------------------------------------------------------

    def _from_sway(self) -> str:
        out = _run(["swaymsg", "-t", "get_tree", "-r"])
        if not out:
            return ""
        try:
            return _parse_sway_tree(json.loads(out))
        except Exception:
            return ""

    def _from_hyprland(self) -> str:
        out = _run(["hyprctl", "activewindow", "-j"])
        return _parse_hyprland(out) if out else ""

    def _from_kdotool(self) -> str:
        out = _run(["kdotool", "getactivewindow", "getwindowclassname"])
        return out.strip()

    def _from_kwin_dbus(self) -> str:
        # KWin exposes the active window UUID; getWindowInfo needs the uuid.
        uuid = _run([
            "qdbus", "org.kde.KWin", "/KWin", "org.kde.KWin.activeWindow",
        ]).strip()
        if not uuid:
            return ""
        info = _run(["qdbus", "org.kde.KWin", "/KWin",
                     "org.kde.KWin.getWindowInfo", uuid])
        return _parse_key_value(info, "resourceClass")

    def _from_xdotool(self) -> str:
        out = _run(["xdotool", "getactivewindow", "getwindowclassname"])
        return out.strip()

    def _from_gnome_extension(self) -> str:
        out = _run([
            "gdbus", "call", "--session",
            "--dest", _GNOME_EXT_DEST,
            "--object-path", _GNOME_EXT_PATH,
            "--method", f"{_GNOME_EXT_IFACE}.GetActiveWindow",
        ])
        return _parse_gdbus_string(out) if out else ""


_default_provider: ActiveWindowProvider | None = None


def get_active_window_class(force: bool = False) -> str:
    """Return the focused window's WM class/app-id, or '' if unknown."""
    global _default_provider
    if _default_provider is None:
        _default_provider = ActiveWindowProvider()
    return _default_provider.get_class(force=force)


def extension_available() -> bool:
    """True when the companion GNOME extension answers on the session bus."""
    return bool(_clean(ActiveWindowProvider()._from_gnome_extension()))


def gnome_extension_dir() -> Path:
    """Path to the bundled GNOME Shell extension source."""
    return Path(__file__).resolve().parent.parent / "gnome-extension"
