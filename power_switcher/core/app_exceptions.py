"""
Application exceptions: per-app mode control (off/soft/default).
Detects the focused application and applies the appropriate mode.
"""
import json
import logging
from pathlib import Path

logger = logging.getLogger(__name__)

_DEFAULT_EXCEPTIONS_PATH = Path("~/.config/power_switcher/exceptions.json").expanduser()

# Known developer/IDE apps (disable auto-detect in these)
DEV_APP_PATTERNS = [
    "jetbrains", "code", "terminal", "konsole", "alacritty", "kitty",
    "wezterm", "foot", "gnome-terminal", "xfce4-terminal", "lutris",
    "steam", "wine", "dosbox", "virtualbox", "vmware",
]

# Chromium/Electron family (may need special handling)
CHROMIUM_KEYWORDS = [
    "chromium", "chrome", "brave", "electron", "slack", "discord",
    "obsidian", "notion", "spotify", "vscode", "cursor",
]


class AppException:
    def __init__(self, name: str, mode: str, window_class: str = ""):
        self.name = name
        self.mode = mode  # "off", "soft", or "default"
        self.window_class = window_class


class ExceptionStore:
    def __init__(self, path: Path | None = None):
        self._path = path or _DEFAULT_EXCEPTIONS_PATH
        self._exceptions: dict[str, str] = {}
        self._window_class_map: dict[str, str] = {}
        self._load()

    def _load(self):
        if not self._path.exists():
            return
        try:
            data = json.loads(self._path.read_text(encoding="utf-8"))
            if isinstance(data, dict):
                self._exceptions = data
            elif isinstance(data, list):
                for item in data:
                    if isinstance(item, dict) and "name" in item and "mode" in item:
                        self._exceptions[item["name"]] = item["mode"]
        except Exception as exc:
            logger.warning("Failed to load exceptions from %s: %s", self._path, exc)

    def _persist(self):
        self._path.parent.mkdir(parents=True, exist_ok=True)
        self._path.write_text(
            json.dumps(self._exceptions, ensure_ascii=False, indent=2),
            encoding="utf-8",
        )

    def get_mode(self, app_name: str) -> str:
        """Return 'off', 'soft', or 'default' for an app."""
        lower = app_name.lower()
        for pattern, mode in self._exceptions.items():
            if pattern.lower() in lower:
                return mode
        return "default"

    def is_dev_app(self, app_name: str) -> bool:
        lower = app_name.lower()
        return any(pat in lower for pat in DEV_APP_PATTERNS)

    def is_chromium(self, app_name: str) -> bool:
        lower = app_name.lower()
        return any(kw in lower for kw in CHROMIUM_KEYWORDS)

    def add(self, name: str, mode: str = "off"):
        self._exceptions[name] = mode
        self._persist()
        logger.info("Exception added: %s → %s", name, mode)

    def remove(self, name: str) -> bool:
        if name in self._exceptions:
            del self._exceptions[name]
            self._persist()
            logger.info("Exception removed: %s", name)
            return True
        return False

    def set_all(self, exceptions: dict[str, str]):
        self._exceptions = dict(exceptions)
        self._persist()

    def all(self) -> dict[str, str]:
        return dict(self._exceptions)


def get_active_window_class(force: bool = False) -> str:
    """Return the WM class of the currently focused window.

    Delegates to :mod:`power_switcher.focus`, which knows how to query each
    supported compositor and caches the result for a short TTL.  Returns an
    empty string when the focused application cannot be determined, which
    callers must treat as "no per-app override".
    """
    from ..focus import get_active_window_class as _focus
    return _focus(force=force)

