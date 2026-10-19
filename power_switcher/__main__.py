import asyncio
import json
import logging
import os
from pathlib import Path

import click
from evdev import ecodes as e

from .config import Config
from .core.key_processor import KeyProcessor
from .core.snippets import SnippetStore
from .core.app_exceptions import ExceptionStore
from .core.stats import Stats
from .layout.detector import detect_layout_backend
from .maps.layout_set import LayoutSet


@click.group()
def cli():
    """PowerSwitcher — DE-independent layout auto-switcher for Linux Wayland"""


@cli.command()
@click.option("--config", default="~/.config/power_switcher/config.toml")
@click.option("-v", "--verbose", is_flag=True)
@click.option("--no-auto", is_flag=True, help="Disable auto-detect, hotkey only")
def daemon(config, verbose, no_auto):
    """Run the layout auto-switcher daemon"""
    logging.basicConfig(
        level=logging.DEBUG if verbose else logging.INFO,
        format="%(asctime)s [%(name)s] %(levelname)s: %(message)s",
    )
    cfg = Config.load(os.path.expanduser(config))
    backend = detect_layout_backend()

    profiles_dir = Path(__file__).parent / "detection" / "profiles"
    words_path = Path(cfg.words_file).expanduser() if cfg.words_file else None
    layout_set = LayoutSet.create(cfg.layouts, profiles_dir, words_path=words_path)

    snippets_path = Path(cfg.snippets_file).expanduser() if cfg.snippets_file else None
    exceptions_path = Path(cfg.exceptions_file).expanduser() if cfg.exceptions_file else None
    stats_path = Path(cfg.stats_file).expanduser() if cfg.stats_file else None

    processor = KeyProcessor(
        layout_backend=backend,
        layout_set=layout_set,
        current_layout=cfg.default_layout,
        threshold=cfg.threshold,
        min_word_len=cfg.min_word_len,
        # Snippets
        snippet_expand_space=cfg.snippet_expand_space,
        snippet_expand_enter=cfg.snippet_expand_enter,
        snippet_expand_tab=cfg.snippet_expand_tab,
        snippets_path=snippets_path,
        # Triggers
        trigger_space=cfg.trigger_space,
        trigger_enter=cfg.trigger_enter,
        trigger_tab=cfg.trigger_tab,
        trigger_punctuation=cfg.trigger_punctuation,
        # Undo
        undo_words=cfg.undo_words,
        undo_seconds=cfg.undo_seconds,
        # Behavior
        developer_mode=cfg.developer_mode,
        arrows_cancel=cfg.arrows_cancel,
        live_fix_enabled=cfg.live_fix_enabled,
        no_correct_after_switch=cfg.no_correct_after_switch,
        lr_shift_pause=cfg.lr_shift_pause,
        manual_only_last_word=cfg.manual_only_last_word,
        sound_enabled=cfg.sound_enabled,
        self_learning=cfg.self_learning,
        shift_tap_switch_layout=cfg.shift_tap_switch_layout,
        # Stats
        stats=Stats.load(stats_path),
        # Exceptions
        exceptions_path=exceptions_path,
    )

    click.echo(f"Layouts:      {' ↔ '.join(cfg.layouts)}")
    click.echo(f"Auto-detect:  {'OFF' if no_auto else 'ON'}")
    click.echo(f"Triggers:     space={cfg.trigger_space} enter={cfg.trigger_enter} tab={cfg.trigger_tab} punctuation={cfg.trigger_punctuation}")
    click.echo(f"Snippets:     {len(processor._snippets)} loaded")
    click.echo(f"Undo:         words={cfg.undo_words} seconds={cfg.undo_seconds}")
    click.echo(f"Hotkeys:      L+R Shift = pause/resume, double Shift = convert/undo")
    click.echo(
        "Layout switch: "
        + ("lone Shift tap (legacy)" if cfg.shift_tap_switch_layout else "desktop shortcut (Super+Space)")
    )

    asyncio.run(processor.run())


