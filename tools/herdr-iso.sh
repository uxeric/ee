#!/usr/bin/env bash
# Isolated headless herdr server for testing, fully separate from the live session.
#
#   herdr-iso.sh start            start server + 1 workspace/tab with panes A and B; prints export lines
#   herdr-iso.sh env              reprint the export lines
#   herdr-iso.sh read A|B|<id>    print a pane's visible screen text
#   herdr-iso.sh herdr <args...>  run any herdr CLI command against the isolated server (no args: attach a client)
#   herdr-iso.sh stop             stop the server, kill leftover pane processes, delete the iso dir
#
# Env knobs: HERDR_ISO_DIR (default /tmp/ee-herdr-iso-<uid>), HERDR_ISO_CWD (pane working dir,
# default: $PWD), HERDR_ISO_BIN (herdr binary).
set -euo pipefail

ISO="${HERDR_ISO_DIR:-/tmp/ee-herdr-iso-$(id -u)}"
HERDR="${HERDR_ISO_BIN:-/usr/bin/herdr}"
REAL_HOME="${HOME}"
REL_SOCK="c/herdr/herdr.sock"

die() { echo "herdr-iso: $*" >&2; exit 1; }

ISO_ENV=(
  env -i
  HOME="$ISO/home" USER="${USER:-eric}" LOGNAME="${LOGNAME:-${USER:-eric}}"
  PATH="$REAL_HOME/.cargo/bin:/usr/local/bin:/usr/bin:/bin"
  CARGO_HOME="${CARGO_HOME:-$REAL_HOME/.cargo}" RUSTUP_HOME="${RUSTUP_HOME:-$REAL_HOME/.rustup}"
  TERM=xterm-256color LANG="${LANG:-C.UTF-8}" SHELL=/bin/bash
  XDG_CONFIG_HOME=c XDG_STATE_HOME="$ISO/state" XDG_RUNTIME_DIR="$ISO/run"
  XDG_CACHE_HOME="$ISO/cache" XDG_DATA_HOME="$ISO/data"
)

check_iso() { case "$ISO" in */ee-herdr-iso*) ;; *) die "refusing: unexpected ISO path $ISO";; esac; }

server_pid() {
  [ -f "$ISO/server.pid" ] || return 1
  local pid; pid="$(cat "$ISO/server.pid")"
  [ -n "$pid" ] && [ -d "/proc/$pid" ] || return 1
  [ "$(readlink "/proc/$pid/cwd" 2>/dev/null)" = "$ISO" ] || return 1
  tr '\0' ' ' <"/proc/$pid/cmdline" | grep -q 'herdr server' || return 1
  echo "$pid"
}

sock_path() { local pid; pid="$(server_pid)" || die "isolated server is not running"; echo "/proc/$pid/cwd/$REL_SOCK"; }

hz() {
  local sock; sock="$(sock_path)"
  (cd "$ISO" && exec "${ISO_ENV[@]}" HERDR_SOCKET_PATH="$sock" "$HERDR" "$@")
}

json_get() { python3 -c 'import json,sys
v=json.load(sys.stdin)
for k in sys.argv[1].split("."): v=v[k]
print(v)' "$1"; }

cmd_start() {
  check_iso
  if server_pid >/dev/null 2>&1; then die "already running (pid $(server_pid)); run stop first"; fi
  rm -rf "$ISO"
  mkdir -p "$ISO"/{home,state,run,cache,data}
  chmod 700 "$ISO/run"

  (cd "$ISO" && exec "${ISO_ENV[@]}" setsid sh -c 'exec env HERDR_SOCKET_PATH="/proc/$$/cwd/'"$REL_SOCK"'" "$0" server' "$HERDR" >"$ISO/server.out" 2>&1 </dev/null) &
  disown || true

  local pid="" i
  for i in $(seq 1 50); do
    for p in $(pgrep -f 'herdr server' || true); do
      if [ "$(readlink "/proc/$p/cwd" 2>/dev/null)" = "$ISO" ] && tr '\0' ' ' <"/proc/$p/cmdline" 2>/dev/null | grep -q "^$HERDR server"; then
        pid="$p"
      fi
    done
    [ -n "$pid" ] && [ -S "$ISO/$REL_SOCK" ] && break
    sleep 0.1
  done
  [ -n "$pid" ] && [ -S "$ISO/$REL_SOCK" ] || { cat "$ISO/server.out" >&2; die "server did not come up"; }
  echo "$pid" >"$ISO/server.pid"

  local sock; sock="$(sock_path)"
  local cwd="${HERDR_ISO_CWD:-$PWD}"
  local out ws tab a b
  out="$(hz workspace create --cwd "$cwd" --label iso --focus)"
  ws="$(json_get result.workspace.workspace_id <<<"$out")"
  tab="$(json_get result.tab.tab_id <<<"$out")"
  a="$(json_get result.root_pane.pane_id <<<"$out")"
  out="$(hz pane split "$a" --direction right --cwd "$cwd" --no-focus)"
  b="$(json_get result.pane.pane_id <<<"$out")"

  cat >"$ISO/ids.env" <<EOF
export HERDR_ISO_SOCKET='$sock'
export HERDR_ISO_SERVER_PID='$pid'
export HERDR_ISO_WORKSPACE='$ws'
export HERDR_ISO_TAB='$tab'
export HERDR_ISO_PANE_A='$a'
export HERDR_ISO_PANE_B='$b'
EOF
  cat "$ISO/ids.env"
}

cmd_env() { [ -f "$ISO/ids.env" ] || die "not started"; cat "$ISO/ids.env"; }

resolve_pane() {
  case "$1" in
    A|a) (. "$ISO/ids.env" && echo "$HERDR_ISO_PANE_A") ;;
    B|b) (. "$ISO/ids.env" && echo "$HERDR_ISO_PANE_B") ;;
    *) echo "$1" ;;
  esac
}

cmd_read() {
  [ $# -ge 1 ] || die "usage: read A|B|<pane_id>"
  [ -f "$ISO/ids.env" ] || die "not started"
  hz pane read "$(resolve_pane "$1")" --source visible
}

cmd_stop() {
  check_iso
  local pid=""
  pid="$(server_pid 2>/dev/null)" || pid=""
  local marker=""
  [ -n "$pid" ] && marker="HERDR_SOCKET_PATH=/proc/$pid/cwd/$REL_SOCK"
  if [ -n "$pid" ]; then
    kill -TERM "$pid" 2>/dev/null || true
    for _ in $(seq 1 50); do [ -d "/proc/$pid" ] || break; sleep 0.1; done
    [ -d "/proc/$pid" ] && kill -KILL "$pid" 2>/dev/null || true
  fi
  if [ -n "$marker" ]; then
    for p in $(pgrep -u "$(id -u)" . || true); do
      if { tr '\0' '\n' <"/proc/$p/environ"; } 2>/dev/null | grep -qxF "$marker"; then
        kill -KILL "$p" 2>/dev/null || true
      fi
    done
  fi
  rm -rf "$ISO"
  echo "stopped${pid:+ (server pid $pid)}; removed $ISO"
}

sub="${1:-}"; shift || true
case "$sub" in
  start) cmd_start ;;
  env) cmd_env ;;
  read) cmd_read "$@" ;;
  herdr) hz "$@" ;;
  stop) cmd_stop ;;
  *) echo "usage: $0 start|env|read A|B|<id>|herdr <args...>|stop" >&2; exit 2 ;;
esac
