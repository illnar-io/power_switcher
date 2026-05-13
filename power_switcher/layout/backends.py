import json
import logging
import os
import re
import subprocess
from abc import ABC, abstractmethod

from evdev import ecodes as e

logger = logging.getLogger(__name__)


class LayoutBackend(ABC):
    @abstractmethod
    def get_current_index(self) -> int: ...

    @abstractmethod
    def get_layout_count(self) -> int: ...

    @abstractmethod
    def set_layout(self, index: int) -> None: ...

    def switch_next(self) -> None:
        count = self.get_layout_count()
        if count >= 2:
            self.set_layout((self.get_current_index() + 1) % count)


class KDELayoutBackend(LayoutBackend):
    """
    KDE Plasma 5.12+ on Wayland only.
    Uses org.kde.keyboard D-Bus interface (no extension needed).
    """
    _DEST = "org.kde.keyboard"
    _PATH = "/Layouts"
    _IFACE = "org.kde.KeyboardLayouts"

    def _gdbus(self, method: str, *args) -> str:
        cmd = ["gdbus", "call", "--session",
               "--dest", self._DEST, "--object-path", self._PATH,
               "--method", f"{self._IFACE}.{method}", *[str(a) for a in args]]
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=2)
        return r.stdout.strip()

    def get_current_index(self) -> int:
        raw = self._gdbus("getLayout")
        m = re.search(r'\d+', raw)
        return int(m.group()) if m else 0

    def get_layout_count(self) -> int:
        raw = self._gdbus("getLayoutsList")
        return len(re.findall(r"'shortName'", raw))

    def set_layout(self, index: int) -> None:
        self._gdbus("setLayout", index)

    @classmethod
    def is_available(cls) -> bool:
        try:
            r = subprocess.run(
                ["gdbus", "introspect", "--session",
                 "--dest", cls._DEST, "--object-path", cls._PATH],
                capture_output=True, timeout=2)
            return r.returncode == 0
        except Exception:
            return False


class GNOMESwitchBackend(LayoutBackend):
    """
    GNOME Shell on Wayland. Switches layouts by reading/writing
    org.gnome.desktop.input-sources and simulating the switch key.
    """
    def __init__(self):
        self._current_index = self._read_current_from_gsettings()
        self._layout_count = self._read_count_from_gsettings()
        self._uinput = None

    def set_uinput(self, uinput) -> None:
        """Inject a UInput device reference for key simulation."""
        self._uinput = uinput

    def _read_count_from_gsettings(self) -> int:
        try:
            r = subprocess.run(
                ["gsettings", "get", "org.gnome.desktop.input-sources", "sources"],
                capture_output=True, text=True, timeout=2,
            )
            raw = r.stdout.strip()
            if raw.startswith("@a(ss)") or raw == "":
                return self._count_from_system()
            items = re.findall(r"\('[^']+', '[^']+'\)", raw)
            return max(len(items), 1)
        except Exception:
            return self._count_from_system()

    def _count_from_system(self) -> int:
        try:
            r = subprocess.run(
                ["localectl", "status"], capture_output=True, text=True, timeout=2,
            )
            layout_line = ""
            for line in r.stdout.splitlines():
                if "X11 Layout" in line:
                    layout_line = line.split(":", 1)[1].strip()
                    break
            if layout_line:
                return max(len(layout_line.split(",")), 1)
        except Exception:
            pass
        return 2

    def _read_current_from_gsettings(self) -> int:
        try:
            r = subprocess.run(
                ["gsettings", "get", "org.gnome.desktop.input-sources", "current"],
                capture_output=True, text=True, timeout=2,
            )
            raw = r.stdout.strip()
            if "uint32" in raw:
                m = re.search(r'uint32\s+(\d+)', raw)
            else:
                m = re.search(r'(\d+)', raw)
            return int(m.group(1)) if m else 0
        except Exception:
            return 0

    def get_current_index(self) -> int:
        return self._current_index

    def get_layout_count(self) -> int:
        return self._layout_count

    def set_layout(self, index: int) -> None:
        if index == self._current_index:
            return
        steps = (index - self._current_index) % self._layout_count
        for _ in range(steps):
            self._emit_switch()

    def switch_next(self) -> None:
        self._emit_switch()

    def _emit_switch(self) -> None:
        if self._uinput is None:
            logger.warning("GNOMESwitchBackend: no UInput device, cannot switch layout")
            self._current_index = (self._current_index + 1) % self._layout_count
            return

        self._uinput.write(e.EV_KEY, e.KEY_LEFTMETA, 1)
        self._uinput.write(e.EV_KEY, e.KEY_SPACE, 1)
        self._uinput.syn()
        import time; time.sleep(0.01)
        self._uinput.write(e.EV_KEY, e.KEY_SPACE, 0)
        self._uinput.write(e.EV_KEY, e.KEY_LEFTMETA, 0)
        self._uinput.syn()

        self._current_index = (self._current_index + 1) % self._layout_count

    @classmethod
    def is_available(cls) -> bool:
        desktop = os.environ.get("XDG_CURRENT_DESKTOP", "").upper()
        if not any(x in desktop for x in ("GNOME", "UBUNTU", "UNITY", "POP")):
            return False
        try:
            r = subprocess.run(
                ["gdbus", "introspect", "--session",
                 "--dest", "org.gnome.Shell", "--object-path", "/org/gnome/Shell"],
                capture_output=True, timeout=2,
            )
            return r.returncode == 0
        except Exception:
            return False


