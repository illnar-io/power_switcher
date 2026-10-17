"""Lightweight, persistent conversion statistics.

Counters are kept in ``~/.config/power_switcher/stats.json`` and are updated in
memory; writes are debounced so a fast typist does not thrash the disk.
"""

from __future__ import annotations

import json
import logging
import time
from dataclasses import asdict, dataclass, field
from pathlib import Path

logger = logging.getLogger(__name__)

_DEFAULT_STATS_PATH = Path("~/.config/power_switcher/stats.json").expanduser()


@dataclass
class Stats:
    """Conversion counters (all monotonic over the lifetime of the install)."""

    path: Path = field(default_factory=lambda: _DEFAULT_STATS_PATH)
    auto_conversions: int = 0
    manual_conversions: int = 0
    snippet_expansions: int = 0
    undos: int = 0
    chars_retyped: int = 0
    first_seen: float = 0.0
    last_conversion: float = 0.0

    _dirty: bool = field(default=False, repr=False, compare=False)
    _last_save: float = field(default=0.0, repr=False, compare=False)

    # ── persistence ─────────────────────────────────────────────────────

    @classmethod
    def load(cls, path: Path | None = None) -> "Stats":
        p = path or _DEFAULT_STATS_PATH
        stats = cls(path=p)
        if p.exists():
            try:
                data = json.loads(p.read_text(encoding="utf-8"))
                for key, value in data.items():
                    if hasattr(stats, key) and not key.startswith("_"):
                        setattr(stats, key, value)
            except Exception as exc:
                logger.warning("Failed to load stats from %s: %s", p, exc)
        if not stats.first_seen:
            stats.first_seen = time.time()
        return stats

    def save(self, force: bool = False):
        if not self._dirty and not force:
            return
        now = time.time()
        if not force and now - self._last_save < 5.0:
            return
        self._last_save = now
        self._dirty = False
        try:
            self.path.parent.mkdir(parents=True, exist_ok=True)
            data = {
                k: v for k, v in asdict(self).items()
                if not k.startswith("_") and k != "path"
            }
            self.path.write_text(
                json.dumps(data, indent=2, ensure_ascii=False),
                encoding="utf-8",
            )
        except Exception as exc:
            logger.warning("Failed to save stats: %s", exc)

    def reset(self):
        self.auto_conversions = 0
        self.manual_conversions = 0
        self.snippet_expansions = 0
        self.undos = 0
        self.chars_retyped = 0
        self.first_seen = time.time()
        self.last_conversion = 0.0
        self._dirty = True
        self.save(force=True)

    # ── recording ───────────────────────────────────────────────────────

    def record_auto(self, changed_chars: int):
        self.auto_conversions += 1
        self.chars_retyped += max(0, changed_chars)
        self.last_conversion = time.time()
        self._dirty = True
        self.save()

    def record_manual(self, changed_chars: int):
        self.manual_conversions += 1
        self.chars_retyped += max(0, changed_chars)
        self.last_conversion = time.time()
        self._dirty = True
        self.save()

    def record_snippet(self):
        self.snippet_expansions += 1
        self._dirty = True
        self.save()

    def record_undo(self):
        self.undos += 1
        self._dirty = True
        self.save()

    @property
    def total_conversions(self) -> int:
        return self.auto_conversions + self.manual_conversions
