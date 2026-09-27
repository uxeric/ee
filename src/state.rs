use std::fs;
use std::path::Path;

use crate::clipboard::{Clipboard, CopyTarget};
use crate::document::Document;
use crate::find;
use crate::herdr::Herdr;
use crate::keys::Action;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    Open,
    Rename,
}

#[derive(Clone, Copy)]
pub enum Mode {
    Normal,
    Find,
    Replace,
    Prompt(PromptKind),
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
    Warn,
    Error,
    Quit,
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
        match self.mode {
            Mode::Prompt(kind) => self.apply_prompt(kind, action, pending),
            Mode::Find | Mode::Replace => self.apply_find(action, pending),
            Mode::Normal => self.apply_normal(action, pending),
        }
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
            Action::BeginningOfFile => self.active_doc().move_by(Document::beginning_of_file, false),
            Action::EndOfFile => self.active_doc().move_by(Document::end_of_file, false),
            Action::BeginningOfLine => self.active_doc().move_by(Document::home, false),
            Action::EndOfLine => self.active_doc().move_by(Document::end, false),
            Action::SelectLeft => self.active_doc().move_by(Document::move_left, true),
            Action::SelectRight => self.active_doc().move_by(Document::move_right, true),
            Action::SelectUp => self.active_doc().move_by(Document::move_up, true),
            Action::SelectDown => self.active_doc().move_by(Document::move_down, true),
            Action::SelectHome => self.active_doc().move_by(Document::home, true),
            Action::SelectEnd => self.active_doc().move_by(Document::end, true),
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
            Action::FindNext => self.find_step(true),
            Action::FindPrevious => self.find_step(false),
            Action::SelectAllOccurrences => self.select_all_occurrences(None),
            Action::SendToPane => self.send_to_pane(),
            Action::NextTab => {
                self.active = (self.active + 1) % self.tabs.len();
                self.cues.push(Cue::TabNext);
            }
            Action::PreviousTab => {
                self.active = (self.active + self.tabs.len() - 1) % self.tabs.len();
                self.cues.push(Cue::TabPrev);
            }
            Action::CloseTab => return self.close_tab(pending == Some(Pending::Close)),
            Action::OpenFile => self.open_prompt(PromptKind::Open, String::new()),
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
            Action::Newline => match kind {
                PromptKind::Open => {
                    self.mode = Mode::Normal;
                    let input = std::mem::take(&mut self.prompt_input);
                    self.open_path(input.trim());
                }
                PromptKind::Rename => self.rename(pending == Some(Pending::Overwrite)),
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

    pub fn open_path(&mut self, path: &str) {
        if path.is_empty() {
            return;
        }
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

    fn send_to_pane(&mut self) {
        let lines = self.tabs[self.active].caret_lines();
        match Herdr::from_env().and_then(|h| h.send(&lines.join("\n"))) {
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
        if whole_line && !doc.has_selection() && doc.extra_carets.is_empty() {
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
        use std::fs;
        use crate::document::Document;
        use crate::keys::Action;

        fn scratch(tag: &str) -> std::path::PathBuf {
            let dir = std::env::temp_dir().join(format!("ee-test-{}-{}", tag, std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            dir
        }

        fn make_tab(path: Option<String>, content: &str) -> Document {
            let name = path
                .as_deref()
                .map(|p| p.split('/').rev().next().unwrap_or("x").to_string())
                .unwrap_or_else(|| "untitled".to_string());
            let mut d = Document::with_content(&name, content);
            d.path = path;
            d
        }

        #[test]
        fn save_all_writes_every_file_tab_and_skips_untitled() {
            let dir = scratch("save-all");
            let (p1, p2) = (dir.join("one.txt"), dir.join("two.txt"));
            let path = |p: &std::path::Path| Some(p.to_string_lossy().to_string());
            let mut state = EditorState::new();
            state.tabs = vec![make_tab(path(&p1), "hello\n"), make_tab(path(&p2), "world\n"), Document::new("untitled")];
            state.apply(Action::InsertChar('x'));
            state.apply(Action::SaveAll);
            assert_eq!(fs::read_to_string(&p1).unwrap(), "xhello\n");
            assert_eq!(fs::read_to_string(&p2).unwrap(), "world\n");
            assert!(!state.tabs[0].dirty);
            assert_eq!(state.status, "2 saved (1 skipped)");
            fs::remove_dir_all(&dir).unwrap();
        }

    

        fn type_prompt(state: &mut EditorState, text: &str) {
            state.apply(Action::OpenFile);
            for c in text.chars() {
                state.apply(Action::InsertChar(c));
            }
        }

        #[test]
        fn tab_switching_wraps_in_both_directions() {
            let mut state = EditorState::new();
            state.tabs = vec![make_tab(None, "a"), make_tab(None, "b"), make_tab(None, "c")];
            state.apply(Action::PreviousTab);
            assert_eq!(state.active, 2);
            state.apply(Action::NextTab);
            assert_eq!(state.active, 0);
            state.apply(Action::NextTab);
            assert_eq!(state.active, 1);
        }

        #[test]
        fn close_tab_confirms_unsaved_changes_and_quits_on_last_tab() {
            let mut state = EditorState::new();
            state.tabs = vec![make_tab(None, "a"), make_tab(None, "b")];
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
            let dir = scratch("open");
            let p = dir.join("e_editor_prompt_open.txt");
            fs::write(&p, "one\ntwo").unwrap();
            let p = p.to_string_lossy().to_string();

            let mut state = EditorState::new();
            type_prompt(&mut state, &p);
            assert!(matches!(state.mode, Mode::Prompt(PromptKind::Open)));
            assert_eq!(state.tabs[0].lines, vec![""], "prompt input must not reach the document");
            state.apply(Action::Newline);
            assert!(matches!(state.mode, Mode::Normal));
            assert_eq!(state.tabs.len(), 1, "a blank untitled tab is replaced");
            assert_eq!(state.tabs[0].lines, vec!["one", "two"]);
            assert_eq!(state.tabs[0].name, "e_editor_prompt_open.txt");

            state.tabs.push(Document::new("untitled"));
            state.active = 1;
            state.apply(Action::InsertChar('z'));
            type_prompt(&mut state, &p);
            state.apply(Action::Newline);
            assert_eq!(state.tabs.len(), 2, "an already-open file is not opened twice");
            assert_eq!(state.active, 0);

            let dir_path = dir.to_string_lossy().to_string();
            type_prompt(&mut state, &dir_path);
            state.apply(Action::Newline);
            assert_eq!(state.tabs.len(), 2, "a directory does not open");
            assert_eq!(state.status, format!("cannot open {}: Is a directory", dir_path));

            let fresh = dir.join("fresh.txt");
            type_prompt(&mut state, &fresh.to_string_lossy());
            state.apply(Action::Newline);
            assert_eq!((state.tabs.len(), state.active), (3, 2), "a missing file opens as a new tab");
            assert_eq!(state.status, "new file: fresh.txt (saving creates it)");
            assert!(!fresh.exists());
            state.apply(Action::SaveAll);
            assert!(fresh.exists(), "saving creates it");

            type_prompt(&mut state, &p);
            state.apply(Action::ClearExtraCaret);
            assert!(matches!(state.mode, Mode::Normal));
            assert_eq!(state.tabs.len(), 3);
            assert_eq!(state.tabs[0].lines, vec!["one", "two"]);
            fs::remove_dir_all(&dir).unwrap();
        }
    

        fn state_with(tabs: &[&str]) -> EditorState {
            let mut state = EditorState::new();
            state.tabs = tabs
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    let mut d = Document::with_content(&format!("t{}", i), t);
                    d.path = None;
                    d
                })
                .collect();
            state
        }

        fn type_text(state: &mut EditorState, text: &str) {
            for c in text.chars() {
                state.apply(Action::InsertChar(c));
            }
        }

        fn selected(state: &EditorState) -> Option<String> {
            state.tabs[state.active].selected_text()
        }

        #[test]
        fn find_selects_matches_incrementally_and_wraps() {
            let mut state = state_with(&["Foo foo\nbar foo"]);
            state.tabs[0].cursor = (0, 2);
            state.apply(Action::Find);
            type_text(&mut state, "foo");
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
            state.apply(Action::FindInFiles);
            type_text(&mut state, "needle");
            assert_eq!((state.active, selected(&state).as_deref()), (0, Some("needle")));
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
            state.apply(Action::Replace);
            type_text(&mut state, "cat");
            state.apply(Action::SwitchField);
            type_text(&mut state, "big cat");
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
            state.apply(Action::Find);
            type_text(&mut state, "ab");
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
            let dir = scratch("rename");
            let old = dir.join("e_editor_rename_old.txt");
            let new = dir.join("e_editor_rename_new.txt");
            let taken = dir.join("e_editor_rename_taken.txt");
            fs::write(&old, "data").unwrap();
            fs::write(&taken, "keep").unwrap();

            let mut state = EditorState::new();
            state.open_path(&old.to_string_lossy());
            state.apply(Action::InsertChar('!'));
            state.apply(Action::Rename);
            assert_eq!(state.prompt_input, old.to_string_lossy());
            state.prompt_input = new.to_string_lossy().to_string();
            state.apply(Action::Newline);
            assert!(!old.exists());
            assert_eq!(fs::read_to_string(&new).unwrap(), "!data");
            assert_eq!(state.tabs[0].name, "e_editor_rename_new.txt");
            assert!(!state.tabs[0].dirty);

            state.apply(Action::Rename);
            state.prompt_input = taken.to_string_lossy().to_string();
            state.apply(Action::Newline);
            assert!(state.status.contains("exists"));
            assert_eq!(fs::read_to_string(&taken).unwrap(), "keep");
            state.apply(Action::Newline);
            assert_eq!(fs::read_to_string(&taken).unwrap(), "!data");
            assert!(!new.exists());
            assert!(matches!(state.mode, Mode::Normal));
            fs::remove_dir_all(&dir).unwrap();
        }

        #[test]
        fn rename_of_untitled_tab_saves_it() {
            let dir = scratch("rename-untitled");
            let target = dir.join("e_editor_rename_untitled.txt");
            let mut state = EditorState::new();
            type_text(&mut state, "hi");
            state.apply(Action::Rename);
            state.prompt_input = target.to_string_lossy().to_string();
            state.apply(Action::Newline);
            assert_eq!(fs::read_to_string(&target).unwrap(), "hi");
            assert_eq!(state.tabs[0].path.as_deref(), Some(&*target.to_string_lossy()));
            fs::remove_dir_all(&dir).unwrap();
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
        fn send_to_pane_outside_herdr_reports_an_error() {
            let mut state = state_with(&["ls"]);
            state.apply(Action::SendToPane);
            assert_eq!(state.status, "not running inside herdr");
            assert!(state.alert == Some(Alert::Error));
            assert_eq!(state.take_cues(), vec![Cue::Error]);
        }
    

        #[test]
        fn a_missing_file_opens_empty_and_saving_creates_it() {
            let dir = scratch("new-file");
            let path = dir.join("fresh.md");
            let (doc, is_new) = Document::open_or_new(&path.to_string_lossy()).unwrap();
            assert!(is_new && !doc.dirty);
            assert_eq!((doc.name.as_str(), doc.lines.clone()), ("fresh.md", vec![String::new()]));
            let mut state = EditorState::new();
            state.tabs = vec![doc];
            type_text(&mut state, "hi");
            state.apply(Action::SaveAll);
            assert_eq!(fs::read_to_string(&path).unwrap(), "hi");
            assert!(Document::open_or_new(&dir.to_string_lossy()).is_err(), "a directory is still an error");
            fs::remove_dir_all(&dir).unwrap();
        }
    }
