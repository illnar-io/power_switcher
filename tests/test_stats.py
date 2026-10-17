"""Tests for the persistent conversion statistics."""
import tempfile
from pathlib import Path

from power_switcher.core.stats import Stats


def test_new_stats_start_at_zero():
    with tempfile.TemporaryDirectory() as td:
        s = Stats.load(Path(td) / "stats.json")
        assert s.total_conversions == 0
        assert s.first_seen > 0


def test_recording_and_persistence():
    with tempfile.TemporaryDirectory() as td:
        path = Path(td) / "stats.json"
        s = Stats.load(path)
        s.record_auto(4)
        s.record_auto(2)
        s.record_manual(3)
        s.record_snippet()
        s.record_undo()
        s.save(force=True)

        again = Stats.load(path)
        assert again.auto_conversions == 2
        assert again.manual_conversions == 1
        assert again.total_conversions == 3
        assert again.chars_retyped == 9
        assert again.snippet_expansions == 1
        assert again.undos == 1


def test_writes_are_debounced_but_force_writes():
    with tempfile.TemporaryDirectory() as td:
        path = Path(td) / "stats.json"
        s = Stats.load(path)
        s.record_auto(1)
        # Debounced: the immediate save happens on first record (last_save=0),
        # subsequent ones within 5s are skipped.
        assert path.exists()
        first = path.read_text()
        s.record_auto(1)
        assert path.read_text() == first
        s.save(force=True)
        assert path.read_text() != first


def test_reset():
    with tempfile.TemporaryDirectory() as td:
        path = Path(td) / "stats.json"
        s = Stats.load(path)
        s.record_auto(5)
        s.reset()
        assert s.total_conversions == 0
        assert Stats.load(path).total_conversions == 0


def test_corrupt_file_is_ignored():
    with tempfile.TemporaryDirectory() as td:
        path = Path(td) / "stats.json"
        path.write_text("{ this is not json")
        s = Stats.load(path)
        assert s.total_conversions == 0
