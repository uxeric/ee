<div align="center">

<img src="assets/readme/hero.svg" width="100%" alt="ee opens and jacks in behind a neon scan; a markdown file renders live, a task typed on the caret line turns into a checkbox as the caret leaves it, and the file-structure popup jumps to the Crew heading and back.">

# Eric's Own Editor

**A keyboard-first plain-text editor that boots like a movie hack.**<br>
The project is `eoe`, the command is `ee`. Type it, jack in, edit text in neon.

<a href="#jack-in"><img alt="built with Rust" src="https://img.shields.io/badge/built_with-Rust-ff2a6d?style=for-the-badge&logo=rust&logoColor=white&labelColor=0b0620"></a>
<a href="https://ratatui.rs"><img alt="ratatui 0.30" src="https://img.shields.io/badge/ratatui-0.30-29f0ff?style=for-the-badge&labelColor=0b0620"></a>
<a href="https://github.com/ratatui/tachyonfx"><img alt="tachyonfx 0.25" src="https://img.shields.io/badge/tachyonfx-0.25-ffb627?style=for-the-badge&labelColor=0b0620"></a>
<a href="https://omarchy.org"><img alt="made for Omarchy 4" src="https://img.shields.io/badge/made_for-Omarchy_4-d6dcf5?style=for-the-badge&labelColor=0b0620"></a>
<img alt="herdr ready" src="https://img.shields.io/badge/herdr-ready-ff2a6d?style=for-the-badge&labelColor=0b0620">

