<div align="center">

<img src="assets/readme/hero.svg" width="100%" alt="ee boots in a terminal: the ee logo decodes out of noise, a neon scan wipes the editor into view, and a markdown file renders live. A task typed on the caret line turns into a checkbox the moment the caret leaves it.">

# Eric's Own Editor

**A keyboard-first plain-text editor that boots like a movie hack.**<br>
The project is `eoe`, the command is `ee`. Type it, jack in, edit text in neon.

<a href="#jack-in"><img alt="built with Rust" src="https://img.shields.io/badge/built_with-Rust-ff2a6d?style=for-the-badge&logo=rust&logoColor=white&labelColor=0b0620"></a>
<a href="https://ratatui.rs"><img alt="ratatui 0.30" src="https://img.shields.io/badge/ratatui-0.30-29f0ff?style=for-the-badge&labelColor=0b0620"></a>
<a href="https://github.com/ratatui/tachyonfx"><img alt="tachyonfx 0.25" src="https://img.shields.io/badge/tachyonfx-0.25-ffb627?style=for-the-badge&labelColor=0b0620"></a>
<a href="https://omarchy.org"><img alt="made for Omarchy 4" src="https://img.shields.io/badge/made_for-Omarchy_4-d6dcf5?style=for-the-badge&labelColor=0b0620"></a>
<a href="#send-lines-to-herdr"><img alt="herdr ready" src="https://img.shields.io/badge/herdr-ready-ff2a6d?style=for-the-badge&labelColor=0b0620"></a>

