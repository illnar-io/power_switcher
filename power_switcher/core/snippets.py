"""
Snippet auto-expansion: trigger → expansion on word boundary.
Layout-independent matching (Keyboop pattern).
"""
import json
import logging
from pathlib import Path

logger = logging.getLogger(__name__)

_SNIPPETS_PATH = Path("~/.config/power_switcher/snippets.json").expanduser()


def _canonical(s: str) -> str:
    """
    Layout-independent canonical form: interpret keystrokes as if in EN layout,
    then lowercase. So '!test', '!TEST', '!еуые' all become the same canonical key.
    """
    from ..maps.keycode_map import build_char_map
    try:
        ru_to_en = build_char_map("ru", "en")
    except Exception:
        ru_to_en = {}
    result = []
    for c in s:
        mapped = ru_to_en.get(c, c)
        result.append(mapped.lower())
    return "".join(result)


class SnippetStore:
    """Ordered snippet pairs with layout-independent matching."""

    def __init__(self, path: Path | None = None):
        self._path = path or _SNIPPETS_PATH
        self._pairs: list[tuple[str, str]] = []
        self._index: dict[str, str] = {}
        self._load()

    def _load(self):
        if not self._path.exists():
            return
        try:
            data = json.loads(self._path.read_text(encoding="utf-8"))
            if isinstance(data, list):
                self._pairs = [
                    (item[0], item[1]) for item in data
                    if isinstance(item, list) and len(item) >= 2
                ]
            elif isinstance(data, dict):
                self._pairs = sorted(data.items())
            self._rebuild_index()
        except Exception as exc:
            logger.warning("Failed to load snippets from %s: %s", self._path, exc)

    def _rebuild_index(self):
        self._index = {}
        for trigger, expansion in self._pairs:
            canon = _canonical(trigger)
            if canon:
                self._index[canon] = expansion

    def _persist(self):
        self._path.parent.mkdir(parents=True, exist_ok=True)
        self._path.write_text(
            json.dumps(self._pairs, ensure_ascii=False, indent=2),
            encoding="utf-8",
        )

    def expansion(self, typed_word: str) -> str | None:
        """Return expansion if typed_word matches a snippet trigger, else None."""
        canon = _canonical(typed_word)
        return self._index.get(canon)

    def pairs(self) -> list[tuple[str, str]]:
        return list(self._pairs)

    def add(self, trigger: str, expansion: str):
        canon = _canonical(trigger)
        if not canon:
            return
        self._pairs.append((trigger, expansion))
        self._index[canon] = expansion
        self._persist()
        logger.info("Snippet added: %s → %s", trigger, expansion)

    def remove(self, trigger: str) -> bool:
        canon = _canonical(trigger)
        before = len(self._pairs)
        self._pairs = [
            (t, e) for t, e in self._pairs if _canonical(t) != canon
        ]
        removed = len(self._pairs) < before
        if removed:
            self._index.pop(canon, None)
            self._persist()
            logger.info("Snippet removed: %s", trigger)
        return removed

    def set_all(self, pairs: list[tuple[str, str]]):
        """Replace all snippets (from editor). Deduplicates by canonical key."""
        seen: dict[str, tuple[str, str]] = {}
        for trigger, expansion in pairs:
            canon = _canonical(trigger)
            if not canon:
                continue
            seen[canon] = (trigger, expansion)
        self._pairs = list(seen.values())
        self._rebuild_index()
        self._persist()

    def __len__(self) -> int:
        return len(self._pairs)
