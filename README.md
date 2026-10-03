<div align="center">

<img src="assets/readme/hero.svg" width="100%" alt="ee opens and jacks in behind a neon scan; a markdown file renders live, a task typed on the caret line turns into a checkbox as the caret leaves it, and the file-structure popup jumps to the Crew heading and back.">

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

`ee` is a plain-text editor for the terminal, built for [Omarchy](https://omarchy.org) (or whatever sub-optimal unix-like system you're running) and for people with WebStorm in their fingers. It does multi-cursor editing, renders markdown live while you write it, edits Word documents the same way, sends lines to the agent or shell next to it in [herdr](https://herdr.dev), and animates every move with [tachyonfx](https://github.com/ratatui/tachyonfx). It takes its colours from your Omarchy theme and follows it when you switch, and keeps your terminal's own background, so Omarchy's blur shows through.

<img src="assets/readme/divider.svg" width="100%" alt="">

## Jack in

```sh
curl -fsSL https://raw.githubusercontent.com/uxeric/ee/HEAD/install.sh | bash
```

It builds `ee` from source into `~/.local/bin/ee`, no root needed, and offers to install Rust if you don't have it. On Omarchy it also adds `ee` to the app launcher. It writes `~/.config/eoe/config.toml` once and never overwrites it. Run the same command again to update.

Then run `ee notes.md`. A file that doesn't exist yet is created on the first save. <kbd>Ctrl</kbd>+<kbd>Q</kbd> quits.

`ee` checks for updates in the background and offers to restart into a new version; `ee --update` updates by hand and `ee --version` shows what you have. You need a terminal that passes `Ctrl` and `Alt` through: Alacritty, kitty, Ghostty, foot and WezTerm all do.

<details>
<summary><b>From a clone, uninstalling, and installer settings</b></summary>

```sh
./install.sh                          # build and install this checkout
./install.sh --force                  # rebuild even if ee is already up to date
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

`EOE_NO_FX=1` turns off every animation, and `EOE_NO_UPDATE_CHECK=1` turns off the update check.

<img src="assets/readme/divider.svg" width="100%" alt="">

## What it does

### Markdown that renders while you write it

In `.md` files, every line is rendered except the one you're editing, which shows its raw source. A long line wraps onto the next rows so the end stays on screen. The caret, a selection, and a click follow those rows. One source line is still one paragraph. A rendered table stays one row per line, so its columns stay lined up; the row you are editing still wraps.

Headings, emphasis, lists, checkboxes, quotes, tables, GitHub alerts, code blocks (highlighted by language), common HTML and shields.io badges all render. Images show as a placeholder. <kbd>Ctrl</kbd>+Click follows a link: to a heading, another file, or your browser.

### Word documents, the same way

A `.docx` file is one paragraph per line. A long paragraph wraps onto the next rows the same way. Lines you are not editing render with the same heading, bold, italic, strike, and link colours as markdown. The line under the caret shows the marks: `# `, `**bold**`, `*italic*`, `~~strike~~`, `[label](url)`, `- ` for a bullet, `1. ` for a numbered item, `> ` for a quote. <kbd>Ctrl</kbd>+Click follows a link, and <kbd>Ctrl</kbd>+<kbd>F12</kbd> jumps to a heading.

The file on disk stays a Word document. Open it and save without typing, and the bytes come back unchanged. A change in one paragraph leaves the others, and every table, picture, header, and style, as Word stored them. A typo inside a coloured word keeps that colour and size. Adding or removing bold, italic, or strike updates those marks on the words you changed, and those words then use the editor's colours, so Word's colour and size on them are cleared. Changing a heading, list, or quote marker updates that paragraph's style. The number you type on a list line is how it looks here; Word keeps its own numbering.

Tables and pictures show as `[table]` and `[picture]`. Typing on that line leaves the file alone. Deleting the line removes the table or picture. A paragraph that holds comments, fields, or tracked changes is rewritten from the line when you edit it, and the status bar says so. Underline and highlight stay until you change that paragraph's text or marks. A new `notes.docx` becomes a small valid document the first time you save it.

### Syntax colours

JSON, TOML, YAML, JavaScript (including JSX), TypeScript, TSX, Python, CSS, HTML, Rust, Bash, Java and Go are coloured from the theme you are using: keywords, strings, comments, functions and types each get their own colour, and comments sit back. Markdown fences in those languages use the same colours. An HTML file colours its tags, attributes and comments; JavaScript and CSS written inside it stay plain. A file over 100,000 lines or 5 MB stays plain, and so does a Bash file over 1,500 lines. The status bar says so.

### Many carets, one keystroke

<p align="center"><img src="assets/readme/multicursor.svg" alt="Three carets added with Ctrl+Down type the same text on three lines at once; then Alt+Shift+E sends all three lines to Claude in the next herdr pane, and a cyan beam crosses the status bar."></p>

`Ctrl+Up` and `Ctrl+Down` leave a caret behind; `Alt`+Click adds one anywhere. Typing, deleting and pasting (even several lines) happen at every caret, and the arrow keys move them all. `Shift+Home`, `Shift+End` and the other `Shift` moves give every caret its own selection; copy takes them all, one per line, and pasting as many lines as there are carets gives each caret its own line. `Alt+Shift+J` puts a caret on every copy of the word under the caret, so typing replaces them all at once. `Esc` goes back to one caret.

### Send lines to herdr

With [herdr](https://herdr.dev) running, `Alt+Shift+E` pastes the caret's line into a herdr pane without pressing Enter, so you can check it, edit it, or send more before you run it. With several carets, it pastes every caret's line, top to bottom, as one paste. Agents such as Claude get the same paste in their prompt box. `Alt+Shift+M` moves the lines instead: they're sent the same way, then removed from your file (one `Ctrl+Z` brings them back). If the send fails, nothing is removed. On Omarchy, moving also takes you there: inside herdr the target pane gets the focus, and from a separate terminal the herdr window comes to the front, so you can press Enter right away. If `ee` can't tell for certain which window that is, it leaves your focus where it was.

- **Inside herdr:** the lines go to the other pane in `ee`'s tab. If the tab has several, `ee` tries the neighbour to the right, then left, below and above. It never sends to its own pane or to other tabs.
- **Outside herdr** (`ee` in a plain terminal): the lines go to whichever herdr pane is focused.
- **No herdr running:** the status bar says so and nothing is sent.

The sent lines answer back: a magenta write head sweeps each one, breaking it into `▓▒░` data blocks, and the text re-forms in cyan behind it.

### Find as you type

<p align="center"><img src="assets/readme/find.svg" alt="The find bar opens and the query neon is typed letter by letter; every match lights up teal, the current match glows magenta, and Enter walks from match to match."></p>

`Ctrl+F` searches the file as you type, and every match lights up. `Shift` `Shift` (tap it twice) or `Alt+Shift+F` searches every open tab. `Ctrl+R` adds a Replace field. `Esc` closes the bar and leaves the match selected, so typing replaces it, and `F3` keeps going afterwards.

### Motion that answers what you do

<p align="center"><img src="assets/readme/tabs.svg" alt="Switching tabs sweeps the new tab's text in from the side; opening a missing file makes the status bar glitch in red with the message cannot open vault.key: No such file or directory."></p>

| When | What you see |
|---|---|
| Starting up | The editor jacks in behind a neon scan (it never holds up your typing) |
| A popup opens (command palette, file structure, open file, theme, keys) | Its frame draws out from the corners while the text decrypts from hex noise, left to right |
| Moving the selection in a popup | The new row locks on with a cyan sweep |
| An update is ready | The update panel decrypts in, with an amber hazard band on its frame |
| A file changed on disk | A panel asks whether to reload it or keep editing |
| Switching tabs | The new tab's text sweeps in from the direction you moved |
| Opening a file | The text decodes in with a cyan glow |
| Jumping to a match | The match flashes magenta |
| Opening the find bar | The bar sweeps in |
| Saving | A cyan beam crosses the status bar |
| Moving lines to herdr | Each line is pulled back into a glowing packet and fired out through the edge on the side of the pane it's going to, trailing a beam; then the gap closes. Several carets launch in a volley |
| Sending to herdr | Each sent line is packetized by a magenta write head and re-forms in cyan |
| A warning | The status bar flashes amber |
| An error | The status bar glitches red |
| Quitting | The screen dissolves |

Popups float on a shadow over faint scanlines, inside a bracketed frame: the title is cut into the top edge, and the bottom edge carries the match count (or, on the keys sheet, the version). The badge at the bottom left says where your typing goes: `edit`, `find`, `find in files`, `replace` or `prompt`. The editor only redraws while an effect runs, so it uses no CPU when idle. `EOE_NO_FX=1` turns all of it off.

<img src="assets/readme/divider.svg" width="100%" alt="">

## Keys

Terminals can't tell `Ctrl+X` from `Ctrl+Shift+X`, so WebStorm's `Ctrl+Shift+letter` actions live on the plain `Ctrl+letter` key. Where that key was taken, the action moved to `Alt+Shift+letter`. Most `Ctrl` keys also answer to `Alt`, which you can turn off in the [config](#config).

### Cheat sheet

| Action | Keys | Action | Keys |
|---|---|---|---|
| Save all | <kbd>Ctrl</kbd>+<kbd>S</kbd> | Find | <kbd>Ctrl</kbd>+<kbd>F</kbd> |
| Undo / redo | <kbd>Ctrl</kbd>+<kbd>Z</kbd> / <kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>Z</kbd> | Find in all tabs | <kbd>Shift</kbd> <kbd>Shift</kbd> |
| Copy / cut / paste | <kbd>Ctrl</kbd>+<kbd>C</kbd> <kbd>X</kbd> <kbd>V</kbd> | Replace | <kbd>Ctrl</kbd>+<kbd>R</kbd> |
| Add caret below | <kbd>Ctrl</kbd>+<kbd>↓</kbd> | Next / previous tab | <kbd>Ctrl</kbd>+<kbd>]</kbd> / <kbd>Ctrl</kbd>+<kbd>[</kbd> |
| Duplicate line | <kbd>Ctrl</kbd>+<kbd>D</kbd> | Open file | <kbd>Ctrl</kbd>+<kbd>N</kbd> |
| Extend selection | <kbd>Ctrl</kbd>+<kbd>W</kbd> | Move by word | <kbd>Ctrl</kbd>+<kbd>←</kbd> <kbd>→</kbd> |
| Command palette | <kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>A</kbd> | File structure | <kbd>Ctrl</kbd>+<kbd>F12</kbd> |
| Select all | <kbd>Ctrl</kbd>+<kbd>A</kbd> | Toggle comment | <kbd>Ctrl</kbd>+<kbd>/</kbd> |
| Go to line | <kbd>Ctrl</kbd>+<kbd>G</kbd> | Back / forward | <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>←</kbd> <kbd>→</kbd> |
| Indent / unindent | <kbd>Tab</kbd> / <kbd>Shift</kbd>+<kbd>Tab</kbd> | Start new line | <kbd>Shift</kbd>+<kbd>Enter</kbd> |
| Send to herdr | <kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>E</kbd> | Switch theme | <kbd>Alt</kbd>+<kbd>`</kbd> |
| Quit | <kbd>Ctrl</kbd>+<kbd>Q</kbd> | Keyboard shortcuts | <kbd>F1</kbd>, or click **F1 help** in the status bar |

<details>
<summary><b>Editing and movement</b></summary>

| Action | Primary | Fallback |
|---|---|---|
| Quit (press twice if there are unsaved changes) | <kbd>Ctrl</kbd>+<kbd>Q</kbd> | <kbd>Alt</kbd>+<kbd>Q</kbd> |
| Insert char | typed char | — |
| Paste from the terminal | terminal paste (bracketed paste) | — |
| Delete char backward | <kbd>Backspace</kbd> / <kbd>Ctrl</kbd>+<kbd>H</kbd> | <kbd>Alt</kbd>+<kbd>H</kbd> |
| Delete char forward | <kbd>Delete</kbd> | — |
| Delete word forward | <kbd>Ctrl</kbd>+<kbd>Delete</kbd> | — |
| Delete word backward | <kbd>Ctrl</kbd>+<kbd>Backspace</kbd> | — |
| Newline | <kbd>Enter</kbd> | — |
| Start a new line above (keeps the indentation) | <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>Enter</kbd> | <kbd>Alt</kbd>+<kbd>Enter</kbd> |
| Start a new line below, from anywhere in the line (keeps its indentation) | <kbd>Shift</kbd>+<kbd>Enter</kbd> | — |
| Move left / right / up / down | arrow keys | — |
| Move by word | <kbd>Ctrl</kbd>+<kbd>←</kbd> / <kbd>Ctrl</kbd>+<kbd>→</kbd> | — |
| Page up / down: every caret moves a screen and the view follows | <kbd>PageUp</kbd> / <kbd>PageDown</kbd> | — |
| Beginning / end of file | <kbd>Ctrl</kbd>+<kbd>Home</kbd> / <kbd>Ctrl</kbd>+<kbd>End</kbd> | — |
| Select while moving, at every caret; moving back to where you started deselects | <kbd>Shift</kbd> + any of the moves above (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>→</kbd> selects by word) | — |
| Extend selection: word, then quotes or brackets, line, paragraph, whole file | <kbd>Ctrl</kbd>+<kbd>W</kbd> | <kbd>Alt</kbd>+<kbd>W</kbd> |
| Shrink selection back one step | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>W</kbd> | <kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>W</kbd> |
| First non-blank character; press again for column 0 (smart Home) | <kbd>Home</kbd> | — |
| Select all | <kbd>Ctrl</kbd>+<kbd>A</kbd> | <kbd>Alt</kbd>+<kbd>A</kbd> |
| End of line | <kbd>End</kbd> / <kbd>Ctrl</kbd>+<kbd>E</kbd> | <kbd>Alt</kbd>+<kbd>E</kbd> |
| Indent: to the next 4-column stop at every caret, or every selected line | <kbd>Tab</kbd> | — |
| Unindent the caret's lines or the selected lines | <kbd>Shift</kbd>+<kbd>Tab</kbd> | — |
| Duplicate line | <kbd>Ctrl</kbd>+<kbd>D</kbd> | <kbd>Alt</kbd>+<kbd>D</kbd> |
| Delete line | <kbd>Ctrl</kbd>+<kbd>Y</kbd> | — |
| Move line up / down | <kbd>Alt</kbd>+<kbd>↑</kbd> / <kbd>Alt</kbd>+<kbd>↓</kbd> (also with <kbd>Shift</kbd>) | — |
| Toggle case of the selection or the word at the caret | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>U</kbd> | <kbd>Alt</kbd>+<kbd>U</kbd> |
| Toggle line comment, using the file type's comment syntax | <kbd>Ctrl</kbd>+<kbd>/</kbd> | <kbd>Alt</kbd>+<kbd>/</kbd> |
| Join lines | <kbd>Ctrl</kbd>+<kbd>J</kbd> | <kbd>Alt</kbd>+<kbd>J</kbd> |
| Reformat (trim trailing whitespace, at most 2 blank lines in a row) | <kbd>Ctrl</kbd>+<kbd>K</kbd> / <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>L</kbd> | <kbd>Alt</kbd>+<kbd>K</kbd> |
| Undo | <kbd>Ctrl</kbd>+<kbd>Z</kbd> | <kbd>Alt</kbd>+<kbd>Z</kbd> |
| Redo | <kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>Z</kbd> (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Z</kbd> with the kitty keyboard protocol) | <kbd>Alt</kbd>+<kbd>Y</kbd> |

`Ctrl+Y` is Delete line, as in WebStorm. Plain movement clears the selection. A long line wraps onto the next rows. Up and down move one of those rows, and Home and End stay at the ends of the source line.

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
| Place the caret / select a word / select a line | click / double-click / triple-click |
| Select with the mouse | drag, or <kbd>Shift</kbd> + click to extend |
| Add or remove a caret at a position | <kbd>Alt</kbd> + left click |
| Follow a link in rendered markdown | <kbd>Ctrl</kbd> + left click |
| Scroll the view (the caret stays put until you move it) | mouse wheel |
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
| Open or create a file: fuzzy-find files under the current folder, or type a path | <kbd>Ctrl</kbd>+<kbd>N</kbd> / <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>N</kbd> / <kbd>Alt</kbd>+<kbd>N</kbd> |
| File structure: jump to a heading in a markdown or Word file | <kbd>Ctrl</kbd>+<kbd>F12</kbd> |
| Command palette: every action, with its keys | <kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>A</kbd> (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>A</kbd> with the kitty keyboard protocol) |
| Keyboard shortcuts: a sheet of the basics, including herdr and the command palette (any key closes it) | <kbd>F1</kbd>, or click **F1 help** at the bottom right |
| Switch theme: Follow Omarchy, Neon, or any installed Omarchy theme, previewed as you move | <kbd>Alt</kbd>+<kbd>`</kbd> (<kbd>Ctrl</kbd>+<kbd>`</kbd> with the kitty keyboard protocol) |
| Last edit location | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Backspace</kbd> |
| Rename the file, or save an untitled tab under a name | <kbd>Shift</kbd>+<kbd>F6</kbd> |
| Next tab | <kbd>Ctrl</kbd>+<kbd>]</kbd> / <kbd>Alt</kbd>+<kbd>→</kbd> / <kbd>Ctrl</kbd>+<kbd>PageDown</kbd> |
| Previous tab | <kbd>Ctrl</kbd>+<kbd>[</kbd> / <kbd>Alt</kbd>+<kbd>←</kbd> / <kbd>Ctrl</kbd>+<kbd>PageUp</kbd> |
| Close tab | <kbd>Ctrl</kbd>+<kbd>F4</kbd> |
| Go to line (`42`, or `42:7` for a column) | <kbd>Ctrl</kbd>+<kbd>G</kbd> / <kbd>Alt</kbd>+<kbd>G</kbd> |
| Back / forward through your jumps, across tabs | <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>←</kbd> / <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>→</kbd> |

- In the file finder, command palette, file structure and theme popups: type to fuzzy-filter, <kbd>↑</kbd>/<kbd>↓</kbd> or <kbd>PageUp</kbd>/<kbd>PageDown</kbd> to choose, <kbd>Enter</kbd> to open or run, <kbd>Esc</kbd> to close. In the theme popup, `ee` recolours as you move; <kbd>Enter</kbd> keeps the theme and saves it to your config, <kbd>Esc</kbd> puts the old one back. In the file finder, a path that starts with `/`, `~` or `.` opens exactly that file, creating it if it's new.
- Jumps are what back and forward remember: go to line, find, following a link, start or end of file, switching tabs and opening files.
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
| Send the caret's line (every caret's line with multi-cursor) to the other herdr pane, without pressing Enter | <kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>E</kbd> / <kbd>Ctrl</kbd>+<kbd>Enter</kbd> |
| Move those lines to the herdr pane: send them, then remove them from the file | <kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>M</kbd> / <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Enter</kbd> |

<kbd>Ctrl</kbd>+<kbd>Enter</kbd> and <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Enter</kbd> need a terminal with the kitty keyboard protocol; the <kbd>Alt</kbd>+<kbd>Shift</kbd> keys work everywhere.

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

`Shift+Enter`, `Ctrl+Shift+W`, `Ctrl+Shift+A`, `Ctrl+Shift+Z` and `Ctrl+Shift+Backspace` need the kitty keyboard protocol all the way to `ee`, because classic terminals, and herdr when your terminal speaks the classic protocol, send them as plain `Enter`, `Ctrl+W`, `Ctrl+A` (select all), `Ctrl+Z` (undo) and `Ctrl+Backspace` (delete word). Their fallbacks work everywhere: `Alt+Shift+W` shrinks the selection, `Alt+Shift+A` opens the command palette and `Alt+Shift+Z` redoes. `Ctrl+/` arrives as `Ctrl+7` in classic terminals, and both work. `Ctrl+Space` (code completion) isn't bound. `Ctrl+Enter` and `Ctrl+Shift+Enter` (send and move to a herdr pane) and ``Ctrl+` `` (switch theme) also need the kitty keyboard protocol; ``Alt+` `` works everywhere.

</details>

<img src="assets/readme/divider.svg" width="100%" alt="">

## Config

`ee` reads `~/.config/eoe/config.toml` (or `$XDG_CONFIG_HOME/eoe/config.toml`) at startup. The file is optional.

```toml
# Let Alt+letter do what Ctrl+letter does. Default: true.
alt_fallback = true

# "omarchy" (the default) follows your Omarchy theme's colours, "neon" keeps ee's own
# palette, and a theme name such as "tokyo-night" pins that Omarchy theme. Alt+` sets it for you.
theme = "omarchy"

# Give an action an extra key. The default keys keep working.
[keys]
save_all = "ctrl+g"
find = "f2"
beginning_of_file = "ctrl+home"   # works in terminals with the kitty keyboard protocol
```

Keys are written like `ctrl+s`, `alt+shift+f`, `shift+f6`, `f3`, `ctrl+enter` or `pageup`. The modifiers are `ctrl`, `alt` and `shift`. The named keys are `enter`, `esc`, `tab`, `backspace`, `delete`, `up`, `down`, `left`, `right`, `home`, `end`, `pageup`, `pagedown`, `space` and `f1` to `f12`.

With `theme = "omarchy"`, `ee` reads the current theme's `colors.toml` (in `~/.local/state/omarchy/current/theme/`) and maps its accent, colours, foreground and background onto its own roles. Switch themes and `ee` recolours the next time you come back to it. Without Omarchy, or with `theme = "neon"`, you get the neon palette. A theme name uses that theme's colours whatever Omarchy is set to; `ee` looks for it in `~/.config/omarchy/themes/` first, then in Omarchy's own themes. The theme picker (<kbd>Ctrl</kbd>+<kbd>`</kbd>) writes this one line and leaves the rest of the file alone.

If the file has a mistake, `ee` starts with the defaults and the status bar says what it couldn't read, for example ``config: unknown action `teleport` (using defaults)``.

<details>
<summary><b>Action names</b></summary>

`quit` `backspace` `newline` `left` `right` `up` `down` `home` `end` `beginning_of_file` `end_of_file` `beginning_of_line` `end_of_line` `delete_word` `delete_char` `duplicate_line` `delete_line` `move_line_up` `move_line_down` `add_caret_up` `add_caret_down` `delete_word_forward` `delete_word_backward` `undo` `redo` `copy` `paste` `cut` `save_all` `find` `find_in_files` `find_next` `find_previous` `replace` `select_all_occurrences` `select_left` `select_right` `select_up` `select_down` `page_up` `page_down` `select_page_up` `select_page_down` `select_home` `select_end` `send_to_pane` `move_to_pane` `clear_extra_carets` `join_lines` `reformat` `rename` `open_file` `next_tab` `previous_tab` `close_tab` `start_new_line` `word_left` `word_right` `select_word_left` `select_word_right` `select_to_file_start` `select_to_file_end` `extend_selection` `shrink_selection` `indent` `unindent` `go_to_line` `navigate_back` `navigate_forward` `select_all` `toggle_case` `toggle_comment` `find_action` `show_help` `file_structure` `switch_theme` `last_edit_location` `start_new_line_above`

</details>

| Environment variable | What it does |
|---|---|
| `EOE_NO_FX=1` | Turns off every animation, including the boot sequence |
| `EOE_LEGACY_KEYS=1` | Uses the classic key encoding instead of the kitty keyboard protocol |
| `EOE_NO_UPDATE_CHECK=1` | Skips the background check for a newer version |

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
    doc --> syntax[syntax.rs<br>tree-sitter]
    state --> find[find.rs]
    state --> clip[clipboard.rs]
    state --> herdr[herdr.rs<br>socket API]
    state -- Cue --> motion[motion.rs<br>tachyonfx]
    main --> ui[ui.rs<br>render]
    ui --> md[markdown.rs<br>live preview]
    ui --> docx[docx.rs<br>Word round-trip]
    ui --> theme[theme.rs<br>palette]
    motion --> ui
```

| File | What lives there |
|---|---|
| `src/main.rs` | Terminal setup, the event loop, mouse and paste handling |
| `src/state.rs` | `EditorState`: tabs, modes (find/replace bar, prompts), find/replace, clipboard actions, open/rename/save |
| `src/document.rs` | `Document`: lines, selection, the active caret and extra carets, occurrences, edits, undo/redo |
| `src/syntax.rs` | Tree-sitter highlighting for JSON, TOML, YAML, JavaScript/JSX, TypeScript/TSX, Python, CSS, HTML, Rust, Bash, Java and Go |
| `src/keys.rs` | The `Action` enum, the keymap, and the find-bar keys |
| `src/config.rs` | Loads `config.toml`: key remaps and the `Alt` fallback |
| `src/ui.rs` | Rendering: tab bar, editor, carets and highlights, find bar, prompt, status bar |
| `src/markdown.rs` | Markdown live preview: one rendered row per source line, with display-to-source column mapping |
| `src/docx.rs` | Word documents: one paragraph per line, with the original XML copied back except where you edited |
| `src/motion.rs` | Animations: editor events (`Cue`s) mapped to tachyonfx effects, plus the boot splash |
| `src/theme.rs` | The palette (neon, or mapped from the Omarchy theme) and shared styles |
| `src/find.rs` | Text matching, with or without case |
| `src/clipboard.rs` | System clipboard through wl-copy, xclip or xsel, with the OSC 52 fallback |
| `src/herdr.rs` | Sends lines to another herdr pane over herdr's socket API |
| `src/update.rs` | The background update check, `ee --update` and `ee --version` |

Every animation in this README is a real `ee` session: the renderer drew each frame, and the frames were encoded as SVG with an embedded JetBrains Mono subset.

```sh
cargo test                 # the unit tests
cargo run -- notes.md      # the debug build
EOE_NO_FX=1 cargo run      # the debug build, no animations
python3 tools/ptydrive.py  # type and save through a real pty, classic and kitty keys
```

`TODO.md` has the implementation plan and task-by-task notes.

<img src="assets/readme/divider.svg" width="100%" alt="">

## FAQ

> [!IMPORTANT]
> **Do you want to add shortcut keys that I like?**<br>
> Probably not.

> [!NOTE]
> **Will you include a plugin system?**<br>
> Nope. Fork and edit the codebase and extend it however you like.

> [!WARNING]
> **Your design choices are terrible.**<br>
> Yeah, probably. Take 5 minutes with an LLM and make it your own.

> [!CAUTION]
> **It's too cyberpunk, and it has too many useless animations.**<br>
> Yup.

> [!TIP]
> **Do you know that Easy Editor has dibs on the `ee` command?**<br>
> Ya. Did you read the name of the editor? Eric's Own Editor, I can do what I want.

<div align="center">
<br>

<img src="assets/readme/divider.svg" width="60%" alt="">

<sub><code>connection closed by remote host</code></sub>

</div>
