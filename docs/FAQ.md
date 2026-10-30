# FAQ and troubleshooting

Most problems are one of the cases below. Each section ends with the command
that shows what the daemon is doing. If none of it helps, use
**Status → Report a problem…** in the settings window — or open an
[issue](https://github.com/illnar-io/power_switcher/issues/new?template=bug_report.yml)
and paste the copied report.

All commands are per-user (`--user`); the daemon never needs root.

## Nothing is converted

1. **Is the daemon running?**
   ```bash
   systemctl --user status power_switcher
   systemctl --user restart power_switcher
   ```
   If the unit is missing, open the settings window → Status → **Install
   service**.

2. **Does the daemon see your keyboard?** The daemon must be able to read
   `/dev/input/event*` and write `/dev/uinput`:
   ```bash
   python -m power_switcher setup     # prints the udev rule and group note
   power-switcher debug               # lists keyboards; press keys to see codes
   ```
   After changing udev rules, unplug/replug the keyboard or reboot. The udev
   rule must be `60-power_switcher.rules` (a `99-` rule is applied too late).

3. **Are at least two layouts configured?** Settings → Layouts. The ids must be
   the ones the desktop actually uses (`en`, `ru`, `ua`, `de`, `fr`, `es`, `it`,
   `pl`), and the desktop must have both layouts enabled.

4. **Is the app excluded?** Applications tab: a mode of **off** disables both
   auto-detect and hotkeys for that app; **manual** leaves only the hotkeys.

5. **Is auto-detect paused or off?** Press L+R Shift together to toggle pause.
   Also check that **Developer mode** is off and that the word is at least
   *Minimum word length* characters (3 by default).

6. **Did you just switch layouts?** For `no_correct_after_switch` seconds
   (1.5 by default) the daemon stays out of the way, so a deliberate switch is
   not undone.

## Wrong conversion

- **A correct word is "fixed" into garbage.** Raise *Threshold* in Settings
  (2.5 → 3.0 or more). A known word of the current language already needs a
  much larger margin, so this mostly protects names and jargon.
- **A word or name must never be touched.** Add it in the **Words** tab. Words
  learned from undo and repeated unknown words are stored in `words.json` and
  survive restarts.
- **A whole application is hostile to conversion** (terminals, code editors,
  games). Add it in **Applications** with mode **off** or **manual**.
- **Mid-word conversion is too eager.** Turn off *Live (mid-word) fix*.
- **The last word is converted after a normal Shift tap.** Turn off *Lone Shift
  tap switches layout* — your desktop already owns `Super+Space`.
- **Autocorrect changes a technical term.** Turn off *Autocorrect misspelled
  words*, or add the term to **Words**. The 20k-word vocabulary does not know
  every technical term.

## Keyboard stops responding

The daemon grabs the physical keyboard and replays it through `/dev/uinput`.
If it dies mid-grab the kernel releases the device, so keys should keep
working; if they do not:

```bash
systemctl --user restart power_switcher
# or, if the unit is not installed:
pkill -f "power-switcher daemon"
```

- **Stuck modifiers** are usually a missed key-up during a conversion. Tap
  Shift/Ctrl/Alt once, or restart the daemon.
- **`/dev/uinput` missing after a kernel update:** `sudo modprobe uinput`; the
  package loads it at boot through `modules-load.d/power_switcher.conf`.
- **A second copy is grabbing the same keyboard** (for example a leftover
  `ps daemon` from a terminal): `pgrep -af "power-switcher daemon|ps daemon"`.
- If it still happens, enable **Debug logging** (Settings → Diagnostics),
  restart, reproduce, then report.

## Missing tray icon

The tray is a StatusNotifierItem. GNOME has no tray of its own and needs the
**AppIndicator and KStatusNotifierItem Support** extension:

```bash
gnome-extensions list | grep -i appindicator
```

If nothing is listed, install the `gnome-shell-extension-appindicator` package
and log out and back in. On KDE, Sway or Hyprland the panel/bar needs an SNI
host (for example the `tray` module in Waybar). `busctl --user tree
org.kde.StatusNotifierWatcher` should show a watcher when one is running; the
daemon restarts the icon when the watcher appears.

## Settings window problems

- The GTK4 window is the separate `power-switcher-gui` binary (cargo builds it
  as `ps-gui`). `power-switcher gui` looks for it next to the daemon, then in
  `$PATH`.
- It needs GTK 4.10 or newer. Run it from a terminal to see the actual error:
  ```bash
  power-switcher-gui
  ```
- Changes are written on **Save** and applied only after a daemon restart
  (Status → **Restart daemon**). There is no live IPC by design.
- “Install service” writes `~/.config/systemd/user/power_switcher.service` and
  runs `systemctl --user daemon-reload` + `enable --now`. A unit pointing at a
  deleted binary exits with `203/EXEC`; reinstall the service from the same
  tab.

## Daemon crashes or will not start

```bash
systemctl --user status power_switcher
journalctl --user -u power_switcher -n 100 --no-pager
```

Check for `203/EXEC` (binary missing — reinstall the package), permission
errors on `/dev/input` or `/dev/uinput` (see *Nothing is converted*), or a
panic message. Report it with the diagnostics from **Status → Report a
problem…**.

## Reporting a bug

1. Settings → **Diagnostics** → enable **Debug logging**, then restart the
   daemon and reproduce the problem.
2. Status → **Report a problem…**, pick the closest category.
3. If the category has a known fix, **Open FAQ** takes you to the section
   above. Otherwise **Copy report and open GitHub issue** puts a ready-to-paste
   report in your clipboard and opens the bug form — paste it into the
   *Diagnostics* field.

The report contains the version, distro, kernel, session type, daemon state,
the relevant settings and the last 100 journal lines. It never contains typed
text or snippet contents, but it does contain file paths: review it before
posting.
