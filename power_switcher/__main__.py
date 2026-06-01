import asyncio
import logging
import os
from pathlib import Path

import click
from evdev import ecodes as e

from .config import Config
from .core.key_processor import KeyProcessor
from .layout.detector import detect_layout_backend
from .maps.layout_set import LayoutSet

@click.group()
def cli():
    """PowerSwitcher — Caramba Switcher clone for Linux Wayland"""


@cli.command()
@click.option("--config", default="~/.config/power_switcher/config.toml")
@click.option("-v", "--verbose", is_flag=True)
@click.option("--no-auto", is_flag=True, help="Disable auto-detect, hotkey only")
def daemon(config, verbose, no_auto):
    """Run Caramba-style daemon (DoubleShift to convert, SingleShift to toggle)"""
    logging.basicConfig(
        level=logging.DEBUG if verbose else logging.INFO,
        format="%(asctime)s [%(name)s] %(levelname)s: %(message)s",
    )
    cfg = Config.load(os.path.expanduser(config))
    backend = detect_layout_backend()

    profiles_dir = Path(__file__).parent / "detection" / "profiles"
    layout_set = LayoutSet.create(cfg.layouts, profiles_dir)

    processor = KeyProcessor(
        layout_backend=backend,
        layout_set=layout_set,
        current_layout=cfg.default_layout,
        threshold=cfg.threshold,
        min_word_len=cfg.min_word_len,
    )

    click.echo(f"Caramba Mode: {'AUTO' if not no_auto else 'MANUAL'}")
    click.echo(f"Layouts:      {' ↔ '.join(cfg.layouts)}")
    click.echo(f"Interaction:  DoubleShift=Convert, SingleShift=Toggle")

    asyncio.run(processor.run())


@cli.command()
def setup():
    """Generate default config and show udev rules"""
    config_path = Path("~/.config/power_switcher/config.toml").expanduser()
    if not config_path.exists():
        config_path.parent.mkdir(parents=True, exist_ok=True)
        with open(config_path, "w") as f:
            f.write('[general]\nlayouts = ["en", "ru"]\ndefault_layout = "en"\n\n[detection]\nthreshold = 2.5\nmin_word_len = 3\n')
        click.echo(f"Created default config at {config_path}")

    click.echo("\nTo allow running without sudo, add these udev rules to /etc/udev/rules.d/99-power_switcher.rules:")
    click.echo('KERNEL=="uinput", GROUP="input", MODE="0660"')
    click.echo('KERNEL=="event*", GROUP="input", MODE="0660"')
    click.echo("\nThen run: sudo udevadm control --reload-rules && sudo udevadm trigger")
    click.echo(f"And ensure your user is in the 'input' group: sudo usermod -aG input $USER")


if __name__ == "__main__":
    cli()