<kbd>[jack in](#jack-in)</kbd>&nbsp;
<kbd>[keys](#keys)</kbd>&nbsp;
<kbd>[faq](#faq)</kbd>

</div>

```diff
+ [ ok ] mounting buffers
+ [ ok ] calibrating neon
+ [ ok ] ignoring your opinions
- [ -- ] plugin system not found (by design)
```

`ee` is a plain-text editor for the terminal, built for [Omarchy](https://omarchy.org) (or whatever sub-optimal unix-like system you're running) and for people with WebStorm in their fingers. It does multi-cursor editing, renders markdown live while you write it, edits Word documents the same way, sends lines to the agent or shell next to it in [herdr](https://herdr.dev), and animates every move. It takes its colours from your Omarchy theme and follows it when you switch.

<img src="assets/readme/divider.svg" width="100%" alt="">

## Jack in

```sh
curl -fsSL https://raw.githubusercontent.com/uxeric/ee/HEAD/install.sh | bash
```

It builds `ee` from source into `~/.local/bin/ee`, no root needed, and offers to install Rust if you don't have it. On Omarchy it also adds `ee` to the app launcher.

`ee` checks for updates in the background and offers to restart into a new version; `ee --update` updates by hand and `ee --version` shows what you have. You need a terminal that passes `Ctrl` and `Alt` through: Alacritty, kitty, Ghostty, foot and WezTerm all do.

## Keys

Terminals can't tell `Ctrl+X` from `Ctrl+Shift+X`, so WebStorm's `Ctrl+Shift+letter` actions live on the plain `Ctrl+letter` key. Where that key was taken, the action moved to `Alt+Shift+letter`. Most `Ctrl` keys also answer to `Alt`, which you can turn off in `~/.config/eoe/config.toml`.

### Cheat sheet

| Action | Keys | Action | Keys |
|---|---|---|---|
| Save all | <kbd>Ctrl</kbd>+<kbd>S</kbd> | Find | <kbd>Ctrl</kbd>+<kbd>F</kbd> |
| Undo / redo | <kbd>Ctrl</kbd>+<kbd>Z</kbd> / <kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>Z</kbd> | Find in all tabs | <kbd>Shift</kbd> <kbd>Shift</kbd> |
| Copy / cut / paste | <kbd>Ctrl</kbd>+<kbd>C</kbd> <kbd>X</kbd> <kbd>V</kbd> | Replace | <kbd>Ctrl</kbd>+<kbd>R</kbd> |
| Add caret below | <kbd>Ctrl</kbd>+<kbd>↓</kbd> | Next / previous tab | <kbd>Ctrl</kbd>+<kbd>]</kbd> / <kbd>Ctrl</kbd>+<kbd>[</kbd> |
| Duplicate line | <kbd>Ctrl</kbd>+<kbd>D</kbd> | Open file | <kbd>Ctrl</kbd>+<kbd>N</kbd> |
| Extend selection | <kbd>Ctrl</kbd>+<kbd>W</kbd> | Move by word | <kbd>Ctrl</kbd>+<kbd>←</kbd> <kbd>→</kbd> |
| Command palette | <kbd>F1</kbd> / <kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>A</kbd> | File structure | <kbd>Ctrl</kbd>+<kbd>F12</kbd> |
| Select all | <kbd>Ctrl</kbd>+<kbd>A</kbd> | Toggle comment | <kbd>Ctrl</kbd>+<kbd>/</kbd> |
| Go to line | <kbd>Ctrl</kbd>+<kbd>G</kbd> | Back / forward | <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>←</kbd> <kbd>→</kbd> |
| Indent / unindent | <kbd>Tab</kbd> / <kbd>Shift</kbd>+<kbd>Tab</kbd> | Start new line | <kbd>Shift</kbd>+<kbd>Enter</kbd> |
| Send to herdr | <kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>E</kbd> | Switch theme | <kbd>Alt</kbd>+<kbd>`</kbd> |
| Quit | <kbd>Ctrl</kbd>+<kbd>Q</kbd> | Click the status bar | **F1 command** |

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
| Command palette: every action, with its keys | <kbd>F1</kbd> / <kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>A</kbd> (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>A</kbd> with the kitty keyboard protocol), or click **F1 command** at the bottom right |
| Switch theme: Follow Omarchy, Neon, or any installed Omarchy theme, previewed as you move | <kbd>Alt</kbd>+<kbd>`</kbd> (<kbd>Ctrl</kbd>+<kbd>`</kbd> with the kitty keyboard protocol) |
| Last edit location | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Backspace</kbd> |
| Rename the file, or save an untitled tab under a name | <kbd>Shift</kbd>+<kbd>F6</kbd> |
| Next tab | <kbd>Ctrl</kbd>+<kbd>]</kbd> / <kbd>Alt</kbd>+<kbd>→</kbd> / <kbd>Ctrl</kbd>+<kbd>PageDown</kbd> |
| Previous tab | <kbd>Ctrl</kbd>+<kbd>[</kbd> / <kbd>Alt</kbd>+<kbd>←</kbd> / <kbd>Ctrl</kbd>+<kbd>PageUp</kbd> |
| Close tab | <kbd>Ctrl</kbd>+<kbd>F4</kbd> |
| Go to line (`42`, or `42:7` for a column) | <kbd>Ctrl</kbd>+<kbd>G</kbd> / <kbd>Alt</kbd>+<kbd>G</kbd> |
| Back / forward through your jumps, across tabs | <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>←</kbd> / <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>→</kbd> |

</details>

<details>
<summary><b>Send to herdr</b></summary>

| Action | Key |
|---|---|
| Send the selection, or the caret's line when nothing is selected (every caret's line with multi-cursor), to the other herdr pane, without pressing Enter | <kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>E</kbd> / <kbd>Ctrl</kbd>+<kbd>Enter</kbd> |
| Move that text to the herdr pane: send it, then remove it from the file | <kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>M</kbd> / <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Enter</kbd> |
| Right-click the text | Copy, Paste, Copy to herdr, Move to herdr |

A right-click inside a selection keeps it, so Copy copies that selection. A right-click anywhere else puts the caret on that line first. Copy and Paste are the clipboard. The herdr rows send the selection when there is one, and the caret's whole lines otherwise, the same as the keys above. Up and down move through the menu, Enter picks, and Esc closes it. Inside herdr, a right-click in the text opens this menu; a right-click on the pane frame still opens herdr's own menu.

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

<img src="assets/readme/divider.svg" width="100%" alt="">

## FAQ

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
> Ya. Did you read the name of the editor? Eric's Own Editor, I can do what I want, but then so can you.

<div align="center">
<br>

<img src="assets/readme/divider.svg" width="60%" alt="">

<sub><code>connection closed by remote host</code></sub>

</div>
