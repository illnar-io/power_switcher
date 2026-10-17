// PowerSwitcher Focus — companion extension for GNOME Shell 45+.
//
// Wayland gives clients no way to query the focused window, and GNOME Shell
// blocks the old `org.gnome.Shell.Eval` and `Introspect.GetWindows` APIs for
// third-party callers.  This tiny extension runs *inside* the shell and
// re-exports the focused window's application id on the session bus, where
// PowerSwitcher reads it via:
//
//     gdbus call --session \
//       --dest org.power_switcher.Focus \
//       --object-path /org/power_switcher/Focus \
//       --method org.power_switcher.Focus.GetActiveWindow

import Gio from 'gi://Gio';

import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';

const BUS_NAME = 'org.power_switcher.Focus';
const OBJECT_PATH = '/org/power_switcher/Focus';

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

const VERSION = '1.0';

export default class PowerSwitcherFocusExtension extends Extension {
    enable() {
        this._nameOwnerId = 0;
        this._dbus = Gio.DBusExportedObject.wrapJSObject(IFACE_XML, this);
        this._exported = false;

        this._nameOwnerId = Gio.bus_own_name(
            Gio.BusType.SESSION,
            BUS_NAME,
            Gio.BusNameOwnerFlags.NONE,
            () => this._export(),
            null,
            () => {
                // Lost the name (e.g. a shell reload): drop the object too.
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
    }

    _export() {
        if (this._exported || !this._dbus)
            return;
        try {
            this._dbus.export(Gio.DBus.session, OBJECT_PATH);
            this._exported = true;
        } catch (e) {
            logError(e, 'PowerSwitcher Focus: failed to export D-Bus object');
        }
    }

    _unexport() {
        if (!this._exported || !this._dbus)
            return;
        try {
            this._dbus.unexport();
        } catch (e) {
            logError(e, 'PowerSwitcher Focus: failed to unexport D-Bus object');
        }
        this._exported = false;
    }

    // ── D-Bus methods ───────────────────────────────────────────────────

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
}
