import asyncio
import logging
import signal
import subprocess
import time
import os
from pathlib import Path

from evdev import InputDevice, UInput, ecodes as e, list_devices

from .key_buffer import KeyBuffer, WORD_SEPARATORS, BUFFER_RESET_KEYS
from .modifier_state import ModifierState, SHIFT_KEYS
from ..maps.layout_set import LayoutSet
from ..layout.backends import GNOMESwitchBackend

logger = logging.getLogger(__name__)

audit_path = Path("~/.cache/power_switcher/audit.log").expanduser()
audit_path.parent.mkdir(parents=True, exist_ok=True)
audit_log = logging.getLogger("audit")
audit_log.setLevel(logging.DEBUG)
handler = logging.FileHandler(audit_path, mode="w")
handler.setFormatter(logging.Formatter("%(asctime)s [%(levelname)s] %(message)s"))
audit_log.addHandler(handler)

class KeyProcessor:
    def __init__(self, layout_backend, layout_set: LayoutSet, current_layout: str = "en", threshold: float = 2.5, min_word_len: int = 3):
        self.layout_backend = layout_backend
        self.layout_set = layout_set
        self._layout = current_layout
        self.threshold = threshold
        self.min_word_len = min_word_len
        self._buf = KeyBuffer()
        self._mods = ModifierState()
        self._pressed: set[int] = set()
        self._injecting_until = 0.0
        self._ui: UInput | None = None
        self._keyboards: list[InputDevice] = []
        self._running = True
        self._is_injecting = False
        self._queued_events: list[InputEvent] = []
        self._sentence: list[str] = []
        self._last_old_word = ""
        self._last_new_word = ""
        self._last_old_layout = ""
        self._last_conv_ts = 0.0

    def _play_sound(self, name: str = "message-new-instant"):
        subprocess.Popen(["paplay", f"/usr/share/sounds/freedesktop/stereo/{name}.oga"], stderr=subprocess.DEVNULL, stdout=subprocess.DEVNULL)

    def _on_stop_signal(self, sig, frame):
        self._running = False
        for kbd in self._keyboards:
            try: kbd.ungrab()
            except Exception: pass
        if self._ui:
            try: self._ui.close()
            except Exception: pass
        os._exit(0)

    def _setup_signals(self):
        for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGTSTP):
            signal.signal(sig, self._on_stop_signal)

    def _find_keyboards(self) -> list[InputDevice]:
        result = []
        for path in list_devices():
            try:
                dev = InputDevice(path)
                name = dev.name.lower()
                if any(x in name for x in ("virtual", "ydotool", "uinput", "mouse", "touchpad", "power_switcher")): continue
                if e.EV_KEY not in dev.capabilities(): continue
                if not {e.KEY_A, e.KEY_Z, e.KEY_ENTER}.issubset(set(dev.capabilities()[e.EV_KEY])): continue
                result.append(dev)
            except Exception: pass
        return result

    async def run(self):
        self._setup_signals()
        keyboards = self._find_keyboards()
        if not keyboards: raise RuntimeError("No physical keyboards found.")
        
        # Slower but safer initialization
        self._ui = UInput.from_device(keyboards[0], name="power_switcher-virtual")
        await asyncio.sleep(0.5)

        if isinstance(self.layout_backend, GNOMESwitchBackend):
            self.layout_backend.set_uinput(self._ui)

        for kbd in keyboards:
            try:
                kbd.grab()
                self._keyboards.append(kbd)
                audit_log.info(f"AUDIT: Grabbed {kbd.name}")
            except Exception as ex: audit_log.error(f"AUDIT: Failed {kbd.name}: {ex}")

        try:
            real_idx = self.layout_backend.get_current_index()
            if real_idx < len(self.layout_set.layouts):
                self._layout = self.layout_set.layouts[real_idx]
                audit_log.info(f"AUDIT: Initial layoutSynced to {self._layout}")
        except Exception: pass

        try:
            await asyncio.gather(*[self._read(kbd) for kbd in keyboards])
        finally:
            self._running = False
            for kbd in self._keyboards:
                try: kbd.ungrab()
                except Exception: pass
            if self._ui: self._ui.close()

    async def _read(self, dev: InputDevice):
        async for ev in dev.async_read_loop():
            if not self._running: break
            await self._on_event(ev)

    async def _on_event(self, ev):
        if ev.type != e.EV_KEY:
            self._ui.write_event(ev); self._ui.syn()
            return

        if self._is_injecting:
            self._queued_events.append(ev)
            return

        if time.time() < self._injecting_until: return

        kc = ev.code
        pressed = ev.value == 1
        repeat = ev.value == 2
        if repeat:
            self._ui.write_event(ev); self._ui.syn()
            return

        action = self._mods.update(kc, pressed)
        if kc in SHIFT_KEYS:
            self._ui.write_event(ev); self._ui.syn()
            if not pressed:
                if action == "double_shift": await self._do_manual_convert()
                elif action == "single_shift" and not self._pressed:
                    if kc == e.KEY_LEFTSHIFT and len(self.layout_set.layouts) > 0:
                        self._set_layout(self.layout_set.layouts[0])
                        self._sentence.clear()
                        self._play_sound("audio-volume-low")
                    elif kc == e.KEY_RIGHTSHIFT and len(self.layout_set.layouts) > 1:
                        self._set_layout(self.layout_set.layouts[1])
                        self._sentence.clear()
                        self._play_sound("audio-volume-low")
            return

        if pressed: self._pressed.add(kc)
        else: self._pressed.discard(kc)

        self._ui.write_event(ev); self._ui.syn()
        if pressed: await self._update_buf(kc)

    async def _update_buf(self, kc: int):
        if kc == e.KEY_BACKSPACE:
            self._buf.backspace()
            if self._sentence: self._sentence.pop()
            return
        if kc in BUFFER_RESET_KEYS:
            self._buf.clear()
            self._sentence.clear()
            return
        if kc in WORD_SEPARATORS:
            if kc == e.KEY_SPACE: self._sentence.append(" ")
            elif kc in (e.KEY_ENTER, e.KEY_KPENTER): self._sentence.append("\n")
            else:
                char = self.layout_set.keycode_to_char(kc, self._layout, self._mods.shift)
                if char: self._sentence.append(char)
                else: self._sentence.clear()
                
            word = self._buf.flush()
            if word: 
                if self.layout_set.is_password(word): self._set_layout("en")
                else: await self._maybe_auto(word, separator_kc=kc)
            return
        if not self._mods.is_plain:
            self._buf.clear()
            self._sentence.clear()
            return
        char = self.layout_set.keycode_to_char(kc, self._layout, self._mods.shift)
        if char:
            self._buf.add(kc, char)
            self._sentence.append(char)
            
            # Eager mid-word auto-conversion if the gain is overwhelmingly high
            word = self._buf.text
            if len(word) >= 4 and not self.layout_set.is_password(word):
                target = self.layout_set.find_best_layout(word, self._layout, threshold=15.0, min_word_len=4)
                if target:
                    audit_log.info(f"AUDIT: Mid-word converting '{word}' ({self._layout}) -> {target}")
                    converted = self.layout_set.convert(word, self._layout, target)
                    asyncio.create_task(self._replace_text(word, converted, target, separator_kc=None))

    def _check_double_caps(self, word: str) -> str | None:
        if len(word) >= 3 and word[0].isupper() and word[1].isupper() and word[2].islower():
            return word[0] + word[1].lower() + word[2:]
        return None

    async def _maybe_auto(self, word: str, separator_kc: int | None = None):
        fix = self._check_double_caps(word)
        if fix:
            await self._replace_text(word, fix, target_layout=None, separator_kc=separator_kc)
            return

        target = self.layout_set.find_best_layout(word, self._layout, self.threshold, self.min_word_len)
        if target:
            audit_log.info(f"AUDIT: Converting '{word}' ({self._layout}) -> {target}")
            converted = self.layout_set.convert(word, self._layout, target)
            await self._replace_text(word, converted, target, separator_kc)

    async def _replace_text(self, old_word: str, new_word: str, target_layout: str | None, separator_kc: int | None, is_undo: bool = False):
        self._is_injecting = True
        self._queued_events.clear()
        try:
            for mod in [e.KEY_LEFTSHIFT, e.KEY_RIGHTSHIFT, e.KEY_LEFTCTRL, e.KEY_LEFTALT]:
                self._ui.write(e.EV_KEY, mod, 0); self._ui.syn()

            backspaces = len(old_word)
            if separator_kc is not None:
                backspaces += 1
                
            for _ in range(backspaces):
                self._ui.write(e.EV_KEY, e.KEY_BACKSPACE, 1); self._ui.syn()
                await asyncio.sleep(0.005)
                self._ui.write(e.EV_KEY, e.KEY_BACKSPACE, 0); self._ui.syn()
                await asyncio.sleep(0.005)

            old_layout = self._layout
            if target_layout and target_layout != self._layout:
                self._set_layout(target_layout)
                await asyncio.sleep(0.1)
                layout_for_typing = target_layout
            else:
                layout_for_typing = self._layout

            for char in new_word:
                res = self.layout_set.char_to_keycode(char, layout_for_typing)
                if res:
                    kc, shifted = res
                    if shifted: self._ui.write(e.EV_KEY, e.KEY_LEFTSHIFT, 1); self._ui.syn(); await asyncio.sleep(0.005)
                    self._ui.write(e.EV_KEY, kc, 1); self._ui.syn()
                    await asyncio.sleep(0.01)
                    self._ui.write(e.EV_KEY, kc, 0); self._ui.syn()
                    await asyncio.sleep(0.01)
                    if shifted: self._ui.write(e.EV_KEY, e.KEY_LEFTSHIFT, 0); self._ui.syn(); await asyncio.sleep(0.005)

            if separator_kc is not None:
                self._ui.write(e.EV_KEY, separator_kc, 1); self._ui.syn()
                await asyncio.sleep(0.01)
                self._ui.write(e.EV_KEY, separator_kc, 0); self._ui.syn()
            
            self._buf.clear()
            if not is_undo:
                self._last_old_word = old_word
                self._last_new_word = new_word
                self._last_old_layout = old_layout
                self._last_conv_ts = time.time()
                self._sentence = list(new_word)
                if separator_kc == e.KEY_SPACE: self._sentence.append(" ")
                elif separator_kc in (e.KEY_ENTER, e.KEY_KPENTER): self._sentence.append("\n")
            else:
                self._last_conv_ts = 0.0
                self._sentence.clear()
            
            self._play_sound("message-new-instant")
        finally:
            self._is_injecting = False
            events_to_replay = list(self._queued_events)
            self._queued_events.clear()
            self._injecting_until = 0 # Temporarily allow replay
            
            for q_ev in events_to_replay:
                if q_ev.type == e.EV_KEY:
                    await self._on_event(q_ev)
                    
            self._injecting_until = time.time() + 0.05

    async def _do_manual_convert(self):
        text = "".join(self._sentence)
        if not text.strip():
            if time.time() - self._last_conv_ts < 3.0 and self._last_old_word:
                await self._replace_text(self._last_new_word, self._last_old_word, self._last_old_layout, separator_kc=e.KEY_SPACE, is_undo=True)
            return

        words = text.split()
        target = None
        if words:
            last_word = words[-1]
            target = self.layout_set.find_best_layout(last_word, self._layout, threshold=-100.0, min_word_len=1)
            
        if not target:
            idx = self.layout_set.layouts.index(self._layout)
            target = self.layout_set.layouts[(idx + 1) % len(self.layout_set.layouts)]
            
        converted = self.layout_set.convert(text, self._layout, target)
        await self._replace_text(text, converted, target, separator_kc=None)

    def _set_layout(self, layout: str):
        try:
            idx = self.layout_set.layouts.index(layout)
            self.layout_backend.set_layout(idx)
            self._layout = layout
            audit_log.info(f"AUDIT: set_layout({layout})")
        except ValueError: pass