@cli.command()
def setup():
    """Generate default config and show udev rules"""
    config_path = Path("~/.config/power_switcher/config.toml").expanduser()
    if not config_path.exists():
        config_path.parent.mkdir(parents=True, exist_ok=True)
        with open(config_path, "w") as f:
            f.write(
                '[general]\n'
                'layouts = ["en", "ru"]\n'
                'default_layout = "en"\n'
                '\n'
                'developer_mode = false\n'
                'arrows_cancel = true\n'
                'live_fix_enabled = true\n'
                'no_correct_after_switch = 1.5\n'
                'lr_shift_pause = true\n'
                'manual_only_last_word = false\n'
                'sound_enabled = true\n'
                'self_learning = true\n'
                'shift_tap_switch_layout = false\n'
                '\n'
                '[detection]\n'
                'threshold = 2.5\n'
                'min_word_len = 3\n'
                '\n'
                '[triggers]\n'
                'space = true\n'
                'enter = true\n'
                'tab = true\n'
                'punctuation = true\n'
                'snippet_space = true\n'
                'snippet_enter = true\n'
                'snippet_tab = true\n'
                '\n'
                '[undo]\n'
                'words = 0\n'
                'seconds = 3.0\n'
            )
        click.echo(f"Created default config at {config_path}")

    click.echo("\nTo allow running without sudo, add these udev rules to /etc/udev/rules.d/99-power_switcher.rules:")
    click.echo('KERNEL=="uinput", GROUP="input", MODE="0660"')
    click.echo('KERNEL=="event*", GROUP="input", MODE="0660"')
    click.echo("\nThen run: sudo udevadm control --reload-rules && sudo udevadm trigger")
    click.echo("And ensure your user is in the 'input' group: sudo usermod -aG input $USER")


@cli.command()
@click.option("--config", default="~/.config/power_switcher/config.toml")
def debug(config):
    """Print keyboard events without grabbing (for device discovery)"""
    import evdev
    devs = [evdev.InputDevice(p) for p in evdev.list_devices()]
    kbds = [d for d in devs if evdev.ecodes.EV_KEY in d.capabilities()]
    click.echo(f"Found keyboards: {[k.name for k in kbds]}")
    if not kbds:
        return
    click.echo(f"\nListening to: {kbds[0].name} (no grab)")
    for ev in kbds[0].read_loop():
        if ev.type == evdev.ecodes.EV_KEY and ev.value == 1:
            click.echo(f"  code={ev.code:4d}  name={evdev.ecodes.KEY[ev.code]}")


# ── Snippets CLI ────────────────────────────────────────────────────────

@cli.group()
def snippets():
    """Manage auto-expansion snippets"""
    pass


@snippets.command("list")
def snippets_list():
    """List all snippets"""
    store = SnippetStore()
    if not store.pairs():
        click.echo("No snippets configured.")
        return
    click.echo(f"{'Trigger':<20} {'Expansion'}")
    click.echo("-" * 60)
    for trigger, expansion in store.pairs():
        display = expansion if len(expansion) <= 40 else expansion[:37] + "..."
        click.echo(f"{trigger:<20} {display}")


@snippets.command("add")
@click.argument("trigger")
@click.argument("expansion")
def snippets_add(trigger, expansion):
    """Add a snippet: power_switcher snippets add '!ee' 'user@example.com'"""
    store = SnippetStore()
    store.add(trigger, expansion)
    click.echo(f"Added: {trigger} → {expansion}")


@snippets.command("remove")
@click.argument("trigger")
def snippets_remove(trigger):
    """Remove a snippet by trigger"""
    store = SnippetStore()
    if store.remove(trigger):
        click.echo(f"Removed: {trigger}")
    else:
        click.echo(f"Snippet not found: {trigger}")


# ── Exceptions CLI ──────────────────────────────────────────────────────

