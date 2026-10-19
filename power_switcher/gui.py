"""
PowerSwitcher Settings GUI — cross-platform tkinter window.
Tabs: Snippets, Exceptions, Settings, Status.
"""
import json
import os
import subprocess
import threading
import tkinter as tk
from tkinter import ttk, messagebox, filedialog
from pathlib import Path

from .config import Config
from .core.snippets import SnippetStore
from .core.app_exceptions import ExceptionStore
from .core.stats import Stats
from .maps.layout_set import LayoutSet


_CONFIG_DIR = Path("~/.config/power_switcher").expanduser()
_CONFIG_PATH = _CONFIG_DIR / "config.toml"


class SnippetsTab(ttk.Frame):
    def __init__(self, parent, store: SnippetStore):
        super().__init__(parent)
        self.store = store
        self._build_ui()
        self._refresh()

    def _build_ui(self):
        toolbar = ttk.Frame(self)
        toolbar.pack(fill=tk.X, padx=5, pady=5)

        ttk.Button(toolbar, text="Add", command=self._add).pack(side=tk.LEFT, padx=2)
        ttk.Button(toolbar, text="Remove", command=self._remove).pack(side=tk.LEFT, padx=2)
        ttk.Button(toolbar, text="Edit", command=self._edit).pack(side=tk.LEFT, padx=2)
        ttk.Button(toolbar, text="Refresh", command=self._refresh).pack(side=tk.LEFT, padx=2)

        columns = ("trigger", "expansion")
        self.tree = ttk.Treeview(self, columns=columns, show="headings", selectmode="browse")
        self.tree.heading("trigger", text="Trigger")
        self.tree.heading("expansion", text="Expansion")
        self.tree.column("trigger", width=150)
        self.tree.column("expansion", width=400)

        scrollbar = ttk.Scrollbar(self, orient=tk.VERTICAL, command=self.tree.yview)
        self.tree.configure(yscrollcommand=scrollbar.set)

        self.tree.pack(side=tk.LEFT, fill=tk.BOTH, expand=True, padx=(5, 0), pady=5)
        scrollbar.pack(side=tk.RIGHT, fill=tk.Y, padx=(0, 5), pady=5)

        self.tree.bind("<Double-1>", lambda e: self._edit())

    def _refresh(self):
        for item in self.tree.get_children():
            self.tree.delete(item)
        for trigger, expansion in self.store.pairs():
            self.tree.insert("", tk.END, values=(trigger, expansion))

    def _add(self):
        dlg = _SnippetDialog(self, "Add Snippet")
        self.wait_window(dlg)
        if dlg.result:
            trigger, expansion = dlg.result
            self.store.add(trigger, expansion)
            self._refresh()

    def _edit(self):
        sel = self.tree.selection()
        if not sel:
            return
        vals = self.tree.item(sel[0], "values")
        old_trigger, old_expansion = vals[0], vals[1]
        dlg = _SnippetDialog(self, "Edit Snippet", old_trigger, old_expansion)
        self.wait_window(dlg)
        if dlg.result:
            trigger, expansion = dlg.result
            self.store.remove(old_trigger)
            self.store.add(trigger, expansion)
            self._refresh()

    def _remove(self):
        sel = self.tree.selection()
        if not sel:
            return
        vals = self.tree.item(sel[0], "values")
        if messagebox.askyesno("Remove", f"Remove snippet '{vals[0]}'?"):
            self.store.remove(vals[0])
            self._refresh()


