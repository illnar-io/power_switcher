import logging
import os

from .backends import (
    GNOMESwitchBackend,
    KDELayoutBackend,
    SwayLayoutBackend,
    HyprlandLayoutBackend,
    LayoutBackend,
)

logger = logging.getLogger(__name__)


def detect_layout_backend() -> LayoutBackend:
    """
    Auto-detect DE and return the best available layout backend.
    """
    desktop = os.environ.get("XDG_CURRENT_DESKTOP", "").upper()
    session = os.environ.get("XDG_SESSION_TYPE", "").lower()
    logger.info(f"XDG_CURRENT_DESKTOP={desktop} XDG_SESSION_TYPE={session}")

    if session and session != "wayland":
        logger.warning(f"XDG_SESSION_TYPE={session!r} is not Wayland. PowerSwitcher might fail.")

    # Sway
    if SwayLayoutBackend.is_available():
        return SwayLayoutBackend()

    # Hyprland
    if HyprlandLayoutBackend.is_available():
        return HyprlandLayoutBackend()

    # GNOME
    if any(x in desktop for x in ("GNOME", "UBUNTU", "UNITY", "POP")):
        if GNOMESwitchBackend.is_available():
            return GNOMESwitchBackend()

    # KDE Plasma
    if any(x in desktop for x in ("KDE", "PLASMA")):
        if KDELayoutBackend.is_available():
            return KDELayoutBackend()

    # Fallbacks
    if SwayLayoutBackend.is_available(): return SwayLayoutBackend()
    if HyprlandLayoutBackend.is_available(): return HyprlandLayoutBackend()
    if GNOMESwitchBackend.is_available(): return GNOMESwitchBackend()
    if KDELayoutBackend.is_available(): return KDELayoutBackend()

    raise RuntimeError(
        "No supported layout backend found.\n"
        "Supported: GNOME, KDE Plasma, Sway, Hyprland.\n"
        f"XDG_CURRENT_DESKTOP={os.environ.get('XDG_CURRENT_DESKTOP', '(unset)')}"
    )