class SwayLayoutBackend(LayoutBackend):
    def get_current_index(self) -> int:
        try:
            r = subprocess.run(["swaymsg", "-t", "get_inputs"], capture_output=True, text=True, timeout=1)
            data = json.loads(r.stdout)
            for item in data:
                if item.get("type") == "keyboard" and "current_layout_index" in item:
                    return item["current_layout_index"]
        except Exception:
            pass
        return 0

    def get_layout_count(self) -> int:
        try:
            r = subprocess.run(["swaymsg", "-t", "get_inputs"], capture_output=True, text=True, timeout=1)
            data = json.loads(r.stdout)
            for item in data:
                if item.get("type") == "keyboard" and "xkb_layout_names" in item:
                    return len(item["xkb_layout_names"])
        except Exception:
            pass
        return 2

    def set_layout(self, index: int) -> None:
        subprocess.run(["swaymsg", "input", "type:keyboard", "xkb_switch_layout", str(index)])

    @classmethod
    def is_available(cls) -> bool:
        return os.environ.get("SWAYSOCK") is not None or "SWAY" in os.environ.get("XDG_CURRENT_DESKTOP", "").upper()


class HyprlandLayoutBackend(LayoutBackend):
    def get_current_index(self) -> int:
        try:
            r = subprocess.run(["hyprctl", "devices", "-j"], capture_output=True, text=True, timeout=1)
            data = json.loads(r.stdout)
            kbds = data.get("keyboards", [])
            for kbd in kbds:
                if kbd.get("main"):
                    return kbd.get("active_keymap_index", 0)
            if kbds:
                return kbds[0].get("active_keymap_index", 0)
        except Exception:
            pass
        return 0

    def get_layout_count(self) -> int:
        return 2

    def set_layout(self, index: int) -> None:
        subprocess.run(["hyprctl", "switchxkblayout", "all", str(index)])

    @classmethod
    def is_available(cls) -> bool:
        return os.environ.get("HYPRLAND_INSTANCE_SIGNATURE") is not None


class GNOMEDBusLayoutBackend(LayoutBackend):
    _DEST = "org.gnome.Shell"
    _PATH = "/org/gnome/Shell"
    _IFACE = "org.gnome.Shell"

    def _eval_js(self, js: str) -> str:
        cmd = ["gdbus", "call", "--session",
               "--dest", self._DEST, "--object-path", self._PATH,
               "--method", f"{self._IFACE}.Eval", js]
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=5)
        return r.stdout.strip()

    def get_current_index(self) -> int:
        js = "imports.ui.status.keyboard.getInputSourceManager().currentSource.index"
        raw = self._eval_js(js)
        m = re.search(r'\d+', raw)
        return int(m.group()) if m else 0

    def get_layout_count(self) -> int:
        js = "imports.ui.status.keyboard.getInputSourceManager().inputSources.size"
        raw = self._eval_js(js)
        m = re.search(r'\d+', raw)
        return int(m.group()) if m else 1

    def set_layout(self, index: int) -> None:
        js = f"imports.ui.status.keyboard.getInputSourceManager().inputSources[{index}].activate()"
        self._eval_js(js)

    @classmethod
    def is_available(cls) -> bool:
        try:
            r = subprocess.run(
                ["gdbus", "call", "--session", "--dest", cls._DEST, "--object-path", cls._PATH,
                 "--method", f"{cls._IFACE}.Eval", "1+1"],
                capture_output=True, text=True, timeout=5,
            )
            return "(true," in r.stdout
        except Exception:
            return False


class GNOMELayoutBackend(LayoutBackend):
    def _g3(self, *args) -> str:
        r = subprocess.run(["g3kb-switch", *args], capture_output=True, text=True, timeout=2)
        return r.stdout.strip()

    def get_current_index(self) -> int:
        layouts = self._g3("-l").splitlines()
        current = self._g3("-p")
        try: return layouts.index(current)
        except ValueError: return 0

    def get_layout_count(self) -> int:
        return len(self._g3("-l").splitlines())

    def set_layout(self, index: int) -> None:
        layouts = self._g3("-l").splitlines()
        if index < len(layouts):
            self._g3("-s", layouts[index])

    @classmethod
    def is_available(cls) -> bool:
        try:
            return subprocess.run(["g3kb-switch", "-p"], capture_output=True, timeout=2).returncode == 0
        except FileNotFoundError:
            return False
