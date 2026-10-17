import asyncio
import logging
import signal
import subprocess
import time
import os
from pathlib import Path

from evdev import InputDevice, UInput, ecodes as e, list_devices

from .key_buffer import KeyBuffer, SentenceBuffer, WORD_SEPARATORS, BUFFER_RESET_KEYS
from .modifier_state import ModifierState, SHIFT_KEYS
from .snippets import SnippetStore
from .undo_history import UndoHistory
from .anti_resonance import AntiResonanceGuard
from .app_exceptions import ExceptionStore, get_active_window_class
from .stats import Stats
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
    def __init__(
        self,
        layout_backend,
        layout_set: LayoutSet,
        current_layout: str = "en",
        threshold: float = 2.5,
        min_word_len: int = 3,
        # Snippets
        snippet_expand_space: bool = True,
        snippet_expand_enter: bool = True,
        snippet_expand_tab: bool = True,
        snippets_path: Path | None = None,
        # Triggers
        trigger_space: bool = True,
        trigger_enter: bool = True,
        trigger_tab: bool = True,
        trigger_punctuation: bool = True,
        # Undo
        undo_words: int = 0,
        undo_seconds: float = 3.0,
        # Behavior
        developer_mode: bool = False,
        arrows_cancel: bool = True,
        live_fix_enabled: bool = True,
        # Layout-switch guards & hotkeys
        no_correct_after_switch: float = 0.0,
        lr_shift_pause: bool = True,
        manual_only_last_word: bool = False,
        sound_enabled: bool = True,
        self_learning: bool = True,
        # Stats
        stats: Stats | None = None,
        # Exceptions
        exceptions_path: Path | None = None,
    ):
        self.layout_backend = layout_backend
        self.layout_set = layout_set
        self._layout = current_layout
        self.threshold = threshold
        self.min_word_len = min_word_len

        # Snippets
        self._snippets = SnippetStore(path=snippets_path)
        self._snippet_expand_space = snippet_expand_space
        self._snippet_expand_enter = snippet_expand_enter
        self._snippet_expand_tab = snippet_expand_tab

        # Triggers
        self._trigger_space = trigger_space
        self._trigger_enter = trigger_enter
        self._trigger_tab = trigger_tab
        self._trigger_punctuation = trigger_punctuation

        # Undo
        self._undo = UndoHistory(window_seconds=undo_seconds)
        self._undo_words = undo_words

        # Behavior
        self._developer_mode = developer_mode
        self._arrows_cancel = arrows_cancel
        self._live_fix_enabled = live_fix_enabled

        # Layout-switch guards & hotkeys
        self._no_correct_after_switch = no_correct_after_switch
        self._lr_shift_pause = lr_shift_pause
        self._manual_only_last_word = manual_only_last_word
        self._sound_enabled = sound_enabled
        self._self_learning = self_learning

        # Statistics
        self.stats = stats if stats is not None else Stats()

        # Exceptions
        self._exceptions = ExceptionStore(path=exceptions_path)

        # Anti-resonance
        self._anti_resonance = AntiResonanceGuard()

        # State
        self._buf = KeyBuffer()
        self._sentence = SentenceBuffer()
        self._mods = ModifierState()
        self._pressed: set[int] = set()
        self._injecting_until = 0.0
        self._ui: UInput | None = None
        self._keyboards: list[InputDevice] = []
        self._running = True
        self._is_injecting = False
        self._queued_events: list = []

        # Live fix state
        self._live_fix_last = ""
        self._word_edited = False
        self._last_real_key_at = 0.0

        # App context
        self._front_app_class = ""
        self._front_app_mode = "default"

        # Auto-detect pause (L+R Shift) and layout-change guard
        self._auto_paused = False
        self._lr_shift_consumed = False
        self._shift_held: set[int] = set()
        self._layout_changed_at = 0.0

    # ── sound feedback ──────────────────────────────────────────────────

    def _play_sound(self, name: str = "message-new-instant"):
        if not self._sound_enabled:
            return
        try:
            subprocess.Popen(
                ["paplay", f"/usr/share/sounds/freedesktop/stereo/{name}.oga"],
                stderr=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
            )
        except FileNotFoundError:
            pass

    # ── signal handling ─────────────────────────────────────────────────

    def _on_stop_signal(self, sig, frame):
        self._running = False
        try:
            self.stats.save(force=True)
        except Exception:
            pass
        for kbd in self._keyboards:
            try:
                kbd.ungrab()
            except Exception:
                pass
        if self._ui:
            try:
                self._ui.close()
            except Exception:
                pass
        os._exit(0)

    def _setup_signals(self):
        for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGTSTP):
            signal.signal(sig, self._on_stop_signal)

    # ── keyboard discovery ──────────────────────────────────────────────

    def _find_keyboards(self) -> list[InputDevice]:
        result = []
        for path in list_devices():
            try:
                dev = InputDevice(path)
                name = dev.name.lower()
                if any(x in name for x in (
                    "virtual", "ydotool", "uinput", "mouse",
                    "touchpad", "power_switcher",
                )):
                    continue
                if e.EV_KEY not in dev.capabilities():
                    continue
                if not {e.KEY_A, e.KEY_Z, e.KEY_ENTER}.issubset(
                    set(dev.capabilities()[e.EV_KEY])
                ):
                    continue
                result.append(dev)
            except Exception:
                pass
        return result

    # ── app context ─────────────────────────────────────────────────────

    def _refresh_front_app(self):
        cls = get_active_window_class()
        if cls != self._front_app_class:
            logger.debug("Focus changed: %r", cls)
        self._front_app_class = cls
        self._front_app_mode = self._exceptions.get_mode(cls)

    def _toggle_auto(self):
        self._auto_paused = not self._auto_paused
        state = "PAUSED" if self._auto_paused else "ACTIVE"
        audit_log.info(f"AUDIT: auto-detect {state} (L+R Shift)")
        self._play_sound("audio-volume-low" if self._auto_paused else "message-new-instant")

    # ── startup ─────────────────────────────────────────────────────────

    async def run(self):
        self._setup_signals()
        keyboards = self._find_keyboards()
        if not keyboards:
            raise RuntimeError("No physical keyboards found.")

        self._ui = UInput.from_device(keyboards[0], name="power_switcher-virtual")
        await asyncio.sleep(0.5)

        if isinstance(self.layout_backend, GNOMESwitchBackend):
            self.layout_backend.set_uinput(self._ui)

        for kbd in keyboards:
            try:
                kbd.grab()
                self._keyboards.append(kbd)
                audit_log.info(f"AUDIT: Grabbed {kbd.name}")
            except Exception as ex:
                audit_log.error(f"AUDIT: Failed {kbd.name}: {ex}")

        try:
            real_idx = self.layout_backend.get_current_index()
            if real_idx < len(self.layout_set.layouts):
                self._layout = self.layout_set.layouts[real_idx]
                audit_log.info(f"AUDIT: Initial layout synced to {self._layout}")
        except Exception:
            pass

        self._refresh_front_app()

        try:
            await asyncio.gather(*[self._read(kbd) for kbd in keyboards])
        finally:
            self._running = False
            try:
                self.stats.save(force=True)
            except Exception:
                pass
            for kbd in self._keyboards:
                try:
                    kbd.ungrab()
                except Exception:
                    pass
            if self._ui:
                self._ui.close()

    async def _read(self, dev: InputDevice):
        async for ev in dev.async_read_loop():
            if not self._running:
                break
            await self._on_event(ev)

    # ── event handling ───────────────────────────────────────────────────

    async def _on_event(self, ev):
        if ev.type != e.EV_KEY:
            self._ui.write_event(ev)
            self._ui.syn()
            return

        if self._is_injecting:
            self._queued_events.append(ev)
            return

        if time.time() < self._injecting_until:
            return

        kc = ev.code
        pressed = ev.value == 1
        repeat = ev.value == 2

        if repeat:
            self._ui.write_event(ev)
            self._ui.syn()
            return

        # Modifier handling
        action = self._mods.update(kc, pressed)
        if kc in SHIFT_KEYS:
            self._ui.write_event(ev)
            self._ui.syn()

            if pressed:
                self._shift_held.add(kc)
                # Both Shift keys held together → pause/resume auto-detect.
                if self._lr_shift_pause and len(self._shift_held) == 2:
                    self._lr_shift_consumed = True
                    self._mods.reset_shift_taps()
                    self._toggle_auto()
                return

            # Release
            self._shift_held.discard(kc)
            if self._lr_shift_consumed:
                if not self._shift_held:
                    self._lr_shift_consumed = False
                return
            if action == "double_shift":
                await self._do_manual_convert()
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

        if pressed:
            self._pressed.add(kc)
        else:
            self._pressed.discard(kc)

        # Pass through to compositor
        self._ui.write_event(ev)
        self._ui.syn()

        if pressed:
            await self._update_buf(kc)

    # ── buffer management ────────────────────────────────────────────────

    def _is_auto_trigger(self, kc: int) -> bool:
        if kc == e.KEY_SPACE:
            return self._trigger_space
        if kc in (e.KEY_ENTER, e.KEY_KPENTER):
            return self._trigger_enter
        if kc == e.KEY_TAB:
            return self._trigger_tab
        return False

    def _is_snippet_trigger(self, kc: int) -> bool:
        if kc == e.KEY_SPACE:
            return self._snippet_expand_space
        if kc in (e.KEY_ENTER, e.KEY_KPENTER):
            return self._snippet_expand_enter
        if kc == e.KEY_TAB:
            return self._snippet_expand_tab
        return False

    def _should_skip_auto(self) -> bool:
        if self._auto_paused:
            return True
        if self._front_app_mode == "off":
            return True
        if self._developer_mode and self._exceptions.is_dev_app(self._front_app_class):
            return True
        return False

    def _ambiguous_separator(self, kc: int) -> bool:
        """True when *kc* produces a letter in some configured layout.

        The comma key is ',' in English but 'б' in Russian; the semicolon key is
        ';' but 'ж'.  Treating such a key as a word separator would chop a word
        typed in the wrong layout in half, so they never end a word.
        """
        for lay in self.layout_set.layouts:
            c = self.layout_set.keycode_to_char(kc, lay, False)
            if c and c.isalpha():
                return True
        return False

    async def _update_buf(self, kc: int):
        # Backspace
        if kc == e.KEY_BACKSPACE:
            self._live_fix_last = ""
            self._word_edited = True
            self._buf.backspace()
            self._sentence.pop()
            return

        # Navigation keys — clear context
        if kc in BUFFER_RESET_KEYS:
            self._live_fix_last = ""
            self._word_edited = False
            self._buf.clear()
            self._sentence.clear()
            self._undo.clear_old()
            if self._arrows_cancel and kc in (e.KEY_UP, e.KEY_DOWN, e.KEY_LEFT, e.KEY_RIGHT):
                self._anti_resonance.reset()
            return

        # Arrow keys clear buffer (configurable)
        if self._arrows_cancel and kc in (e.KEY_UP, e.KEY_DOWN, e.KEY_LEFT, e.KEY_RIGHT):
            self._live_fix_last = ""
            self._word_edited = False
            self._buf.clear()
            self._sentence.clear()
            self._anti_resonance.reset()
            return

        # Shortcuts (Ctrl/Alt combos) — clear context
        if not self._mods.is_plain:
            self._live_fix_last = ""
            self._word_edited = False
            self._buf.clear()
            self._sentence.clear()
            return

        # Word boundary (Space / Enter / Tab / punctuation)
        # NOTE: a keycode is only a boundary if it does *not* produce a letter in
        # the active layout.  In Cyrillic layouts the punctuation keys become
        # letters (б, ю, ж, э, ё, х, ъ …), so deciding by keycode would split
        # most Russian/Ukrainian words in half.
        #
        # The mirror case matters too: while typing Russian *in* the English
        # layout the comma key produces ',' but stands for 'б'.  A key is
        # therefore only a separator when it is punctuation in *every* configured
        # layout — otherwise the word is kept intact and detection re-maps it.
        char = self.layout_set.keycode_to_char(kc, self._layout, self._mods.shift)
        hard_sep = kc in (e.KEY_SPACE, e.KEY_ENTER, e.KEY_KPENTER, e.KEY_TAB)
        is_boundary = hard_sep or (
            kc in WORD_SEPARATORS
            and not (char and char.isalpha())
            and not self._ambiguous_separator(kc)
        )

        if is_boundary:
            self._live_fix_last = ""
            self._word_edited = False

            # Record separator in sentence
            if kc == e.KEY_SPACE:
                self._sentence.append(" ")
            elif kc in (e.KEY_ENTER, e.KEY_KPENTER):
                self._sentence.append("\n")
            else:
                if char:
                    self._sentence.append(char)
                else:
                    self._sentence.clear()

            word = self._buf.flush()
            if not word:
                return

            # Snippet expansion (check BEFORE auto-detect)
            if self._is_snippet_trigger(kc) and not self._is_injecting:
                expansion = self._snippets.expansion(word)
                if expansion:
                    await self._expand_snippet(word, expansion, kc)
                    return

            # Password detection
            if self.layout_set.is_password(word):
                self._set_layout("en")
                return

            # Auto-detection.  Space/Enter/Tab are gated by the trigger
            # options; other punctuation is gated by trigger_punctuation.
            if hard_sep and self._is_auto_trigger(kc):
                await self._maybe_auto(word, separator_kc=kc)
            elif not hard_sep and self._trigger_punctuation:
                await self._maybe_auto(
                    word, separator_kc=kc, separator_shift=self._mods.shift
                )
            return

        # Regular character
        if char:
            self._buf.add(kc, char)
            self._sentence.append(char)
            self._last_real_key_at = time.time()

            # Mid-word eager conversion (very high confidence only)
            word = self._buf.text
            if (
                self._live_fix_enabled
                and len(word) >= 4
                and not self.layout_set.is_password(word)
                and not self._word_edited
            ):
                target = self.layout_set.find_best_layout(
                    word, self._layout, threshold=15.0, min_word_len=4
                )
                if target:
                    converted = self.layout_set.convert(word, self._layout, target)
                    if self._anti_resonance.allow(word, converted):
                        audit_log.info(
                            f"AUDIT: Mid-word converting '{word}' ({self._layout}) -> {target}"
                        )
                        asyncio.create_task(
                            self._replace_text(word, converted, target, separator_kc=None)
                        )

    # ── double caps fix ─────────────────────────────────────────────────

    @staticmethod
    def _changed_char_count(old: str, new: str) -> int:
        """Number of characters actually different between two strings."""
        return sum(1 for a, b in zip(old, new) if a != b) + abs(len(old) - len(new))

    def _check_double_caps(self, word: str) -> str | None:
        if len(word) >= 3 and word[0].isupper() and word[1].isupper() and word[2].islower():
            return word[0] + word[1].lower() + word[2:]
        return None

    # ── auto-detection ───────────────────────────────────────────────────

    async def _maybe_auto(
        self,
        word: str,
        separator_kc: int | None = None,
        separator_shift: bool = False,
    ):
        # Refresh per-app context lazily, only when a decision is about to be
        # made (the provider caches, so this is cheap).
        self._refresh_front_app()

        if self._should_skip_auto():
            return

        # "Don't correct after layout change": give the user a grace period
        # after any manual/programmatic layout switch.
        if (
            self._no_correct_after_switch > 0
            and time.time() - self._layout_changed_at < self._no_correct_after_switch
        ):
            return

        # Double caps fix
        fix = self._check_double_caps(word)
        if fix:
            await self._replace_text(
                word, fix, target_layout=None,
                separator_kc=separator_kc, separator_shift=separator_shift,
            )
            self.stats.record_auto(self._changed_char_count(word, fix))
            return

        target = self.layout_set.find_best_layout(
            word, self._layout, self.threshold, self.min_word_len
        )
        if target:
            converted = self.layout_set.convert(word, self._layout, target)
            if not self._anti_resonance.allow(word, converted):
                return
            audit_log.info(f"AUDIT: Converting '{word}' ({self._layout}) -> {target}")
            await self._replace_text(
                word, converted, target,
                separator_kc=separator_kc, separator_shift=separator_shift,
            )
            self.stats.record_auto(self._changed_char_count(word, converted))
            logger.debug("auto-detected %r → %r", word, converted)

    # ── snippet expansion ────────────────────────────────────────────────

    async def _expand_snippet(self, trigger: str, expansion: str, separator_kc: int):
        """Replace trigger word with snippet expansion."""
        audit_log.info(f"AUDIT: Snippet '{trigger}' -> '{expansion}'")

        # Erase trigger + the separator key that was already sent
        erase_count = len(trigger) + 1  # +1 for the separator
        self._is_injecting = True
        self._queued_events.clear()
        try:
            for _ in range(erase_count):
                self._ui.write(e.EV_KEY, e.KEY_BACKSPACE, 1)
                self._ui.syn()
                await asyncio.sleep(0.005)
                self._ui.write(e.EV_KEY, e.KEY_BACKSPACE, 0)
                self._ui.syn()
                await asyncio.sleep(0.005)

            # Type the expansion using keycodes in current layout
            for char in expansion:
                res = self.layout_set.char_to_keycode(char, self._layout)
                if res:
                    kc, shifted = res
                    if shifted:
                        self._ui.write(e.EV_KEY, e.KEY_LEFTSHIFT, 1)
                        self._ui.syn()
                        await asyncio.sleep(0.005)
                    self._ui.write(e.EV_KEY, kc, 1)
                    self._ui.syn()
                    await asyncio.sleep(0.008)
                    self._ui.write(e.EV_KEY, kc, 0)
                    self._ui.syn()
                    await asyncio.sleep(0.008)
                    if shifted:
                        self._ui.write(e.EV_KEY, e.KEY_LEFTSHIFT, 0)
                        self._ui.syn()
                        await asyncio.sleep(0.005)

            # Re-send the separator (space/enter/tab)
            self._ui.write(e.EV_KEY, separator_kc, 1)
            self._ui.syn()
            await asyncio.sleep(0.01)
            self._ui.write(e.EV_KEY, separator_kc, 0)
            self._ui.syn()

            self._buf.clear()
            self._sentence.clear()
            self._play_sound("message-new-instant")
            self.stats.record_snippet()
        finally:
            self._is_injecting = False
            events_to_replay = list(self._queued_events)
            self._queued_events.clear()
            self._injecting_until = 0
            for q_ev in events_to_replay:
                if q_ev.type == e.EV_KEY:
                    await self._on_event(q_ev)
            self._injecting_until = time.time() + 0.05

    # ── text replacement ─────────────────────────────────────────────────

    async def _replace_text(
        self,
        old_word: str,
        new_word: str,
        target_layout: str | None,
        separator_kc: int | None,
        separator_shift: bool = False,
        is_undo: bool = False,
    ):
        self._is_injecting = True
        self._queued_events.clear()
        try:
            # Release all modifiers to prevent ⌥⌫ / ⇧⌫
            for mod in [e.KEY_LEFTSHIFT, e.KEY_RIGHTSHIFT, e.KEY_LEFTCTRL, e.KEY_LEFTALT]:
                self._ui.write(e.EV_KEY, mod, 0)
                self._ui.syn()

            # Erase the original word
            backspaces = len(old_word)
            if separator_kc is not None:
                backspaces += 1

            for _ in range(backspaces):
                self._ui.write(e.EV_KEY, e.KEY_BACKSPACE, 1)
                self._ui.syn()
                await asyncio.sleep(0.005)
                self._ui.write(e.EV_KEY, e.KEY_BACKSPACE, 0)
                self._ui.syn()
                await asyncio.sleep(0.005)

            # Switch layout if needed (before typing)
            old_layout = self._layout
            if target_layout and target_layout != self._layout:
                self._set_layout(target_layout)
                await asyncio.sleep(0.1)
                layout_for_typing = target_layout
            else:
                layout_for_typing = self._layout

            # Type the converted text using keycodes
            for char in new_word:
                res = self.layout_set.char_to_keycode(char, layout_for_typing)
                if res:
                    kc, shifted = res
                    if shifted:
                        self._ui.write(e.EV_KEY, e.KEY_LEFTSHIFT, 1)
                        self._ui.syn()
                        await asyncio.sleep(0.005)
                    self._ui.write(e.EV_KEY, kc, 1)
                    self._ui.syn()
                    await asyncio.sleep(0.01)
                    self._ui.write(e.EV_KEY, kc, 0)
                    self._ui.syn()
                    await asyncio.sleep(0.01)
                    if shifted:
                        self._ui.write(e.EV_KEY, e.KEY_LEFTSHIFT, 0)
                        self._ui.syn()
                        await asyncio.sleep(0.005)

            # Re-send separator if needed (preserving Shift for shifted
            # punctuation such as '?' or ':').
            if separator_kc is not None:
                if separator_shift:
                    self._ui.write(e.EV_KEY, e.KEY_LEFTSHIFT, 1)
                    self._ui.syn()
                    await asyncio.sleep(0.005)
                self._ui.write(e.EV_KEY, separator_kc, 1)
                self._ui.syn()
                await asyncio.sleep(0.01)
                self._ui.write(e.EV_KEY, separator_kc, 0)
                self._ui.syn()
                if separator_shift:
                    await asyncio.sleep(0.005)
                    self._ui.write(e.EV_KEY, e.KEY_LEFTSHIFT, 0)
                    self._ui.syn()

            self._buf.clear()
            if not is_undo:
                self._undo.push(
                    old_word, new_word, old_layout,
                    separator_kc, separator_shift,
                )
                self._sentence.clear()
                for c in new_word:
                    self._sentence.append(c)
                if separator_kc is not None:
                    sep_char = self.layout_set.keycode_to_char(
                        separator_kc, layout_for_typing, separator_shift
                    )
                    if sep_char:
                        self._sentence.append(sep_char)
                    elif separator_kc == e.KEY_SPACE:
                        self._sentence.append(" ")
                    elif separator_kc in (e.KEY_ENTER, e.KEY_KPENTER):
                        self._sentence.append("\n")
            else:
                self._sentence.clear()

            self._play_sound("message-new-instant")
        finally:
            self._is_injecting = False
            events_to_replay = list(self._queued_events)
            self._queued_events.clear()
            self._injecting_until = 0
            for q_ev in events_to_replay:
                if q_ev.type == e.EV_KEY:
                    await self._on_event(q_ev)
            self._injecting_until = time.time() + 0.05

    # ── manual convert (double-shift) ────────────────────────────────────

    async def _do_manual_convert(self):
        text = self._sentence.text
        if not text.strip():
            # Empty buffer — try undo
            if self._undo_words > 0:
                entries = self._undo.get_undo_words(self._undo_words)
                if entries:
                    await self._undo_entries(entries)
                    return
            elif self._undo.last:
                entry = self._undo.pop_last()
                self._learn_from_undo(entry)
                await self._replace_text(
                    entry.converted, entry.original,
                    entry.layout, separator_kc=entry.separator_keycode,
                    separator_shift=entry.separator_shift,
                    is_undo=True,
                )
                self.stats.record_undo()
                return
            return

        # Manual (re)conversion of what is on screen.
        if self._manual_only_last_word:
            prefix, text, suffix = self._split_last_word(text)
        else:
            prefix = suffix = ""

        words = text.split()
        target = None
        if words:
            last_word = words[-1]
            target = self.layout_set.find_best_layout(
                last_word, self._layout, threshold=-100.0, min_word_len=1
            )

        if not target:
            idx = self.layout_set.layouts.index(self._layout)
            target = self.layout_set.layouts[(idx + 1) % len(self.layout_set.layouts)]

        converted = self.layout_set.convert(text, self._layout, target)
        # When only the last word is converted we must retype the whole
        # sentence prefix so the cursor lands in the right place.
        if self._manual_only_last_word:
            await self._replace_text(
                prefix + text + suffix, prefix + converted + suffix,
                target, separator_kc=None,
            )
        else:
            await self._replace_text(text, converted, target, separator_kc=None)
        self.stats.record_manual(self._changed_char_count(text, converted))

    @staticmethod
    def _split_last_word(text: str) -> tuple[str, str, str]:
        """Split ``text`` into (prefix, last_word, trailing_whitespace).

        ``prefix + last_word + suffix`` reproduces the original string exactly,
        so the caller can retype the whole line while converting only the final
        word.
        """
        stripped = text.rstrip()
        suffix = text[len(stripped):]
        cut = max(stripped.rfind(" "), stripped.rfind("\n"), stripped.rfind("\t"))
        if cut >= 0:
            return stripped[:cut + 1], stripped[cut + 1:], suffix
        return "", stripped, suffix

    def _learn_from_undo(self, entry):
        """Remember a word the user forced back to its original form."""
        if not self._self_learning:
            return
        original = entry.original
        # Only learn plain alphabetic words (never URLs, code, etc.).
        if original.isalpha() and len(original) >= 2:
            self.layout_set.add_learned(original)
            audit_log.info(f"AUDIT: learned word '{original}' (user reverted)")
            logger.info("Learned word: %r", original)

    async def _undo_entries(self, entries: list):
        """Undo multiple entries (flexible undo by word count)."""
        if not entries:
            return
        # Undo in reverse order
        for entry in reversed(entries):
            self._learn_from_undo(entry)
            await self._replace_text(
                entry.converted, entry.original,
                entry.layout, separator_kc=entry.separator_keycode,
                separator_shift=entry.separator_shift,
                is_undo=True,
            )
        self.stats.record_undo()

    # ── layout management ────────────────────────────────────────────────

    def _set_layout(self, layout: str):
        try:
            idx = self.layout_set.layouts.index(layout)
            self.layout_backend.set_layout(idx)
            self._layout = layout
            self._layout_changed_at = time.time()
            audit_log.info(f"AUDIT: set_layout({layout})")
        except ValueError:
            pass
