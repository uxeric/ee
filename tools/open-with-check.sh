#!/usr/bin/env bash
# Sandbox the Omarchy Files menu install. Never touches the real HOME or launcher.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
bash -n "$root/install.sh"
python3 -m py_compile "$root/nautilus/ee.py"

python3 - "$root/nautilus/ee.py" << 'PY'
import importlib.util
import sys

import gi

gi.require_version("Gio", "2.0")
from gi.repository import Gio

spec = importlib.util.spec_from_file_location("ee_files", sys.argv[1])
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)


class File:
    def __init__(self, mime, path, directory=False):
        self.mime = mime
        self.path = path
        self.directory = directory

    def is_directory(self):
        return self.directory

    def get_mime_type(self):
        return self.mime

    def get_location(self):
        return self

    def get_path(self):
        return self.path


def mime_with_default():
    for mime in ("text/plain", "image/png", "text/markdown"):
        if Gio.AppInfo.get_default_for_type(mime, False) is not None:
            return mime
    raise SystemExit("no sample mime has a default application")


def mime_without_default():
    for mime in ("application/octet-stream", "application/vnd.sqlite3", "application/x-sharedlib"):
        if Gio.AppInfo.get_default_for_type(mime, False) is None:
            return mime
    raise SystemExit("no sample mime lacks a default application")


handled = mime_with_default()
unhandled = mime_without_default()
one = mod.paths_without_default([File(unhandled, "/tmp/a.dat")])
assert one == ["/tmp/a.dat"], one
assert mod.paths_without_default([File(handled, "/tmp/notes.txt")]) == []
assert mod.paths_without_default([File(unhandled, "/tmp/dir", directory=True)]) == []
mixed = mod.paths_without_default(
    [File(unhandled, "/tmp/a.dat"), File(handled, "/tmp/notes.txt")]
)
assert mixed == [], mixed
print(f"menu offers {unhandled} and skips {handled}")
PY

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
mkdir -p "$scratch/home/bin" "$scratch/config" "$scratch/data"
cat > "$scratch/home/bin/omarchy-tui-install" << 'EOF'
#!/bin/bash
name="$1"
cmd="$2"
dir="${HOME}/.local/share/applications"
mkdir -p "$dir"
cat > "$dir/$name.desktop" << DESK
[Desktop Entry]
Version=1.0
Name=$name
Comment=$name
Exec=xdg-terminal-exec --app-id=TUI.tile -e $cmd
Terminal=false
Type=Application
Icon=ee
StartupNotify=true
DESK
EOF
cat > "$scratch/home/bin/omarchy-tui-remove" << 'EOF'
#!/bin/bash
rm -f "${HOME}/.local/share/applications/$1.desktop"
EOF
chmod +x "$scratch/home/bin/omarchy-tui-install" "$scratch/home/bin/omarchy-tui-remove"

export HOME="$scratch/home"
export XDG_CONFIG_HOME="$scratch/config"
export XDG_DATA_HOME="$scratch/data"
export EOE_PREFIX="$scratch/prefix"
export EOE_SOURCE_DIR="$root"
export PATH="$scratch/home/bin:$PATH"
# shellcheck disable=SC1091
source "$root/install.sh"
find_source > /dev/null
add_to_omarchy > /dev/null
entry="$HOME/.local/share/applications/ee.desktop"
grep -q '^Exec=.*%F$' "$entry"
grep -q '^Keywords=' "$entry"
if grep -q '^MimeType=' "$entry"; then
    echo "ee.desktop must not claim a MimeType" >&2
    exit 1
fi
ext="$XDG_DATA_HOME/nautilus-python/extensions/ee.py"
test -f "$ext"
grep -q 'Open with ee' "$ext"
remove_from_omarchy > /dev/null
test ! -e "$entry"
test ! -e "$ext"
echo "launcher passes %F and the Files menu installs and removes"
