# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-10-30

First public release: the Rust daemon replaces the Python one, and the
installed commands are `power-switcher` / `power-switcher-gui`.

### Added

- `evdev` keyboard grab with `uinput` pass-through and direct text injection:
  wrong-layout words are erased with Backspace and re-typed in the right layout,
  without `ydotool` or clipboard access.
- Weighted 2–5 gram detector with a per-language word vocabulary for English,
  Russian, Ukrainian, German, French, Spanish, Italian and Polish, generated from
  the reference word-frequency lists in `data/frequency/`.
- Layout gate: a conversion needs a real word on at least one side, with a wider
  n-gram margin and a keyboard-run check when neither vocabulary knows the word.
- Autocorrect (`spell_check`, opt-out): single-edit neighbours ranked by
  `word_logprob + ln(edit_prior)`, with guards for names, ALL-CAPS and
  first-letter edits.
- L2 plausibility path for Russian IT jargon typed in the English layout.
- Snippets with layout-independent matching, per-application modes, learned
  words, conversion statistics and flexible undo.
- Hotkeys: L+R Shift pauses, double Shift or `ctrl+alt+z` converts the last
  sentence, `shift_tap_switch_layout` remains opt-in.
- Compositor backends for GNOME, KDE Plasma, Sway and Hyprland, plus the
  bundled `power-switcher-focus@powerswitcher` shell extension that exposes
  focus and layout switching to the daemon.
- GTK4 settings window with Settings, Snippets, Applications, Words and Status
  tabs; it edits the same files as the daemon and can install/restart the
  user service.
- StatusNotifierItem tray icon (Settings / Restart daemon / Exit) built on
  `ksni`, with a symbolic icon that inverts with the desktop theme.
- Packaging for Fedora/openSUSE (RPM), Debian/Ubuntu (deb), Arch
  (`pkg.tar.zst`), a portable tarball with `install.sh`, and an AppImage
  builder. Private user data is written `0600` atomically.

### Changed

- Layout switching is left to the desktop (`Super+Space`); the daemon re-reads
  the real layout every 250 ms instead of trusting stale dconf state.
- Autocorrect and the layout gate were tuned against a 31,990-case golden
  corpus to keep random strings, vim keys and technical terms intact.

### Fixed

- Tray, application and dock icons now use the project artwork: the tray
  reports `power-switcher-symbolic`, the desktop entry declares
  `StartupWMClass=org.power_switcher.Settings` so GNOME links the window to its
  entry, and the icon set ships in every package.
- A lone Shift tap no longer switches the layout.
- Learned words are persisted to `words.json`, so un-converted words stay
  protected across daemon restarts.

[Unreleased]: https://github.com/illnar-io/power_switcher/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/illnar-io/power_switcher/releases/tag/v0.1.0
