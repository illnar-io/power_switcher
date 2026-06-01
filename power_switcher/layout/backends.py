import json
import logging
import os
import re
import subprocess
import time
from abc import ABC, abstractmethod
from pathlib import Path

from evdev import ecodes as e

logger = logging.getLogger(__name__)

class LayoutBackend(ABC):
    @abstractmethod
    def get_current_index(self) -> int: ...
    @abstractmethod
    def get_layout_count(self) -> int: ...
    @abstractmethod
    def set_layout(self, index: int) -> None: ...

class GNOMESwitchBackend(LayoutBackend):
    def __init__(self):
        self._uinput = None
        self._ibus_addr = self._discover_ibus_addr()
        self._sources = self._read_sources()
        logger.info(f"GNOMESwitchBackend: IBus={self._ibus_addr}, Sources={self._sources}")

    def set_uinput(self, uinput) -> None:
        self._uinput = uinput

    def _read_sources(self) -> list[str]:
        try:
            r = subprocess.run(["dconf", "read", "/org/gnome/desktop/input-sources/sources"], capture_output=True, text=True, timeout=1)
            found = re.findall(r"\('[^']+', '([^']+)'\)", r.stdout)
            if found: return found
        except Exception: pass
        return ["us", "ru"]

    def _read_current(self) -> int:
        try:
            r = subprocess.run(["dconf", "read", "/org/gnome/desktop/input-sources/current"], capture_output=True, text=True, timeout=1)
            # Output is often like "uint32 1"
            m = re.search(r'\s(\d+)', r.stdout)
            return int(m.group(1)) if m else 0
        except Exception: return 0

    def _discover_ibus_addr(self) -> str | None:
        try:
            # Check both cache and config locations
            search_paths = [Path("~/.cache/ibus/bus/").expanduser(), Path("~/.config/ibus/bus/").expanduser()]
            files = []
            for p in search_paths:
                if p.exists(): files.extend(list(p.glob("*")))
            if not files: return None
            
            files.sort(key=lambda p: p.stat().st_mtime, reverse=True)
            for f in files:
                for line in f.read_text().splitlines():
                    if "IBUS_ADDRESS=" in line:
                        addr = line.split("IBUS_ADDRESS=", 1)[1].strip()
                        if "path=" in addr: return addr
        except Exception: pass
        return None

    def get_current_index(self) -> int:
        return self._read_current()

    def get_layout_count(self) -> int:
        return len(self._sources)

    def set_layout(self, index: int) -> None:
        if index >= len(self._sources): return
        
        current = self.get_current_index()
        if current == index:
            return
            
        # GNOME Wayland ignores programmatic switches. Super+Space cycles through layouts.
        # We calculate how many steps forward we need to go in the list.
        steps = (index - current) % len(self._sources)
        
        logger.info(f"GNOMESwitchBackend: Simulating {steps}x Super+Space to reach layout index {index}")
        for _ in range(steps):
            self._simulate_switch()
            time.sleep(0.05)

    def _simulate_switch(self) -> None:
        if self._uinput:
            # Super+Space
            self._uinput.write(e.EV_KEY, e.KEY_LEFTMETA, 1); self._uinput.syn()
            time.sleep(0.01)
            self._uinput.write(e.EV_KEY, e.KEY_SPACE, 1); self._uinput.syn()
            time.sleep(0.02)
            self._uinput.write(e.EV_KEY, e.KEY_SPACE, 0); self._uinput.syn()
            time.sleep(0.01)
            self._uinput.write(e.EV_KEY, e.KEY_LEFTMETA, 0); self._uinput.syn()

    @classmethod
    def is_available(cls) -> bool:
        return "GNOME" in os.environ.get("XDG_CURRENT_DESKTOP", "").upper()

class KDELayoutBackend(LayoutBackend):
    _DEST = "org.kde.keyboard"; _PATH = "/Layouts"; _IFACE = "org.kde.KeyboardLayouts"
    def _gdbus(self, method: str, *args) -> str:
        cmd = ["gdbus", "call", "--session", "--dest", self._DEST, "--object-path", self._PATH, "--method", f"{self._IFACE}.{method}", *[str(a) for a in args]]
        return subprocess.run(cmd, capture_output=True, text=True, timeout=2).stdout.strip()
    def get_current_index(self) -> int:
        m = re.search(r'\d+', self._gdbus("getLayout"))
        return int(m.group()) if m else 0
    def get_layout_count(self) -> int:
        return len(re.findall(r"'shortName'", self._gdbus("getLayoutsList")))
    def set_layout(self, index: int) -> None:
        self._gdbus("setLayout", index)
    @classmethod
    def is_available(cls) -> bool:
        try: return subprocess.run(["gdbus", "introspect", "--session", "--dest", cls._DEST, "--object-path", cls._PATH], capture_output=True, timeout=1).returncode == 0
        except Exception: return False

class SwayLayoutBackend(LayoutBackend):
    def get_current_index(self) -> int:
        try:
            r = subprocess.run(["swaymsg", "-t", "get_inputs"], capture_output=True, text=True, timeout=1)
            for item in json.loads(r.stdout):
                if item.get("type") == "keyboard" and "current_layout_index" in item: return item["current_layout_index"]
        except Exception: pass
        return 0
    def get_layout_count(self) -> int:
        try:
            r = subprocess.run(["swaymsg", "-t", "get_inputs"], capture_output=True, text=True, timeout=1)
            for item in json.loads(r.stdout):
                if item.get("type") == "keyboard" and "xkb_layout_names" in item: return len(item["xkb_layout_names"])
        except Exception: pass
        return 2
    def set_layout(self, index: int) -> None:
        subprocess.run(["swaymsg", "input", "type:keyboard", "xkb_switch_layout", str(index)])
    @classmethod
    def is_available(cls) -> bool: return os.environ.get("SWAYSOCK") is not None

class HyprlandLayoutBackend(LayoutBackend):
    def get_current_index(self) -> int:
        try:
            r = subprocess.run(["hyprctl", "devices", "-j"], capture_output=True, text=True, timeout=1)
            for kbd in json.loads(r.stdout).get("keyboards", []):
                if kbd.get("main"): return kbd.get("active_keymap_index", 0)
        except Exception: pass
        return 0
    def get_layout_count(self) -> int: return 2
    def set_layout(self, index: int) -> None: subprocess.run(["hyprctl", "switchxkblayout", "all", str(index)])
    @classmethod
    def is_available(cls) -> bool: return os.environ.get("HYPRLAND_INSTANCE_SIGNATURE") is not None