<kbd>[jack in](#jack-in)</kbd>&nbsp;
<kbd>[what it does](#what-it-does)</kbd>&nbsp;
<kbd>[keys](#keys)</kbd>&nbsp;
<kbd>[config](#config)</kbd>&nbsp;
<kbd>[under the hood](#under-the-hood)</kbd>&nbsp;
<kbd>[faq](#faq)</kbd>

</div>

```diff
+ [ ok ] mounting buffers
+ [ ok ] calibrating neon
+ [ ok ] ignoring your opinions
- [ -- ] plugin system not found (by design)
```

`ee` is a plain-text editor for the terminal, built for [Omarchy](https://omarchy.org) and for people with WebStorm in their fingers. It does multi-cursor editing, renders markdown live while you write it, sends lines to the agent or shell next to it in [herdr](https://herdr.dev), and animates every move with [tachyonfx](https://github.com/ratatui/tachyonfx). It keeps your terminal's own background, so Omarchy's blur shows through the neon.

<img src="assets/readme/divider.svg" width="100%" alt="">

## Jack in

```sh
curl -fsSL https://raw.githubusercontent.com/uxeric/ee/HEAD/install.sh | bash
```

The installer builds `ee` from source and installs it for you, no root needed. Run it again any time to update.

1. **Rust:** it checks for Rust 1.91 or newer. If Rust is missing, it asks before installing it: on Omarchy with Omarchy's own `omarchy-install-dev-env rust`, elsewhere with `pacman` or [rustup](https://rustup.rs).
2. **Source:** it downloads the source into `~/.local/share/eoe/src`, or builds the checkout it's run from.
3. **Binary:** it compiles a release build and installs it as `~/.local/bin/ee`.
4. **Config:** it writes a commented `~/.config/eoe/config.toml`, but only if you don't have one. Your config is never overwritten.
5. **Omarchy:** on Omarchy it also adds *Eric's Own Editor*, with its own icon, to the app launcher (<kbd>Super</kbd>+<kbd>Space</kbd>) through `omarchy-tui-install`, so it opens tiled in your terminal like any other Omarchy TUI.
6. **PATH:** it warns if `~/.local/bin` isn't on your `PATH`, or if another `ee` comes first (hello, Easy Editor).

You need a terminal that passes `Ctrl` and `Alt` through: Alacritty, kitty, Ghostty, foot and WezTerm all do.

<details>
<summary><b>From a clone, uninstalling, and installer settings</b></summary>

```sh
./install.sh                          # build and install this checkout
./install.sh --uninstall              # remove ee (and its Omarchy launcher entry), keep your config
./install.sh --uninstall --purge      # remove ee and your config
curl -fsSL https://raw.githubusercontent.com/uxeric/ee/HEAD/install.sh | bash -s -- --uninstall
```

| Variable | Default | What it sets |
|---|---|---|
| `EOE_PREFIX` | `~/.local` | Install prefix; `ee` goes in `$EOE_PREFIX/bin` |
| `EOE_REPO` | `https://github.com/uxeric/ee.git` | Where to download the source from |
| `EOE_REF` | the default branch | The branch or tag to build |
| `EOE_SOURCE_DIR` | | Build this directory instead of downloading |
| `EOE_YES=1` | | Answer yes to prompts, such as installing Rust |
| `EOE_NO_OMARCHY=1` | | Skip the Omarchy integration: no launcher entry, and Rust comes from `pacman` or rustup |

Omarchy's own default-editor setting (`omarchy-default-editor`) only accepts a fixed list of editors, so the installer doesn't set `ee` there.

Prefer doing it by hand? `cargo build --release && install -Dm755 target/release/ee ~/.local/bin/ee`.

</details>

Then run `ee notes.md`. If the file doesn't exist yet, `ee` opens it empty and the first save creates it. It opens with a short boot sequence; any key skips it. `Ctrl+Q` quits, and asks for a second press if something is unsaved. `EOE_NO_FX=1 ee` turns off every animation.

<img src="assets/readme/divider.svg" width="100%" alt="">

## What it does

### Markdown that renders while you write it

In `.md` files, every line is shown rendered except the lines you're editing. The caret's line, and any line in a selection, show their raw source, so you always edit the real text. One source line is always one row, so nothing jumps around.

- **Headings** lose their `#`s and take a colour per level.
- **Bold**, *italic*, ~~strikethrough~~ and `code` lose their markers; links show only their text.
- **Lists** get `•` `◦` `▪` bullets by depth; tasks become `□` and `✓`.
- **Quotes** get a `▌` bar, rules become a full-width line, and fenced code gets a labelled rule over a tinted body.
- **Tables** line up. Columns follow `:---`, `:---:` and `---:` alignment, and columns of numbers right-align on their own. The header sits on a violet band over a double cyan rule, and body rows are zebra-striped.

`Alt`+Click on a rendered line lands on the character you clicked. The recording at the top is a real session: `- [ ] jack out` is typed raw, then becomes a checkbox the moment the caret moves on.

### Many carets, one keystroke

<p align="center"><img src="assets/readme/multicursor.svg" alt="Three carets added with Ctrl+Down type the same text on three lines at once; then Alt+Shift+E sends all three lines to Claude in the next herdr pane, and a cyan beam crosses the status bar."></p>

`Ctrl+Up` and `Ctrl+Down` leave a caret behind; `Alt`+Click adds one anywhere. Typing, deleting and pasting a single line happen at every caret, and the arrow keys move them all. `Alt+Shift+J` puts a caret on every copy of the word under the caret, so typing replaces them all at once. `Esc` goes back to one caret.

### Send lines to herdr

When `ee` runs inside [herdr](https://herdr.dev), `Alt+Shift+E` sends the caret's line to the pane next to it and presses Enter. With several carets, it sends every caret's line, top to bottom, as one paste. If herdr sees an agent such as Claude in that pane, the lines arrive as a prompt.

- **Target:** the other pane in `ee`'s tab. If the tab has several, `ee` tries the neighbour to the right, then left, below and above.
- **Never itself:** `ee` never sends to its own pane or to other tabs.
- **Outside herdr:** the status bar says "not running inside herdr" and nothing is sent.

### Find as you type

<p align="center"><img src="assets/readme/find.svg" alt="The find bar opens and the query neon is typed letter by letter; every match lights up teal, the current match glows magenta, and Enter walks from match to match."></p>

`Ctrl+F` searches the file as you type, and every match lights up. `Shift` `Shift` (tap it twice) or `Alt+Shift+F` searches every open tab. `Ctrl+R` adds a Replace field. `Esc` closes the bar and leaves the match selected, so typing replaces it, and `F3` keeps going afterwards.

### Motion that answers what you do

<p align="center"><img src="assets/readme/tabs.svg" alt="Switching tabs sweeps the new tab's text in from the side; opening a missing file makes the status bar glitch in red with the message cannot open vault.key: No such file or directory."></p>

| When | What you see |
|---|---|
| Starting up | The `ee` logo decodes out of noise, then the editor jacks in behind a neon scan |
| Switching tabs | The new tab's text sweeps in from the direction you moved |
| Opening a file | The text decodes in with a cyan glow |
| Jumping to a match | The match flashes magenta |
| Opening the find bar | The bar sweeps in |
| Saving, or sending to herdr | A cyan beam crosses the status bar |
| A warning | The status bar flashes amber |
| An error | The status bar glitches red |
| Quitting | The screen dissolves |

The badge at the bottom left says where your typing goes: `edit`, `find`, `find in files`, `replace` or `prompt`. The editor only redraws while an effect runs, so it uses no CPU when idle. `EOE_NO_FX=1` turns all of it off.

<img src="assets/readme/divider.svg" width="100%" alt="">

## Keys

Terminals can't tell `Ctrl+X` from `Ctrl+Shift+X`, so WebStorm's `Ctrl+Shift+letter` actions live on the plain `Ctrl+letter` key. Where that key was taken, the action moved to `Alt+Shift+letter`. Most `Ctrl` keys also answer to `Alt`, which you can turn off in the [config](#config).

### Cheat sheet

| Action | Keys | Action | Keys |
|---|---|---|---|
| Save all | <kbd>Ctrl</kbd>+<kbd>S</kbd> | Find | <kbd>Ctrl</kbd>+<kbd>F</kbd> |
| Undo / redo | <kbd>Ctrl</kbd>+<kbd>Z</kbd> / <kbd>Alt</kbd>+<kbd>Y</kbd> | Find in all tabs | <kbd>Shift</kbd> <kbd>Shift</kbd> |
| Copy / cut / paste | <kbd>Ctrl</kbd>+<kbd>C</kbd> <kbd>X</kbd> <kbd>V</kbd> | Replace | <kbd>Ctrl</kbd>+<kbd>R</kbd> |
| Add caret below | <kbd>Ctrl</kbd>+<kbd>↓</kbd> | Next / previous tab | <kbd>Ctrl</kbd>+<kbd>]</kbd> / <kbd>Ctrl</kbd>+<kbd>[</kbd> |
| Duplicate line | <kbd>Ctrl</kbd>+<kbd>D</kbd> | Open file | <kbd>Ctrl</kbd>+<kbd>N</kbd> |
| Send to herdr | <kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>E</kbd> | Quit | <kbd>Ctrl</kbd>+<kbd>Q</kbd> |

<details>
<summary><b>Editing and movement</b></summary>

| Action | Primary | Fallback |
|---|---|---|
| Quit (press twice if there are unsaved changes) | <kbd>Ctrl</kbd>+<kbd>Q</kbd> | <kbd>Alt</kbd>+<kbd>Q</kbd> |
| Insert char | typed char | — |
| Paste from the terminal | terminal paste (bracketed paste) | — |
| Delete char backward | <kbd>Backspace</kbd> / <kbd>Ctrl</kbd>+<kbd>H</kbd> | <kbd>Alt</kbd>+<kbd>H</kbd> |
| Delete char forward | <kbd>Delete</kbd> | — |
| Delete word forward | <kbd>Ctrl</kbd>+<kbd>W</kbd> / <kbd>Ctrl</kbd>+<kbd>Delete</kbd> | <kbd>Alt</kbd>+<kbd>W</kbd> |
| Delete word backward | <kbd>Ctrl</kbd>+<kbd>Backspace</kbd> | — |
| Newline | <kbd>Enter</kbd> | — |
| Move left / right / up / down | arrow keys | — |
| Select while moving | <kbd>Shift</kbd> + arrow keys / <kbd>Home</kbd> / <kbd>End</kbd> | — |
| Beginning of line | <kbd>Home</kbd> / <kbd>Ctrl</kbd>+<kbd>A</kbd> | <kbd>Alt</kbd>+<kbd>A</kbd> |
| End of line | <kbd>End</kbd> / <kbd>Ctrl</kbd>+<kbd>E</kbd> | <kbd>Alt</kbd>+<kbd>E</kbd> |
| Duplicate line | <kbd>Ctrl</kbd>+<kbd>D</kbd> | <kbd>Alt</kbd>+<kbd>D</kbd> |
| Delete line | <kbd>Ctrl</kbd>+<kbd>Y</kbd> | — |
| Move line up / down | <kbd>Alt</kbd>+<kbd>↑</kbd> / <kbd>Alt</kbd>+<kbd>↓</kbd> | — |
| Join lines | <kbd>Ctrl</kbd>+<kbd>J</kbd> | <kbd>Alt</kbd>+<kbd>J</kbd> |
| Reformat (trim trailing whitespace, at most 2 blank lines in a row) | <kbd>Ctrl</kbd>+<kbd>K</kbd> | <kbd>Alt</kbd>+<kbd>K</kbd> |
| Undo | <kbd>Ctrl</kbd>+<kbd>Z</kbd> | <kbd>Alt</kbd>+<kbd>Z</kbd> |
| Redo | — | <kbd>Alt</kbd>+<kbd>Y</kbd> |

Redo has no `Ctrl` key because `Ctrl+Y` is Delete line. Plain movement clears the selection.

</details>

<details>
<summary><b>Clipboard</b></summary>

| Action | Primary | Fallback |
|---|---|---|
| Copy (the selection, or the whole line if nothing is selected) | <kbd>Ctrl</kbd>+<kbd>C</kbd> | <kbd>Alt</kbd>+<kbd>C</kbd> |
| Cut (the selection, or the whole line) | <kbd>Ctrl</kbd>+<kbd>X</kbd> | <kbd>Alt</kbd>+<kbd>X</kbd> |
| Paste (a copied whole line goes above the current line) | <kbd>Ctrl</kbd>+<kbd>V</kbd> | <kbd>Alt</kbd>+<kbd>V</kbd> |

The system clipboard works through `wl-copy`/`wl-paste` (Wayland), `xclip` or `xsel` (X11), `pbcopy`/`pbpaste` (macOS), or `clip`/PowerShell (Windows). Without any of those, Copy asks the terminal to set the clipboard (OSC 52, which also works over SSH in terminals that support it). Copy and paste inside the editor always work.

</details>

<details>
<summary><b>Multi-cursor</b></summary>

| Action | Key |
|---|---|
| Add caret up / down | <kbd>Ctrl</kbd>+<kbd>↑</kbd> / <kbd>Ctrl</kbd>+<kbd>↓</kbd> |
| Add or remove a caret at a position | <kbd>Alt</kbd> + left click |
| Select all occurrences of the word or selection (press again to clear) | <kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>J</kbd> |
| Back to one caret, clear the selection | <kbd>Esc</kbd> |

Typing, deleting and pasting a single line apply to every caret. The arrow keys move every caret. After select all occurrences, typing replaces every occurrence.

</details>

<details>
<summary><b>Find and replace</b></summary>

| Action | Key |
|---|---|
| Find in the current file | <kbd>Ctrl</kbd>+<kbd>F</kbd> / <kbd>Alt</kbd>+<kbd>F</kbd> |
| Find in all open files | <kbd>Shift</kbd> <kbd>Shift</kbd> (tap twice) / <kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>F</kbd> |
| Replace | <kbd>Ctrl</kbd>+<kbd>R</kbd> / <kbd>Alt</kbd>+<kbd>R</kbd> |
| Next / previous match (also after closing the bar) | <kbd>F3</kbd> / <kbd>Shift</kbd>+<kbd>F3</kbd> |

In the find bar, typing searches as you type. <kbd>Enter</kbd> or <kbd>↓</kbd> goes to the next match, <kbd>↑</kbd> to the previous one, and <kbd>Alt</kbd>+<kbd>C</kbd> toggles match case. <kbd>Esc</kbd> closes the bar and leaves the match selected. A selection on one line pre-fills the query, and <kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>J</kbd> in the bar puts a caret on every match.

In the replace bar, <kbd>Tab</kbd> switches between the Find and Replace fields. <kbd>Enter</kbd> in the Replace field (or <kbd>Alt</kbd>+<kbd>P</kbd>) replaces the current match and moves to the next one, and <kbd>Alt</kbd>+<kbd>A</kbd> replaces all. Replace all is a single undo step.

</details>

<details>
<summary><b>Files and tabs</b></summary>

| Action | Key |
|---|---|
| Save all | <kbd>Ctrl</kbd>+<kbd>S</kbd> / <kbd>Alt</kbd>+<kbd>S</kbd> |
| Open or create a file (a path prompt: <kbd>Enter</kbd> opens, <kbd>Esc</kbd> cancels) | <kbd>Ctrl</kbd>+<kbd>N</kbd> / <kbd>Alt</kbd>+<kbd>N</kbd> |
| Rename the file, or save an untitled tab under a name | <kbd>Shift</kbd>+<kbd>F6</kbd> |
| Next tab | <kbd>Ctrl</kbd>+<kbd>]</kbd> / <kbd>Alt</kbd>+<kbd>→</kbd> / <kbd>Ctrl</kbd>+<kbd>PageDown</kbd> |
| Previous tab | <kbd>Ctrl</kbd>+<kbd>[</kbd> / <kbd>Alt</kbd>+<kbd>←</kbd> / <kbd>Ctrl</kbd>+<kbd>PageUp</kbd> |
| Close tab | <kbd>Ctrl</kbd>+<kbd>F4</kbd> |

- Tab switching wraps around.
- Closing a tab with unsaved changes shows a warning; press <kbd>Ctrl</kbd>+<kbd>F4</kbd> again right away to discard them. Closing the last tab quits.
- A file that doesn't exist, named on the command line or typed into the <kbd>Ctrl</kbd>+<kbd>N</kbd> prompt, opens as an empty tab, and saving creates it. Anything else that can't be opened, such as a directory, is an error: the prompt shows why in the status bar, and on the command line `ee` stops with a message.
- Opening a file that's already open switches to its tab. Opening a file from an empty, unmodified untitled tab replaces that tab.
- Rename writes the tab, unsaved edits included, to the new path and deletes the old file. If the new path exists, press <kbd>Enter</kbd> a second time to overwrite it.

</details>

<details>
<summary><b>Send to herdr</b></summary>

| Action | Key |
|---|---|
| Send the caret's line (every caret's line with multi-cursor) to the other herdr pane and press Enter | <kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>E</kbd> / <kbd>Ctrl</kbd>+<kbd>Enter</kbd> |

<kbd>Ctrl</kbd>+<kbd>Enter</kbd> needs a terminal with the kitty keyboard protocol; <kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>E</kbd> works everywhere.

</details>

<details>
<summary><b>Not applicable in plain text</b></summary>

These code-editor keys are bound and show "not applicable":

| Action | Key |
|---|---|
| Go to declaration | <kbd>Ctrl</kbd>+<kbd>B</kbd> / <kbd>Alt</kbd>+<kbd>B</kbd> |
| Quick definition | <kbd>Ctrl</kbd>+<kbd>I</kbd> / <kbd>Alt</kbd>+<kbd>I</kbd> |

</details>

<details>
<summary><b>Keyboard protocol</b></summary>

When the terminal supports the kitty keyboard protocol (Alacritty, kitty, Ghostty, foot and WezTerm do), `ee` turns it on. That's what makes <kbd>Ctrl</kbd>+<kbd>[</kbd> (otherwise the same key as <kbd>Esc</kbd>) and the double <kbd>Shift</kbd> tap work. <kbd>Ctrl</kbd>+<kbd>]</kbd> works in any terminal. Start with `EOE_LEGACY_KEYS=1` to use the classic key encoding instead, and try that first if typing accents with dead keys or an input method misbehaves.

Not deliverable as distinct keys, so unbound by default: `Ctrl+Home`, `Ctrl+End`, `Ctrl+Left`, `Ctrl+Right`, `Ctrl+Space` and their `Alt` variants (beginning/end of file, code completion). `Ctrl+Enter` only arrives in terminals with the kitty keyboard protocol.

</details>

<img src="assets/readme/divider.svg" width="100%" alt="">

## Config

`ee` reads `~/.config/eoe/config.toml` (or `$XDG_CONFIG_HOME/eoe/config.toml`) at startup. The file is optional.

```toml
# Let Alt+letter do what Ctrl+letter does. Default: true.
alt_fallback = true

# Give an action an extra key. The default keys keep working.
[keys]
save_all = "ctrl+g"
find = "f2"
beginning_of_file = "ctrl+home"   # works in terminals with the kitty keyboard protocol
```

Keys are written like `ctrl+s`, `alt+shift+f`, `shift+f6`, `f3`, `ctrl+enter` or `pageup`. The modifiers are `ctrl`, `alt` and `shift`. The named keys are `enter`, `esc`, `tab`, `backspace`, `delete`, `up`, `down`, `left`, `right`, `home`, `end`, `pageup`, `pagedown`, `space` and `f1` to `f12`.

If the file has a mistake, `ee` starts with the defaults and the status bar says what it couldn't read, for example ``config: unknown action `teleport` (using defaults)``.

<details>
<summary><b>Action names</b></summary>

`quit` `backspace` `newline` `left` `right` `up` `down` `home` `end` `beginning_of_file` `end_of_file` `beginning_of_line` `end_of_line` `delete_word` `delete_char` `duplicate_line` `delete_line` `move_line_up` `move_line_down` `add_caret_up` `add_caret_down` `delete_word_forward` `delete_word_backward` `undo` `redo` `copy` `paste` `cut` `save_all` `find` `find_in_files` `find_next` `find_previous` `replace` `select_all_occurrences` `select_left` `select_right` `select_up` `select_down` `select_home` `select_end` `send_to_pane` `clear_extra_carets` `join_lines` `reformat` `rename` `open_file` `next_tab` `previous_tab` `close_tab`

</details>

| Environment variable | What it does |
|---|---|
| `EOE_NO_FX=1` | Turns off every animation, including the boot sequence |
| `EOE_LEGACY_KEYS=1` | Uses the classic key encoding instead of the kitty keyboard protocol |

<img src="assets/readme/divider.svg" width="100%" alt="">

## Under the hood

```mermaid
%%{init: {'theme':'base','themeVariables':{'background':'#0b0620','primaryColor':'#140c2e','primaryTextColor':'#d6dcf5','primaryBorderColor':'#29f0ff','lineColor':'#ff2a6d','secondaryColor':'#241452','tertiaryColor':'#0b0620','edgeLabelBackground':'#0b0620','fontFamily':'monospace'}}}%%
flowchart LR
    term([terminal]) -- keys, mouse, paste --> main[main.rs<br>event loop]
    config[config.rs<br>~/.config/eoe] --> keys[keys.rs<br>keymap]
    keys --> main
    main -- Action --> state[state.rs<br>EditorState]
    state --> doc[document.rs<br>lines, carets, undo]
    state --> find[find.rs]
    state --> clip[clipboard.rs]
    state --> herdr[herdr.rs<br>socket API]
    state -- Cue --> motion[motion.rs<br>tachyonfx]
    main --> ui[ui.rs<br>render]
    ui --> md[markdown.rs<br>live preview]
    ui --> theme[theme.rs<br>neon palette]
    motion --> ui
```

| File | What lives there |
|---|---|
| `src/main.rs` | Terminal setup, the event loop, mouse and paste handling |
| `src/state.rs` | `EditorState`: tabs, modes (find/replace bar, prompts), find/replace, clipboard actions, open/rename/save |
| `src/document.rs` | `Document`: lines, selection, the active caret and extra carets, occurrences, edits, undo/redo |
| `src/keys.rs` | The `Action` enum, the keymap, and the find-bar keys |
| `src/config.rs` | Loads `config.toml`: key remaps and the `Alt` fallback |
| `src/ui.rs` | Rendering: tab bar, editor, carets and highlights, find bar, prompt, status bar |
| `src/markdown.rs` | Markdown live preview: one rendered row per source line, with display-to-source column mapping |
| `src/motion.rs` | Animations: editor events (`Cue`s) mapped to tachyonfx effects, plus the boot splash |
| `src/theme.rs` | The palette and shared styles |
| `src/find.rs` | Text matching, with or without case |
| `src/clipboard.rs` | System clipboard through platform tools, with the OSC 52 fallback |
| `src/herdr.rs` | Sends lines to another herdr pane over herdr's socket API |

Every animation in this README is a real `ee` session: the renderer drew each frame, and the frames were encoded as SVG with an embedded JetBrains Mono subset.

```sh
cargo test                 # the unit tests
cargo run -- notes.md      # the debug build
EOE_NO_FX=1 cargo run      # the debug build, no animations
```

`TODO.md` has the implementation plan and task-by-task notes.

<img src="assets/readme/divider.svg" width="100%" alt="">

## FAQ

> [!IMPORTANT]
> **Do you like the shortcut keys that I like?**<br>
> Probably not.

> [!NOTE]
> **Will I ever include a plugin system?**<br>
> Nope. Edit the codebase and extend it however you like.

> [!WARNING]
> **Your design choices stink.**<br>
> Yeah, probably. Take 5 minutes with an LLM and make it your own.

> [!CAUTION]
> **It's too cyberpunk.**<br>
> Yes it is!

> [!TIP]
> **Do you know that Easy Editor has dibs on the `ee` command?**<br>
> Yup. Did you read the name of the editor? Eric's Own Editor. I can do what I want!

<div align="center">
<br>

<img src="assets/readme/divider.svg" width="60%" alt="">

<sub><code>connection closed by remote host</code></sub>

</div>
