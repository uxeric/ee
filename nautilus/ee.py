"""Open with ee, for a file Files has no default application for."""

from gi import require_version

require_version("Nautilus", "4.1")

from gi.repository import GLib, GObject, Gio, Nautilus


def paths_without_default(files):
    """Paths to open, or none.

    A selection is offered only when every file is a real file and none of
    them has a default application. One file that already has an app, or a
    directory, drops the item so ee does not sit beside Open With Neovim.
    """
    paths = []
    seen = set()
    for file in files:
        if file.is_directory():
            return []
        location = file.get_location()
        path = location.get_path() if location is not None else None
        if not path:
            return []
        mime = file.get_mime_type() or ""
        if Gio.AppInfo.get_default_for_type(mime, False) is not None:
            return []
        if path not in seen:
            seen.add(path)
            paths.append(path)
    return paths


def ee_opens_files():
    app = Gio.DesktopAppInfo.new("ee.desktop")
    return app is not None and app.supports_files()


class OpenWithEe(GObject.GObject, Nautilus.MenuProvider):
    def _on_activate(self, _menu, paths):
        app = Gio.DesktopAppInfo.new("ee.desktop")
        if app is None:
            return
        files = [Gio.File.new_for_path(path) for path in paths]
        try:
            app.launch(files, None)
        except GLib.Error:
            return

    def get_file_items(self, *args):
        files = args[0] if len(args) == 1 else args[1]
        paths = paths_without_default(files)
        if not paths or not ee_opens_files():
            return []
        item = Nautilus.MenuItem(
            name="EeNautilus::open_with_ee",
            label="Open with ee",
            icon="ee",
        )
        item.connect("activate", self._on_activate, paths)
        return [item]
