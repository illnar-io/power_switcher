"""
Anti-resonance guard: detect conversion oscillation.
If word A → B → A within a short window, freeze auto-conversion for N seconds.
"""
import time
import logging

logger = logging.getLogger(__name__)


class AntiResonanceGuard:
    def __init__(self, freeze_seconds: float = 3.0, history_size: int = 10):
        self._freeze_seconds = freeze_seconds
        self._history_size = history_size
        self._history: list[tuple[str, str, float]] = []
        self._frozen_until: float = 0.0

    def allow(self, word: str, produced: str) -> bool:
        """
        Check if this conversion is allowed.
        Returns False if we're in a freeze window (oscillation detected).
        Records the conversion in history.
        """
        now = time.time()

        if now < self._frozen_until:
            return False

        for prev_word, prev_produced, prev_time in self._history:
            if prev_produced == word and now - prev_time < self._freeze_seconds:
                self._frozen_until = now + self._freeze_seconds
                logger.warning(
                    "Anti-resonance: oscillation detected (%d -> %d -> %d chars) — "
                    "freezing auto for %.1fs",
                    len(prev_word), len(prev_produced), len(word),
                    self._freeze_seconds,
                )
                self._history.clear()
                return False

            if word == prev_word and produced == prev_produced and now - prev_time < self._freeze_seconds:
                self._frozen_until = now + self._freeze_seconds
                logger.warning(
                    "Anti-resonance: repeated conversion (%d -> %d chars) — "
                    "freezing auto for %.1fs",
                    len(word), len(produced), self._freeze_seconds,
                )
                self._history.clear()
                return False

        self._history.append((word, produced, now))
        if len(self._history) > self._history_size:
            self._history.pop(0)

        return True

    def reset(self):
        self._history.clear()
        self._frozen_until = 0.0

    @property
    def is_frozen(self) -> bool:
        return time.time() < self._frozen_until