class _SnippetDialog(tk.Toplevel):
    def __init__(self, parent, title, trigger="", expansion=""):
        super().__init__(parent)
        self.title(title)
        self.result = None
        self.transient(parent)
        self.grab_set()
        self.resizable(False, False)

        frame = ttk.Frame(self, padding=10)
        frame.pack(fill=tk.BOTH, expand=True)

        ttk.Label(frame, text="Trigger:").grid(row=0, column=0, sticky=tk.W, pady=2)
        self.trigger_var = tk.StringVar(value=trigger)
        ttk.Entry(frame, textvariable=self.trigger_var, width=30).grid(row=0, column=1, pady=2)

        ttk.Label(frame, text="Expansion:").grid(row=1, column=0, sticky=tk.W, pady=2)
        self.expansion_var = tk.StringVar(value=expansion)
        ttk.Entry(frame, textvariable=self.expansion_var, width=50).grid(row=1, column=1, pady=2)

        btn_frame = ttk.Frame(frame)
        btn_frame.grid(row=2, column=0, columnspan=2, pady=(10, 0))
        ttk.Button(btn_frame, text="OK", command=self._ok).pack(side=tk.LEFT, padx=5)
        ttk.Button(btn_frame, text="Cancel", command=self.destroy).pack(side=tk.LEFT, padx=5)

        self.bind("<Return>", lambda e: self._ok())
        self.bind("<Escape>", lambda e: self.destroy())

    def _ok(self):
        t = self.trigger_var.get().strip()
        e = self.expansion_var.get().strip()
        if t:
            self.result = (t, e)
        self.destroy()


class ExceptionsTab(ttk.Frame):
    def __init__(self, parent, store: ExceptionStore):
        super().__init__(parent)
        self.store = store
        self._build_ui()
        self._refresh()

    def _build_ui(self):
        toolbar = ttk.Frame(self)
        toolbar.pack(fill=tk.X, padx=5, pady=5)

        ttk.Button(toolbar, text="Add", command=self._add).pack(side=tk.LEFT, padx=2)
        ttk.Button(toolbar, text="Remove", command=self._remove).pack(side=tk.LEFT, padx=2)
        ttk.Button(toolbar, text="Refresh", command=self._refresh).pack(side=tk.LEFT, padx=2)

        columns = ("name", "mode")
        self.tree = ttk.Treeview(self, columns=columns, show="headings", selectmode="browse")
        self.tree.heading("name", text="Application")
        self.tree.heading("mode", text="Mode (off/soft/default)")
        self.tree.column("name", width=300)
        self.tree.column("mode", width=200)

        scrollbar = ttk.Scrollbar(self, orient=tk.VERTICAL, command=self.tree.yview)
        self.tree.configure(yscrollcommand=scrollbar.set)

        self.tree.pack(side=tk.LEFT, fill=tk.BOTH, expand=True, padx=(5, 0), pady=5)
        scrollbar.pack(side=tk.RIGHT, fill=tk.Y, padx=(0, 5), pady=5)

    def _refresh(self):
        for item in self.tree.get_children():
            self.tree.delete(item)
        for name, mode in self.store.all().items():
            self.tree.insert("", tk.END, values=(name, mode))

    def _add(self):
        dlg = _ExceptionDialog(self, "Add Exception")
        self.wait_window(dlg)
        if dlg.result:
            name, mode = dlg.result
            self.store.add(name, mode)
            self._refresh()

    def _remove(self):
        sel = self.tree.selection()
        if not sel:
            return
        vals = self.tree.item(sel[0], "values")
        if messagebox.askyesno("Remove", f"Remove exception for '{vals[0]}'?"):
            self.store.remove(vals[0])
            self._refresh()


class _ExceptionDialog(tk.Toplevel):
    def __init__(self, parent, title, name="", mode="off"):
        super().__init__(parent)
        self.title(title)
        self.result = None
        self.transient(parent)
        self.grab_set()
        self.resizable(False, False)

        frame = ttk.Frame(self, padding=10)
        frame.pack(fill=tk.BOTH, expand=True)

        ttk.Label(frame, text="Application name:").grid(row=0, column=0, sticky=tk.W, pady=2)
        self.name_var = tk.StringVar(value=name)
        ttk.Entry(frame, textvariable=self.name_var, width=40).grid(row=0, column=1, pady=2)

        ttk.Label(frame, text="Mode:").grid(row=1, column=0, sticky=tk.W, pady=2)
        self.mode_var = tk.StringVar(value=mode)
        mode_frame = ttk.Frame(frame)
        mode_frame.grid(row=1, column=1, sticky=tk.W, pady=2)
        ttk.Radiobutton(mode_frame, text="off (disable auto)", variable=self.mode_var, value="off").pack(side=tk.LEFT)
        ttk.Radiobutton(mode_frame, text="soft (manual only)", variable=self.mode_var, value="soft").pack(side=tk.LEFT, padx=5)
        ttk.Radiobutton(mode_frame, text="default", variable=self.mode_var, value="default").pack(side=tk.LEFT)

        btn_frame = ttk.Frame(frame)
        btn_frame.grid(row=2, column=0, columnspan=2, pady=(10, 0))
        ttk.Button(btn_frame, text="OK", command=self._ok).pack(side=tk.LEFT, padx=5)
        ttk.Button(btn_frame, text="Cancel", command=self.destroy).pack(side=tk.LEFT, padx=5)

        self.bind("<Return>", lambda e: self._ok())
        self.bind("<Escape>", lambda e: self.destroy())

    def _ok(self):
        n = self.name_var.get().strip()
        if n:
            self.result = (n, self.mode_var.get())
        self.destroy()


