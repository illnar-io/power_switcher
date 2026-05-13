import subprocess


def inject_text(text: str, delay: int = 5) -> None:
    """Type unicode text via ydotool."""
    subprocess.run(
        ["ydotool", "type", "--delay", str(delay), "--", text],
        capture_output=True,
    )
