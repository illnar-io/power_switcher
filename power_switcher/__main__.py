import asyncio
import logging
import os

import click
from evdev import ecodes as e
from pathlib import Path

from .config import Config
from .core.key_processor import KeyProcessor
from .layout.detector import detect_layout_backend
from .maps.layout_set import LayoutSet

_KEY_NAMES = {
    "alt": e.KEY_LEFTALT,   "lalt": e.KEY_LEFTALT,   "ralt": e.KEY_RIGHTALT,
    "ctrl": e.KEY_LEFTCTRL, "lctrl": e.KEY_LEFTCTRL, "rctrl": e.KEY_RIGHTCTRL,
    "shift": e.KEY_LEFTSHIFT,
    "meta": e.KEY_LEFTMETA, "super": e.KEY_LEFTMETA,
    "pause": e.KEY_PAUSE,
    "z": e.KEY_Z, "x": e.KEY_X, "c": e.KEY_C, "v": e.KEY_V,
    "f1": e.KEY_F1, "f2": e.KEY_F2, "f12": e.KEY_F12,
}


def parse_hotkey(s: str) -> frozenset:
    codes = set()
    for part in s.lower().split("+"):
        kc = _KEY_NAMES.get(part.strip())
        if kc:
            codes.add(kc)
    return frozenset(codes)

@click.group()
def cli():
    """PowerSwitcher — DE-independent layout auto-switcher for Linux Wayland"""


@cli.command()

@click.option("--config", default="~/.config/power_switcher/config.toml")
@click.option("-v", "--verbose", is_flag=True)
@click.option("--no-auto", is_flag=True, help="Disable auto-detect, hotkey only")
def daemon(config, verbose, no_auto):
    """Run power_switcher daemon (main mode)"""
    logging.basicConfig(
        level=logging.DEBUG if verbose else logging.INFO,
        format="%(asctime)s [%(name)s] %(levelname)s: %(message)s",
    )
    cfg = Config.load(os.path.expanduser(config))
    backend = detect_layout_backend()

    if len(cfg.layouts) < 2:
        click.echo(f"ERROR: layouts must have at least 2 entries, got {cfg.layouts}")
        raise SystemExit(1)

    profiles_dir = Path(__file__).parent / "detection" / "profiles"
    layout_set = LayoutSet.create(cfg.layouts, profiles_dir)

    processor = KeyProcessor(
        layout_backend=backend,
        layout_set=layout_set,
        hotkey_convert=parse_hotkey(cfg.hotkey_convert),
        hotkey_selection=parse_hotkey(cfg.hotkey_selection),
        current_layout=cfg.default_layout,
        auto_detect=not no_auto,
        threshold=cfg.threshold,
        min_word_len=cfg.min_word_len,
    )

    click.echo(f"Backend:   {type(backend).__name__}")
    click.echo(f"Layouts:   {' ↔ '.join(cfg.layouts)} (default: {cfg.default_layout})")
    click.echo(f"Hotkeys:   convert={cfg.hotkey_convert}  selection={cfg.hotkey_selection}")
    click.echo(f"AutoDetect: {'OFF' if no_auto else 'ON'} (threshold={cfg.threshold})")

    asyncio.run(processor.run())



@cli.command()
def debug():
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


@cli.command()
@click.option("--config", default="~/.config/power_switcher/config.toml")
def layouts(config):
    """List available layout definitions"""
    from .maps.keycode_map import LAYOUTS
    click.echo(f"Built-in layouts: {sorted(LAYOUTS.keys())}")


@cli.command()
def setup():
    """Generate udev rules and default config"""
    import os
    from pathlib import Path

    # 1. Config
    config_path = Path("~/.config/power_switcher/config.toml").expanduser()
    if not config_path.exists():
        config_path.parent.mkdir(parents=True, exist_ok=True)
        with open(config_path, "w") as f:
            f.write('[general]\n')
            f.write('layouts = ["en", "ru"]\n')
            f.write('hotkey_convert = "alt+pause"\n')
            f.write('hotkey_selection = "ctrl+alt+z"\n')
            f.write('default_layout = "en"\n')
            f.write('auto_detect = true\n\n')
            f.write('[detection]\n')
            f.write('threshold = 2.5\n')
            f.write('min_word_len = 3\n')
        click.echo(f"Created default config at {config_path}")
    else:
        click.echo(f"Config already exists at {config_path}")

    # 2. Udev rules
    rules_path = Path("/etc/udev/rules.d/99-power_switcher.rules")
    click.echo(f"\nTo allow running without sudo, add these udev rules to {rules_path}:")
    click.echo('KERNEL=="uinput", GROUP="input", MODE="0660"')
    click.echo('KERNEL=="event*", GROUP="input", MODE="0660"')
    click.echo("\nThen run: sudo udevadm control --reload-rules && sudo udevadm trigger")
    click.echo(f"And ensure your user is in the 'input' group: sudo usermod -aG input $USER")


if __name__ == "__main__":
    cli()