class SettingsTab(ttk.Frame):
    def __init__(self, parent, config: Config):
        super().__init__(parent)
        self.config = config
        self._build_ui()

    def _build_ui(self):
        canvas = tk.Canvas(self)
        scrollbar = ttk.Scrollbar(self, orient=tk.VERTICAL, command=canvas.yview)
        scroll_frame = ttk.Frame(canvas)

        scroll_frame.bind("<Configure>", lambda e: canvas.configure(scrollregion=canvas.bbox("all")))
        canvas.create_window((0, 0), window=scroll_frame, anchor=tk.NW)
        canvas.configure(yscrollcommand=scrollbar.set)

        canvas.pack(side=tk.LEFT, fill=tk.BOTH, expand=True, padx=5, pady=5)
        scrollbar.pack(side=tk.RIGHT, fill=tk.Y)

        row = 0

        # ── Layouts ─────────────────────────────────────────────────────
        ttk.Label(scroll_frame, text="Layouts", font=("", 12, "bold")).grid(row=row, column=0, columnspan=2, sticky=tk.W, pady=(10, 5))
        row += 1

        ttk.Label(scroll_frame, text="Layouts (comma-separated):").grid(row=row, column=0, sticky=tk.W, pady=2)
        self.layouts_var = tk.StringVar(value=", ".join(self.config.layouts))
        ttk.Entry(scroll_frame, textvariable=self.layouts_var, width=40).grid(row=row, column=1, sticky=tk.W, pady=2)
        row += 1

        ttk.Label(scroll_frame, text="Default layout:").grid(row=row, column=0, sticky=tk.W, pady=2)
        self.default_layout_var = tk.StringVar(value=self.config.default_layout)
        ttk.Entry(scroll_frame, textvariable=self.default_layout_var, width=20).grid(row=row, column=1, sticky=tk.W, pady=2)
        row += 1

        # ── Detection ───────────────────────────────────────────────────
        ttk.Label(scroll_frame, text="Detection", font=("", 12, "bold")).grid(row=row, column=0, columnspan=2, sticky=tk.W, pady=(15, 5))
        row += 1

        ttk.Label(scroll_frame, text="Threshold:").grid(row=row, column=0, sticky=tk.W, pady=2)
        self.threshold_var = tk.StringVar(value=str(self.config.threshold))
        ttk.Entry(scroll_frame, textvariable=self.threshold_var, width=10).grid(row=row, column=1, sticky=tk.W, pady=2)
        row += 1

        ttk.Label(scroll_frame, text="Min word length:").grid(row=row, column=0, sticky=tk.W, pady=2)
        self.min_word_len_var = tk.StringVar(value=str(self.config.min_word_len))
        ttk.Entry(scroll_frame, textvariable=self.min_word_len_var, width=10).grid(row=row, column=1, sticky=tk.W, pady=2)
        row += 1

        self.auto_detect_var = tk.BooleanVar(value=self.config.auto_detect)
        ttk.Checkbutton(scroll_frame, text="Enable auto-detection", variable=self.auto_detect_var).grid(row=row, column=0, columnspan=2, sticky=tk.W, pady=2)
        row += 1

        # ── Triggers ────────────────────────────────────────────────────
        ttk.Label(scroll_frame, text="Triggers", font=("", 12, "bold")).grid(row=row, column=0, columnspan=2, sticky=tk.W, pady=(15, 5))
        row += 1

        self.trigger_space_var = tk.BooleanVar(value=self.config.trigger_space)
        ttk.Checkbutton(scroll_frame, text="Space triggers auto-detection", variable=self.trigger_space_var).grid(row=row, column=0, columnspan=2, sticky=tk.W, pady=2)
        row += 1

        self.trigger_enter_var = tk.BooleanVar(value=self.config.trigger_enter)
        ttk.Checkbutton(scroll_frame, text="Enter triggers auto-detection", variable=self.trigger_enter_var).grid(row=row, column=0, columnspan=2, sticky=tk.W, pady=2)
        row += 1

        self.trigger_tab_var = tk.BooleanVar(value=self.config.trigger_tab)
        ttk.Checkbutton(scroll_frame, text="Tab triggers auto-detection", variable=self.trigger_tab_var).grid(row=row, column=0, columnspan=2, sticky=tk.W, pady=2)
        row += 1

        self.trigger_punctuation_var = tk.BooleanVar(value=self.config.trigger_punctuation)
        ttk.Checkbutton(scroll_frame, text="Punctuation (. , ; ! ?) triggers auto-detection", variable=self.trigger_punctuation_var).grid(row=row, column=0, columnspan=2, sticky=tk.W, pady=2)
        row += 1

        self.snippet_space_var = tk.BooleanVar(value=self.config.snippet_expand_space)
        ttk.Checkbutton(scroll_frame, text="Space triggers snippet expansion", variable=self.snippet_space_var).grid(row=row, column=0, columnspan=2, sticky=tk.W, pady=2)
        row += 1

        self.snippet_enter_var = tk.BooleanVar(value=self.config.snippet_expand_enter)
        ttk.Checkbutton(scroll_frame, text="Enter triggers snippet expansion", variable=self.snippet_enter_var).grid(row=row, column=0, columnspan=2, sticky=tk.W, pady=2)
        row += 1

        self.snippet_tab_var = tk.BooleanVar(value=self.config.snippet_expand_tab)
        ttk.Checkbutton(scroll_frame, text="Tab triggers snippet expansion", variable=self.snippet_tab_var).grid(row=row, column=0, columnspan=2, sticky=tk.W, pady=2)
        row += 1

        # ── Undo ────────────────────────────────────────────────────────
        ttk.Label(scroll_frame, text="Undo", font=("", 12, "bold")).grid(row=row, column=0, columnspan=2, sticky=tk.W, pady=(15, 5))
        row += 1

        ttk.Label(scroll_frame, text="Undo words (0=single):").grid(row=row, column=0, sticky=tk.W, pady=2)
        self.undo_words_var = tk.StringVar(value=str(self.config.undo_words))
        ttk.Entry(scroll_frame, textvariable=self.undo_words_var, width=10).grid(row=row, column=1, sticky=tk.W, pady=2)
        row += 1

        ttk.Label(scroll_frame, text="Undo seconds:").grid(row=row, column=0, sticky=tk.W, pady=2)
        self.undo_seconds_var = tk.StringVar(value=str(self.config.undo_seconds))
        ttk.Entry(scroll_frame, textvariable=self.undo_seconds_var, width=10).grid(row=row, column=1, sticky=tk.W, pady=2)
        row += 1

        # ── Behavior ────────────────────────────────────────────────────
        ttk.Label(scroll_frame, text="Behavior", font=("", 12, "bold")).grid(row=row, column=0, columnspan=2, sticky=tk.W, pady=(15, 5))
        row += 1

        self.developer_mode_var = tk.BooleanVar(value=self.config.developer_mode)
        ttk.Checkbutton(scroll_frame, text="Developer mode (disable auto in IDEs/terminals)", variable=self.developer_mode_var).grid(row=row, column=0, columnspan=2, sticky=tk.W, pady=2)
        row += 1

        self.arrows_cancel_var = tk.BooleanVar(value=self.config.arrows_cancel)
        ttk.Checkbutton(scroll_frame, text="Arrow keys clear buffer", variable=self.arrows_cancel_var).grid(row=row, column=0, columnspan=2, sticky=tk.W, pady=2)
        row += 1

        self.live_fix_var = tk.BooleanVar(value=self.config.live_fix_enabled)
        ttk.Checkbutton(scroll_frame, text="Mid-word eager conversion (live fix)", variable=self.live_fix_var).grid(row=row, column=0, columnspan=2, sticky=tk.W, pady=2)
        row += 1

        self.lr_shift_pause_var = tk.BooleanVar(value=self.config.lr_shift_pause)
        ttk.Checkbutton(scroll_frame, text="L+R Shift toggles auto-detect pause", variable=self.lr_shift_pause_var).grid(row=row, column=0, columnspan=2, sticky=tk.W, pady=2)
        row += 1

        self.shift_tap_switch_var = tk.BooleanVar(value=self.config.shift_tap_switch_layout)
        ttk.Checkbutton(scroll_frame, text="Lone Shift tap switches layout (legacy; prefer Super+Space)", variable=self.shift_tap_switch_var).grid(row=row, column=0, columnspan=2, sticky=tk.W, pady=2)
        row += 1

        self.manual_only_last_word_var = tk.BooleanVar(value=self.config.manual_only_last_word)
        ttk.Checkbutton(scroll_frame, text="Manual convert: only the last word", variable=self.manual_only_last_word_var).grid(row=row, column=0, columnspan=2, sticky=tk.W, pady=2)
        row += 1

        self.self_learning_var = tk.BooleanVar(value=self.config.self_learning)
        ttk.Checkbutton(scroll_frame, text="Learn words you un-convert (never convert again)", variable=self.self_learning_var).grid(row=row, column=0, columnspan=2, sticky=tk.W, pady=2)
        row += 1

        self.sound_enabled_var = tk.BooleanVar(value=self.config.sound_enabled)
        ttk.Checkbutton(scroll_frame, text="Play sound on conversion", variable=self.sound_enabled_var).grid(row=row, column=0, columnspan=2, sticky=tk.W, pady=2)
        row += 1

        ttk.Label(scroll_frame, text="Don't correct for N s after switch (0=off):").grid(row=row, column=0, sticky=tk.W, pady=2)
        self.no_correct_after_switch_var = tk.StringVar(value=str(self.config.no_correct_after_switch))
        ttk.Entry(scroll_frame, textvariable=self.no_correct_after_switch_var, width=10).grid(row=row, column=1, sticky=tk.W, pady=2)
        row += 1

        # ── Hotkeys ─────────────────────────────────────────────────────
        ttk.Label(scroll_frame, text="Hotkeys", font=("", 12, "bold")).grid(row=row, column=0, columnspan=2, sticky=tk.W, pady=(15, 5))
        row += 1

        ttk.Label(scroll_frame, text="Convert hotkey:").grid(row=row, column=0, sticky=tk.W, pady=2)
        self.hotkey_convert_var = tk.StringVar(value=self.config.hotkey_convert)
        ttk.Entry(scroll_frame, textvariable=self.hotkey_convert_var, width=20).grid(row=row, column=1, sticky=tk.W, pady=2)
        row += 1

        ttk.Label(scroll_frame, text="Selection hotkey:").grid(row=row, column=0, sticky=tk.W, pady=2)
        self.hotkey_selection_var = tk.StringVar(value=self.config.hotkey_selection)
        ttk.Entry(scroll_frame, textvariable=self.hotkey_selection_var, width=20).grid(row=row, column=1, sticky=tk.W, pady=2)
        row += 1

        # ── Save ────────────────────────────────────────────────────────
        ttk.Button(scroll_frame, text="Save Settings", command=self._save).grid(row=row, column=0, columnspan=2, pady=(20, 10))
        row += 1

    def _save(self):
        try:
            self.config.layouts = [x.strip() for x in self.layouts_var.get().split(",") if x.strip()]
            self.config.default_layout = self.default_layout_var.get().strip()
            self.config.threshold = float(self.threshold_var.get())
            self.config.min_word_len = int(self.min_word_len_var.get())
            self.config.auto_detect = self.auto_detect_var.get()
            self.config.trigger_space = self.trigger_space_var.get()
            self.config.trigger_enter = self.trigger_enter_var.get()
            self.config.trigger_tab = self.trigger_tab_var.get()
            self.config.trigger_punctuation = self.trigger_punctuation_var.get()
            self.config.snippet_expand_space = self.snippet_space_var.get()
            self.config.snippet_expand_enter = self.snippet_enter_var.get()
            self.config.snippet_expand_tab = self.snippet_tab_var.get()
            self.config.undo_words = int(self.undo_words_var.get())
            self.config.undo_seconds = float(self.undo_seconds_var.get())
            self.config.developer_mode = self.developer_mode_var.get()
            self.config.arrows_cancel = self.arrows_cancel_var.get()
            self.config.live_fix_enabled = self.live_fix_var.get()
            self.config.lr_shift_pause = self.lr_shift_pause_var.get()
            self.config.shift_tap_switch_layout = self.shift_tap_switch_var.get()
            self.config.manual_only_last_word = self.manual_only_last_word_var.get()
            self.config.self_learning = self.self_learning_var.get()
            self.config.sound_enabled = self.sound_enabled_var.get()
            self.config.no_correct_after_switch = float(self.no_correct_after_switch_var.get())
            self.config.hotkey_convert = self.hotkey_convert_var.get().strip()
            self.config.hotkey_selection = self.hotkey_selection_var.get().strip()
            self._write_config()
            messagebox.showinfo("Saved", "Settings saved successfully.")
        except Exception as exc:
            messagebox.showerror("Error", f"Failed to save: {exc}")

    def _write_config(self):
        c = self.config
        _CONFIG_DIR.mkdir(parents=True, exist_ok=True)
        lines = [
            "[general]",
            f"layouts = {c.layouts!r}",
            f"default_layout = \"{c.default_layout}\"",
            f"auto_detect = {str(c.auto_detect).lower()}",
            f"developer_mode = {str(c.developer_mode).lower()}",
            f"arrows_cancel = {str(c.arrows_cancel).lower()}",
            f"live_fix_enabled = {str(c.live_fix_enabled).lower()}",
            f"lr_shift_pause = {str(c.lr_shift_pause).lower()}",
            f"shift_tap_switch_layout = {str(c.shift_tap_switch_layout).lower()}",
            f"manual_only_last_word = {str(c.manual_only_last_word).lower()}",
            f"self_learning = {str(c.self_learning).lower()}",
            f"sound_enabled = {str(c.sound_enabled).lower()}",
            f"no_correct_after_switch = {c.no_correct_after_switch}",
            f"hotkey_convert = \"{c.hotkey_convert}\"",
            f"hotkey_selection = \"{c.hotkey_selection}\"",
            "",
            "[detection]",
            f"threshold = {c.threshold}",
            f"min_word_len = {c.min_word_len}",
            "",
            "[triggers]",
            f"space = {str(c.trigger_space).lower()}",
            f"enter = {str(c.trigger_enter).lower()}",
            f"tab = {str(c.trigger_tab).lower()}",
            f"punctuation = {str(c.trigger_punctuation).lower()}",
            f"snippet_space = {str(c.snippet_expand_space).lower()}",
            f"snippet_enter = {str(c.snippet_expand_enter).lower()}",
            f"snippet_tab = {str(c.snippet_expand_tab).lower()}",
            "",
            "[undo]",
            f"words = {c.undo_words}",
            f"seconds = {c.undo_seconds}",
            "",
        ]
        _CONFIG_PATH.write_text("\n".join(lines), encoding="utf-8")


