use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, ModifierKeyCode};

use crate::config::Config;
use crate::state::Mode;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    Quit,
    InsertChar(char),
    Backspace,
    Newline,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    BeginningOfFile,
    EndOfFile,
    BeginningOfLine,
    EndOfLine,
    DeleteWord,
    DeleteChar,
    DuplicateLine,
    DeleteLine,
    MoveLineUp,
    MoveLineDown,
    AddCaretUp,
    AddCaretDown,
    DeleteWordForward,
    DeleteWordBackward,
    Undo,
    Redo,
    Copy,
    Paste,
    Cut,
    SaveAll,
    Find,
    FindInFiles,
    FindNext,
    FindPrevious,
    ToggleMatchCase,
    SwitchField,
    ReplaceOne,
    ReplaceAll,
    Replace,
    SelectAllOccurrences,
    SelectLeft,
    SelectRight,
    SelectUp,
    SelectDown,
    SelectHome,
    SelectEnd,
    InsertText(String),
    SendToPane,
    StartNewLine,
    WordLeft,
    WordRight,
    SelectWordLeft,
    SelectWordRight,
    SelectFileStart,
    SelectFileEnd,
    ExtendSelection,
    ShrinkSelection,
    Indent,
    Unindent,
    GoToLine,
    NavigateBack,
    NavigateForward,
    SelectAll,
    ToggleCase,
    ToggleComment,
    FindAction,
    FileStructure,
    LastEditLocation,
    StartNewLineAbove,
    ClickAt(usize, usize, u8),
    DragTo(usize, usize),
    ShiftClickAt(usize, usize),
    FollowLink(String),
    AddCaretAt(usize, usize),
    ClearExtraCaret,
    JoinLines,
    Reformat,
    Rename,
    OpenFile,
    NextTab,
    PreviousTab,
    CloseTab,
    GoToDeclaration,
    QuickDefinition,
}

const DOUBLE_SHIFT_WINDOW: Duration = Duration::from_millis(400);

#[derive(Default)]
pub struct DoubleShift {
    pressed: bool,
    released_at: Option<Instant>,
}

impl DoubleShift {
    pub fn feed(&mut self, event: &KeyEvent, now: Instant) -> bool {
        let is_shift = matches!(
            event.code,
            KeyCode::Modifier(ModifierKeyCode::LeftShift | ModifierKeyCode::RightShift)
        );
        if !is_shift {
            *self = Self::default();
            return false;
        }
        match event.kind {
            KeyEventKind::Press => {
                if self.released_at.is_some_and(|t| now.duration_since(t) <= DOUBLE_SHIFT_WINDOW) {
                    *self = Self::default();
                    return true;
                }
                self.pressed = true;
                self.released_at = None;
            }
            KeyEventKind::Release if self.pressed => {
                self.pressed = false;
                self.released_at = Some(now);
            }
            _ => {}
        }
        false
    }
}