@cli.group()
def exceptions():
    """Manage per-application exceptions"""
    pass


@exceptions.command("list")
def exceptions_list():
    """List all application exceptions"""
    store = ExceptionStore()
    all_exc = store.all()
    if not all_exc:
        click.echo("No exceptions configured.")
        return
    click.echo(f"{'Application':<30} {'Mode'}")
    click.echo("-" * 50)
    for name, mode in all_exc.items():
        click.echo(f"{name:<30} {mode}")


@exceptions.command("add")
@click.argument("name")
@click.option("--mode", default="off", type=click.Choice(["off", "soft", "default"]))
def exceptions_add(name, mode):
    """Add an application exception: power_switcher exceptions add 'code' --mode off"""
    store = ExceptionStore()
    store.add(name, mode)
    click.echo(f"Added: {name} → {mode}")


@exceptions.command("remove")
@click.argument("name")
def exceptions_remove(name):
    """Remove an application exception"""
    store = ExceptionStore()
    if store.remove(name):
        click.echo(f"Removed: {name}")
    else:
        click.echo(f"Exception not found: {name}")


# ── Words CLI (exceptions / learned words) ───────────────────────────────

@cli.group()
def words():
    """Manage word exceptions and learned words"""
    pass


def _load_layout_set() -> LayoutSet:
    profiles_dir = Path(__file__).parent / "detection" / "profiles"
    return LayoutSet.create(["en", "ru"], profiles_dir)


@words.command("list")
def words_list():
    """List learned words and word exceptions"""
    ls = _load_layout_set()
    learned = ls.learned_words()
    exc = ls.exceptions()
    if learned:
        click.echo("Learned words (never converted):")
        for w in learned:
            click.echo(f"  {w}")
    if exc:
        click.echo("Word exceptions (never converted):")
        for w in exc:
            click.echo(f"  {w}")
    if not learned and not exc:
        click.echo("No words configured.")


@words.command("add")
@click.argument("word")
def words_add(word):
    """Never auto-convert WORD again"""
    ls = _load_layout_set()
    ls.add_exception(word)
    click.echo(f"Added word exception: {word}")


@words.command("remove")
@click.argument("word")
def words_remove(word):
    """Remove a word exception or learned word"""
    ls = _load_layout_set()
    if ls.remove_exception(word) or ls.remove_learned(word):
        click.echo(f"Removed: {word}")
    else:
        click.echo(f"Word not found: {word}")


# ── Stats CLI ────────────────────────────────────────────────────────────

@cli.group()
def stats():
    """Show or reset conversion statistics"""
    pass


@stats.command("show")
def stats_show():
    """Show conversion statistics"""
    s = Stats.load()
    total = s.total_conversions
    click.echo(f"Automatic conversions: {s.auto_conversions}")
    click.echo(f"Manual conversions:    {s.manual_conversions}")
    click.echo(f"Total conversions:     {total}")
    click.echo(f"Snippet expansions:    {s.snippet_expansions}")
    click.echo(f"Undos:                 {s.undos}")
    click.echo(f"Characters retyped:    {s.chars_retyped}")
    if s.first_seen:
        import time as _time
        days = max(0, (_time.time() - s.first_seen) / 86400)
        click.echo(f"Tracking for:          {days:.1f} days")


@stats.command("reset")
def stats_reset():
    """Reset all statistics"""
    s = Stats.load()
    s.reset()
    click.echo("Statistics reset.")


@cli.command()
def gui():
    """Open the settings GUI window"""
    try:
        from .gui import main
        main()
    except ImportError as exc:
        click.echo(f"GUI requires tkinter: {exc}")
        click.echo("Install: sudo apt install python3-tk (Debian/Ubuntu) or sudo dnf install python3-tkinter (Fedora)")
    except Exception as exc:
        click.echo(f"GUI error: {exc}")


if __name__ == "__main__":
    cli()
