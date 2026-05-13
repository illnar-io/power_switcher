import asyncio
import logging
import signal
import subprocess
import time

from evdev import InputDevice, UInput, ecodes as e, list_devices

from .key_buffer import KeyBuffer, WORD_SEPARATORS, BUFFER_RESET_KEYS, MODIFIER_KEYS
from .modifier_state import ModifierState
from ..maps.layout_set import LayoutSet
from ..layout.backends import GNOMESwitchBackend

logger = logging.getLogger(__name__)


class KeyProcessor:
    def __init__(
        self,
        layout_backend,
        layout_set: LayoutSet,
        hotkey_convert: frozenset,
        hotkey_selection: frozenset,
        current_layout: str = "en",
        auto_detect: bool = True,
        threshold: float = 2.5,
        min_word_len: int = 3,
    ):
        self.layout_backend = layout_backend
        self.layout_set = layout_set
        self.hotkey_convert = hotkey_convert
        self.hotkey_selection = hotkey_selection
        self.auto_detect = auto_detect
        self.threshold = threshold
        self.min_word_len = min_word_len
        self._layout = current_layout

        self._buf = KeyBuffer()
        self._mods = ModifierState()
        self._pressed: set[int] = set()
        self._injecting = False
        self._ui: UInput | None = None
        self._keyboards: list[InputDevice] = []

        self._undo_data: dict | None = None
        self._undo_ts: float = 0.0
        self.undo_window = 3.0

        self._stop_event = asyncio.Event()

    def _setup_signals(self):
        try:
            loop = asyncio.get_running_loop()
            for sig in (signal.SIGINT, signal.SIGTERM):
                loop.add_signal_handler(sig, self._stop_event.set)
        except Exception:
            pass

    # ── startup ──────────────────────────────────────────────────────────

    def _find_keyboards(self) -> list[InputDevice]:
        result = []
        for path in list_devices():
            try:
                dev = InputDevice(path)
                caps = dev.capabilities()
                if e.EV_KEY not in caps:
                    continue

                # Must have basic alpha keys
                keys = caps[e.EV_KEY]
                if not {e.KEY_A, e.KEY_Z, e.KEY_ENTER}.issubset(set(keys)):
                    continue

                # Ignore mice/touchpads
                if e.EV_REL in caps and len(caps[e.EV_REL]) > 3:
                    continue
                if e.EV_ABS in caps and len(caps[e.EV_ABS]) > 3:
                    continue

                # Ignore common non-keyboard devices
                name = dev.name.lower()
                if any(x in name for x in ("mouse", "pointer", "touchpad", "video bus", "button")):
                    if "keyboard" not in name:
                        continue

                result.append(dev)
            except Exception:
                pass
        return result

    async def run(self):
        self._setup_signals()
        keyboards = self._find_keyboards()
        if not keyboards:
            raise RuntimeError("No keyboard devices found. Try running with sudo or check group permissions.")

        logger.info(f"Keyboards found: {[k.name for k in keyboards]}")
        
        self._ui = UInput.from_device(keyboards[0], name="power_switcher-virtual")
        logger.info("Virtual UInput device created")
        
        await asyncio.sleep(0.5)

        if isinstance(self.layout_backend, GNOMESwitchBackend):
            self.layout_backend.set_uinput(self._ui)

        for kbd in keyboards:
            try:
                kbd.grab()
                self._keyboards.append(kbd)
                logger.debug(f"Grabbed {kbd.name}")
            except IOError as err:
                logger.error(f"Failed to grab {kbd.name}: {err}")
        logger.info("All physical keyboards grabbed")

        try:
            tasks = [asyncio.create_task(self._read(kbd)) for kbd in keyboards]
            await self._stop_event.wait()
            logger.info("Stop signal received, shutting down...")
            for t in tasks:
                t.cancel()
        finally:
            for kbd in self._keyboards:
                try:
                    kbd.ungrab()
                    logger.debug(f"Ungrabbed {kbd.name}")
                except Exception:
                    pass
            if self._ui:
                self._ui.close()
            logger.info("Cleanup complete")

    async def _read(self, dev: InputDevice):
        try:
            async for ev in dev.async_read_loop():
                await self._on_event(ev)
        except asyncio.CancelledError:
            pass

    # ── event handling ────────────────────────────────────────────────────

    async def _on_event(self, ev):
        if ev.type != e.EV_KEY:
            self._ui.write_event(ev)
            self._ui.syn()
            return

        if ev.value == 2:
            self._ui.write_event(ev)
            self._ui.syn()
            return

        kc = ev.code
        pressed = ev.value == 1

        if kc in MODIFIER_KEYS:
            self._mods.update(kc, pressed)
            if pressed:
                self._pressed.add(kc)
            else:
                self._pressed.discard(kc)
            self._ui.write_event(ev)
            self._ui.syn()
            return

        if pressed:
            self._pressed.add(kc)
        else:
            self._pressed.discard(kc)

        if pressed:
            combo = frozenset(self._pressed)
            if combo == self.hotkey_convert:
                await self._do_manual_convert()
                return
            if combo == self.hotkey_selection:
                await self._do_selection_convert()
                return

        self._ui.write_event(ev)
        self._ui.syn()

        if pressed and not self._injecting:
            await self._update_buf(kc)

    # ── buffer management ─────────────────────────────────────────────────

    async def _update_buf(self, kc: int):
        if kc == e.KEY_BACKSPACE:
            self._buf.backspace()
            return

        if kc in BUFFER_RESET_KEYS:
            self._buf.clear()
            return

        if kc in WORD_SEPARATORS:
            word = self._buf.flush()
            if self.auto_detect and word:
                asyncio.create_task(self._maybe_auto(word))
            return

        if not self._mods.is_plain:
            self._buf.clear()
            return

        char = self.layout_set.keycode_to_char(kc, self._layout, self._mods.shift)
        if char:
            self._buf.add(kc, char)

    # ── auto-detection ────────────────────────────────────────────────────

    async def _maybe_auto(self, word: str):
        target = self.layout_set.find_best_layout(
            word, self._layout, self.threshold, self.min_word_len
        )
        if target:
            logger.info(f"Auto-detect: converting '{word}' to {target}")
            await self._convert(word, target)

    # ── manual convert (hotkey) ───────────────────────────────────────────

    async def _do_manual_convert(self):
        if self._undo_data and (time.time() - self._undo_ts) < self.undo_window:
            await self._undo()
            return

        word = self._buf.text
        if not word:
            logger.debug("Manual convert: buffer empty")
            return

        target = self.layout_set.find_best_layout(
            word, self._layout, threshold=-100.0, min_word_len=1
        )
        if not target:
            idx = self.layout_set.layouts.index(self._layout)
            target = self.layout_set.layouts[(idx + 1) % len(self.layout_set.layouts)]

        await self._convert(word, target)

    # ── selection convert (second hotkey) ─────────────────────────────────

    async def _do_selection_convert(self):
        saved = subprocess.run(
            ["wl-paste", "--no-newline"], capture_output=True, text=True
        ).stdout

        self._raw_keys([e.KEY_LEFTCTRL, e.KEY_C])
        await asyncio.sleep(0.08)

        selected = subprocess.run(
            ["wl-paste", "--no-newline"], capture_output=True, text=True
        ).stdout

        if not selected or selected == saved:
            return

        target = self.layout_set.find_best_layout(
            selected, self._layout, threshold=-100.0, min_word_len=1
        )
        if not target:
            idx = self.layout_set.layouts.index(self._layout)
            target = self.layout_set.layouts[(idx + 1) % len(self.layout_set.layouts)]

        converted = self.layout_set.convert(selected, self._layout, target)

        self._inject_text(converted, target)
        self._set_layout(target)

        await asyncio.sleep(0.05)
        subprocess.run(["wl-copy"], input=saved, text=True)

    # ── core conversion ───────────────────────────────────────────────────

    async def _convert(self, word: str, target_layout: str):
        converted = self.layout_set.convert(word, self._layout, target_layout)

        if converted == word and self._layout == target_layout:
            return

        self._undo_data = {
            "original": word, "converted": converted, "layout": self._layout
        }
        self._undo_ts = time.time()

        self._injecting = True
        try:
            # +1 for the separator (Space/Enter)
            for _ in range(len(word) + 1):
                self._raw_key(e.KEY_BACKSPACE)
                await asyncio.sleep(0.005)

            self._set_layout(target_layout)
            await asyncio.sleep(0.12)
            
            self._inject_text(converted, target_layout)
            self._raw_key(e.KEY_SPACE)
            self._buf.clear()
        finally:
            self._injecting = False

        logger.info(f"Converted: '{word}' → '{converted}' (now {self._layout})")

    async def _undo(self):
        if not self._undo_data:
            return
        converted = self._undo_data["converted"]
        original = self._undo_data["original"]
        original_layout = self._undo_data["layout"]

        self._injecting = True
        try:
            for _ in range(len(converted) + 1):
                self._raw_key(e.KEY_BACKSPACE)
                await asyncio.sleep(0.005)

            self._set_layout(original_layout)
            await asyncio.sleep(0.12)

            self._inject_text(original, original_layout)
            self._raw_key(e.KEY_SPACE)
            self._buf.clear()
        finally:
            self._injecting = False

        self._undo_data = None
        logger.info(f"Undo: '{converted}' → '{original}'")

    # ── low-level injection helpers ───────────────────────────────────────

    def _raw_key(self, kc: int):
        self._ui.write(e.EV_KEY, kc, 1)
        self._ui.syn()
        self._ui.write(e.EV_KEY, kc, 0)
        self._ui.syn()

    def _raw_keys(self, keys: list[int]):
        for k in keys:
            self._ui.write(e.EV_KEY, k, 1)
        self._ui.syn()
        time.sleep(0.01)
        for k in reversed(keys):
            self._ui.write(e.EV_KEY, k, 0)
        self._ui.syn()

    def _inject_text(self, text: str, layout: str):
        for char in text:
            res = self.layout_set.char_to_keycode(char, layout)
            if res:
                kc, shifted = res
                if shifted:
                    self._ui.write(e.EV_KEY, e.KEY_LEFTSHIFT, 1)
                self._ui.write(e.EV_KEY, kc, 1)
                self._ui.syn()
                self._ui.write(e.EV_KEY, kc, 0)
                self._ui.syn()
                if shifted:
                    self._ui.write(e.EV_KEY, e.KEY_LEFTSHIFT, 0)
                    self._ui.syn()
            else:
                logger.warning(f"No keycode for char '{char}' in layout {layout}")

    def _set_layout(self, layout: str):
        try:
            idx = self.layout_set.layouts.index(layout)
            self.layout_backend.set_layout(idx)
            self._layout = layout
        except ValueError:
            logger.error(f"Cannot switch to unknown layout: {layout}")
