#!/usr/bin/env bash
# eoe installer: builds `ee` from source and installs it for the current user.
#
#   curl -fsSL https://raw.githubusercontent.com/uxeric/eoe/main/install.sh | bash
#   ./install.sh                 # from a checkout: builds that checkout
#   ./install.sh --uninstall     # removes ee (add --purge to remove the config too)
#
# Settings (environment variables):
#   EOE_REPO        git URL to clone          (default: https://github.com/uxeric/eoe.git)
#   EOE_REF         branch or tag to build    (default: main)
#   EOE_PREFIX      install prefix            (default: ~/.local, so ee goes in ~/.local/bin)
#   EOE_SOURCE_DIR  build this directory instead of cloning
#   EOE_YES=1       answer yes to prompts (e.g. installing Rust)
#   EOE_NO_OMARCHY=1  skip the Omarchy integration (app launcher entry, Omarchy's Rust installer)

set -euo pipefail

REPO="${EOE_REPO:-https://github.com/uxeric/eoe.git}"
REF="${EOE_REF:-main}"
PREFIX="${EOE_PREFIX:-$HOME/.local}"
BIN_DIR="$PREFIX/bin"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/eoe"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/eoe"
MIN_RUST="1.91"
LAUNCHER_NAME="Eric's Own Editor"

if [[ -t 1 && -z "${NO_COLOR:-}" ]]; then
    ICE=$'\e[38;2;41;240;255m' HOT=$'\e[38;2;255;42;109m' AMBER=$'\e[38;2;255;182;39m'
    GHOST=$'\e[38;2;92;90;138m' BOLD=$'\e[1m' RESET=$'\e[0m'
else
    ICE="" HOT="" AMBER="" GHOST="" BOLD="" RESET=""
fi

step() { printf '%s›%s %s\n' "$HOT" "$RESET" "$*"; }
ok() { printf '  %s✓%s %s\n' "$ICE" "$RESET" "$*"; }
warn() { printf '  %s!%s %s\n' "$AMBER" "$RESET" "$*" >&2; }
die() { printf '%s✗ %s%s\n' "$HOT" "$*" "$RESET" >&2; exit 1; }

banner() {
    printf '\n%s%s' "$HOT" "$BOLD"
    printf '  ▄▀▀▀▀▄  ▄▀▀▀▀▄\n'
    printf '%s  █▄▄▄▄█  █▄▄▄▄█\n' "$AMBER"
    printf '%s  █       █\n' "$ICE"
    printf '  ▀▄▄▄▄▀  ▀▄▄▄▄▀%s\n' "$RESET"
    printf '  %seric'"'"'s own editor · installer%s\n\n' "$GHOST" "$RESET"
}

confirm() {
    [[ "${EOE_YES:-}" == 1 ]] && return 0
    local answer=""
    { exec 3< /dev/tty; } 2> /dev/null || return 1
    printf '  %s?%s %s [y/N] ' "$AMBER" "$RESET" "$1" 2> /dev/null > /dev/tty
    read -r answer <&3 || true
    exec 3<&-
    [[ "$answer" == [yY]* ]]
}

is_omarchy() {
    [[ -z "${EOE_NO_OMARCHY:-}" ]] && command -v omarchy-tui-install > /dev/null
}

version_at_least() {
    [[ "$(printf '%s\n%s\n' "$2" "$1" | sort -V | head -n1)" == "$2" ]]
}

ensure_rust() {
    if ! command -v cargo > /dev/null && [[ -x "$HOME/.cargo/bin/cargo" ]]; then
        export PATH="$HOME/.cargo/bin:$PATH"
    fi
    if command -v cargo > /dev/null; then
        local have
        have="$(rustc --version | awk '{print $2}')"
        version_at_least "$have" "$MIN_RUST" || die "ee needs Rust $MIN_RUST or newer (found $have). Run: rustup update"
        ok "Rust $have"
        return
    fi
    warn "Rust is not installed."
    if is_omarchy && command -v omarchy-install-dev-env > /dev/null; then
        confirm "Install Rust the Omarchy way (omarchy-install-dev-env rust)?" \
            || die "Install Rust ($MIN_RUST+) and run this again."
        omarchy-install-dev-env rust
        export PATH="$HOME/.cargo/bin:$PATH"
    elif command -v pacman > /dev/null; then
        confirm "Install Rust with pacman (sudo pacman -S --needed rust)?" \
            || die "Install Rust ($MIN_RUST+) and run this again."
        sudo pacman -S --needed --noconfirm rust
    else
        confirm "Install Rust with rustup (https://rustup.rs)?" \
            || die "Install Rust ($MIN_RUST+) and run this again."
        curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal
        export PATH="$HOME/.cargo/bin:$PATH"
    fi
    command -v cargo > /dev/null || die "Rust was installed but cargo is not on PATH. Open a new shell and run this again."
    ok "Rust $(rustc --version | awk '{print $2}')"
}

is_checkout() {
    [[ -f "$1/Cargo.toml" ]] && grep -q '^name = "eoe"' "$1/Cargo.toml"
}

