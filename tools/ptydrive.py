#!/usr/bin/env python3
"""Drive ee in a pseudo-terminal with a sandboxed environment.

    sys.path.insert(0, 'tools'); from ptydrive import run, SAVE, QUIT
    out = run(['notes.txt'], ['x', SAVE, QUIT])

run() strips WAYLAND_DISPLAY, DISPLAY and HERDR_* (never touch the real clipboard or herdr
session), points HOME and the XDG dirs at a throwaway folder unless `home` is given, answers the
kitty keyboard query (or declines it with kitty=False), sends each key after `delay` seconds and
returns everything ee wrote. Check saves by reading the file, not by parsing the output.

`python3 tools/ptydrive.py` runs a smoke test: type and save, in a classic terminal and over the kitty
protocol.
"""
import fcntl
import os
import pty as ptymod
import shutil
import struct
import subprocess
import sys
import tempfile
import threading
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIN = os.path.join(ROOT, 'target', 'debug', 'ee')


def csi_u(code, mods=None):
    return f'\x1b[{code}' + (f';{mods}' if mods else '') + 'u'


CTRL = 5
SAVE, QUIT = '\x13', '\x11'
KITTY_SAVE, KITTY_QUIT = csi_u(115, CTRL), csi_u(113, CTRL)


def sandbox_env(home, extra=None):
    env = {
        'HOME': home,
        'PATH': os.environ.get('PATH', '/usr/bin:/bin'),
        'TERM': 'xterm-256color',
        'XDG_CONFIG_HOME': os.path.join(home, '.config'),
        'XDG_DATA_HOME': os.path.join(home, '.local', 'share'),
        'EOE_NO_UPDATE_CHECK': '1',
    }
    env.update(extra or {})
    return env


def run(args, keys, home=None, kitty=False, size=(24, 80), settle=1.5, delay=0.3, env=None, binary=BIN):
    home = home or tempfile.mkdtemp(prefix='ee-pty-')
    master, child = ptymod.openpty()
    fcntl.ioctl(master, 0x5414, struct.pack('HHHH', size[0], size[1], 0, 0))
    proc = subprocess.Popen([binary] + list(args), stdin=child, stdout=child, stderr=subprocess.DEVNULL, env=sandbox_env(home, env))
    os.close(child)
    out = bytearray()

    def reader():
        answered = False
        while True:
            try:
                data = os.read(master, 65536)
            except OSError:
                break
            if not data:
                break
            out.extend(data)
            if not answered and b'\x1b[?u' in out:
                answered = True
                os.write(master, (b'\x1b[?0u' if kitty else b'') + b'\x1b[?62;22c')

    threading.Thread(target=reader, daemon=True).start()
    time.sleep(settle)
    for key in keys:
        text, pause = key if isinstance(key, tuple) else (key, delay)
        os.write(master, text.encode())
        time.sleep(pause)
    try:
        proc.wait(timeout=3)
    except subprocess.TimeoutExpired:
        proc.kill()
    return bytes(out)


def smoke():
    ok = True
    for kitty, save, quit_ in [(False, SAVE, QUIT), (True, KITTY_SAVE, KITTY_QUIT)]:
        home = tempfile.mkdtemp(prefix='ee-pty-')
        path = os.path.join(home, 'note.txt')
        with open(path, 'w') as f:
            f.write('hello\n')
        run([path], ['x', save, quit_], home=home, kitty=kitty)
        got = open(path).read()
        shutil.rmtree(home)
        passed = got == 'xhello\n'
        ok &= passed
        print('PASS' if passed else 'FAIL', 'kitty' if kitty else 'classic', repr(got))
    return ok


if __name__ == '__main__':
    sys.exit(0 if smoke() else 1)
