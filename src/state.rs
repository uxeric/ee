use std::fs;
use std::path::Path;

use crate::clipboard::{Clipboard, CopyTarget};
use crate::document::Document;
use crate::find;
use crate::herdr::Herdr;
use crate::keys::Action;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    Rename,
    GoToLine,
}

#[derive(Clone, Copy)]
pub enum Mode {
    Normal,
    Find,
    Replace,
    Prompt(PromptKind),
    Picker,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cue {
    TabNext,
    TabPrev,
    Opened,
    FindOpened,
    Matched,
    Saved,
    Sent,
    Moved,
    Warn,
    Error,
    Quit,
    PickerOpened,
    UpdateOffered,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Alert {
    Warn,
    Error,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pending {
    Close,
    Quit,
    Overwrite,
}

pub struct FindBar {
    pub query: String,
    pub replacement: String,
    pub case_sensitive: bool,
    pub all_tabs: bool,
    pub editing_replacement: bool,
    origin: (usize, (usize, usize)),
}

pub type Hit = (usize, usize, usize, usize);

pub struct EditorState {
    pub tabs: Vec<Document>,
    pub active: usize,
    pub mode: Mode,
    pub status: String,
    pub prompt_input: String,
    pub find: FindBar,
    pub alert: Option<Alert>,
    cues: Vec<Cue>,
    clipboard: Clipboard,
    pending: Option<Pending>,
    back: Vec<(u64, (usize, usize))>,
    forward: Vec<(u64, (usize, usize))>,
    pub picker: Option<Picker>,
    last_edit: Option<(u64, (usize, usize))>,
    drag_anchor: Option<(usize, usize)>,
    pub update: Option<(String, String)>,
    pub restart_for_update: bool,
    pub help: bool,
    pub page_rows: usize,
    pub theme: crate::theme::ThemeFile,
    pub theme_roots: crate::theme::Roots,
    pub theme_choice: String,
    pub config_path: Option<std::path::PathBuf>,
    theme_before: Option<crate::theme::Palette>,
}

impl EditorState {
    pub fn new() -> Self {
        Self {
            tabs: vec![Document::new("untitled")],
            active: 0,
            mode: Mode::Normal,
            status: String::new(),
            prompt_input: String::new(),
            find: FindBar {
                query: String::new(),
                replacement: String::new(),
                case_sensitive: false,
                all_tabs: false,
                editing_replacement: false,
                origin: (0, (0, 0)),
            },
            alert: None,
            cues: Vec::new(),
            clipboard: Clipboard::new(),
            pending: None,
            back: Vec::new(),
            forward: Vec::new(),
            picker: None,
            last_edit: None,
            drag_anchor: None,
            update: None,
            restart_for_update: false,
            help: false,
            page_rows: 1,
            theme: crate::theme::ThemeFile::default(),
            theme_roots: crate::theme::Roots::default(),
            theme_choice: "omarchy".to_string(),
            config_path: None,
            theme_before: None,
        }
    }

    pub fn active_doc(&mut self) -> &mut Document {
        &mut self.tabs[self.active]
    }

    pub fn mode_str(&self) -> &'static str {
        match self.mode {
            Mode::Normal => "Edit",
            Mode::Find if self.find.all_tabs => "Find in files",
            Mode::Find => "Find",
            Mode::Replace => "Replace",
            Mode::Prompt(_) => "Prompt",
            Mode::Picker => match self.picker.as_ref().map(|p| p.kind) {
                Some(PickerKind::Actions) => "Actions",
                Some(PickerKind::Structure) => "Outline",
                _ => "Open",
            },
        }
    }

    pub fn take_cues(&mut self) -> Vec<Cue> {
        std::mem::take(&mut self.cues)
    }

    pub fn warn(&mut self, message: String) {
        self.status = message;
        self.alert = Some(Alert::Warn);
        self.cues.push(Cue::Warn);
    }

    fn fail(&mut self, message: String) {
        self.status = message;
        self.alert = Some(Alert::Error);
        self.cues.push(Cue::Error);
    }

    pub fn apply(&mut self, action: Action) -> bool {
        self.status.clear();
        self.alert = None;
        let pending = self.pending.take();
        if self.update.is_some() {
            return self.apply_update_offer(action, pending);
        }
        if self.help {
            self.help = false;
            return true;
        }
        if action == Action::ShowHelp {
            self.help = true;
            self.cues.push(Cue::PickerOpened);
            return true;
        }
        let before = (self.tabs[self.active].id, self.tabs[self.active].rev());
        let keep = match self.mode {
            Mode::Prompt(kind) => self.apply_prompt(kind, action, pending),
            Mode::Find | Mode::Replace => self.apply_find(action, pending),
            Mode::Picker => self.apply_picker(action, pending),
            Mode::Normal => self.apply_normal(action, pending),
        };
        if let Some(doc) = self.tabs.get(self.active) {
            if doc.id == before.0 && doc.rev() != before.1 {
                self.last_edit = Some((doc.id, doc.cursor));
            }
        }
        keep
    }

    fn page(&mut self, direction: isize, select: bool) {
        let rows = direction * self.page_rows.max(1) as isize;
        let doc = self.active_doc();
        doc.move_by(|d| d.move_lines(rows), select);
        doc.scroll_top.set(doc.scroll_top.get().saturating_add_signed(rows));
    }

    fn apply_normal(&mut self, action: Action, pending: Option<Pending>) -> bool {
        match action {
            Action::Quit => return self.quit(pending == Some(Pending::Quit)),
            Action::InsertChar(c) => self.active_doc().insert_char(c),
            Action::InsertText(t) => self.active_doc().insert_text(&t),
            Action::Backspace => self.active_doc().backspace(),
            Action::Newline => self.active_doc().newline(),
            Action::Left => self.active_doc().move_by(Document::move_left, false),
            Action::Right => self.active_doc().move_by(Document::move_right, false),
            Action::Up => self.active_doc().move_by(Document::move_up, false),
            Action::Down => self.active_doc().move_by(Document::move_down, false),
            Action::Home => self.active_doc().move_by(Document::home, false),
            Action::End => self.active_doc().move_by(Document::end, false),
            Action::BeginningOfFile => {
                self.mark_jump();
                self.active_doc().move_by(Document::beginning_of_file, false)
            }
            Action::EndOfFile => {
                self.mark_jump();
                self.active_doc().move_by(Document::end_of_file, false)
            }
            Action::BeginningOfLine => self.active_doc().move_by(Document::line_start, false),
            Action::EndOfLine => self.active_doc().move_by(Document::end, false),
            Action::SelectLeft => self.active_doc().move_by(Document::move_left, true),
            Action::SelectRight => self.active_doc().move_by(Document::move_right, true),
            Action::SelectUp => self.active_doc().move_by(Document::move_up, true),
            Action::SelectDown => self.active_doc().move_by(Document::move_down, true),
            Action::PageUp => self.page(-1, false),
            Action::PageDown => self.page(1, false),
            Action::SelectPageUp => self.page(-1, true),
            Action::SelectPageDown => self.page(1, true),
            Action::SelectHome => self.active_doc().move_by(Document::home, true),
            Action::SelectEnd => self.active_doc().move_by(Document::end, true),
            Action::WordLeft => self.active_doc().move_by(Document::word_left, false),
            Action::WordRight => self.active_doc().move_by(Document::word_right, false),
            Action::SelectWordLeft => self.active_doc().move_by(Document::word_left, true),
            Action::SelectWordRight => self.active_doc().move_by(Document::word_right, true),
            Action::SelectFileStart => self.active_doc().move_by(Document::beginning_of_file, true),
            Action::SelectFileEnd => self.active_doc().move_by(Document::end_of_file, true),
            Action::StartNewLine => self.active_doc().start_new_line(),
            Action::ExtendSelection => self.active_doc().extend_selection(),
            Action::ShrinkSelection => self.active_doc().shrink_selection(),
            Action::Indent => self.active_doc().indent(),
            Action::Unindent => self.active_doc().unindent(),
            Action::GoToLine => self.open_prompt(PromptKind::GoToLine, String::new()),
            Action::NavigateBack => self.navigate(true),
            Action::NavigateForward => self.navigate(false),
            Action::SelectAll => self.active_doc().select_all(),
            Action::StartNewLineAbove => self.active_doc().start_new_line_above(),
            Action::ToggleCase => {
                if !self.active_doc().toggle_case() {
                    self.status = "nothing to change case of: select text or put the caret on a word".to_string();
                }
            }
            Action::ToggleComment => match comment_syntax(&self.tabs[self.active]) {
                Some((prefix, suffix)) => self.active_doc().toggle_comment(prefix, suffix),
                None => self.warn(format!("no comment syntax for {}", self.tabs[self.active].name)),
            },
            Action::LastEditLocation => match self.last_edit {
                Some((id, (line, col))) => match self.tabs.iter().position(|d| d.id == id) {
                    Some(tab) => {
                        self.mark_jump();
                        if tab != self.active {
                            self.cues.push(Cue::TabPrev);
                        }
                        self.active = tab;
                        self.tabs[tab].go_to(line, col);
                    }
                    None => self.status = "the last edited tab is closed".to_string(),
                },
                None => self.status = "no edits yet".to_string(),
            },
            Action::FindAction => self.open_picker(PickerKind::Actions),
            Action::FileStructure => self.open_picker(PickerKind::Structure),
            Action::SwitchTheme => self.open_picker(PickerKind::Themes),
            Action::ClickAt(line, col, clicks) => {
                let doc = self.active_doc();
                doc.click_at(line, col, clicks);
                self.drag_anchor = Some(doc.selection_anchor());
            }
            Action::DragTo(line, col) => {
                if let Some(anchor) = self.drag_anchor {
                    self.active_doc().select_to(anchor, line, col);
                }
            }
            Action::ShiftClickAt(line, col) => {
                let doc = self.active_doc();
                let anchor = doc.selection_anchor();
                doc.select_to(anchor, line, col);
                self.drag_anchor = Some(anchor);
            }
            Action::DeleteWord => self.active_doc().delete_word_forward(),
            Action::DeleteChar => self.active_doc().delete_char_forward(),
            Action::DuplicateLine => self.active_doc().duplicate_line(),
            Action::DeleteLine => self.active_doc().delete_line(),
            Action::MoveLineUp => self.active_doc().move_line_up(),
            Action::MoveLineDown => self.active_doc().move_line_down(),
            Action::AddCaretUp => self.active_doc().add_caret_up(),
            Action::AddCaretDown => self.active_doc().add_caret_down(),
            Action::AddCaretAt(l, c) => self.active_doc().add_caret_at(l, c),
            Action::ClearExtraCaret => self.active_doc().clear_extra_carets(),
            Action::DeleteWordForward => self.active_doc().delete_word_forward(),
            Action::DeleteWordBackward => self.active_doc().delete_word_backward(),
            Action::Undo => self.active_doc().undo(),
            Action::Redo => self.active_doc().redo(),
            Action::SaveAll => self.save_all(),
            Action::Copy => self.copy(),
            Action::Cut => {
                self.copy();
                self.active_doc().cut();
            }
            Action::Paste => self.paste(),
            Action::JoinLines => self.active_doc().join_lines(),
            Action::Reformat => {
                self.status = if self.active_doc().reformat() {
                    "reformatted".to_string()
                } else {
                    "already formatted".to_string()
                };
            }
            Action::Find => self.open_find(Mode::Find, false),
            Action::FindInFiles => self.open_find(Mode::Find, true),
            Action::Replace => self.open_find(Mode::Replace, false),
            Action::FindNext | Action::FindPrevious if self.find.query.is_empty() => {
                self.open_find(Mode::Find, false)
            }
            Action::FindNext => {
                self.mark_jump();
                self.find_step(true)
            }
            Action::FindPrevious => {
                self.mark_jump();
                self.find_step(false)
            }
            Action::SelectAllOccurrences => self.select_all_occurrences(None),
            Action::SendToPane => self.send_to_pane(false),
            Action::MoveToPane => self.send_to_pane(true),
            Action::FollowLink(url) => self.follow_link(&url),
            Action::NextTab => {
                self.mark_jump();
                self.active = (self.active + 1) % self.tabs.len();
                self.cues.push(Cue::TabNext);
            }
            Action::PreviousTab => {
                self.mark_jump();
                self.active = (self.active + self.tabs.len() - 1) % self.tabs.len();
                self.cues.push(Cue::TabPrev);
            }
            Action::CloseTab => return self.close_tab(pending == Some(Pending::Close)),
            Action::OpenFile => self.open_picker(PickerKind::Files),
            Action::Rename => {
                let doc = &self.tabs[self.active];
                let current = doc.path.clone().unwrap_or_else(|| doc.name.clone());
                self.open_prompt(PromptKind::Rename, current);
            }
            _ => {
                self.status = "not applicable".to_string();
            }
        }
        true
    }

    fn quit(&mut self, confirmed: bool) -> bool {
        let dirty = self.tabs.iter().filter(|d| d.dirty).count();
        if dirty > 0 && !confirmed {
            self.pending = Some(Pending::Quit);
            self.warn(format!("{} unsaved tab(s): quit again to discard", dirty));
            return true;
        }
        false
    }

    fn open_prompt(&mut self, kind: PromptKind, initial: String) {
        self.prompt_input = initial;
        self.mode = Mode::Prompt(kind);
    }

    fn apply_prompt(&mut self, kind: PromptKind, action: Action, pending: Option<Pending>) -> bool {
        match action {
            Action::Quit => return self.quit(pending == Some(Pending::Quit)),
            Action::InsertChar(c) => self.prompt_input.push(c),
            Action::InsertText(t) => self.prompt_input.extend(t.chars().filter(|c| *c != '\n')),
            Action::Backspace => {
                self.prompt_input.pop();
            }
            Action::ClearExtraCaret => self.mode = Mode::Normal,
            Action::Newline | Action::StartNewLine => match kind {
                PromptKind::Rename => self.rename(pending == Some(Pending::Overwrite)),
                PromptKind::GoToLine => {
                    self.mode = Mode::Normal;
                    let input = std::mem::take(&mut self.prompt_input);
                    self.go_to_line(input.trim());
                }
            },
            _ => {}
        }
        true
    }

    fn close_tab(&mut self, confirmed: bool) -> bool {
        if self.tabs[self.active].dirty && !confirmed {
            self.pending = Some(Pending::Close);
            self.warn("unsaved changes: close again to discard".to_string());
            return true;
        }
        self.tabs.remove(self.active);
        if self.tabs.is_empty() {
            return false;
        }
        self.active = self.active.min(self.tabs.len() - 1);
        self.cues.push(Cue::TabPrev);
        true
    }

    pub fn note_new_file(&mut self, name: &str) {
        self.status = format!("new file: {} (saving creates it)", name);
    }

    pub fn offer_update(&mut self, from: String, to: String) {
        self.update = Some((from, to));
        self.cues.push(Cue::UpdateOffered);
    }

    pub fn has_unsaved(&self) -> bool {
        self.tabs.iter().any(|d| d.dirty)
    }

    fn apply_update_offer(&mut self, action: Action, pending: Option<Pending>) -> bool {
        match action {
            Action::Newline if self.has_unsaved() => {
                self.warn("save your changes first (Ctrl+S), then press Enter to update".to_string());
            }
            Action::Newline => {
                self.restart_for_update = true;
                return false;
            }
            Action::ClearExtraCaret => self.update = None,
            Action::SaveAll => self.save_all(),
            Action::Quit => return self.quit(pending == Some(Pending::Quit)),
            _ => {}
        }
        true
    }

    pub fn use_theme(&mut self, choice: &str) -> bool {
        let path = self.theme_roots.colors(choice);
        let known = matches!(choice, "neon" | "omarchy") || path.is_some();
        self.theme_choice = if known { choice.to_string() } else { "neon".to_string() };
        self.theme.switch(path);
        known
    }

    fn end_theme_preview(&mut self) {
        if let Some(palette) = self.theme_before.take() {
            crate::theme::set(palette);
        }
    }

    fn preview_theme(&mut self) {
        let Some(picker) = self.picker.as_ref().filter(|p| p.kind == PickerKind::Themes) else {
            return;
        };
        if let Some(PickTarget::Theme(choice)) = picker.shown.get(picker.selected).map(|(i, _)| &picker.items[*i].target) {
            crate::theme::set(crate::theme::load(self.theme_roots.colors(choice).as_deref()));
        }
    }

    fn choose_theme(&mut self, choice: String) {
        self.theme_before = None;
        self.use_theme(&choice);
        let name = match choice.as_str() {
            "omarchy" => "follows Omarchy".to_string(),
            other => other.to_string(),
        };
        match self.config_path.clone().map(|p| crate::config::save_theme(&p, &choice)) {
            Some(Err(e)) => self.warn(format!("theme: {}, but config.toml wasn't saved: {}", name, e)),
            Some(Ok(())) => self.status = format!("theme: {} (saved to config.toml)", name),
            None => self.status = format!("theme: {}", name),
        }
    }

    fn open_picker(&mut self, kind: PickerKind) {
        self.end_theme_preview();
        let items: Vec<PickItem> = match kind {
            PickerKind::Actions => crate::keys::PALETTE
                .iter()
                .filter_map(|&(name, label, keys)| {
                    Action::from_name(name).map(|a| PickItem { label: label.to_string(), detail: keys.to_string(), target: PickTarget::Action(a) })
                })
                .collect(),
            PickerKind::Structure => {
                let doc = &self.tabs[self.active];
                let Some(view) = doc.markdown_view() else {
                    self.status = "no outline for this file: File structure lists markdown headings".to_string();
                    return;
                };
                let items: Vec<PickItem> = view
                    .lines
                    .iter()
                    .enumerate()
                    .filter(|(_, l)| l.anchor.is_some())
                    .map(|(i, l)| {
                        let level = doc.lines[i].trim_start().chars().take_while(|c| *c == '#').count().max(1);
                        let text: String = l.segs.iter().map(|s| s.text.as_str()).collect();
                        PickItem {
                            label: format!("{}{}", "  ".repeat(level - 1), text.trim()),
                            detail: format!("line {}", i + 1),
                            target: PickTarget::Line(i),
                        }
                    })
                    .collect();
                if items.is_empty() {
                    self.status = "this file has no headings".to_string();
                    return;
                }
                items
            }
            PickerKind::Themes => {
                self.theme_before = Some(crate::theme::pal());
                let theme = |label: &str, detail: String, choice: &str| PickItem { label: label.to_string(), detail, target: PickTarget::Theme(choice.to_string()) };
                let mut items = vec![
                    theme("Follow Omarchy", self.theme_roots.current_name().unwrap_or_default(), "omarchy"),
                    theme("Neon", "ee's own".to_string(), "neon"),
                ];
                items.extend(self.theme_roots.installed().into_iter().map(|(name, yours)| theme(&name, if yours { "yours".to_string() } else { String::new() }, &name)));
                items
            }
            PickerKind::Files => {
                let root = std::env::current_dir().unwrap_or_default();
                list_files(&root)
                    .into_iter()
                    .map(|f| PickItem { label: f.clone(), detail: String::new(), target: PickTarget::File(root.join(f).to_string_lossy().to_string()) })
                    .collect()
            }
        };
        let mut picker = Picker { kind, input: String::new(), items, shown: Vec::new(), selected: 0 };
        picker.refilter();
        if let Some(current) = picker.shown.iter().position(|(i, _)| matches!(&picker.items[*i].target, PickTarget::Theme(c) if *c == self.theme_choice)) {
            picker.selected = current;
        }
        self.picker = Some(picker);
        self.mode = Mode::Picker;
        self.cues.push(Cue::PickerOpened);
    }

    fn apply_picker(&mut self, action: Action, pending: Option<Pending>) -> bool {
        let Some(picker) = self.picker.as_mut() else {
            self.mode = Mode::Normal;
            return true;
        };
        match action {
            Action::Quit => return self.quit(pending == Some(Pending::Quit)),
            Action::InsertChar(c) => {
                picker.input.push(c);
                picker.refilter();
            }
            Action::InsertText(t) => {
                picker.input.extend(t.chars().filter(|c| *c != '\n'));
                picker.refilter();
            }
            Action::Backspace => {
                picker.input.pop();
                picker.refilter();
            }
            Action::Up | Action::FindPrevious if !picker.shown.is_empty() => {
                picker.selected = (picker.selected + picker.shown.len() - 1) % picker.shown.len();
            }
            Action::Down | Action::FindNext if !picker.shown.is_empty() => {
                picker.selected = (picker.selected + 1) % picker.shown.len();
            }
            Action::PageUp => picker.selected = picker.selected.saturating_sub(self.page_rows.max(1)),
            Action::PageDown if !picker.shown.is_empty() => {
                picker.selected = (picker.selected + self.page_rows.max(1)).min(picker.shown.len() - 1);
            }
            Action::ClearExtraCaret => {
                self.mode = Mode::Normal;
                self.picker = None;
                self.end_theme_preview();
            }
            Action::FindAction => self.open_picker(PickerKind::Actions),
            Action::FileStructure => self.open_picker(PickerKind::Structure),
            Action::OpenFile => self.open_picker(PickerKind::Files),
            Action::SwitchTheme => self.open_picker(PickerKind::Themes),
            Action::Newline => {
                let picker = self.picker.take().unwrap();
                self.mode = Mode::Normal;
                let typed = picker.input.trim().to_string();
                let chosen = picker.shown.get(picker.selected).map(|(i, _)| picker.items[*i].target.clone());
                let literal = typed.starts_with(['/', '~', '.']);
                match (picker.kind, chosen) {
                    (PickerKind::Files, _) if literal => self.open_path(&expand_home(&typed)),
                    (_, Some(PickTarget::Action(a))) => return self.apply(a),
                    (_, Some(PickTarget::Line(line))) => {
                        self.mark_jump();
                        self.active_doc().go_to(line, 0);
                    }
                    (_, Some(PickTarget::File(path))) => self.open_path(&path),
                    (_, Some(PickTarget::Theme(choice))) => self.choose_theme(choice),
                    (PickerKind::Files, None) if !typed.is_empty() => self.open_path(&expand_home(&typed)),
                    _ => {}
                }
                self.end_theme_preview();
            }
            _ => {}
        }
        self.preview_theme();
        true
    }

    fn mark_jump(&mut self) {
        let doc = &self.tabs[self.active];
        let here = (doc.id, doc.cursor);
        if self.back.last() != Some(&here) {
            self.back.push(here);
            if self.back.len() > 100 {
                self.back.remove(0);
            }
        }
        self.forward.clear();
    }

    fn navigate(&mut self, back: bool) {
        let doc = &self.tabs[self.active];
        let here = (doc.id, doc.cursor);
        loop {
            let entry = if back { self.back.pop() } else { self.forward.pop() };
            let Some((id, (line, col))) = entry else {
                return;
            };
            let Some(tab) = self.tabs.iter().position(|d| d.id == id) else {
                continue;
            };
            if (id, (line, col)) == here {
                continue;
            }
            if back {
                self.forward.push(here);
            } else {
                self.back.push(here);
            }
            if tab != self.active {
                self.cues.push(if back { Cue::TabPrev } else { Cue::TabNext });
            }
            self.active = tab;
            self.tabs[tab].go_to(line, col);
            return;
        }
    }

    fn go_to_line(&mut self, input: &str) {
        if input.is_empty() {
            return;
        }
        let (line, col) = input.split_once(':').unwrap_or((input, "1"));
        match (line.trim().parse::<usize>(), col.trim().parse::<usize>()) {
            (Ok(line), Ok(col)) if line > 0 && col > 0 => {
                self.mark_jump();
                self.active_doc().go_to(line - 1, col - 1);
            }
            _ => self.warn(format!("not a line number: {} (try 42 or 42:7)", input)),
        }
    }

    pub fn open_path(&mut self, path: &str) {
        if path.is_empty() {
            return;
        }
        self.mark_jump();
        if let Some(i) = self
            .tabs
            .iter()
            .position(|d| d.path.as_deref().is_some_and(|p| same_file(p, path)))
        {
            self.active = i;
            self.cues.push(Cue::Opened);
            return;
        }
        match Document::open_or_new(path) {
            Ok((doc, is_new)) => {
                self.cues.push(Cue::Opened);
                if is_new {
                    self.note_new_file(&doc.name);
                }
                if self.tabs[self.active].is_blank_untitled() {
                    self.tabs[self.active] = doc;
                } else {
                    self.tabs.push(doc);
                    self.active = self.tabs.len() - 1;
                }
            }
            Err(e) => self.fail(format!("cannot open {}: {}", path, plain(&e))),
        }
    }

    fn rename(&mut self, confirmed: bool) {
        let target = self.prompt_input.trim().to_string();
        let old = self.tabs[self.active].path.clone();
        if target.is_empty() || old.as_deref().is_some_and(|p| same_file(p, &target)) {
            self.mode = Mode::Normal;
            return;
        }
        if Path::new(&target).exists() && !confirmed {
            self.pending = Some(Pending::Overwrite);
            self.warn(format!("{} exists: press Enter again to overwrite", target));
            return;
        }
        let doc = &mut self.tabs[self.active];
        if let Err(e) = fs::write(&target, doc.full_content().as_bytes()) {
            self.fail(format!("cannot write {}: {}", target, plain(&e)));
            return;
        }
        doc.name = Path::new(&target)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| target.clone());
        doc.path = Some(target.clone());
        doc.dirty = false;
        self.mode = Mode::Normal;
        self.cues.push(Cue::Saved);
        match old {
            Some(old) => match fs::remove_file(&old) {
                Ok(()) => self.status = format!("renamed to {}", target),
                Err(e) => self.warn(format!("saved as {}, but cannot remove {}: {}", target, old, plain(&e))),
            },
            None => self.status = format!("saved as {}", target),
        }
    }

    fn jump_to_anchor(&mut self, anchor: &str) -> bool {
        let doc = &mut self.tabs[self.active];
        let Some(view) = doc.markdown_view() else {
            return false;
        };
        let Some(line) = view.lines.iter().position(|l| l.anchor.as_deref() == Some(anchor)) else {
            return false;
        };
        doc.clear_extra_carets();
        doc.cursor = (line, 0);
        true
    }

    fn follow_link(&mut self, url: &str) {
        if url.is_empty() {
            return;
        }
        if let Some(anchor) = url.strip_prefix('#') {
            self.mark_jump();
            if !self.jump_to_anchor(anchor) {
                self.warn(format!("no heading for #{} in this file", anchor));
            }
            return;
        }
        let external = ["http://", "https://", "mailto:"].iter().any(|p| url.starts_with(p));
        if external {
            match open_external(url) {
                Ok(()) => self.status = format!("opened {}", url),
                Err(e) => self.fail(format!("cannot open {}: {}", url, plain(&e))),
            }
            return;
        }
        let (file, anchor) = url.split_once('#').unwrap_or((url, ""));
        let base = self.tabs[self.active]
            .path
            .as_deref()
            .and_then(|p| Path::new(p).parent())
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let target = base.join(file).to_string_lossy().to_string();
        self.open_path(&target);
        if !anchor.is_empty() && self.alert.is_none() {
            self.jump_to_anchor(anchor);
        }
    }

    fn send_to_pane(&mut self, remove: bool) {
        let lines = self.tabs[self.active].caret_lines();
        match Herdr::from_env().and_then(|h| h.send(&lines.join("\n"))) {
            Ok(target) if remove => {
                self.active_doc().remove_caret_lines();
                self.status = format!("moved {} line(s) to {}", lines.len(), target);
                self.cues.push(Cue::Moved);
            }
            Ok(target) => {
                self.status = format!("sent {} line(s) to {}", lines.len(), target);
                self.cues.push(Cue::Sent);
            }
            Err(e) => self.fail(e),
        }
    }

    fn copy(&mut self) {
        let (text, whole_line) = self.tabs[self.active].copy_text();
        self.status = match self.clipboard.copy(text, whole_line) {
            CopyTarget::System => "copied".to_string(),
            CopyTarget::Terminal => "copied (no system clipboard tool; sent to terminal)".to_string(),
        };
    }

    fn paste(&mut self) {
        let (text, whole_line) = self.clipboard.paste();
        if text.is_empty() {
            self.status = "clipboard empty".to_string();
            return;
        }
        let doc = self.active_doc();
        if whole_line && !doc.has_selection() {
            doc.insert_lines_above(&text);
        } else {
            doc.insert_text(&text);
        }
    }

    fn select_all_occurrences(&mut self, query: Option<(String, bool)>) {
        let doc = &mut self.tabs[self.active];
        if query.is_none() && !doc.occurrences.is_empty() {
            doc.clear_extra_carets();
            return;
        }
        let Some((text, case_sensitive)) = query.or_else(|| doc.occurrence_target().map(|t| (t, true)))
        else {
            self.status = "nothing to select".to_string();
            return;
        };
        let ranges = find::find_all(&doc.lines, &text, case_sensitive);
        self.status = format!("{} occurrence(s)", ranges.len());
        doc.set_occurrences(ranges);
    }

    fn open_find(&mut self, mode: Mode, all_tabs: bool) {
        let doc = &self.tabs[self.active];
        if let Some(((sl, _), (el, _))) = doc.selection {
            if sl == el {
                self.find.query = doc.selected_text().unwrap_or_default();
            }
        }
        let origin = doc.selection.map(|(start, _)| start).unwrap_or(doc.cursor);
        self.find.origin = (self.active, origin);
        if matches!(self.mode, Mode::Normal) {
            self.mark_jump();
        }
        self.find.all_tabs = all_tabs;
        self.find.editing_replacement = false;
        self.mode = mode;
        self.cues.push(Cue::FindOpened);
        if !self.find.query.is_empty() {
            self.search_from_origin();
        }
    }

    fn apply_find(&mut self, action: Action, pending: Option<Pending>) -> bool {
        let replacing = matches!(self.mode, Mode::Replace);
        let editing_query = !(replacing && self.find.editing_replacement);
        match action {
            Action::Quit => return self.quit(pending == Some(Pending::Quit)),
            Action::InsertChar(_) | Action::InsertText(_) | Action::Backspace => {
                let field = if editing_query {
                    &mut self.find.query
                } else {
                    &mut self.find.replacement
                };
                match action {
                    Action::InsertChar(c) => field.push(c),
                    Action::InsertText(t) => field.extend(t.chars().filter(|c| *c != '\n')),
                    _ => {
                        field.pop();
                    }
                }
                if editing_query {
                    self.search_from_origin();
                }
            }
            Action::Newline if !editing_query => self.replace_one(),
            Action::Newline | Action::FindNext => self.find_step(true),
            Action::FindPrevious => self.find_step(false),
            Action::ToggleMatchCase => {
                self.find.case_sensitive = !self.find.case_sensitive;
                self.search_from_origin();
            }
            Action::SwitchField if replacing => {
                self.find.editing_replacement = !self.find.editing_replacement
            }
            Action::ReplaceOne if replacing => self.replace_one(),
            Action::ReplaceAll if replacing => self.replace_all(),
            Action::Find => self.open_find(Mode::Find, false),
            Action::FindInFiles => self.open_find(Mode::Find, true),
            Action::Replace => self.open_find(Mode::Replace, false),
            Action::SelectAllOccurrences => {
                self.mode = Mode::Normal;
                let query = (self.find.query.clone(), self.find.case_sensitive);
                if !query.0.is_empty() {
                    self.select_all_occurrences(Some(query));
                }
            }
            Action::ClearExtraCaret => self.mode = Mode::Normal,
            Action::Undo => self.active_doc().undo(),
            Action::Redo => self.active_doc().redo(),
            Action::SaveAll => self.save_all(),
            _ => {}
        }
        true
    }

    pub fn find_hits(&self) -> Vec<Hit> {
        let tabs: Vec<usize> = if self.find.all_tabs {
            (0..self.tabs.len()).collect()
        } else {
            vec![self.active]
        };
        let mut hits = Vec::new();
        for t in tabs {
            for (l, s, e) in find::find_all(&self.tabs[t].lines, &self.find.query, self.find.case_sensitive) {
                hits.push((t, l, s, e));
            }
        }
        hits
    }

    pub fn current_hit(&self, hits: &[Hit]) -> Option<usize> {
        let doc = &self.tabs[self.active];
        hits.iter().position(|&(t, l, s, e)| t == self.active && doc.selection == Some(((l, s), (l, e))))
    }

    fn go_to_hit(&mut self, (t, l, s, e): Hit) {
        self.active = t;
        let doc = &mut self.tabs[t];
        doc.clear_extra_carets();
        doc.selection = Some(((l, s), (l, e)));
        doc.cursor = (l, e);
        self.cues.push(Cue::Matched);
    }

    fn search_from(&mut self, from: (usize, (usize, usize))) {
        let hits = self.find_hits();
        let (tab, (line, col)) = from;
        match hits.iter().find(|h| (h.0, h.1, h.2) >= (tab, line, col)).or(hits.first()) {
            Some(&hit) => self.go_to_hit(hit),
            None => {
                self.active = tab;
                let doc = &mut self.tabs[tab];
                doc.selection = None;
                doc.cursor = (line, col);
            }
        }
    }

    fn search_from_origin(&mut self) {
        let origin = self.find.origin;
        self.search_from(origin);
    }

    fn find_step(&mut self, forward: bool) {
        let hits = self.find_hits();
        if hits.is_empty() {
            if matches!(self.mode, Mode::Normal) {
                self.status = "no matches".to_string();
            }
            return;
        }
        let doc = &self.tabs[self.active];
        let (line, col) = doc.selection.map(|(start, _)| start).unwrap_or(doc.cursor);
        let here = (self.active, line, col);
        let next = if forward {
            hits.iter().find(|h| (h.0, h.1, h.2) > here).or(hits.first())
        } else {
            hits.iter().rev().find(|h| (h.0, h.1, h.2) < here).or(hits.last())
        };
        let hit = *next.unwrap();
        self.go_to_hit(hit);
        self.find.origin = (hit.0, (hit.1, hit.2));
    }

    fn replace_one(&mut self) {
        let hits = self.find_hits();
        let Some(i) = self.current_hit(&hits) else {
            self.find_step(true);
            return;
        };
        let (t, l, s, e) = hits[i];
        let replacement = self.find.replacement.clone();
        let doc = &mut self.tabs[t];
        doc.replace_ranges(&[(l, s, e)], &replacement);
        let after = (l, s + replacement.chars().count());
        doc.cursor = after;
        self.status = "replaced 1".to_string();
        if !self.find_hits().is_empty() {
            self.search_from((t, after));
        }
    }

    fn replace_all(&mut self) {
        let doc = &mut self.tabs[self.active];
        let ranges = find::find_all(&doc.lines, &self.find.query, self.find.case_sensitive);
        doc.replace_ranges(&ranges, &self.find.replacement);
        self.status = format!("replaced {}", ranges.len());
    }

    pub fn save_all(&mut self) {
        let mut saved = 0;
        let mut failed = 0;
        let mut skipped = 0;
        for doc in &mut self.tabs {
            let path = doc.path.clone();
            match path {
                Some(p) => match fs::write(p.as_str(), doc.full_content().as_bytes()) {
                    Ok(_) => {
                        doc.dirty = false;
                        saved += 1;
                    }
                    Err(_) => failed += 1,
                },
                None => skipped += 1,
            }
        }
        if saved + failed == 0 {
            self.status = if skipped == 0 {
                "nothing to save".to_string()
            } else {
                format!("{} skipped (no path; Shift+F6 to name it)", skipped)
            };
        } else {
            let mut s = format!("{} saved", saved);
            if failed > 0 {
                s.push_str(&format!(" ({} failed)", failed));
            }
            if skipped > 0 {
                s.push_str(&format!(" ({} skipped)", skipped));
            }
            if failed > 0 {
                self.fail(s);
            } else {
                self.status = s;
                self.cues.push(Cue::Saved);
            }
        }
    }
}

fn expand_home(path: &str) -> String {
    match (path.strip_prefix("~/"), std::env::var("HOME")) {
        (Some(rest), Ok(home)) => format!("{}/{}", home, rest),
        _ => path.to_string(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickerKind {
    Actions,
    Structure,
    Files,
    Themes,
}

#[derive(Clone)]
enum PickTarget {
    Action(Action),
    Line(usize),
    File(String),
    Theme(String),
}

#[derive(Clone)]
pub struct PickItem {
    pub label: String,
    pub detail: String,
    target: PickTarget,
}

pub struct Picker {
    pub kind: PickerKind,
    pub input: String,
    items: Vec<PickItem>,
    pub shown: Vec<(usize, Vec<usize>)>,
    pub selected: usize,
}

impl Picker {
    pub fn total(&self) -> usize {
        self.items.len()
    }

    pub fn item(&self, index: usize) -> &PickItem {
        &self.items[index]
    }

    fn refilter(&mut self) {
        let mut scored: Vec<(i32, usize, Vec<usize>)> = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| fuzzy(&self.input, &item.label).map(|(score, hits)| (score, i, hits)))
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        self.shown = scored.into_iter().map(|(_, i, hits)| (i, hits)).collect();
        self.selected = 0;
    }
}

pub fn fuzzy(query: &str, text: &str) -> Option<(i32, Vec<usize>)> {
    let q: Vec<char> = query.chars().filter(|c| !c.is_whitespace()).flat_map(char::to_lowercase).collect();
    if q.is_empty() {
        return Some((0, Vec::new()));
    }
    let t: Vec<char> = text.chars().collect();
    let lower: Vec<char> = t.iter().map(|c| c.to_lowercase().next().unwrap_or(*c)).collect();
    let mut hits = Vec::with_capacity(q.len());
    let mut score = 0;
    let mut qi = 0;
    let mut last: Option<usize> = None;
    for (i, &c) in lower.iter().enumerate() {
        if qi < q.len() && c == q[qi] {
            let boundary = i == 0 || !t[i - 1].is_alphanumeric();
            score += if last == Some(i.wrapping_sub(1)) { 5 } else { 1 } + if boundary { 8 } else { 0 };
            if let Some(l) = last {
                score -= (i - l - 1).min(3) as i32;
            }
            hits.push(i);
            last = Some(i);
            qi += 1;
        }
    }
    let first_letter = t.iter().position(|c| c.is_alphanumeric());
    if hits.first().copied() == first_letter {
        score += 6;
    }
    score -= (t.len() / 12) as i32;
    (qi == q.len()).then_some((score, hits))
}

fn comment_syntax(doc: &Document) -> Option<(&'static str, &'static str)> {
    let name = doc.path.as_deref().unwrap_or(&doc.name);
    let file = Path::new(name).file_name().map(|f| f.to_string_lossy().to_lowercase()).unwrap_or_default();
    if matches!(file.as_str(), "makefile" | "dockerfile" | ".bashrc" | ".zshrc" | ".profile" | ".gitignore" | ".env") {
        return Some(("#", ""));
    }
    let ext = file.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    Some(match ext {
        "rs" | "js" | "mjs" | "ts" | "jsx" | "tsx" | "c" | "h" | "cpp" | "hpp" | "cc" | "go" | "java" | "kt" | "swift" | "zig" | "scss" | "jsonc" | "dart" | "cs" => ("//", ""),
        "sh" | "bash" | "zsh" | "fish" | "py" | "rb" | "toml" | "yaml" | "yml" | "conf" | "cfg" | "pl" | "r" | "nix" | "ps1" | "tf" => ("#", ""),
        "lua" | "sql" | "hs" | "elm" => ("--", ""),
        "md" | "markdown" | "html" | "htm" | "xml" | "svg" | "vue" => ("<!--", "-->"),
        "css" => ("/*", "*/"),
        "ini" => (";", ""),
        "vim" => ("\"", ""),
        _ => return None,
    })
}

fn list_files(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || name == "target" || name == "node_modules" {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                if depth < 8 {
                    stack.push((path, depth + 1));
                }
            } else if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().to_string());
                if out.len() >= 5000 {
                    return out;
                }
            }
        }
    }
    out.sort();
    out
}