pub const PALETTE: &[(&str, &str, &str)] = &[
    ("save_all", "Save all", "Ctrl+S"),
    ("open_file", "Open or create a file", "Ctrl+N"),
    ("rename", "Rename file / save as", "Shift+F6"),
    ("close_tab", "Close tab", "Ctrl+F4"),
    ("next_tab", "Next tab", "Ctrl+]"),
    ("previous_tab", "Previous tab", "Ctrl+["),
    ("find", "Find", "Ctrl+F"),
    ("find_in_files", "Find in all open files", "Shift Shift"),
    ("replace", "Replace", "Ctrl+R"),
    ("find_next", "Next match", "F3"),
    ("find_previous", "Previous match", "Shift+F3"),
    ("go_to_line", "Go to line", "Ctrl+G"),
    ("file_structure", "File structure (headings)", "Ctrl+F12"),
    ("navigate_back", "Back", "Ctrl+Alt+Left"),
    ("navigate_forward", "Forward", "Ctrl+Alt+Right"),
    ("last_edit_location", "Last edit location", "Ctrl+Shift+Backspace"),
    ("select_all", "Select all", "Ctrl+A"),
    ("extend_selection", "Extend selection", "Ctrl+W"),
    ("shrink_selection", "Shrink selection", "Ctrl+Shift+W"),
    ("select_all_occurrences", "Select all occurrences", "Alt+Shift+J"),
    ("add_caret_up", "Add caret above", "Ctrl+Up"),
    ("add_caret_down", "Add caret below", "Ctrl+Down"),
    ("clear_extra_carets", "Back to one caret", "Esc"),
    ("copy", "Copy", "Ctrl+C"),
    ("cut", "Cut", "Ctrl+X"),
    ("paste", "Paste", "Ctrl+V"),
    ("undo", "Undo", "Ctrl+Z"),
    ("redo", "Redo", "Alt+Y"),
    ("duplicate_line", "Duplicate line", "Ctrl+D"),
    ("delete_line", "Delete line", "Ctrl+Y"),
    ("move_line_up", "Move line up", "Alt+Shift+Up"),
    ("move_line_down", "Move line down", "Alt+Shift+Down"),
    ("join_lines", "Join lines", "Ctrl+J"),
    ("reformat", "Reformat", "Ctrl+Alt+L"),
    ("toggle_case", "Toggle case", "Ctrl+Shift+U"),
    ("toggle_comment", "Toggle line comment", "Ctrl+/"),
    ("indent", "Indent", "Tab"),
    ("unindent", "Unindent", "Shift+Tab"),
    ("start_new_line", "Start new line below", "Shift+Enter"),
    ("start_new_line_above", "Start new line above", "Ctrl+Alt+Enter"),
    ("word_left", "Previous word", "Ctrl+Left"),
    ("word_right", "Next word", "Ctrl+Right"),
    ("beginning_of_file", "Start of file", "Ctrl+Home"),
    ("end_of_file", "End of file", "Ctrl+End"),
    ("send_to_pane", "Send lines to herdr", "Alt+Shift+E"),
    ("quit", "Quit", "Ctrl+Q"),
];

impl Action {
    pub fn from_name(name: &str) -> Option<Action> {
        Some(match name {
            "quit" => Action::Quit,
            "backspace" => Action::Backspace,
            "newline" => Action::Newline,
            "left" => Action::Left,
            "right" => Action::Right,
            "up" => Action::Up,
            "down" => Action::Down,
            "home" => Action::Home,
            "end" => Action::End,
            "beginning_of_file" => Action::BeginningOfFile,
            "end_of_file" => Action::EndOfFile,
            "beginning_of_line" => Action::BeginningOfLine,
            "end_of_line" => Action::EndOfLine,
            "delete_word" => Action::DeleteWord,
            "delete_char" => Action::DeleteChar,
            "duplicate_line" => Action::DuplicateLine,
            "delete_line" => Action::DeleteLine,
            "move_line_up" => Action::MoveLineUp,
            "move_line_down" => Action::MoveLineDown,
            "add_caret_up" => Action::AddCaretUp,
            "add_caret_down" => Action::AddCaretDown,
            "delete_word_forward" => Action::DeleteWordForward,
            "delete_word_backward" => Action::DeleteWordBackward,
            "undo" => Action::Undo,
            "redo" => Action::Redo,
            "copy" => Action::Copy,
            "paste" => Action::Paste,
            "cut" => Action::Cut,
            "save_all" => Action::SaveAll,
            "find" => Action::Find,
            "find_in_files" => Action::FindInFiles,
            "find_next" => Action::FindNext,
            "find_previous" => Action::FindPrevious,
            "replace" => Action::Replace,
            "select_all_occurrences" => Action::SelectAllOccurrences,
            "select_left" => Action::SelectLeft,
            "select_right" => Action::SelectRight,
            "select_up" => Action::SelectUp,
            "select_down" => Action::SelectDown,
            "select_home" => Action::SelectHome,
            "select_end" => Action::SelectEnd,
            "send_to_pane" => Action::SendToPane,
            "clear_extra_carets" => Action::ClearExtraCaret,
            "join_lines" => Action::JoinLines,
            "reformat" => Action::Reformat,
            "rename" => Action::Rename,
            "open_file" => Action::OpenFile,
            "next_tab" => Action::NextTab,
            "previous_tab" => Action::PreviousTab,
            "close_tab" => Action::CloseTab,
            "start_new_line" => Action::StartNewLine,
            "word_left" => Action::WordLeft,
            "word_right" => Action::WordRight,
            "select_word_left" => Action::SelectWordLeft,
            "select_word_right" => Action::SelectWordRight,
            "select_to_file_start" => Action::SelectFileStart,
            "select_to_file_end" => Action::SelectFileEnd,
            "extend_selection" => Action::ExtendSelection,
            "shrink_selection" => Action::ShrinkSelection,
            "indent" => Action::Indent,
            "unindent" => Action::Unindent,
            "go_to_line" => Action::GoToLine,
            "navigate_back" => Action::NavigateBack,
            "navigate_forward" => Action::NavigateForward,
            "select_all" => Action::SelectAll,
            "toggle_case" => Action::ToggleCase,
            "toggle_comment" => Action::ToggleComment,
            "find_action" => Action::FindAction,
            "file_structure" => Action::FileStructure,
            "last_edit_location" => Action::LastEditLocation,
            "start_new_line_above" => Action::StartNewLineAbove,
            _ => return None,
        })
    }
}