class WordsTab(ttk.Frame):
    """Manage learned words and word-level exceptions (never auto-converted)."""

    def __init__(self, parent):
        super().__init__(parent)
        self.layout_set = LayoutSet.create(["en", "ru"])
        self._build_ui()
        self._refresh()

    def _build_ui(self):
        toolbar = ttk.Frame(self)
        toolbar.pack(fill=tk.X, padx=5, pady=5)
        ttk.Button(toolbar, text="Add Word", command=self._add).pack(side=tk.LEFT, padx=2)
        ttk.Button(toolbar, text="Remove", command=self._remove).pack(side=tk.LEFT, padx=2)
        ttk.Button(toolbar, text="Refresh", command=self._refresh).pack(side=tk.LEFT, padx=2)

        columns = ("word", "kind")
        self.tree = ttk.Treeview(self, columns=columns, show="headings", selectmode="browse")
        self.tree.heading("word", text="Word")
        self.tree.heading("kind", text="Kind")
        self.tree.column("word", width=250)
        self.tree.column("kind", width=200)
        self.tree.pack(fill=tk.BOTH, expand=True, padx=5, pady=5)

        ttk.Label(
            self,
            text="Learned words are added automatically when you undo a conversion.",
        ).pack(anchor=tk.W, padx=5, pady=(0, 5))

    def _refresh(self):
        for item in self.tree.get_children():
            self.tree.delete(item)
        for word in self.layout_set.learned_words():
            self.tree.insert("", tk.END, values=(word, "learned"))
        for word in self.layout_set.exceptions():
            self.tree.insert("", tk.END, values=(word, "exception"))

    def _add(self):
        dlg = _WordDialog(self)
        self.wait_window(dlg)
        if dlg.result:
            self.layout_set.add_exception(dlg.result)
            self._refresh()

    def _remove(self):
        sel = self.tree.selection()
        if not sel:
            return
        word = self.tree.item(sel[0], "values")[0]
        if messagebox.askyesno("Remove", f"Stop protecting '{word}'?"):
            self.layout_set.remove_exception(word)
            self.layout_set.remove_learned(word)
            self._refresh()