fn open_external(url: &str) -> std::io::Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    std::process::Command::new("xdg-open")
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
}

pub fn plain(e: &std::io::Error) -> String {
    let text = e.to_string();
    match text.find(" (os error") {
        Some(i) => text[..i].to_string(),
        None => text,
    }
}

fn same_file(a: &str, b: &str) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("ee-test-{}-{}", tag, std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }

        fn root(&self) -> String {
            self.0.to_string_lossy().into_owned()
        }

        fn path(&self, name: &str) -> String {
            self.0.join(name).to_string_lossy().into_owned()
        }

        fn write(&self, name: &str, content: &str) -> String {
            let path = self.path(name);
            fs::write(&path, content).unwrap();
            path
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn state_with(tabs: &[&str]) -> EditorState {
        let mut state = EditorState::new();
        state.tabs = tabs.iter().enumerate().map(|(i, t)| Document::with_content(&format!("t{}", i), t)).collect();
        state
    }

    fn type_text(state: &mut EditorState, text: &str) {
        for c in text.chars() {
            state.apply(Action::InsertChar(c));
        }
    }

    fn type_into(state: &mut EditorState, action: Action, text: &str) {
        state.apply(action);
        type_text(state, text);
    }

    #[test]
    fn save_all_writes_every_file_tab_and_skips_untitled() {
        let dir = Scratch::new("save-all");
        let (one, two) = (dir.path("one.txt"), dir.path("two.txt"));
        let mut state = state_with(&["hello\n", "world\n", ""]);
        state.tabs[0].path = Some(one.clone());
        state.tabs[1].path = Some(two.clone());
        state.apply(Action::InsertChar('x'));
        state.apply(Action::SaveAll);
        assert_eq!(fs::read_to_string(&one).unwrap(), "xhello\n");
        assert_eq!(fs::read_to_string(&two).unwrap(), "world\n");
        assert!(!state.tabs[0].dirty);
        assert_eq!(state.status, "2 saved (1 skipped)");
    }

    #[test]
    fn tab_switching_wraps_in_both_directions() {
        let mut state = state_with(&["a", "b", "c"]);
        state.apply(Action::PreviousTab);
        assert_eq!(state.active, 2);
        state.apply(Action::NextTab);
        assert_eq!(state.active, 0);
        state.apply(Action::NextTab);
        assert_eq!(state.active, 1);
    }

    #[test]
    fn close_tab_confirms_unsaved_changes_and_quits_on_last_tab() {
        let mut state = state_with(&["a", "b"]);
        state.active = 1;
        state.apply(Action::InsertChar('x'));

        assert!(state.apply(Action::CloseTab));
        assert_eq!(state.tabs.len(), 2);
        assert!(state.status.contains("unsaved"));
        state.apply(Action::Right);
        state.apply(Action::CloseTab);
        assert_eq!(state.tabs.len(), 2, "an intervening action must reset the confirmation");

        state.apply(Action::CloseTab);
        assert_eq!(state.tabs.len(), 1);
        assert_eq!(state.active, 0);
        assert_eq!(state.tabs[0].lines[0], "a");

        assert!(!state.apply(Action::CloseTab));
    }

    #[test]
    fn open_file_prompt_opens_switches_and_reports_errors() {
        let dir = Scratch::new("open");
        let p = dir.write("prompt_open.txt", "one\ntwo");

        let mut state = EditorState::new();
        type_into(&mut state, Action::OpenFile, &p);
        assert!(matches!(state.mode, Mode::Picker));
        assert_eq!(state.tabs[0].lines, vec![""], "prompt input must not reach the document");
        state.apply(Action::Newline);
        assert!(matches!(state.mode, Mode::Normal));
        assert_eq!(state.tabs.len(), 1, "a blank untitled tab is replaced");
        assert_eq!(state.tabs[0].lines, vec!["one", "two"]);
        assert_eq!(state.tabs[0].name, "prompt_open.txt");

        state.tabs.push(Document::new("untitled"));
        state.active = 1;
        state.apply(Action::InsertChar('z'));
        type_into(&mut state, Action::OpenFile, &p);
        state.apply(Action::Newline);
        assert_eq!(state.tabs.len(), 2, "an already-open file is not opened twice");
        assert_eq!(state.active, 0);

        type_into(&mut state, Action::OpenFile, &dir.root());
        state.apply(Action::Newline);
        assert_eq!(state.tabs.len(), 2, "a directory does not open");
        assert_eq!(state.status, format!("cannot open {}: Is a directory", dir.root()));

        let fresh = dir.path("fresh.txt");
        type_into(&mut state, Action::OpenFile, &fresh);
        state.apply(Action::Newline);
        assert_eq!((state.tabs.len(), state.active), (3, 2), "a missing file opens as a new tab");
        assert_eq!(state.status, "new file: fresh.txt (saving creates it)");
        assert!(!state.tabs[2].dirty, "a new, untouched file is not unsaved work");
        assert!(!Path::new(&fresh).exists());
        state.apply(Action::SaveAll);
        assert!(Path::new(&fresh).exists(), "saving creates it");

        type_into(&mut state, Action::OpenFile, &p);
        state.apply(Action::ClearExtraCaret);
        assert!(matches!(state.mode, Mode::Normal));
        assert_eq!(state.tabs.len(), 3);
        assert_eq!(state.tabs[0].lines, vec!["one", "two"]);
    }

    #[test]
    fn find_selects_matches_incrementally_and_wraps() {
        let mut state = state_with(&["Foo foo\nbar foo"]);
        state.tabs[0].cursor = (0, 2);
        type_into(&mut state, Action::Find, "foo");
        assert_eq!(state.tabs[0].selection, Some(((0, 4), (0, 7))), "first match after the caret");
        assert_eq!(state.current_hit(&state.find_hits()), Some(1));
        state.apply(Action::Newline);
        assert_eq!(state.tabs[0].selection, Some(((1, 4), (1, 7))));
        state.apply(Action::FindNext);
        assert_eq!(state.tabs[0].selection, Some(((0, 0), (0, 3))), "wraps to the top");
        state.apply(Action::FindPrevious);
        assert_eq!(state.tabs[0].selection, Some(((1, 4), (1, 7))), "wraps to the bottom");

        state.apply(Action::ToggleMatchCase);
        assert_eq!(state.find_hits().len(), 2);
        state.apply(Action::Backspace);
        type_text(&mut state, "X");
        assert!(state.find_hits().is_empty());
        assert_eq!(state.tabs[0].selection, None);

        state.apply(Action::Backspace);
        state.apply(Action::InsertChar('o'));
        state.apply(Action::ClearExtraCaret);
        assert!(matches!(state.mode, Mode::Normal));
        let current = state.tabs[0].selection;
        state.apply(Action::FindNext);
        assert_ne!(state.tabs[0].selection, current, "F3 keeps searching after the bar closes");
        state.apply(Action::InsertChar('Z'));
        assert_eq!(state.tabs[0].full_content(), "Foo Z\nbar foo", "the found text stays selected");
    }

    #[test]
    fn find_in_files_walks_matches_across_tabs() {
        let mut state = state_with(&["x needle", "none", "needle needle"]);
        type_into(&mut state, Action::FindInFiles, "needle");
        assert_eq!((state.active, state.tabs[0].selected_text().as_deref()), (0, Some("needle")));
        state.apply(Action::FindNext);
        assert_eq!((state.active, state.tabs[2].selection), (2, Some(((0, 0), (0, 6)))));
        state.apply(Action::FindNext);
        assert_eq!((state.active, state.tabs[2].selection), (2, Some(((0, 7), (0, 13)))));
        state.apply(Action::FindNext);
        assert_eq!(state.active, 0);
        state.apply(Action::Find);
        assert_eq!(state.find_hits().len(), 1, "plain Find is limited to the active tab");
    }

    #[test]
    fn replace_next_and_replace_all() {
        let mut state = state_with(&["a cat, a cat, a cat"]);
        type_into(&mut state, Action::Replace, "cat");
        type_into(&mut state, Action::SwitchField, "big cat");
        state.apply(Action::Newline);
        assert_eq!(state.tabs[0].lines[0], "a big cat, a cat, a cat");
        assert_eq!(state.tabs[0].selection, Some(((0, 13), (0, 16))), "moves on past the replacement");
        state.apply(Action::ReplaceAll);
        assert_eq!(state.tabs[0].lines[0], "a big big cat, a big cat, a big cat");
        assert!(state.status.contains("replaced 3"));
        state.apply(Action::Undo);
        assert_eq!(state.tabs[0].lines[0], "a big cat, a cat, a cat", "replace all is one undo step");
    }

    #[test]
    fn select_all_occurrences_edits_every_copy_and_toggles_off() {
        let mut state = state_with(&["let id = id + id_2;\nid"]);
        state.tabs[0].cursor = (0, 5);
        state.apply(Action::SelectAllOccurrences);
        assert!(state.status.contains("4 occurrence"));
        state.apply(Action::SelectAllOccurrences);
        assert!(state.tabs[0].occurrences.is_empty());
        state.apply(Action::SelectAllOccurrences);
        type_text(&mut state, "key");
        assert_eq!(state.tabs[0].full_content(), "let key = key + key_2;\nkey");

        let mut state = state_with(&["Ab ab AB"]);
        type_into(&mut state, Action::Find, "ab");
        state.apply(Action::SelectAllOccurrences);
        assert!(matches!(state.mode, Mode::Normal));
        assert_eq!(state.tabs[0].occurrences.len(), 3, "uses the find query and its case setting");
    }

    #[test]
    fn copy_cut_and_paste_between_tabs() {
        let mut state = state_with(&["hello world", "line1\nline2"]);
        state.tabs[0].cursor = (0, 6);
        for _ in 0..5 {
            state.apply(Action::SelectRight);
        }
        state.apply(Action::Copy);
        state.apply(Action::NextTab);
        state.apply(Action::End);
        state.apply(Action::Paste);
        assert_eq!(state.tabs[1].lines, vec!["line1world", "line2"]);

        state.apply(Action::Down);
        state.apply(Action::Cut);
        assert_eq!(state.tabs[1].lines, vec!["line1world"]);
        state.apply(Action::Paste);
        assert_eq!(state.tabs[1].lines, vec!["line2", "line1world"], "a cut line pastes above");

        state.apply(Action::InsertText("a\nb".to_string()));
        assert_eq!(state.tabs[1].lines, vec!["line2", "line1a", "bworld"]);
    }

    #[test]
    fn rename_moves_the_file_and_confirms_overwrite() {
        let dir = Scratch::new("rename");
        let old = dir.write("old.txt", "data");
        let new = dir.path("new.txt");
        let taken = dir.write("taken.txt", "keep");

        let mut state = EditorState::new();
        state.open_path(&old);
        state.apply(Action::InsertChar('!'));
        state.apply(Action::Rename);
        assert_eq!(state.prompt_input, old);
        state.prompt_input = new.clone();
        state.apply(Action::Newline);
        assert!(!Path::new(&old).exists());
        assert_eq!(fs::read_to_string(&new).unwrap(), "!data");
        assert_eq!(state.tabs[0].name, "new.txt");
        assert!(!state.tabs[0].dirty);

        state.apply(Action::Rename);
        state.prompt_input = taken.clone();
        state.apply(Action::Newline);
        assert!(state.status.contains("exists"));
        assert_eq!(fs::read_to_string(&taken).unwrap(), "keep");
        state.apply(Action::Newline);
        assert_eq!(fs::read_to_string(&taken).unwrap(), "!data");
        assert!(!Path::new(&new).exists());
        assert!(matches!(state.mode, Mode::Normal));
    }

    #[test]
    fn rename_of_untitled_tab_saves_it() {
        let dir = Scratch::new("rename-untitled");
        let target = dir.path("untitled.txt");
        let mut state = EditorState::new();
        type_text(&mut state, "hi");
        state.apply(Action::Rename);
        state.prompt_input = target.clone();
        state.apply(Action::Newline);
        assert_eq!(fs::read_to_string(&target).unwrap(), "hi");
        assert_eq!(state.tabs[0].path.as_deref(), Some(target.as_str()));
    }

    #[test]
    fn quit_with_unsaved_changes_needs_confirmation() {
        let mut state = EditorState::new();
        assert!(!state.apply(Action::Quit), "a clean editor quits at once");
        let mut state = EditorState::new();
        state.apply(Action::InsertChar('x'));
        assert!(state.apply(Action::Quit));
        assert!(state.status.contains("unsaved"));
        state.apply(Action::Right);
        assert!(state.apply(Action::Quit), "an intervening action resets the confirmation");
        assert!(!state.apply(Action::Quit));
    }

    #[test]
    fn send_or_move_outside_herdr_reports_an_error_and_keeps_the_lines() {
        for action in [Action::SendToPane, Action::MoveToPane] {
            let mut state = state_with(&["ls\npwd"]);
            state.apply(action);
            assert_eq!(state.status, "herdr is disabled in tests");
            assert!(state.alert == Some(Alert::Error));
            assert_eq!(state.take_cues(), vec![Cue::Error]);
            assert_eq!((state.tabs[0].lines.clone(), state.tabs[0].dirty), (vec!["ls".to_string(), "pwd".to_string()], false));
        }
    }

    #[test]
    fn following_links_jumps_to_headings_and_opens_files() {
        let dir = Scratch::new("links");
        dir.write("other.md", "# Top\n\ntext\n\n## Deep Dive\n");
        let main = dir.write("main.md", "see [x](#usage)\n\n## Usage\n");
        let mut state = EditorState::new();
        state.open_path(&main);
        state.apply(Action::FollowLink("#usage".into()));
        assert_eq!(state.tabs[0].cursor, (2, 0));
        state.apply(Action::FollowLink("#nope".into()));
        assert_eq!(state.status, "no heading for #nope in this file");
        state.apply(Action::FollowLink("other.md#deep-dive".into()));
        assert_eq!((state.tabs.len(), state.active), (2, 1), "relative links open beside the current file");
        assert_eq!(state.tabs[1].cursor, (4, 0));
        state.apply(Action::FollowLink("https://example.com".into()));
        assert_eq!(state.status, "opened https://example.com");
    }

    #[test]
    fn go_to_line_and_navigate_back_and_forward_across_tabs() {
        let mut state = state_with(&["a\nb\nc\nd\ne", "x\ny"]);
        type_into(&mut state, Action::GoToLine, "4:2");
        state.apply(Action::Newline);
        assert_eq!(state.tabs[0].cursor, (3, 1));
        state.apply(Action::NextTab);
        state.apply(Action::EndOfFile);
        assert_eq!((state.active, state.tabs[1].cursor), (1, (1, 1)));

        state.apply(Action::NavigateBack);
        assert_eq!((state.active, state.tabs[1].cursor), (1, (0, 0)), "back to before the jump to the end");
        state.apply(Action::NavigateBack);
        assert_eq!((state.active, state.tabs[0].cursor), (0, (3, 1)), "back across tabs");
        state.apply(Action::NavigateBack);
        assert_eq!(state.tabs[0].cursor, (0, 0), "back to before go to line");
        state.apply(Action::NavigateBack);
        assert_eq!(state.tabs[0].cursor, (0, 0), "nothing further back");
        state.apply(Action::NavigateForward);
        state.apply(Action::NavigateForward);
        assert_eq!((state.active, state.tabs[1].cursor), (1, (0, 0)));

        type_into(&mut state, Action::GoToLine, "nope");
        state.apply(Action::Newline);
        assert!(state.status.starts_with("not a line number: nope"));
    }

    #[test]
    fn fuzzy_matching_prefers_word_starts_and_contiguous_runs() {
        assert!(fuzzy("xyz", "Save all").is_none());
        let (_, hits) = fuzzy("sa", "Save all").unwrap();
        assert_eq!(hits, vec![0, 1]);
        let word_start = fuzzy("gl", "Go to line").unwrap().0;
        let buried = fuzzy("gl", "ringlet").unwrap().0;
        assert!(word_start > buried);
        let prefix = fuzzy("cr", "  Crew").unwrap().0;
        let initials = fuzzy("cr", "Night City run log").unwrap().0;
        assert!(prefix > initials, "a match at the first letter wins");
        let run = fuzzy("lin", &format!("alin{}", "-".repeat(24))).unwrap().0;
        let scattered = fuzzy("lin", "alxixn").unwrap().0;
        assert!(run > scattered, "adjacent letters beat scattered ones");
    }

    #[test]
    fn pickers_run_actions_jump_to_headings_and_open_files() {
        let mut state = state_with(&["hello"]);
        type_into(&mut state, Action::FindAction, "select all");
        assert!(matches!(state.mode, Mode::Picker));
        state.apply(Action::Newline);
        assert!(matches!(state.mode, Mode::Normal));
        assert_eq!(state.tabs[0].selected_text().as_deref(), Some("hello"), "the chosen action ran");

        let mut state = EditorState::new();
        state.tabs[0] = Document::with_content("n.md", "# Top\n\ntext\n\n## Deep dive\n");
        type_into(&mut state, Action::FileStructure, "deep");
        state.apply(Action::Newline);
        assert_eq!(state.tabs[0].cursor, (4, 0));
        state.apply(Action::NavigateBack);
        assert_eq!(state.tabs[0].cursor, (0, 0), "outline jumps can be undone with back");
        state.tabs[0] = Document::with_content("t.txt", "plain");
        state.apply(Action::FileStructure);
        assert!(matches!(state.mode, Mode::Normal));
        assert!(state.status.starts_with("no outline for this file"));
    }

    #[test]
    fn file_finder_prefers_a_typed_path_and_opens_unmatched_names_as_new_files() {
        let dir = Scratch::new("finder");
        let typed = dir.path("typed.md");
        let mut state = EditorState::new();
        state.apply(Action::OpenFile);
        let picker = state.picker.as_mut().unwrap();
        picker.items = vec![PickItem { label: typed.clone(), detail: String::new(), target: PickTarget::File(dir.path("decoy.md")) }];
        picker.refilter();
        type_text(&mut state, &typed);
        assert_eq!(state.picker.as_ref().unwrap().shown.len(), 1, "the decoy matches what was typed");
        state.apply(Action::Newline);
        assert_eq!(state.tabs[0].name, "typed.md", "a typed path wins over a matching entry");

        state.apply(Action::OpenFile);
        let picker = state.picker.as_mut().unwrap();
        picker.items.clear();
        picker.refilter();
        type_text(&mut state, "no-such-ee-file.md");
        state.apply(Action::Newline);
        assert_eq!(state.tabs[state.active].name, "no-such-ee-file.md", "an unmatched name opens as a new file");
        assert!(!Path::new("no-such-ee-file.md").exists(), "opening writes nothing");
    }

    #[test]
    fn back_skips_closed_tabs_and_the_spot_you_are_on() {
        let mut state = state_with(&["a", "b", "c"]);
        state.apply(Action::NextTab);
        state.apply(Action::NextTab);
        state.apply(Action::PreviousTab);
        state.apply(Action::CloseTab);
        state.apply(Action::NavigateBack);
        assert_eq!(state.active, 0, "the closed tab's entry is skipped");
        assert_eq!(state.tabs[state.active].name, "t0");
    }

    #[test]
    fn page_up_and_down_move_every_caret_a_screen_and_select_with_shift() {
        let text: Vec<String> = (0..30).map(|n| if n == 12 { "x".to_string() } else { format!("line {}", n) }).collect();
        let mut state = state_with(&[&text.join("\n")]);
        state.page_rows = 10;
        state.tabs[0].cursor = (2, 4);
        state.tabs[0].extra_carets = vec![(3, 1)];
        state.apply(Action::PageDown);
        assert_eq!((state.tabs[0].cursor, state.tabs[0].extra_carets.clone()), ((12, 1), vec![(13, 1)]), "the column clamps to a short line");
        assert_eq!(state.tabs[0].scroll_top.get(), 10, "the view moves with the carets");
        state.apply(Action::ClearExtraCaret);
        state.apply(Action::SelectPageDown);
        assert_eq!(state.tabs[0].selection, Some(((12, 1), (22, 1))));
        state.apply(Action::PageDown);
        state.apply(Action::PageDown);
        assert_eq!(state.tabs[0].cursor, (29, 1), "stops on the last line");
        state.apply(Action::SelectPageUp);
        assert_eq!(state.tabs[0].selection, Some(((19, 1), (29, 1))));
        state.apply(Action::PageUp);
        state.apply(Action::PageUp);
        state.apply(Action::PageUp);
        assert_eq!((state.tabs[0].cursor, state.tabs[0].selection), ((0, 1), None), "stops on the first line");

        state.apply(Action::FindAction);
        state.page_rows = 5;
        state.apply(Action::PageDown);
        assert_eq!(state.picker.as_ref().unwrap().selected, 5, "pickers page through their list");
        state.apply(Action::PageUp);
        state.apply(Action::PageUp);
        assert_eq!(state.picker.as_ref().unwrap().selected, 0, "and stop at the top instead of wrapping");
        for _ in 0..20 {
            state.apply(Action::PageDown);
        }
        let picker = state.picker.as_ref().unwrap();
        assert_eq!(picker.selected, picker.shown.len() - 1, "and at the bottom");
    }

    #[test]
    fn the_theme_picker_previews_live_restores_on_esc_and_saves_on_enter() {
        use crate::theme::{pal, Roots, NEON_PALETTE};
        use ratatui::style::Color;
        let dir = Scratch::new("themes");
        let theme = |rel: &str, background: &str| {
            let folder = dir.0.join(rel);
            fs::create_dir_all(&folder).unwrap();
            fs::write(folder.join("colors.toml"), format!("background = \"{}\"\nforeground = \"#ffffff\"\n", background)).unwrap();
        };
        theme("user/amber", "#110000");
        theme("system/amber", "#ffffff");
        theme("system/blue", "#000011");
        theme("current/theme", "#001100");
        fs::write(dir.0.join("current/theme.name"), "blue\n").unwrap();
        let config = dir.path("config.toml");
        let mut state = state_with(&["x"]);
        state.theme_roots = Roots { user: Some(dir.0.join("user")), system: Some(dir.0.join("system")), current: Some(dir.0.join("current")) };
        state.config_path = Some(config.clone().into());
        assert!(state.use_theme("neon"));
        let background = || pal().void;

        state.apply(Action::SwitchTheme);
        let picker = state.picker.as_ref().unwrap();
        let rows: Vec<(String, String)> = (0..picker.total()).map(|i| (picker.item(i).label.clone(), picker.item(i).detail.clone())).collect();
        assert_eq!(rows, [("Follow Omarchy", "blue"), ("Neon", "ee's own"), ("amber", "yours"), ("blue", "")].map(|(l, d)| (l.to_string(), d.to_string())));
        assert_eq!(picker.selected, 1, "opens on the theme in use");
        state.apply(Action::Down);
        assert_eq!(background(), Color::Rgb(0x11, 0, 0), "moving previews the theme, yours over the built-in one");
        state.apply(Action::ClearExtraCaret);
        assert_eq!((pal(), state.theme_choice.as_str()), (NEON_PALETTE, "neon"), "Esc puts the old colours back");
        assert!(!Path::new(&config).exists(), "Esc saves nothing");

        type_into(&mut state, Action::SwitchTheme, "blu");
        assert_eq!(background(), Color::Rgb(0, 0, 0x11), "filtering previews the first match");
        state.apply(Action::Newline);
        assert_eq!((state.theme_choice.as_str(), state.status.as_str()), ("blue", "theme: blue (saved to config.toml)"));
        assert_eq!(crate::config::parse(&fs::read_to_string(&config).unwrap()).unwrap().theme, "blue");
        assert_eq!(background(), Color::Rgb(0, 0, 0x11));
        state.apply(Action::SwitchTheme);
        assert_eq!(state.picker.as_ref().unwrap().selected, 3, "reopens on the chosen theme");
        state.apply(Action::ClearExtraCaret);

        type_into(&mut state, Action::SwitchTheme, "follow");
        state.apply(Action::Newline);
        assert_eq!((background(), state.status.as_str()), (Color::Rgb(0, 0x11, 0), "theme: follows Omarchy (saved to config.toml)"));
        assert_eq!(crate::config::parse(&fs::read_to_string(&config).unwrap()).unwrap().theme, "omarchy");

        assert!(!state.use_theme("gone"), "an uninstalled name is reported");
        assert_eq!((pal(), state.theme_choice.as_str()), (NEON_PALETTE, "neon"));
    }

    #[test]
    fn help_opens_from_any_mode_and_the_next_key_only_closes_it() {
        let mut state = state_with(&["abc"]);
        state.apply(Action::ShowHelp);
        assert!(state.help);
        assert_eq!(state.take_cues(), vec![Cue::PickerOpened]);
        state.apply(Action::InsertChar('x'));
        assert!(!state.help, "any key closes it");
        assert_eq!(state.tabs[0].lines, vec!["abc"], "and does nothing else");
        state.apply(Action::Find);
        state.apply(Action::ShowHelp);
        assert!(state.help && matches!(state.mode, Mode::Find), "F1 works in the find bar too");
        state.apply(Action::ShowHelp);
        assert!(!state.help, "F1 again closes it");
        state.apply(Action::ShowHelp);
        assert!(state.apply(Action::Quit), "Ctrl+Q with the sheet open only closes it");
        assert!(!state.help);
    }

    #[test]
    fn last_edit_location_returns_to_the_last_change_across_tabs() {
        let mut state = state_with(&["one\ntwo", "other"]);
        state.apply(Action::LastEditLocation);
        assert_eq!(state.status, "no edits yet");
        state.apply(Action::Down);
        state.apply(Action::InsertChar('x'));
        state.apply(Action::NextTab);
        state.apply(Action::LastEditLocation);
        assert_eq!((state.active, state.tabs[0].cursor), (0, (1, 1)));
    }

    #[test]
    fn the_update_offer_waits_for_saved_work_before_restarting() {
        let dir = Scratch::new("update-offer");
        let mut state = state_with(&["x"]);
        state.offer_update("aaaaaaa".into(), "bbbbbbb".into());
        assert_eq!(state.take_cues(), vec![Cue::UpdateOffered]);
        state.apply(Action::InsertChar('z'));
        assert_eq!(state.tabs[0].lines[0], "x", "the modal takes the keys");
        state.apply(Action::ClearExtraCaret);
        assert!(state.update.is_none(), "Esc keeps working");

        state.apply(Action::InsertChar('z'));
        state.offer_update("aaaaaaa".into(), "bbbbbbb".into());
        assert!(state.apply(Action::Newline), "unsaved work blocks the restart");
        assert!(state.status.starts_with("save your changes first"));
        assert!(state.apply(Action::Quit), "quitting over unsaved work still asks first");
        assert!(state.status.contains("unsaved"));
        let path = dir.path("x.txt");
        state.tabs[0].path = Some(path.clone());
        state.apply(Action::SaveAll);
        assert_eq!(fs::read_to_string(&path).unwrap(), "zx", "the modal can save");
        assert!(!state.apply(Action::Newline), "Enter restarts");
        assert!(state.restart_for_update);
    }
}