pub struct Keymap {
    config: Config,
}

impl Keymap {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// Mapping for a plain `Ctrl+letter`. A terminal cannot distinguish
    /// `Ctrl+key` from `Ctrl+Shift+key`, so the former "Ctrl+Shift" actions
    /// live here. Conflicts (f/d/r) keep their primary (non-shift) action.
    fn ctrl_char_action(c: char) -> Option<Action> {
        match c {
            'a' => Some(Action::SelectAll),
            'A' => Some(Action::FindAction),
            'b' => Some(Action::GoToDeclaration),
            'c' => Some(Action::Copy),
            'd' => Some(Action::DuplicateLine),
            'e' => Some(Action::EndOfLine),
            'f' => Some(Action::Find),
            'g' => Some(Action::GoToLine),
            'h' => Some(Action::Backspace),
            'i' => Some(Action::QuickDefinition),
            'j' => Some(Action::JoinLines),
            'k' => Some(Action::Reformat),
            'n' | 'N' => Some(Action::OpenFile),
            'q' => Some(Action::Quit),
            'r' => Some(Action::Replace),
            's' => Some(Action::SaveAll),
            'v' => Some(Action::Paste),
            'u' | 'U' => Some(Action::ToggleCase),
            '/' => Some(Action::ToggleComment),
            'w' => Some(Action::ExtendSelection),
            'W' => Some(Action::ShrinkSelection),
            'x' => Some(Action::Cut),
            'y' => Some(Action::DeleteLine),
            'z' => Some(Action::Undo),
            '[' => Some(Action::PreviousTab),
            ']' | '5' => Some(Action::NextTab),
            _ => None,
        }
    }

    pub fn dispatch_in(&self, mode: Mode, event: &KeyEvent) -> Option<Action> {
        if matches!(mode, Mode::Find | Mode::Replace) {
            if let Some(action) = Keymap::find_bar_action(event) {
                return Some(action);
            }
        }
        self.dispatch(event)
    }

