// PowerSwitcher helper — companion extension for GNOME Shell 45+.
//
// Wayland gives clients no way to query the focused window, and GNOME Shell
// blocks the old `org.gnome.Shell.Eval` and `Introspect.GetWindows` APIs for
// third-party callers.  This tiny extension runs *inside* the shell and
// re-exports two things on the session bus:
//
//  1. the focused window's application id (per-app exceptions):
//
//     gdbus call --session \
//       --dest org.power_switcher.Focus \
//       --object-path /org/power_switcher/Focus \
//       --method org.power_switcher.Focus.GetActiveWindow
//
//  2. the input-source (keyboard layout) list / current index / switching:
//
//     gdbus call --session \
//       --dest org.power_switcher.Focus \
//       --object-path /org/power_switcher/Layout \
//       --method org.power_switcher.Layout.GetCurrentIndex
//
// GNOME 41+ blocks `org.gnome.Shell.Eval`, and since GNOME 4x the dconf key
// `org.gnome.desktop.input-sources current` is deprecated and ignored on
// Wayland — an in-shell extension is the only supported way to switch the
// input source programmatically.  Injected Super+Space does *not* trigger the
// shell's built-in keybindings (verified on GNOME 51), so this interface is
// what the Rust daemon's GNOME backend uses.

import Gio from 'gi://Gio';

import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Keyboard from 'resource:///org/gnome/shell/ui/status/keyboard.js';

const BUS_NAME = 'org.power_switcher.Focus';
const OBJECT_PATH = '/org/power_switcher/Focus';
const LAYOUT_PATH = '/org/power_switcher/Layout';

const IFACE_XML = `
<node>
  <interface name="org.power_switcher.Focus">
    <method name="GetActiveWindow">
      <arg type="s" direction="out" name="wm_class"/>
    </method>
    <method name="GetActiveWindowTitle">
      <arg type="s" direction="out" name="title"/>
    </method>
    <method name="GetVersion">
      <arg type="s" direction="out" name="version"/>
    </method>
  </interface>
</node>`;

const LAYOUT_IFACE_XML = `
<node>
  <interface name="org.power_switcher.Layout">
    <method name="GetCurrentIndex">
      <arg type="u" direction="out" name="index"/>
    </method>
    <method name="GetInputSources">
      <arg type="as" direction="out" name="ids"/>
    </method>
    <method name="ActivateInputSource">
      <arg type="u" direction="in" name="index"/>
      <arg type="b" direction="out" name="ok"/>
    </method>
    <method name="GetVersion">
      <arg type="s" direction="out" name="version"/>
    </method>
  </interface>
</node>`;

const VERSION = '2.0';

export default class PowerSwitcherHelperExtension extends Extension {
    enable() {
        this._nameOwnerId = 0;
        this._dbus = Gio.DBusExportedObject.wrapJSObject(IFACE_XML, this);
        this._layoutDbus = Gio.DBusExportedObject.wrapJSObject(LAYOUT_IFACE_XML, this);
        this._exported = false;
        this._layoutExported = false;

        this._nameOwnerId = Gio.bus_own_name(
            Gio.BusType.SESSION,
            BUS_NAME,
            Gio.BusNameOwnerFlags.NONE,
            () => this._export(),
            null,
            () => {
                // Lost the name (e.g. a shell reload): drop the objects too.
                this._unexport();
            },
        );
    }

    disable() {
        if (this._nameOwnerId) {
            Gio.bus_unown_name(this._nameOwnerId);
            this._nameOwnerId = 0;
        }
        this._unexport();
        this._dbus = null;
        this._layoutDbus = null;
    }

    _export() {
        if (!this._exported && this._dbus) {
            try {
                this._dbus.export(Gio.DBus.session, OBJECT_PATH);
                this._exported = true;
            } catch (e) {
                logError(e, 'PowerSwitcher: failed to export focus D-Bus object');
            }
        }
        if (!this._layoutExported && this._layoutDbus) {
            try {
                this._layoutDbus.export(Gio.DBus.session, LAYOUT_PATH);
                this._layoutExported = true;
            } catch (e) {
                logError(e, 'PowerSwitcher: failed to export layout D-Bus object');
            }
        }
    }

    _unexport() {
        if (this._exported && this._dbus) {
            try {
                this._dbus.unexport();
            } catch (e) {
                logError(e, 'PowerSwitcher: failed to unexport focus D-Bus object');
            }
        }
        this._exported = false;
        if (this._layoutExported && this._layoutDbus) {
            try {
                this._layoutDbus.unexport();
            } catch (e) {
                logError(e, 'PowerSwitcher: failed to unexport layout D-Bus object');
            }
        }
        this._layoutExported = false;
    }

    // ── Focus D-Bus methods ─────────────────────────────────────────────

    GetActiveWindow() {
        const win = global.display.focus_window;
        if (!win)
            return '';
        return (
            win.get_wm_class() ||
            win.get_gtk_application_id() ||
            win.get_wm_class_instance() ||
            ''
        );
    }

    GetActiveWindowTitle() {
        const win = global.display.focus_window;
        return win ? (win.get_title() || '') : '';
    }

    GetVersion() {
        return VERSION;
    }

    // ── Layout D-Bus methods ────────────────────────────────────────────

    GetCurrentIndex() {
        const manager = Keyboard.getInputSourceManager();
        const sources = manager.inputSources;
        const current = manager.currentSource;
        for (const key of Object.keys(sources)) {
            if (sources[key] === current)
                return parseInt(key, 10);
        }
        // Fall back to matching by source id (e.g. after a shell reload).
        if (current && current.id) {
            for (const key of Object.keys(sources)) {
                if (sources[key].id === current.id)
                    return parseInt(key, 10);
            }
        }
        return 0;
    }

    GetInputSources() {
        const sources = Keyboard.getInputSourceManager().inputSources;
        return Object.keys(sources)
            .sort((a, b) => parseInt(a, 10) - parseInt(b, 10))
            .map(key => sources[key].id);
    }

    ActivateInputSource(index) {
        const sources = Keyboard.getInputSourceManager().inputSources;
        const key = String(index);
        if (!(key in sources))
            return false;
        sources[key].activate();
        return true;
    }
}