class _WordDialog(tk.Toplevel):
    def __init__(self, parent):
        super().__init__(parent)
        self.title("Add Word Exception")
        self.result = None
        self.transient(parent)
        self.grab_set()
        self.resizable(False, False)

        frame = ttk.Frame(self, padding=10)
        frame.pack(fill=tk.BOTH, expand=True)
        ttk.Label(frame, text="Word:").grid(row=0, column=0, sticky=tk.W, pady=2)
        self.word_var = tk.StringVar()
        ttk.Entry(frame, textvariable=self.word_var, width=30).grid(row=0, column=1, pady=2)

        btn_frame = ttk.Frame(frame)
        btn_frame.grid(row=1, column=0, columnspan=2, pady=(10, 0))
        ttk.Button(btn_frame, text="OK", command=self._ok).pack(side=tk.LEFT, padx=5)
        ttk.Button(btn_frame, text="Cancel", command=self.destroy).pack(side=tk.LEFT, padx=5)
        self.bind("<Return>", lambda e: self._ok())
        self.bind("<Escape>", lambda e: self.destroy())

    def _ok(self):
        w = self.word_var.get().strip()
        if w:
            self.result = w
        self.destroy()


class StatusTab(ttk.Frame):
    def __init__(self, parent):
        super().__init__(parent)
        self._build_ui()

    def _build_ui(self):
        info_frame = ttk.LabelFrame(self, text="Daemon Status", padding=10)
        info_frame.pack(fill=tk.X, padx=10, pady=10)

        self.status_label = ttk.Label(info_frame, text="Checking...", font=("", 11))
        self.status_label.pack(anchor=tk.W, pady=2)

        self.pid_label = ttk.Label(info_frame, text="PID: —")
        self.pid_label.pack(anchor=tk.W, pady=2)

        self.uptime_label = ttk.Label(info_frame, text="Uptime: —")
        self.uptime_label.pack(anchor=tk.W, pady=2)

        self.stats_label = ttk.Label(info_frame, text="Stats: —")
        self.stats_label.pack(anchor=tk.W, pady=2)

        btn_frame = ttk.Frame(info_frame)
        btn_frame.pack(fill=tk.X, pady=(10, 0))
        ttk.Button(btn_frame, text="Refresh", command=self._refresh).pack(side=tk.LEFT, padx=2)
        ttk.Button(btn_frame, text="Start Daemon", command=self._start_daemon).pack(side=tk.LEFT, padx=2)
        ttk.Button(btn_frame, text="Stop Daemon", command=self._stop_daemon).pack(side=tk.LEFT, padx=2)

        # Log viewer
        log_frame = ttk.LabelFrame(self, text="Recent Log", padding=5)
        log_frame.pack(fill=tk.BOTH, expand=True, padx=10, pady=(0, 10))

        self.log_text = tk.Text(log_frame, height=15, state=tk.DISABLED, wrap=tk.WORD)
        log_scroll = ttk.Scrollbar(log_frame, orient=tk.VERTICAL, command=self.log_text.yview)
        self.log_text.configure(yscrollcommand=log_scroll.set)
        self.log_text.pack(side=tk.LEFT, fill=tk.BOTH, expand=True)
        log_scroll.pack(side=tk.RIGHT, fill=tk.Y)

        self._refresh()

    def _refresh(self):
        try:
            r = subprocess.run(
                ["systemctl", "--user", "is-active", "power_switcher"],
                capture_output=True, text=True, timeout=2,
            )
            active = r.stdout.strip() == "active"
            self.status_label.config(
                text="Running" if active else "Not running",
                foreground="green" if active else "red",
            )

            if active:
                r2 = subprocess.run(
                    ["systemctl", "--user", "show", "power_switcher", "--property=MainPID,ActiveEnterTimestamp"],
                    capture_output=True, text=True, timeout=2,
                )
                for line in r2.stdout.splitlines():
                    if line.startswith("MainPID="):
                        self.pid_label.config(text=f"PID: {line.split('=', 1)[1]}")
                    elif line.startswith("ActiveEnterTimestamp="):
                        self.uptime_label.config(text=f"Since: {line.split('=', 1)[1]}")
            else:
                self.pid_label.config(text="PID: —")
                self.uptime_label.config(text="Uptime: —")
        except Exception as exc:
            self.status_label.config(text=f"Error: {exc}", foreground="red")

        self._load_stats()
        self._load_log()

    def _load_stats(self):
        try:
            s = Stats.load()
            self.stats_label.config(
                text=(
                    f"Conversions: {s.total_conversions} "
                    f"(auto {s.auto_conversions} / manual {s.manual_conversions})  "
                    f"Snippets: {s.snippet_expansions}  Undos: {s.undos}"
                )
            )
        except Exception:
            self.stats_label.config(text="Stats: —")

    def _load_log(self):
        log_path = Path("~/.cache/power_switcher/audit.log").expanduser()
        self.log_text.config(state=tk.NORMAL)
        self.log_text.delete("1.0", tk.END)
        if log_path.exists():
            lines = log_path.read_text(encoding="utf-8", errors="ignore").splitlines()[-50:]
            self.log_text.insert(tk.END, "\n".join(lines))
        else:
            self.log_text.insert(tk.END, "No log file found.")
        self.log_text.config(state=tk.DISABLED)
        self.log_text.see(tk.END)

    def _start_daemon(self):
        try:
            subprocess.run(
                ["systemctl", "--user", "start", "power_switcher"],
                capture_output=True, timeout=5,
            )
            self.after(500, self._refresh)
        except Exception as exc:
            messagebox.showerror("Error", f"Failed to start: {exc}")

    def _stop_daemon(self):
        try:
            subprocess.run(
                ["systemctl", "--user", "stop", "power_switcher"],
                capture_output=True, timeout=5,
            )
            self.after(500, self._refresh)
        except Exception as exc:
            messagebox.showerror("Error", f"Failed to stop: {exc}")


class SettingsWindow(tk.Tk):
    def __init__(self):
        super().__init__()
        self.title("PowerSwitcher Settings")
        self.geometry("700x550")
        self.minsize(600, 450)

        self.config = Config.load(str(_CONFIG_PATH))
        self.snippet_store = SnippetStore()
        self.exception_store = ExceptionStore()

        self._build_ui()

    def _build_ui(self):
        notebook = ttk.Notebook(self)
        notebook.pack(fill=tk.BOTH, expand=True, padx=5, pady=5)

        notebook.add(SnippetsTab(notebook, self.snippet_store), text="Snippets")
        notebook.add(ExceptionsTab(notebook, self.exception_store), text="Exceptions")
        notebook.add(WordsTab(notebook), text="Words")
        notebook.add(SettingsTab(notebook, self.config), text="Settings")
        notebook.add(StatusTab(notebook), text="Status")

        # Status bar
        self.status_bar = ttk.Label(self, text="PowerSwitcher v0.1.0", relief=tk.SUNKEN, anchor=tk.W)
        self.status_bar.pack(fill=tk.X, side=tk.BOTTOM)


def main():
    app = SettingsWindow()
    app.mainloop()


if __name__ == "__main__":
    main()