    fn find_bar_action(event: &KeyEvent) -> Option<Action> {
        let ctrl = event.modifiers.contains(KeyModifiers::CONTROL);
        let alt = event.modifiers.contains(KeyModifiers::ALT);
        let shift = event.modifiers.contains(KeyModifiers::SHIFT);
        match event.code {
            KeyCode::Up if !ctrl && !alt => Some(Action::FindPrevious),
            KeyCode::Down if !ctrl && !alt => Some(Action::FindNext),
            KeyCode::Enter if shift && !ctrl && !alt => Some(Action::FindPrevious),
            KeyCode::Tab | KeyCode::BackTab => Some(Action::SwitchField),
            KeyCode::Char(c) if alt && !ctrl => match c.to_ascii_lowercase() {
                'c' => Some(Action::ToggleMatchCase),
                'a' => Some(Action::ReplaceAll),
                'p' => Some(Action::ReplaceOne),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn dispatch(&self, event: &KeyEvent) -> Option<Action> {
        for (action, binding) in &self.config.remaps {
            if binding.matches(event) {
                return Some(action.clone());
            }
        }

        let code = event.code;
        let ctrl = event.modifiers.contains(KeyModifiers::CONTROL);
        let alt = event.modifiers.contains(KeyModifiers::ALT);
        let shift = event.modifiers.contains(KeyModifiers::SHIFT);
        let select = shift && !ctrl && !alt;

        match code {
            KeyCode::Char(c) => {
                if ctrl && alt && c == 'l' {
                    Some(Action::Reformat)
                } else if ctrl && c == '7' {
                    Some(Action::ToggleComment)
                } else if ctrl {
                    Keymap::ctrl_char_action(c)
                } else if alt && c == 'A' {
                    Some(Action::FindAction)
                } else if alt && c == 'F' {
                    Some(Action::FindInFiles)
                } else if alt && c == 'J' {
                    Some(Action::SelectAllOccurrences)
                } else if alt && c == 'E' {
                    Some(Action::SendToPane)
                } else if alt && c == 'W' {
                    Some(Action::ShrinkSelection)
                } else if alt {
                    if self.config.fallback_enabled {
                        // Alt+Y is the "Ctrl+Shift+Y" proxy; redo is no longer
                        // on Ctrl+Y (now DeleteLine), so keep it here.
                        if c == 'y' {
                            Some(Action::Redo)
                        } else {
                            Keymap::ctrl_char_action(c)
                        }
                    } else {
                        None
                    }
                } else {
                    Some(Action::InsertChar(c))
                }
            }
            KeyCode::Backspace => {
                if ctrl && shift {
                    Some(Action::LastEditLocation)
                } else if ctrl {
                    Some(Action::DeleteWordBackward)
                } else if alt {
                    None
                } else {
                    Some(Action::Backspace)
                }
            },
            KeyCode::Delete => {
                if ctrl {
                    Some(Action::DeleteWordForward)
                } else {
                    Some(Action::DeleteChar)
                }
            },
            KeyCode::Enter => {
                if alt {
                    Some(Action::StartNewLineAbove)
                } else if ctrl {
                    Some(Action::SendToPane)
                } else if shift {
                    Some(Action::StartNewLine)
                } else {
                    Some(Action::Newline)
                }
            }
            KeyCode::Left if ctrl && alt => Some(Action::NavigateBack),
            KeyCode::Right if ctrl && alt => Some(Action::NavigateForward),
            KeyCode::Tab if !ctrl && !alt => Some(Action::Indent),
            KeyCode::BackTab => Some(Action::Unindent),
            KeyCode::Left => {
                if ctrl && !alt {
                    if shift {
                        Some(Action::SelectWordLeft)
                    } else {
                        Some(Action::WordLeft)
                    }
                } else if alt {
                    Some(Action::PreviousTab)
                } else if select {
                    Some(Action::SelectLeft)
                } else {
                    Some(Action::Left)
                }
            }
            KeyCode::Right => {
                if ctrl && !alt {
                    if shift {
                        Some(Action::SelectWordRight)
                    } else {
                        Some(Action::WordRight)
                    }
                } else if alt {
                    Some(Action::NextTab)
                } else if select {
                    Some(Action::SelectRight)
                } else {
                    Some(Action::Right)
                }
            }
            KeyCode::PageUp if ctrl => Some(Action::PreviousTab),
            KeyCode::PageDown if ctrl => Some(Action::NextTab),
            KeyCode::F(4) if ctrl => Some(Action::CloseTab),
            KeyCode::F(12) if ctrl => Some(Action::FileStructure),
            KeyCode::F(3) if !ctrl && !alt => {
                if shift {
                    Some(Action::FindPrevious)
                } else {
                    Some(Action::FindNext)
                }
            }
            KeyCode::F(6) if select => Some(Action::Rename),
            KeyCode::Up => {
                if ctrl {
                    Some(Action::AddCaretUp)
                } else if alt {
                    Some(Action::MoveLineUp)
                } else if select {
                    Some(Action::SelectUp)
                } else {
                    Some(Action::Up)
                }
            },
            KeyCode::Down => {
                if ctrl {
                    Some(Action::AddCaretDown)
                } else if alt {
                    Some(Action::MoveLineDown)
                } else if select {
                    Some(Action::SelectDown)
                } else {
                    Some(Action::Down)
                }
            },
            KeyCode::Home => {
                if ctrl && !alt {
                    if shift {
                        Some(Action::SelectFileStart)
                    } else {
                        Some(Action::BeginningOfFile)
                    }
                } else if alt {
                    None
                } else if select {
                    Some(Action::SelectHome)
                } else {
                    Some(Action::Home)
                }
            }
            KeyCode::End => {
                if ctrl && !alt {
                    if shift {
                        Some(Action::SelectFileEnd)
                    } else {
                        Some(Action::EndOfFile)
                    }
                } else if alt {
                    None
                } else if select {
                    Some(Action::SelectEnd)
                } else {
                    Some(Action::End)
                }
            }
            KeyCode::Esc => Some(Action::ClearExtraCaret),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(code: KeyCode, ctrl: bool, alt: bool, shift: bool) -> KeyEvent {
        let mut modifiers = KeyModifiers::NONE;
        if ctrl {
            modifiers |= KeyModifiers::CONTROL;
        }
        if alt {
            modifiers |= KeyModifiers::ALT;
        }
        if shift {
            modifiers |= KeyModifiers::SHIFT;
        }
        KeyEvent::new(code, modifiers)
    }

    fn keymap() -> Keymap {
        Keymap::new(Config::default())
    }

    #[test]
    fn letter_bindings_on_ctrl_and_alt() {
        let km = keymap();
        let cases: &[(char, Action)] = &[
            ('a', Action::SelectAll),
            ('b', Action::GoToDeclaration),
            ('c', Action::Copy),
            ('d', Action::DuplicateLine),
            ('e', Action::EndOfLine),
            ('f', Action::Find),
            ('h', Action::Backspace),
            ('i', Action::QuickDefinition),
            ('j', Action::JoinLines),
            ('k', Action::Reformat),
            ('n', Action::OpenFile),
            ('q', Action::Quit),
            ('r', Action::Replace),
            ('s', Action::SaveAll),
            ('v', Action::Paste),
            ('w', Action::ExtendSelection),
            ('x', Action::Cut),
            ('z', Action::Undo),
            ('u', Action::ToggleCase),
            ('/', Action::ToggleComment),
        ];
        for (c, expected) in cases {
            let code = KeyCode::Char(*c);
            assert_eq!(km.dispatch(&event(code, true, false, false)).as_ref(), Some(expected), "Ctrl+{}", c);
            assert_eq!(km.dispatch(&event(code, false, true, false)).as_ref(), Some(expected), "Alt+{}", c);
        }
        assert_eq!(km.dispatch(&event(KeyCode::Char('y'), true, false, false)), Some(Action::DeleteLine));
        assert_eq!(km.dispatch(&event(KeyCode::Char('y'), false, true, false)), Some(Action::Redo), "Alt+Y is redo");
    }

    #[test]
    fn tab_bindings() {
        let mut config = Config::default();
        config.fallback_enabled = false;
        for km in [keymap(), Keymap::new(config)] {
            assert_eq!(km.dispatch(&event(KeyCode::Left, false, true, false)), Some(Action::PreviousTab));
            assert_eq!(km.dispatch(&event(KeyCode::Right, false, true, false)), Some(Action::NextTab));
            assert_eq!(km.dispatch(&event(KeyCode::PageUp, true, false, false)), Some(Action::PreviousTab));
            assert_eq!(km.dispatch(&event(KeyCode::PageDown, true, false, false)), Some(Action::NextTab));
            assert_eq!(km.dispatch(&event(KeyCode::F(4), true, false, false)), Some(Action::CloseTab));
            assert_eq!(km.dispatch(&event(KeyCode::F(4), false, false, false)), None);
            assert_eq!(km.dispatch(&event(KeyCode::Char('['), true, false, false)), Some(Action::PreviousTab));
            assert_eq!(km.dispatch(&event(KeyCode::Char(']'), true, false, false)), Some(Action::NextTab));
            assert_eq!(km.dispatch(&event(KeyCode::Char('5'), true, false, false)), Some(Action::NextTab), "legacy Ctrl+]");
        }
    }

    fn shift(kind: KeyEventKind) -> KeyEvent {
        KeyEvent::new_with_kind(KeyCode::Modifier(ModifierKeyCode::LeftShift), KeyModifiers::SHIFT, kind)
    }

    #[test]
    fn double_shift_needs_two_quick_clean_taps() {
        let t0 = Instant::now();
        let ms = |n: u64| t0 + Duration::from_millis(n);
        let tap_tap = |gap: u64, between: Option<KeyEvent>| {
            let mut d = DoubleShift::default();
            let mut fired = d.feed(&shift(KeyEventKind::Press), ms(0));
            fired |= d.feed(&shift(KeyEventKind::Release), ms(80));
            if let Some(ev) = between {
                fired |= d.feed(&ev, ms(100));
            }
            fired |= d.feed(&shift(KeyEventKind::Repeat), ms(90));
            fired | d.feed(&shift(KeyEventKind::Press), ms(80 + gap))
        };
        assert!(tap_tap(200, None));
        assert!(!tap_tap(500, None), "too slow");
        assert!(!tap_tap(200, Some(event(KeyCode::Char('A'), false, false, true))), "another key in between");

        let mut d = DoubleShift::default();
        d.feed(&shift(KeyEventKind::Press), ms(0));
        d.feed(&event(KeyCode::Char('A'), false, false, true), ms(50));
        d.feed(&shift(KeyEventKind::Release), ms(100));
        assert!(!d.feed(&shift(KeyEventKind::Press), ms(200)), "shift used to type a capital is not a tap");
    }

    #[test]
    fn arrow_delete_and_escape_bindings() {
        let km = keymap();
        assert_eq!(km.dispatch(&event(KeyCode::Up, true, false, false)), Some(Action::AddCaretUp));
        assert_eq!(km.dispatch(&event(KeyCode::Down, true, false, false)), Some(Action::AddCaretDown));
        assert_eq!(km.dispatch(&event(KeyCode::Up, false, true, false)), Some(Action::MoveLineUp));
        assert_eq!(km.dispatch(&event(KeyCode::Down, false, true, false)), Some(Action::MoveLineDown));
        assert_eq!(km.dispatch(&event(KeyCode::Backspace, true, false, false)), Some(Action::DeleteWordBackward));
        assert_eq!(km.dispatch(&event(KeyCode::Delete, true, false, false)), Some(Action::DeleteWordForward));
        assert_eq!(km.dispatch(&event(KeyCode::Delete, false, false, false)), Some(Action::DeleteChar));
        assert_eq!(km.dispatch(&event(KeyCode::Esc, false, false, false)), Some(Action::ClearExtraCaret));
    }

    #[test]
    fn plain_chars_insert() {
        let km = keymap();
        assert_eq!(km.dispatch(&event(KeyCode::Char('a'), false, false, false)), Some(Action::InsertChar('a')));
        assert_eq!(km.dispatch(&event(KeyCode::Char(' '), false, false, false)), Some(Action::InsertChar(' ')));
        assert_eq!(km.dispatch(&event(KeyCode::Char('X'), false, false, false)), Some(Action::InsertChar('X')));
    }

    #[test]
    fn navigation_keys() {
        let km = keymap();
        assert_eq!(km.dispatch(&event(KeyCode::Left, false, false, false)), Some(Action::Left));
        assert_eq!(km.dispatch(&event(KeyCode::Right, false, false, false)), Some(Action::Right));
        assert_eq!(km.dispatch(&event(KeyCode::Up, false, false, false)), Some(Action::Up));
        assert_eq!(km.dispatch(&event(KeyCode::Down, false, false, false)), Some(Action::Down));
        assert_eq!(km.dispatch(&event(KeyCode::Home, false, false, false)), Some(Action::Home));
        assert_eq!(km.dispatch(&event(KeyCode::End, false, false, false)), Some(Action::End));
        assert_eq!(km.dispatch(&event(KeyCode::Backspace, false, false, false)), Some(Action::Backspace));
        assert_eq!(km.dispatch(&event(KeyCode::Enter, false, false, false)), Some(Action::Newline));
    }

    #[test]
    fn unmapped_keys_return_none() {
        let km = keymap();
        assert_eq!(km.dispatch(&event(KeyCode::Char(' '), true, false, false)), None);
        assert_eq!(km.dispatch(&event(KeyCode::Home, false, true, false)), None);
        assert_eq!(km.dispatch(&event(KeyCode::End, false, true, false)), None);
        assert_eq!(km.dispatch(&event(KeyCode::Char(' '), false, true, false)), None);
        assert_eq!(km.dispatch(&event(KeyCode::Char('o'), true, false, false)), None);
        assert_eq!(km.dispatch(&event(KeyCode::Char('o'), false, true, false)), None);
        assert_eq!(km.dispatch(&event(KeyCode::Char('o'), true, false, true)), None);
    }

    #[test]
    fn fallback_disabled_disables_alt_bindings() {
        let mut config = Config::default();
        config.fallback_enabled = false;
        let km = Keymap::new(config);
        assert_eq!(km.dispatch(&event(KeyCode::Char('w'), false, true, false)), None);
        assert_eq!(km.dispatch(&event(KeyCode::Char('z'), false, true, false)), None);
        assert_eq!(km.dispatch(&event(KeyCode::Char('r'), false, true, false)), None);
        assert_eq!(km.dispatch(&event(KeyCode::Char('a'), false, true, false)), None);
        assert_eq!(km.dispatch(&event(KeyCode::Char('e'), false, true, false)), None);
        // Ctrl bindings still work
        assert_eq!(km.dispatch(&event(KeyCode::Char('q'), true, false, false)), Some(Action::Quit));
        assert_eq!(km.dispatch(&event(KeyCode::Char('w'), true, false, false)), Some(Action::ExtendSelection));
    }

    #[test]
    fn selection_find_and_rename_bindings() {
        let km = keymap();
        let cases = [
            (KeyCode::Left, Action::SelectLeft),
            (KeyCode::Right, Action::SelectRight),
            (KeyCode::Up, Action::SelectUp),
            (KeyCode::Down, Action::SelectDown),
            (KeyCode::Home, Action::SelectHome),
            (KeyCode::End, Action::SelectEnd),
            (KeyCode::F(3), Action::FindPrevious),
            (KeyCode::F(6), Action::Rename),
        ];
        for (code, expected) in cases {
            assert_eq!(km.dispatch(&event(code, false, false, true)), Some(expected));
        }
        assert_eq!(km.dispatch(&event(KeyCode::F(3), false, false, false)), Some(Action::FindNext));
        assert_eq!(km.dispatch(&event(KeyCode::F(6), false, false, false)), None);
        assert_eq!(km.dispatch(&event(KeyCode::Char('F'), false, true, true)), Some(Action::FindInFiles));
        assert_eq!(km.dispatch(&event(KeyCode::Char('J'), false, true, true)), Some(Action::SelectAllOccurrences));
        assert_eq!(km.dispatch(&event(KeyCode::Char('E'), false, true, true)), Some(Action::SendToPane));
        assert_eq!(km.dispatch(&event(KeyCode::Enter, true, false, false)), Some(Action::SendToPane));
    }

    #[test]
    fn word_file_line_and_selection_keys() {
        let km = keymap();
        let cases = [
            (event(KeyCode::Left, true, false, false), Action::WordLeft),
            (event(KeyCode::Right, true, false, false), Action::WordRight),
            (event(KeyCode::Left, true, false, true), Action::SelectWordLeft),
            (event(KeyCode::Right, true, false, true), Action::SelectWordRight),
            (event(KeyCode::Home, true, false, false), Action::BeginningOfFile),
            (event(KeyCode::End, true, false, false), Action::EndOfFile),
            (event(KeyCode::Home, true, false, true), Action::SelectFileStart),
            (event(KeyCode::End, true, false, true), Action::SelectFileEnd),
            (event(KeyCode::Enter, false, false, true), Action::StartNewLine),
            (event(KeyCode::Char('W'), true, false, true), Action::ShrinkSelection),
            (event(KeyCode::Char('W'), false, true, true), Action::ShrinkSelection),
            (event(KeyCode::Tab, false, false, false), Action::Indent),
            (event(KeyCode::BackTab, false, false, true), Action::Unindent),
            (event(KeyCode::Char('g'), true, false, false), Action::GoToLine),
            (event(KeyCode::Char('g'), false, true, false), Action::GoToLine),
            (event(KeyCode::Left, true, true, false), Action::NavigateBack),
            (event(KeyCode::Right, true, true, false), Action::NavigateForward),
            (event(KeyCode::Enter, true, true, false), Action::StartNewLineAbove),
            (event(KeyCode::Enter, false, true, false), Action::StartNewLineAbove),
            (event(KeyCode::Char('7'), true, false, false), Action::ToggleComment),
            (event(KeyCode::Char('U'), true, false, true), Action::ToggleCase),
            (event(KeyCode::Char('l'), true, true, false), Action::Reformat),
            (event(KeyCode::Up, false, true, true), Action::MoveLineUp),
            (event(KeyCode::Down, false, true, true), Action::MoveLineDown),
            (event(KeyCode::Backspace, true, false, true), Action::LastEditLocation),
            (event(KeyCode::F(12), true, false, false), Action::FileStructure),
            (event(KeyCode::Char('A'), true, false, true), Action::FindAction),
            (event(KeyCode::Char('A'), false, true, true), Action::FindAction),
            (event(KeyCode::Char('N'), true, false, true), Action::OpenFile),
        ];
        for (ev, expected) in cases {
            assert_eq!(km.dispatch(&ev), Some(expected.clone()), "{:?}", ev);
        }
    }

    #[test]
    fn find_bar_keys_apply_only_in_find_modes() {
        let km = keymap();
        let bar = [
            (event(KeyCode::Up, false, false, false), Action::FindPrevious),
            (event(KeyCode::Down, false, false, false), Action::FindNext),
            (event(KeyCode::Tab, false, false, false), Action::SwitchField),
            (event(KeyCode::Char('c'), false, true, false), Action::ToggleMatchCase),
            (event(KeyCode::Char('a'), false, true, false), Action::ReplaceAll),
            (event(KeyCode::Char('p'), false, true, false), Action::ReplaceOne),
        ];
        for (ev, expected) in &bar {
            assert_eq!(km.dispatch_in(Mode::Find, ev).as_ref(), Some(expected));
            assert_eq!(km.dispatch_in(Mode::Replace, ev).as_ref(), Some(expected));
            assert_eq!(km.dispatch_in(Mode::Normal, ev), km.dispatch(ev));
        }
        let enter = event(KeyCode::Enter, false, false, false);
        assert_eq!(km.dispatch_in(Mode::Find, &enter), Some(Action::Newline));
    }
}