find_source() {
    if [[ -n "${EOE_SOURCE_DIR:-}" ]]; then
        is_checkout "$EOE_SOURCE_DIR" || die "EOE_SOURCE_DIR=$EOE_SOURCE_DIR is not an eoe checkout."
        SOURCE="$EOE_SOURCE_DIR"
        ok "building $SOURCE"
        return
    fi
    local here=""
    if [[ -n "${BASH_SOURCE[0]:-}" && -f "${BASH_SOURCE[0]}" ]]; then
        here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    fi
    if [[ -n "$here" ]] && is_checkout "$here"; then
        SOURCE="$here"
        ok "building this checkout ($SOURCE)"
        return
    fi
    command -v git > /dev/null || die "git is needed to download eoe. Install git and run this again."
    SOURCE="$DATA_DIR/src"
    if [[ -d "$SOURCE/.git" ]]; then
        git -C "$SOURCE" fetch --quiet --depth 1 origin "$REF" || die "Could not update $SOURCE from $REPO."
        git -C "$SOURCE" checkout --quiet --force FETCH_HEAD
        ok "updated $SOURCE ($REF)"
    else
        mkdir -p "$DATA_DIR"
        git clone --quiet --depth 1 --branch "$REF" "$REPO" "$SOURCE" \
            || die "Could not clone $REPO ($REF). Set EOE_REPO to the right URL."
        ok "downloaded $REPO ($REF)"
    fi
}

write_config() {
    local file="$CONFIG_DIR/config.toml"
    if [[ -e "$file" ]]; then
        ok "kept your config ($file)"
        return
    fi
    mkdir -p "$CONFIG_DIR"
    cat > "$file" << 'EOF'
# eoe config. Every setting is optional; delete a line to get the default back.
# Key and action names are listed in the README, under Config.

# Let Alt+letter do what Ctrl+letter does.
alt_fallback = true

# Give an action an extra key. The default keys keep working.
[keys]
# save_all = "ctrl+g"
# find = "f2"
# beginning_of_file = "ctrl+home"
EOF
    ok "wrote a default config ($file)"
}

add_to_omarchy() {
    local icon="$SOURCE/assets/icon.png"
    [[ -f "$icon" ]] || icon="accessories-text-editor"
    if omarchy-tui-install "$LAUNCHER_NAME" "$BIN_DIR/ee" tile "$icon" > /dev/null; then
        ok "added $LAUNCHER_NAME to the Omarchy app launcher (Super+Space)"
    else
        warn "could not add the app launcher entry; ee itself is installed"
    fi
}

remove_from_omarchy() {
    local entry="$HOME/.local/share/applications/$LAUNCHER_NAME.desktop"
    [[ -e "$entry" ]] || return 0
    command -v omarchy-tui-remove > /dev/null || { rm -f "$entry"; return 0; }
    OMARCHY_REMOVE_NOTIFY=false omarchy-tui-remove "$LAUNCHER_NAME" > /dev/null
    ok "removed $LAUNCHER_NAME from the Omarchy app launcher"
}

check_path() {
    case ":$PATH:" in
        *":$BIN_DIR:"*) ;;
        *)
            warn "$BIN_DIR is not on your PATH. Add this to your shell's rc file:"
            printf '      export PATH="%s:$PATH"\n' "$BIN_DIR" >&2
            return
            ;;
    esac
    local found
    found="$(command -v ee || true)"
    if [[ -n "$found" && "$found" != "$BIN_DIR/ee" ]]; then
        warn "another ee comes first on your PATH: $found (Easy Editor, probably)."
        warn "Put $BIN_DIR earlier in PATH, or run $BIN_DIR/ee."
    fi
}

install_ee() {
    banner
    if is_omarchy; then
        ok "Omarchy $(omarchy-version 2> /dev/null || true) detected: installing the Omarchy way"
    fi
    step "checking the toolchain"
    ensure_rust
    step "getting the source"
    find_source
    step "compiling (this takes a minute the first time)"
    (cd "$SOURCE" && cargo build --release --locked --quiet) || die "The build failed. The compiler output above says why."
    ok "built target/release/ee"
    step "installing"
    install -Dm755 "$SOURCE/target/release/ee" "$BIN_DIR/ee"
    ok "installed $BIN_DIR/ee"
    write_config
    if is_omarchy; then
        add_to_omarchy
    fi
    check_path
    printf '\n  %s%sjacked in.%s run %see notes.md%s to start.\n\n' "$ICE" "$BOLD" "$RESET" "$BOLD" "$RESET"
}

uninstall_ee() {
    local purge="$1"
    step "uninstalling"
    if [[ -e "$BIN_DIR/ee" ]]; then
        rm -f "$BIN_DIR/ee"
        ok "removed $BIN_DIR/ee"
    else
        ok "ee was not installed in $BIN_DIR"
    fi
    remove_from_omarchy
    if [[ -d "$DATA_DIR" ]]; then
        rm -rf "$DATA_DIR"
        ok "removed the downloaded source ($DATA_DIR)"
    fi
    if [[ "$purge" == 1 && -d "$CONFIG_DIR" ]]; then
        rm -rf "$CONFIG_DIR"
        ok "removed your config ($CONFIG_DIR)"
    elif [[ -d "$CONFIG_DIR" ]]; then
        ok "kept your config ($CONFIG_DIR); add --purge to remove it"
    fi
}

main() {
    local mode=install purge=0
    for arg in "$@"; do
        case "$arg" in
            --uninstall) mode=uninstall ;;
            --purge) purge=1 ;;
            -h | --help)
                sed -n '2,16p' "${BASH_SOURCE[0]:-/dev/null}" 2> /dev/null | sed 's/^# \{0,1\}//'
                exit 0
                ;;
            *) die "unknown option: $arg (try --help)" ;;
        esac
    done
    if [[ "$mode" == uninstall ]]; then
        uninstall_ee "$purge"
    else
        install_ee
    fi
}

main "$@"
