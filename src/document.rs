use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::SystemTime;

use crate::markdown::{self, MdView};

const MAX_UNDO: usize = 100;
const INDENT: usize = 4;

static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

type Selection = ((usize, usize), (usize, usize));

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiskStamp {
    Absent,
    Present { modified: SystemTime, len: u64 },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiskKind {
    Changed,
    Deleted,
}

fn char_class(c: char) -> u8 {
    if c.is_alphanumeric() || c == '_' {
        0
    } else {
        1
    }
}

fn contains(outer: Selection, inner: Selection) -> bool {
    outer.0 <= inner.0 && inner.1 <= outer.1
}

fn enclosing_pair(chars: &[char], start: usize, end: usize, open: char, close: char) -> Option<(usize, usize)> {
    if open == close {
        let o = chars[..start.min(chars.len())].iter().rposition(|&c| c == open)?;
        let c = chars.get(end..)?.iter().position(|&c| c == close)? + end;
        return Some((o, c));
    }
    let mut depth = 0;
    let mut o = None;
    for i in (0..start.min(chars.len())).rev() {
        if chars[i] == close {
            depth += 1;
        } else if chars[i] == open {
            if depth == 0 {
                o = Some(i);
                break;
            }
            depth -= 1;
        }
    }
    let o = o?;
    let mut depth = 0;
    for (i, &ch) in chars.iter().enumerate().skip(end) {
        if ch == open {
            depth += 1;
        } else if ch == close {
            if depth == 0 {
                return Some((o, i));
            }
            depth -= 1;
        }
    }
    None
}

fn file_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

pub struct Snapshot {
    lines: Vec<String>,
    cursor: (usize, usize),
    extra_carets: Vec<(usize, usize)>,
    selection: Option<((usize, usize), (usize, usize))>,
    occurrences: Vec<(usize, usize, usize)>,
    docx_anchors: Option<Vec<crate::docx::Anchor>>,
}

pub struct Document {
    pub id: u64,
    pub name: String,
    pub lines: Vec<String>,
    pub cursor: (usize, usize),
    pub extra_carets: Vec<(usize, usize)>,
    pub selection: Option<((usize, usize), (usize, usize))>,
    pub occurrences: Vec<(usize, usize, usize)>,
    pub path: Option<String>,
    pub dirty: bool,
    pub scroll_top: Cell<usize>,
    pub scroll_left: Cell<usize>,
    pub view_anchor: Cell<Option<((usize, usize), u64)>>,
    pub wrap_width: Cell<usize>,
    disk: Option<DiskStamp>,
    expansions: Vec<(Option<Selection>, (usize, usize))>,
    expanded_to: Option<Selection>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    rev: u64,
    markdown: RefCell<Option<(u64, crate::theme::Palette, Rc<MdView>)>>,
    rust_plain: Cell<bool>,
    rust_preview: RefCell<Option<(u64, crate::theme::Palette, Rc<MdView>)>>,
    syntax: RefCell<Option<(u64, crate::syntax::Lang, crate::syntax::Highlighter)>>,
    syntax_over: Cell<bool>,
    docx: Option<crate::docx::Session>,
}

impl Document {
    pub fn new(name: &str) -> Self {
        Self {
            id: next_id(),
            name: name.to_string(),
            lines: vec![String::new()],
            cursor: (0, 0),
            selection: None,
            occurrences: Vec::new(),
            extra_carets: Vec::new(),
            path: None,
            dirty: false,
            scroll_top: Cell::new(0),
            scroll_left: Cell::new(0),
            view_anchor: Cell::new(None),
            wrap_width: Cell::new(0),
            disk: None,
            expansions: Vec::new(),
            expanded_to: None,
            undo: Vec::new(),
            redo: Vec::new(),
            rev: 0,
            markdown: RefCell::new(None),
            rust_plain: Cell::new(false),
            rust_preview: RefCell::new(None),
            syntax: RefCell::new(None),
            syntax_over: Cell::new(false),
            docx: None,
        }
    }

    /// Create a document from pre-loaded `content`, split into lines by '\n'.
    pub fn with_content(name: &str, content: &str) -> Self {
        Self {
            id: next_id(),
            name: name.to_string(),
            lines: content.split('\n').map(|s| s.to_string()).collect(),
            cursor: (0, 0),
            selection: None,
            occurrences: Vec::new(),
            extra_carets: Vec::new(),
            path: None,
            dirty: false,
            scroll_top: Cell::new(0),
            scroll_left: Cell::new(0),
            view_anchor: Cell::new(None),
            wrap_width: Cell::new(0),
            disk: None,
            expansions: Vec::new(),
            expanded_to: None,
            undo: Vec::new(),
            redo: Vec::new(),
            rev: 0,
            markdown: RefCell::new(None),
            rust_plain: Cell::new(false),
            rust_preview: RefCell::new(None),
            syntax: RefCell::new(None),
            syntax_over: Cell::new(false),
            docx: None,
        }
    }

    pub fn from_file(path: &str) -> std::io::Result<Self> {
        if crate::docx::is_path(path) {
            let bytes = std::fs::read(path)?;
            let (lines, session) = crate::docx::load(&bytes)?;
            let mut doc = Self::with_content(&file_name(path), "");
            doc.lines = if lines.is_empty() { vec![String::new()] } else { lines };
            doc.docx = Some(session);
            doc.path = Some(path.to_string());
            doc.note_disk();
            return Ok(doc);
        }
        let content = std::fs::read_to_string(path)?;
        let mut doc = Self::with_content(&file_name(path), &content);
        doc.path = Some(path.to_string());
        doc.note_disk();
        Ok(doc)
    }

    pub fn open_or_new(path: &str) -> std::io::Result<(Self, bool)> {
        match Self::from_file(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let mut doc = if crate::docx::is_path(path) {
                    let (lines, session) = crate::docx::blank();
                    let mut doc = Self::with_content(&file_name(path), "");
                    doc.lines = lines;
                    doc.docx = Some(session);
                    doc
                } else {
                    Self::new(&file_name(path))
                };
                doc.path = Some(path.to_string());
                doc.note_disk();
                Ok((doc, true))
            }
            other => other.map(|doc| (doc, false)),
        }
    }

    pub fn is_blank_untitled(&self) -> bool {
        self.path.is_none() && !self.dirty && self.lines.len() == 1 && self.lines[0].is_empty()
    }

    fn capture(&self) -> Snapshot {
        Snapshot {
            lines: self.lines.clone(),
            cursor: self.cursor,
            extra_carets: self.extra_carets.clone(),
            selection: self.selection,
            occurrences: self.occurrences.clone(),
            docx_anchors: self.docx.as_ref().map(|s| s.anchors()),
        }
    }

    fn snapshot(&mut self) {
        if let Some(session) = &mut self.docx {
            session.note_edit(self.rev, self.dirty);
            session.realign(&self.lines);
        }
        self.rev += 1;
        self.dirty = true;
        self.undo.push(self.capture());
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn restore_snap(&mut self, snap: Snapshot) {
        self.lines = snap.lines;
        self.cursor = snap.cursor;
        self.extra_carets = snap.extra_carets;
        self.selection = snap.selection;
        self.occurrences = snap.occurrences;
        if let Some(anchors) = snap.docx_anchors {
            if let Some(session) = &mut self.docx {
                session.restore(anchors, &self.lines);
            }
        }
        self.dirty = true;
        self.rev += 1;
    }

    pub fn undo(&mut self) {
        if let Some(session) = &mut self.docx {
            session.realign(&self.lines);
        }
        if let Some(snap) = self.undo.pop() {
            self.redo.push(self.capture());
            self.restore_snap(snap);
        }
    }

    pub fn redo(&mut self) {
        if let Some(session) = &mut self.docx {
            session.realign(&self.lines);
        }
        if let Some(snap) = self.redo.pop() {
            self.undo.push(self.capture());
            self.restore_snap(snap);
        }
    }

    pub fn has_selection(&self) -> bool {
        self.selection.is_some() || !self.occurrences.is_empty()
    }

    pub fn covers(&self, line: usize, col: usize) -> bool {
        if self.occurrences.iter().any(|&(l, s, e)| l == line && s <= col && col < e) {
            return true;
        }
        let Some(((sl, sc), (el, ec))) = self.selection else {
            return false;
        };
        line >= sl && line <= el && !(line == sl && col < sc) && !(line == el && col >= ec)
    }

    fn delete_selection(&mut self) {
        if !self.occurrences.is_empty() {
            let spans: Vec<(usize, (usize, usize))> =
                self.occurrences.iter().map(|&(l, s, e)| (l, (s, e))).collect();
            self.occurrences.clear();
            self.delete_at_carets(&spans);
            return;
        }
        let Some((start, end)) = self.selection else {
            return;
        };
        let (sl, sc) = start;
        let (el, ec) = end;
        if sl == el {
            let chars: Vec<char> = self.lines[sl].chars().collect();
            let mut new = String::new();
            new.extend(chars.iter().take(sc));
            new.extend(chars.iter().skip(ec));
            self.lines[sl] = new;
        } else {
            let first: Vec<char> = self.lines[sl].chars().collect();
            let last: Vec<char> = self.lines[el].chars().collect();
            self.lines[sl] = first[..sc].iter().collect();
            self.lines.drain(sl + 1..=el);
            self.lines[sl].extend(last[ec..].iter().copied());
        }
        self.cursor = (sl, sc);
        self.selection = None;
    }

    pub fn insert_char(&mut self, c: char) {
        self.snapshot();
        self.delete_selection();
        self.insert_char_at_carets(c);
    }

    fn insert_char_at_carets(&mut self, c: char) {
        if self.cursor.0 >= self.lines.len() {
            self.lines.push(String::new());
            self.cursor = (self.lines.len() - 1, 0);
        }
        let mut lines_seen: Vec<usize> = Vec::new();
        let mut all: Vec<(usize, usize)> = vec![self.cursor];
        all.extend_from_slice(&self.extra_carets);
        for (line, _) in &all {
            if *line < self.lines.len() && !lines_seen.contains(line) {
                lines_seen.push(*line);
            }
        }
        for line in &lines_seen {
            let mut cols: Vec<usize> = Vec::new();
            if self.cursor.0 == *line {
                cols.push(self.cursor.1);
            }
            for e in &self.extra_carets {
                if e.0 == *line {
                    cols.push(e.1);
                }
            }
            cols.sort();
            cols.dedup();
            let mut chars: Vec<char> = self.lines[*line].chars().collect();
            for col in cols.iter().rev() {
                chars.insert(*col, c);
            }
            self.lines[*line] = chars.into_iter().collect();
            if self.cursor.0 == *line {
                let c0 = self.cursor.1;
                self.cursor.1 = c0 + cols.iter().filter(|&&cc| cc <= c0).count();
            }
            for e in &mut self.extra_carets {
                if e.0 == *line {
                    let c0 = e.1;
                    e.1 = c0 + cols.iter().filter(|&&cc| cc <= c0).count();
                }
            }
        }
    }

    pub fn backspace(&mut self) {
        self.snapshot();
        if self.has_selection() {
            self.delete_selection();
            return;
        }
        if self.extra_carets.is_empty() {
            let (line, col) = self.cursor;
            if col > 0 {
                let mut chars: Vec<char> = self.lines[line].chars().collect();
                chars.remove(col - 1);
                self.lines[line] = chars.into_iter().collect();
                self.cursor = (line, col - 1);
            } else if line > 0 {
                let prev_len = self.lines[line - 1].chars().count();
                let drained: Vec<char> = self.lines[line].chars().collect();
                self.lines[line - 1].extend(drained);
                self.lines.remove(line);
                self.cursor = (line - 1, prev_len);
            }
            return;
        }
        let mut spans: Vec<(usize, (usize, usize))> = Vec::new();
        let (line, col) = self.cursor;
        if line < self.lines.len() && col > 0 {
            spans.push((line, (col - 1, col)));
        }
        for e in &self.extra_carets {
            if e.0 < self.lines.len() && e.1 > 0 {
                spans.push((e.0, (e.1 - 1, e.1)));
            }
        }
        Self::delete_at_carets(self, &spans);
    }

    pub fn newline(&mut self) {
        self.snapshot();
        self.delete_selection();
        let (line, col) = self.cursor;
        if line < self.lines.len() {
            let chars: Vec<char> = self.lines[line].chars().collect();
            let right: Vec<char> = chars[col..].to_vec();
            self.lines[line] = chars[..col].iter().copied().collect();
            self.lines.insert(line + 1, right.into_iter().collect());
            self.cursor = (line + 1, 0);
        } else {
            self.lines.push(String::new());
            self.cursor = (self.lines.len() - 1, 0);
        }
    }

    pub fn delete_char_forward(&mut self) {
        self.snapshot();
        if self.has_selection() {
            self.delete_selection();
            return;
        }
        if self.extra_carets.is_empty() {
            let (line, col) = self.cursor;
            if line < self.lines.len() && col < self.lines[line].chars().count() {
                let mut chars: Vec<char> = self.lines[line].chars().collect();
                chars.remove(col);
                self.lines[line] = chars.into_iter().collect();
            }
            return;
        }
        let mut spans: Vec<(usize, (usize, usize))> = Vec::new();
        let (line, col) = self.cursor;
        if line < self.lines.len() && col < self.lines[line].chars().count() {
            spans.push((line, (col, col + 1)));
        }
        for e in &self.extra_carets {
            if e.0 < self.lines.len() && e.1 < self.lines[e.0].chars().count() {
                spans.push((e.0, (e.1, e.1 + 1)));
            }
        }
        Self::delete_at_carets(self, &spans);
    }

    pub fn delete_word_forward(&mut self) {
        self.snapshot();
        if self.has_selection() {
            self.delete_selection();
            return;
        }
        let mut spans: Vec<(usize, (usize, usize))> = Vec::new();
        let (line, col) = self.cursor;
        if line < self.lines.len() {
            let chars: Vec<char> = self.lines[line].chars().collect();
            if let Some((s, e)) = Self::word_span(&chars, col, true) {
                spans.push((line, (s, e)));
            }
        }
        for ec in &self.extra_carets {
            let el = ec.0;
            if el < self.lines.len() {
                let chars: Vec<char> = self.lines[el].chars().collect();
                if let Some((s, e)) = Self::word_span(&chars, ec.1, true) {
                    spans.push((el, (s, e)));
                }
            }
        }
        Self::delete_at_carets(self, &spans);
    }

    pub fn delete_word_backward(&mut self) {
        self.snapshot();
        if self.has_selection() {
            self.delete_selection();
            return;
        }
        let mut spans: Vec<(usize, (usize, usize))> = Vec::new();
        let (line, col) = self.cursor;
        if line < self.lines.len() {
            let chars: Vec<char> = self.lines[line].chars().collect();
            if let Some((s, e)) = Self::word_span(&chars, col, false) {
                spans.push((line, (s, e)));
            }
        }
        for ec in &self.extra_carets {
            let el = ec.0;
            if el < self.lines.len() {
                let chars: Vec<char> = self.lines[el].chars().collect();
                if let Some((s, e)) = Self::word_span(&chars, ec.1, false) {
                    spans.push((el, (s, e)));
                }
            }
        }
        Self::delete_at_carets(self, &spans);
    }

    /// Span `(start, end)` deleted by a word delete from `start`, forward or
    /// backward. `forward` skips leading whitespace then the word; `backward`
    /// skips trailing whitespace then the word to the left.
    fn word_span(chars: &[char], start: usize, forward: bool) -> Option<(usize, usize)> {
        if forward {
            let n = chars.len();
            if start >= n {
                return None;
            }
            let mut i = start;
            while i < n && chars[i].is_whitespace() {
                i += 1;
            }
            while i < n && !chars[i].is_whitespace() {
                i += 1;
            }
            if i <= start {
                None
            } else {
                Some((start, i))
            }
        } else {
            let mut i = start;
            while i > 0 && chars[i - 1].is_whitespace() {
                i -= 1;
            }
            while i > 0 && !chars[i - 1].is_whitespace() {
                i -= 1;
            }
            if i >= start {
                None
            } else {
                Some((i, start))
            }
        }
    }

    /// Deletes every char in each `(line, (start, end))` span and repositions
    /// the active caret and extra carets on the affected lines.
    fn delete_at_carets(self: &mut Document, spans: &[(usize, (usize, usize))]) {
        let mut lines_seen: Vec<usize> = Vec::new();
        for (l, _) in spans {
            if *l < self.lines.len() && !lines_seen.contains(l) {
                lines_seen.push(*l);
            }
        }
        for line in &lines_seen {
            let n = self.lines[*line].chars().count();
            let chars: Vec<char> = self.lines[*line].chars().collect();
            let mut removed = vec![false; n];
            for (l, (s, e)) in spans {
                if *l != *line {
                    continue;
                }
                for i in *s..*e {
                    if i < n {
                        removed[i] = true;
                    }
                }
            }
            let mut new: Vec<char> = Vec::new();
            for (i, ch) in chars.iter().enumerate() {
                if !removed[i] {
                    new.push(*ch);
                }
            }
            self.lines[*line] = new.into_iter().collect();
            let reposition = |pos: &mut usize| {
                if *pos > n {
                    *pos = n;
                }
                let rb = removed[..*pos].iter().filter(|&&r| r).count();
                *pos = *pos - rb;
            };
            if self.cursor.0 == *line {
                reposition(&mut self.cursor.1);
            }
            for e in &mut self.extra_carets {
                if e.0 == *line {
                    reposition(&mut e.1);
                }
            }
        }
    }

    pub fn move_by(&mut self, movement: impl Fn(&mut Document), select: bool) {
        if select && !(self.extra_carets.is_empty() && self.occurrences.is_empty()) {
            return self.select_at_every_caret(movement);
        }
        self.occurrences.clear();
        if select {
            let anchor = match self.selection {
                Some((start, end)) if self.cursor == start => end,
                Some((start, _)) => start,
                None => self.cursor,
            };
            movement(self);
            self.selection = match anchor.cmp(&self.cursor) {
                std::cmp::Ordering::Less => Some((anchor, self.cursor)),
                std::cmp::Ordering::Greater => Some((self.cursor, anchor)),
                std::cmp::Ordering::Equal => None,
            };
            return;
        }
        self.selection = None;
        let active = self.cursor;
        let extras = std::mem::take(&mut self.extra_carets);
        let mut moved: Vec<(usize, usize)> = Vec::new();
        for pos in extras {
            self.cursor = pos;
            movement(self);
            moved.push(self.cursor);
        }
        self.cursor = active;
        movement(self);
        moved.sort();
        moved.dedup();
        moved.retain(|p| *p != self.cursor);
        self.extra_carets = moved;
    }

    fn select_at_every_caret(&mut self, movement: impl Fn(&mut Document)) {
        let mut ranges = std::mem::take(&mut self.occurrences);
        if let Some(((sl, sc), (_, ec))) = self.selection.take().filter(|((sl, _), (el, _))| sl == el) {
            ranges.push((sl, sc, ec));
        }
        let anchor_of = |caret: (usize, usize)| {
            ranges
                .iter()
                .find_map(|&(l, s, e)| match caret {
                    (cl, cc) if cl == l && cc == e => Some((l, s)),
                    (cl, cc) if cl == l && cc == s => Some((l, e)),
                    _ => None,
                })
                .unwrap_or(caret)
        };
        let active = self.cursor;
        let mut carets = vec![active];
        carets.append(&mut self.extra_carets);
        let mut moved = Vec::new();
        let mut selected = Vec::new();
        for caret in carets {
            let anchor = anchor_of(caret);
            self.cursor = caret;
            movement(self);
            moved.push(self.cursor);
            if anchor.0 == self.cursor.0 && anchor.1 != self.cursor.1 {
                selected.push((anchor.0, anchor.1.min(self.cursor.1), anchor.1.max(self.cursor.1)));
            }
        }
        self.cursor = moved[0];
        let mut extras: Vec<(usize, usize)> = moved[1..].to_vec();
        extras.sort();
        extras.dedup();
        extras.retain(|p| *p != self.cursor);
        self.extra_carets = extras;
        selected.sort();
        let mut merged: Vec<(usize, usize, usize)> = Vec::new();
        for (l, s, e) in selected {
            match merged.last_mut() {
                Some(last) if last.0 == l && s < last.2 => last.2 = last.2.max(e),
                _ => merged.push((l, s, e)),
            }
        }
        self.occurrences = merged;
    }

    pub fn move_left(&mut self) {
        let (line, col) = self.cursor;
        let new = if col > 0 {
            (line, col - 1)
        } else if line > 0 {
            (line - 1, self.lines[line - 1].chars().count())
        } else {
            self.cursor
        };
        self.cursor = new;
    }

    pub fn move_right(&mut self) {
        let (line, col) = self.cursor;
        let new = if col < self.lines[line].chars().count() {
            (line, col + 1)
        } else if line + 1 < self.lines.len() {
            (line + 1, 0)
        } else {
            self.cursor
        };
        self.cursor = new;
    }

    pub fn move_up(&mut self) {
        self.move_lines(-1);
    }

    pub fn move_down(&mut self) {
        self.move_lines(1);
    }

    pub fn move_lines(&mut self, delta: isize) {
        let n = self.lines.len();
        let (line, col) = self.cursor;
        let width = self.wrap_width.get();
        let next = {
            let len_of = |i: usize| self.lines[i].chars().count();
            crate::wrap::shift(n, len_of, line, col, delta, width)
        };
        self.cursor = next;
    }

    /// Remove a caret position from `extra_carets` so the invariant holds:
    /// the active `cursor` is never also listed in `extra_carets`.
    fn remove_extra(&mut self, pos: (usize, usize)) {
        self.extra_carets.retain(|p| *p != pos);
    }

    fn add_caret_by(&mut self, delta: isize) {
        let old = self.cursor;
        self.move_lines(delta);
        if self.cursor == old {
            return;
        }
        self.remove_extra(self.cursor);
        self.extra_carets.push(old);
    }

    /// Move the active caret up one visual row, leaving a caret behind.
    pub fn add_caret_up(&mut self) {
        self.add_caret_by(-1);
    }

    /// Move the active caret down one visual row, leaving a caret behind.
    pub fn add_caret_down(&mut self) {
        self.add_caret_by(1);
    }

    /// Add (or remove, if it already exists) a caret at an explicit position.
    /// Used for mouse toggling. The caret becomes the active one.
    pub fn add_caret_at(&mut self, line: usize, col: usize) {
        if line >= self.lines.len() {
            return;
        }
        let m = self.lines[line].chars().count();
        let nc = col.min(m);
        let pos = (line, nc);
        if self.cursor == pos {
            return;
        }
        // Clicking an existing secondary caret removes it.
        if self.extra_carets.contains(&pos) {
            self.remove_extra(pos);
            return;
        }
        // Fresh position: keep the line we leave as a caret and make the
        // clicked position the active one.
        self.remove_extra(pos);
        self.extra_carets.push(self.cursor);
        self.cursor = pos;
    }

    pub fn clear_extra_carets(&mut self) {
        self.extra_carets.clear();
        self.occurrences.clear();
        self.selection = None;
    }

    /// Move the line containing the active caret up (swap with the line above).
    pub fn move_line_up(&mut self) {
        let (line, _col) = self.cursor;
        if line > 0 {
            self.snapshot();
            self.lines.swap(line - 1, line);
            self.cursor.0 = line - 1;
        }
    }

    /// Move the line containing the active caret down (swap with the line below).
    pub fn move_line_down(&mut self) {
        let (line, _col) = self.cursor;
        if line + 1 < self.lines.len() {
            self.snapshot();
            self.lines.swap(line, line + 1);
            self.cursor.0 = line + 1;
        }
    }

    /// Duplicate the line containing the active caret, inserting the copy below.
    pub fn duplicate_line(&mut self) {
        let (line, _col) = self.cursor;
        if line >= self.lines.len() {
            return;
        }
        self.snapshot();
        let content = self.lines[line].clone();
        self.lines.insert(line + 1, content);
        self.cursor.0 = line + 1;
        for e in &mut self.extra_carets {
            if e.0 >= line + 1 {
                e.0 += 1;
            }
        }
    }

    /// Delete the line containing the active caret (keeps at least one line).
    pub fn delete_line(&mut self) {
        let (line, col) = self.cursor;
        if line >= self.lines.len() || self.lines.len() == 1 {
            return;
        }
        self.snapshot();
        self.lines.remove(line);
        let n = self.lines.len();
        let new_line = line.min(n - 1);
        let m = self.lines[new_line].chars().count();
        self.cursor = (new_line, col.min(m));
        self.extra_carets.retain(|(l, _)| *l < n);
        for e in &mut self.extra_carets {
            let m = self.lines[e.0].chars().count();
            if e.1 > m {
                e.1 = m;
            }
        }
    }

    pub fn word_right(&mut self) {
        let (line, col) = self.cursor;
        let chars: Vec<char> = self.lines[line].chars().collect();
        if col >= chars.len() {
            if line + 1 < self.lines.len() {
                self.cursor = (line + 1, 0);
            }
            return;
        }
        let mut i = col;
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i < chars.len() {
            let class = char_class(chars[i]);
            while i < chars.len() && char_class(chars[i]) == class {
                i += 1;
            }
        }
        self.cursor = (line, i);
    }

    pub fn word_left(&mut self) {
        let (line, col) = self.cursor;
        if col == 0 {
            if line > 0 {
                self.cursor = (line - 1, self.lines[line - 1].chars().count());
            }
            return;
        }
        let chars: Vec<char> = self.lines[line].chars().collect();
        let mut i = col.min(chars.len());
        while i > 0 && chars[i - 1].is_whitespace() {
            i -= 1;
        }
        if i > 0 {
            let class = char_class(chars[i - 1]);
            while i > 0 && char_class(chars[i - 1]) == class {
                i -= 1;
            }
        }
        self.cursor = (line, i);
    }

    pub fn start_new_line(&mut self) {
        self.snapshot();
        self.selection = None;
        self.occurrences.clear();
        let mut caret_lines: Vec<usize> = self.extra_carets.iter().map(|&(l, _)| l).collect();
        caret_lines.push(self.cursor.0);
        caret_lines.sort();
        caret_lines.dedup();
        let mut lines = Vec::with_capacity(self.lines.len() + caret_lines.len());
        let mut new_pos: Vec<(usize, usize)> = Vec::new();
        for (i, line) in self.lines.iter().enumerate() {
            lines.push(line.clone());
            if caret_lines.binary_search(&i).is_ok() {
                let indent: String = line.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
                new_pos.push((lines.len(), indent.chars().count()));
                lines.push(indent);
            }
        }
        let active = caret_lines.binary_search(&self.cursor.0).unwrap();
        self.lines = lines;
        self.cursor = new_pos[active];
        self.extra_carets = new_pos.iter().enumerate().filter(|(i, _)| *i != active).map(|(_, p)| *p).collect();
    }

    pub fn extend_selection(&mut self) {
        if self.selection != self.expanded_to {
            self.expansions.clear();
        }
        let current = self.selection.unwrap_or((self.cursor, self.cursor));
        let Some(next) = self.expansion_candidates(current).into_iter().filter(|c| contains(*c, current) && *c != current).min_by_key(|c| self.span_len(*c)) else {
            return;
        };
        self.expansions.push((self.selection, self.cursor));
        self.occurrences.clear();
        self.selection = Some(next);
        self.cursor = next.1;
        self.expanded_to = self.selection;
    }

    pub fn shrink_selection(&mut self) {
        if self.selection != self.expanded_to {
            self.expansions.clear();
            return;
        }
        if let Some((selection, cursor)) = self.expansions.pop() {
            self.selection = selection;
            self.cursor = cursor;
            self.expanded_to = selection;
        }
    }

    fn span_len(&self, ((sl, sc), (el, ec)): Selection) -> usize {
        if sl == el {
            return ec - sc;
        }
        let mut n = self.lines[sl].chars().count() - sc + ec;
        for l in sl + 1..el {
            n += self.lines[l].chars().count() + 1;
        }
        n + 1
    }

    fn expansion_candidates(&self, ((sl, sc), (el, ec)): Selection) -> Vec<Selection> {
        let mut out = Vec::new();
        let len = |l: usize| self.lines[l].chars().count();
        if sl == el {
            let chars: Vec<char> = self.lines[sl].chars().collect();
            let is_word = |c: char| c.is_alphanumeric() || c == '_';
            let (mut a, mut b) = (sc, ec);
            while a > 0 && is_word(chars[a - 1]) {
                a -= 1;
            }
            while b < chars.len() && is_word(chars[b]) {
                b += 1;
            }
            out.push(((sl, a), (sl, b)));
            for (open, close) in [('(', ')'), ('[', ']'), ('{', '}'), ('<', '>'), ('"', '"'), ('\'', '\''), ('`', '`')] {
                if let Some((o, c)) = enclosing_pair(&chars, sc, ec, open, close) {
                    out.push(((sl, o + 1), (sl, c)));
                    out.push(((sl, o), (sl, c + 1)));
                }
            }
        }
        let first = self.lines[sl].chars().take_while(|c| c.is_whitespace()).count();
        let last_len = self.lines[el].trim_end().chars().count();
        out.push(((sl, first), (el, last_len.max(first.min(len(el))))));
        out.push(((sl, 0), (el, len(el))));
        let blank = |l: usize| self.lines[l].trim().is_empty();
        if !blank(sl) && !blank(el) {
            let (mut a, mut b) = (sl, el);
            while a > 0 && !blank(a - 1) {
                a -= 1;
            }
            while b + 1 < self.lines.len() && !blank(b + 1) {
                b += 1;
            }
            out.push(((a, 0), (b, len(b))));
        }
        let last = self.lines.len() - 1;
        out.push(((0, 0), (last, len(last))));
        out
    }

    pub fn home(&mut self) {
        let (line, col) = self.cursor;
        let first = self.lines[line].chars().take_while(|c| c.is_whitespace()).count();
        self.cursor = (line, if col == first { 0 } else { first });
    }

    pub fn line_start(&mut self) {
        self.cursor.0 = self.cursor.0.min(self.lines.len() - 1);
        self.cursor.1 = 0;
    }

    fn selected_lines(&self) -> Option<(usize, usize)> {
        let ((sl, _), (el, ec)) = self.selection?;
        Some((sl, if ec == 0 && el > sl { el - 1 } else { el }))
    }

    fn all_carets(&self) -> Vec<(usize, usize)> {
        let mut carets = vec![self.cursor];
        carets.extend(self.extra_carets.iter().copied());
        carets
    }

    fn set_carets(&mut self, carets: Vec<(usize, usize)>) {
        self.cursor = carets[0];
        self.extra_carets = carets[1..].to_vec();
    }

    pub fn indent(&mut self) {
        self.occurrences.clear();
        if let Some((sl, last)) = self.selected_lines() {
            self.snapshot();
            let pad = " ".repeat(INDENT);
            let shifted: Vec<bool> = (0..self.lines.len())
                .map(|l| sl <= l && l <= last && !self.lines[l].is_empty())
                .collect();
            for (l, shift) in shifted.iter().enumerate() {
                if *shift {
                    self.lines[l].insert_str(0, &pad);
                }
            }
            let bump = |(l, c): (usize, usize)| if shifted[l] && c > 0 { (l, c + INDENT) } else { (l, c) };
            let ((a, b), cursor) = (self.selection.unwrap(), self.cursor);
            self.selection = Some((bump(a), bump(b)));
            self.cursor = bump(cursor);
            return;
        }
        self.snapshot();
        let mut carets = self.all_carets();
        let mut order: Vec<usize> = (0..carets.len()).collect();
        order.sort_by(|&x, &y| carets[y].cmp(&carets[x]));
        for idx in order {
            let (l, c) = carets[idx];
            let n = INDENT - c % INDENT;
            let byte = self.lines[l].char_indices().nth(c).map_or(self.lines[l].len(), |(b, _)| b);
            self.lines[l].insert_str(byte, &" ".repeat(n));
            for (j, p) in carets.iter_mut().enumerate() {
                if p.0 == l && (p.1 > c || j == idx) {
                    p.1 += n;
                }
            }
        }
        self.set_carets(carets);
    }

    pub fn unindent(&mut self) {
        let lines: Vec<usize> = match self.selected_lines() {
            Some((sl, last)) => (sl..=last).collect(),
            None => {
                let mut ls: Vec<usize> = self.all_carets().iter().map(|&(l, _)| l).collect();
                ls.sort();
                ls.dedup();
                ls
            }
        };
        let removed: Vec<(usize, usize)> = lines
            .iter()
            .map(|&l| {
                let line = &self.lines[l];
                let n = if line.starts_with('\t') { 1 } else { line.chars().take(INDENT).take_while(|c| *c == ' ').count() };
                (l, n)
            })
            .filter(|&(_, n)| n > 0)
            .collect();
        if removed.is_empty() {
            return;
        }
        self.snapshot();
        self.occurrences.clear();
        for &(l, n) in &removed {
            self.lines[l].drain(..n);
        }
        let pull = |(l, c): (usize, usize)| match removed.iter().find(|&&(rl, _)| rl == l) {
            Some(&(_, n)) => (l, c.saturating_sub(n)),
            None => (l, c),
        };
        self.selection = self.selection.map(|(a, b)| (pull(a), pull(b)));
        let carets = self.all_carets().into_iter().map(pull).collect();
        self.set_carets(carets);
    }

    pub fn select_all(&mut self) {
        self.extra_carets.clear();
        self.occurrences.clear();
        let last = self.lines.len() - 1;
        let end = (last, self.lines[last].chars().count());
        self.selection = if end == (0, 0) { None } else { Some(((0, 0), end)) };
        self.cursor = end;
    }

    pub fn start_new_line_above(&mut self) {
        self.snapshot();
        self.selection = None;
        self.occurrences.clear();
        self.extra_carets.clear();
        let line = self.cursor.0;
        let indent: String = self.lines[line].chars().take_while(|c| *c == ' ' || *c == '\t').collect();
        let col = indent.chars().count();
        self.lines.insert(line, indent);
        self.cursor = (line, col);
    }

    fn word_range_at(&self, (line, col): (usize, usize)) -> Option<(usize, usize, usize)> {
        let chars: Vec<char> = self.lines[line].chars().collect();
        let is_word = |c: char| c.is_alphanumeric() || c == '_';
        let (mut a, mut b) = (col.min(chars.len()), col.min(chars.len()));
        while a > 0 && is_word(chars[a - 1]) {
            a -= 1;
        }
        while b < chars.len() && is_word(chars[b]) {
            b += 1;
        }
        (a < b).then_some((line, a, b))
    }

    pub fn toggle_case(&mut self) -> bool {
        let ranges: Vec<(usize, usize, usize)> = if !self.occurrences.is_empty() {
            self.occurrences.clone()
        } else if let Some(((sl, sc), (el, ec))) = self.selection {
            (sl..=el)
                .map(|l| (l, if l == sl { sc } else { 0 }, if l == el { ec } else { self.lines[l].chars().count() }))
                .collect()
        } else {
            match self.word_range_at(self.cursor) {
                Some(r) => vec![r],
                None => return false,
            }
        };
        let text: String = ranges
            .iter()
            .flat_map(|&(l, s, e)| self.lines[l].chars().skip(s).take(e - s).collect::<Vec<_>>())
            .collect();
        let to_upper = text.chars().any(|c| c.is_lowercase());
        let flip = |c: char| {
            let mapped: Vec<char> = if to_upper { c.to_uppercase().collect() } else { c.to_lowercase().collect() };
            if mapped.len() == 1 { mapped[0] } else { c }
        };
        self.snapshot();
        for &(l, s, e) in &ranges {
            let chars: Vec<char> = self.lines[l].chars().collect();
            self.lines[l] = chars
                .iter()
                .enumerate()
                .map(|(i, &c)| if i >= s && i < e { flip(c) } else { c })
                .collect();
        }
        true
    }

    pub fn toggle_comment(&mut self, prefix: &str, suffix: &str) {
        let lines: Vec<usize> = match self.selected_lines() {
            Some((sl, last)) => (sl..=last).collect(),
            None => {
                let mut ls: Vec<usize> = self.all_carets().iter().map(|&(l, _)| l).collect();
                ls.sort();
                ls.dedup();
                ls
            }
        };
        let targets: Vec<usize> = lines.iter().copied().filter(|&l| !self.lines[l].trim().is_empty()).collect();
        if targets.is_empty() {
            return;
        }
        let commented = |line: &str| {
            let t = line.trim();
            t.starts_with(prefix) && (suffix.is_empty() || t.ends_with(suffix))
        };
        let uncomment = targets.iter().all(|&l| commented(&self.lines[l]));
        let indent = targets
            .iter()
            .map(|&l| self.lines[l].chars().take_while(|c| c.is_whitespace()).count())
            .min()
            .unwrap_or(0);
        self.snapshot();
        self.occurrences.clear();
        let mut shifts: Vec<(usize, usize, isize)> = Vec::new();
        for &l in &targets {
            let chars: Vec<char> = self.lines[l].chars().collect();
            if uncomment {
                let lead = chars.iter().take_while(|c| c.is_whitespace()).count();
                let mut body: String = chars[lead..].iter().collect();
                body = body[prefix.len()..].to_string();
                let mut removed = prefix.chars().count();
                if body.starts_with(' ') {
                    body.remove(0);
                    removed += 1;
                }
                if !suffix.is_empty() {
                    body = body.trim_end().trim_end_matches(suffix).trim_end().to_string();
                }
                self.lines[l] = chars[..lead].iter().collect::<String>() + &body;
                shifts.push((l, lead, -(removed as isize)));
            } else {
                let head: String = chars[..indent].iter().collect();
                let tail: String = chars[indent..].iter().collect();
                let tail = if suffix.is_empty() { tail } else { format!("{} {}", tail, suffix) };
                self.lines[l] = format!("{}{} {}", head, prefix, tail);
                shifts.push((l, indent, prefix.chars().count() as isize + 1));
            }
        }
        let clamp = |this: &Document, (l, c): (usize, usize)| {
            let (l, c) = match shifts.iter().find(|s| s.0 == l) {
                Some(&(_, at, d)) if c >= at => (l, (c as isize + d).max(at as isize) as usize),
                _ => (l, c),
            };
            (l, c.min(this.lines[l].chars().count()))
        };
        self.selection = self.selection.map(|(a, b)| (clamp(self, a), clamp(self, b)));
        let carets = self.all_carets().into_iter().map(|p| clamp(self, p)).collect();
        self.set_carets(carets);
    }

    pub fn click_at(&mut self, line: usize, col: usize, clicks: u8) {
        self.extra_carets.clear();
        self.occurrences.clear();
        self.selection = None;
        let line = line.min(self.lines.len() - 1);
        self.cursor = (line, col.min(self.lines[line].chars().count()));
        match clicks {
            2 => {
                if let Some((l, a, b)) = self.word_range_at(self.cursor) {
                    self.selection = Some(((l, a), (l, b)));
                    self.cursor = (l, b);
                }
            }
            n if n >= 3 => {
                let end = if line + 1 < self.lines.len() { (line + 1, 0) } else { (line, self.lines[line].chars().count()) };
                self.selection = Some(((line, 0), end));
                self.cursor = end;
            }
            _ => {}
        }
    }

    pub fn select_to(&mut self, anchor: (usize, usize), line: usize, col: usize) {
        let line = line.min(self.lines.len() - 1);
        let pos = (line, col.min(self.lines[line].chars().count()));
        self.extra_carets.clear();
        self.occurrences.clear();
        self.selection = match anchor.cmp(&pos) {
            std::cmp::Ordering::Less => Some((anchor, pos)),
            std::cmp::Ordering::Greater => Some((pos, anchor)),
            std::cmp::Ordering::Equal => None,
        };
        self.cursor = pos;
    }

    pub fn selection_anchor(&self) -> (usize, usize) {
        match self.selection {
            Some((start, end)) if self.cursor == start => end,
            Some((start, _)) => start,
            None => self.cursor,
        }
    }

    pub fn go_to(&mut self, line: usize, col: usize) {
        self.clear_extra_carets();
        let line = line.min(self.lines.len() - 1);
        self.cursor = (line, col.min(self.lines[line].chars().count()));
    }

    pub fn end(&mut self) {
        let (line, _) = self.cursor;
        let new = (line, self.lines[line].chars().count());
        self.cursor = new;
    }

    pub fn beginning_of_file(&mut self) {
        let new = (0, 0);
        self.cursor = new;
    }

    pub fn end_of_file(&mut self) {
        let last = self.lines.len() - 1;
        let new = (last, self.lines[last].chars().count());
        self.cursor = new;
    }

    pub fn rev(&self) -> u64 {
        self.rev
    }

    pub fn is_markdown(&self) -> bool {
        let name = self.path.as_deref().unwrap_or(&self.name).to_lowercase();
        name.ends_with(".md") || name.ends_with(".markdown")
    }

    pub fn is_docx(&self) -> bool {
        crate::docx::is_path(self.path.as_deref().unwrap_or(&self.name))
    }

    pub fn is_rust(&self) -> bool {
        let name = self.path.as_deref().unwrap_or(&self.name);
        let file = std::path::Path::new(name).file_name().and_then(|n| n.to_str()).unwrap_or(name);
        matches!(file.to_ascii_lowercase().rsplit_once('.'), Some((stem, "rs")) if !stem.is_empty())
    }

    pub fn toggle_rust_view(&mut self) -> bool {
        let on = !self.rust_plain.get();
        self.rust_plain.set(on);
        on
    }

    pub fn markdown_view(&self) -> Option<Rc<MdView>> {
        if !self.is_markdown() && !self.is_docx() {
            return None;
        }
        let mut cache = self.markdown.borrow_mut();
        let palette = crate::theme::pal();
        match cache.as_ref() {
            Some((rev, built_with, view)) if *rev == self.rev && *built_with == palette && view.lines.len() == self.lines.len() => {
                Some(view.clone())
            }
            _ => {
                let view = Rc::new(if self.is_docx() { crate::docx::render(&self.lines) } else { markdown::build(&self.lines) });
                *cache = Some((self.rev, palette, view.clone()));
                Some(view)
            }
        }
    }

    /// The markdown or Word preview, or the Rust type-stripped view when that
    /// beautifier is on. The caret line is still drawn from the source.
    pub fn rendered_view(&self) -> Option<Rc<MdView>> {
        if self.is_markdown() || self.is_docx() {
            return self.markdown_view();
        }
        if !self.rust_plain.get() || !self.is_rust() || crate::syntax::too_large(&self.lines) {
            return None;
        }
        let mut cache = self.rust_preview.borrow_mut();
        let palette = crate::theme::pal();
        match cache.as_ref() {
            Some((rev, built_with, view)) if *rev == self.rev && *built_with == palette && view.lines.len() == self.lines.len() => {
                Some(view.clone())
            }
            _ => {
                let view = Rc::new(crate::rust_view::build(&self.lines)?);
                *cache = Some((self.rev, palette, view.clone()));
                Some(view)
            }
        }
    }

    pub fn syntax_limited(&self) -> bool {
        self.syntax_over.get()
    }

    pub fn syntax_roles(&self, rows: std::ops::Range<usize>) -> Vec<Vec<(std::ops::Range<usize>, crate::syntax::Role)>> {
        let n = rows.end.saturating_sub(rows.start);
        let empty = vec![Vec::new(); n];
        if self.is_markdown() || self.is_docx() {
            self.syntax_over.set(false);
            return empty;
        }
        let first = self.lines.first().map(String::as_str).unwrap_or("");
        let name = self.path.as_deref().unwrap_or(&self.name);
        #[cfg(feature = "syntax")]
        if crate::syntax::dotenv_file(name) {
            if crate::syntax::too_large(&self.lines) {
                self.syntax_over.set(true);
                *self.syntax.borrow_mut() = None;
                return empty;
            }
            self.syntax_over.set(false);
            return crate::syntax::dotenv_roles(&self.lines, rows);
        }
        let Some(lang) = crate::syntax::Lang::for_file(name, first) else {
            self.syntax_over.set(false);
            return empty;
        };
        if crate::syntax::refuses(lang, &self.lines) {
            self.syntax_over.set(true);
            *self.syntax.borrow_mut() = None;
            return empty;
        }
        self.syntax_over.set(false);
        let mut slot = self.syntax.borrow_mut();
        let same_lang = slot.as_ref().is_some_and(|(_, cached, _)| *cached == lang);
        if !same_lang {
            *slot = crate::syntax::Highlighter::new(lang, &self.lines).map(|hl| (self.rev, lang, hl));
        } else if slot.as_ref().is_some_and(|(rev, _, _)| *rev != self.rev) {
            if let Some((rev, _, hl)) = slot.as_mut() {
                hl.update(&self.lines);
                *rev = self.rev;
            }
        }
        match slot.as_ref() {
            Some((_, _, hl)) => hl.roles(rows),
            None => empty,
        }
    }

    pub fn is_raw_line(&self, line: usize) -> bool {
        self.cursor.0 == line
            || self.extra_carets.iter().any(|&(l, _)| l == line)
            || self.occurrences.iter().any(|&(l, _, _)| l == line)
            || self.selection.is_some_and(|((sl, _), (el, _))| sl <= line && line <= el)
    }

    pub fn caret_line_indexes(&self) -> Vec<usize> {
        let mut lines: Vec<usize> = self.extra_carets.iter().map(|&(l, _)| l).collect();
        lines.push(self.cursor.0);
        lines.sort();
        lines.dedup();
        lines
    }

    pub fn caret_lines(&self) -> Vec<String> {
        self.caret_line_indexes().into_iter().map(|l| self.lines[l].clone()).collect()
    }

    pub fn remove_caret_lines(&mut self) {
        let lines = self.caret_line_indexes();
        self.snapshot();
        for &l in lines.iter().rev() {
            self.lines.remove(l);
        }
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.extra_carets.clear();
        self.selection = None;
        self.occurrences.clear();
        let line = lines[0].min(self.lines.len() - 1);
        self.cursor = (line, self.cursor.1.min(self.lines[line].chars().count()));
    }

    pub fn selected_text(&self) -> Option<String> {
        let ((sl, sc), (el, ec)) = self.selection?;
        if sl == el {
            return Some(self.lines[sl].chars().skip(sc).take(ec - sc).collect());
        }
        let mut out: String = self.lines[sl].chars().skip(sc).collect();
        for l in sl + 1..el {
            out.push('\n');
            out.push_str(&self.lines[l]);
        }
        out.push('\n');
        out.extend(self.lines[el].chars().take(ec));
        Some(out)
    }

    pub fn copy_text(&self) -> (String, bool) {
        if !self.occurrences.is_empty() {
            let texts: Vec<String> =
                self.occurrences.iter().map(|&(l, s, e)| self.lines[l].chars().skip(s).take(e - s).collect()).collect();
            return (texts.join("\n"), false);
        }
        match self.selected_text() {
            Some(t) => (t, false),
            None => (format!("{}\n", self.lines[self.cursor.0]), true),
        }
    }

    pub fn cut(&mut self) {
        if self.has_selection() {
            self.snapshot();
            self.delete_selection();
        } else if self.lines.len() > 1 {
            self.delete_line();
        } else if !self.lines[0].is_empty() {
            self.snapshot();
            self.lines[0].clear();
            self.cursor = (0, 0);
        }
    }

    fn insert_piece_per_caret(&mut self, pieces: &[&str]) {
        let active = self.cursor;
        let mut carets: Vec<(usize, usize)> = vec![active];
        carets.extend(self.extra_carets.iter().copied());
        carets.sort();
        let mut placed = carets.clone();
        for i in (0..carets.len()).rev() {
            let (line, col) = carets[i];
            let n = pieces[i].chars().count();
            let mut chars: Vec<char> = self.lines[line].chars().collect();
            let at = col.min(chars.len());
            chars.splice(at..at, pieces[i].chars());
            self.lines[line] = chars.into_iter().collect();
            placed[i] = (line, at + n);
            for later in placed.iter_mut().skip(i + 1).filter(|p| p.0 == line) {
                later.1 += n;
            }
        }
        let index = carets.iter().position(|&p| p == active).unwrap_or(0);
        self.cursor = placed[index];
        self.extra_carets = placed.into_iter().enumerate().filter(|(i, _)| *i != index).map(|(_, p)| p).collect();
    }

    pub fn insert_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.snapshot();
        self.delete_selection();
        if !text.contains('\n') {
            for c in text.chars() {
                self.insert_char_at_carets(c);
            }
            return;
        }
        let pieces: Vec<&str> = text.split('\n').collect();
        if !self.extra_carets.is_empty() && pieces.len() == self.extra_carets.len() + 1 && !text.ends_with('\n') {
            return self.insert_piece_per_caret(&pieces);
        }
        let active = self.cursor;
        let mut carets: Vec<(usize, usize)> = vec![active];
        carets.extend(self.extra_carets.iter().copied());
        carets.sort();
        let last = pieces.len() - 1;
        let end_col = pieces[last].chars().count();
        let mut placed = carets.clone();
        for i in (0..carets.len()).rev() {
            let (line, col) = carets[i];
            let chars: Vec<char> = self.lines[line].chars().collect();
            let at = col.min(chars.len());
            let mut new_lines: Vec<String> = pieces.iter().map(|p| p.to_string()).collect();
            new_lines[0].insert_str(0, &chars[..at].iter().collect::<String>());
            new_lines[last].extend(&chars[at..]);
            self.lines.splice(line..=line, new_lines);
            placed[i] = (line + last, end_col);
            for later in placed.iter_mut().skip(i + 1) {
                later.0 += last;
            }
        }
        let index = carets.iter().position(|&p| p == active).unwrap_or(0);
        self.cursor = placed[index];
        self.extra_carets = placed.into_iter().enumerate().filter(|(i, _)| *i != index).map(|(_, p)| p).collect();
    }

    pub fn insert_lines_above(&mut self, text: &str) {
        let body = text.strip_suffix('\n').unwrap_or(text);
        self.snapshot();
        self.selection = None;
        self.occurrences.clear();
        let new: Vec<String> = body.split('\n').map(|l| l.to_string()).collect();
        let n = new.len();
        let lines = self.caret_line_indexes();
        for &line in lines.iter().rev() {
            self.lines.splice(line..line, new.iter().cloned());
        }
        let shift = |l: usize| l + n * lines.iter().filter(|&&x| x <= l).count();
        self.cursor.0 = shift(self.cursor.0);
        for caret in &mut self.extra_carets {
            caret.0 = shift(caret.0);
        }
    }

    pub fn join_lines(&mut self) {
        let line = self.cursor.0;
        if line + 1 >= self.lines.len() {
            return;
        }
        self.snapshot();
        self.selection = None;
        self.occurrences.clear();
        let col = self.lines[line].chars().count();
        let next = self.lines.remove(line + 1);
        self.lines[line].push_str(&next);
        self.cursor = (line, col);
        for e in &mut self.extra_carets {
            if e.0 == line + 1 {
                *e = (line, e.1 + col);
            } else if e.0 > line + 1 {
                e.0 -= 1;
            }
        }
        let cursor = self.cursor;
        self.remove_extra(cursor);
    }

    pub fn reformat(&mut self) -> bool {
        let mut body: Vec<&str> = self.lines.iter().map(|l| l.as_str()).collect();
        let final_newline = body.len() > 1 && body[body.len() - 1].is_empty();
        if final_newline {
            body.pop();
        }
        let mut out: Vec<String> = Vec::with_capacity(body.len() + 1);
        let mut new_index: Vec<usize> = Vec::with_capacity(self.lines.len());
        let mut blanks = 0;
        for l in body {
            let t = l.trim_end();
            if t.is_empty() {
                blanks += 1;
            } else {
                blanks = 0;
            }
            if blanks <= 2 {
                out.push(t.to_string());
            }
            new_index.push(out.len() - 1);
        }
        if final_newline {
            out.push(String::new());
            new_index.push(out.len() - 1);
        }
        if out == self.lines {
            return false;
        }
        self.snapshot();
        self.lines = out;
        self.selection = None;
        self.occurrences.clear();
        self.extra_carets.clear();
        let line = new_index[self.cursor.0];
        self.cursor = (line, self.cursor.1.min(self.lines[line].chars().count()));
        true
    }

    pub fn occurrence_target(&self) -> Option<String> {
        if let Some(((sl, _), (el, _))) = self.selection {
            return if sl == el { self.selected_text() } else { None };
        }
        let (line, col) = self.cursor;
        let chars: Vec<char> = self.lines[line].chars().collect();
        let is_word = |c: char| c.is_alphanumeric() || c == '_';
        let mut s = col;
        while s > 0 && is_word(chars[s - 1]) {
            s -= 1;
        }
        let mut e = col;
        while e < chars.len() && is_word(chars[e]) {
            e += 1;
        }
        if s == e {
            None
        } else {
            Some(chars[s..e].iter().collect())
        }
    }

    pub fn set_occurrences(&mut self, ranges: Vec<(usize, usize, usize)>) {
        if ranges.is_empty() {
            return;
        }
        let (cl, cc) = self.cursor;
        let active = ranges
            .iter()
            .position(|&(l, s, e)| l == cl && s <= cc && cc <= e)
            .unwrap_or(0);
        self.selection = None;
        self.cursor = (ranges[active].0, ranges[active].2);
        self.extra_carets = ranges
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != active)
            .map(|(_, &(l, _, e))| (l, e))
            .collect();
        self.occurrences = ranges;
    }

    pub fn replace_ranges(&mut self, ranges: &[(usize, usize, usize)], text: &str) {
        if ranges.is_empty() {
            return;
        }
        self.snapshot();
        self.selection = None;
        self.occurrences.clear();
        self.extra_carets.clear();
        let mut sorted = ranges.to_vec();
        sorted.sort();
        for &(l, s, e) in sorted.iter().rev() {
            let chars: Vec<char> = self.lines[l].chars().collect();
            let mut new: String = chars[..s].iter().collect();
            new.push_str(text);
            new.extend(chars[e..].iter());
            self.lines[l] = new;
        }
        let (l, _) = self.cursor;
        let line = l.min(self.lines.len() - 1);
        self.cursor = (line, self.cursor.1.min(self.lines[line].chars().count()));
    }

    pub fn full_content(&self) -> String {
        self.lines.join("\n")
    }

    /// Balance a docx line after an edit. A keystroke on a table, picture, or other frozen
    /// block is rolled back, so the file stays clean.
    pub fn settle_docx(&mut self) -> Option<String> {
        if self.docx.is_none() {
            return None;
        }
        let before = self.lines.clone();
        let mut session = self.docx.take().unwrap();
        let note = session.settle(&mut self.lines);
        self.docx = Some(session);
        self.remap_docx_carets(&before);
        if note.as_deref().is_some_and(|n| n.contains("unchanged")) {
            if let Some(snap) = self.undo.last() {
                if snap.lines == self.lines {
                    let snap = self.undo.pop().unwrap();
                    if let Some(session) = &mut self.docx {
                        let (rev, dirty) = session.rollback_point();
                        self.rev = rev;
                        self.dirty = dirty;
                        if let Some(anchors) = snap.docx_anchors {
                            session.restore(anchors, &self.lines);
                        }
                    }
                    self.cursor = snap.cursor;
                    self.extra_carets = snap.extra_carets;
                    self.selection = snap.selection;
                    self.occurrences = snap.occurrences;
                }
            }
        }
        note
    }

    fn remap_docx_carets(&mut self, before: &[String]) {
        if self.docx.is_none() {
            return;
        }
        let cursor = {
            let (line, col) = self.cursor;
            (line, self.mapped_col(before, line, col))
        };
        let extra: Vec<(usize, usize)> = self.extra_carets.iter().map(|&(line, col)| (line, self.mapped_col(before, line, col))).collect();
        let selection = self.selection.map(|(start, end)| {
            ((start.0, self.mapped_col(before, start.0, start.1)), (end.0, self.mapped_col(before, end.0, end.1)))
        });
        let occurrences: Vec<(usize, usize, usize)> = self
            .occurrences
            .iter()
            .map(|&(line, start, end)| (line, self.mapped_col(before, line, start), self.mapped_col(before, line, end)))
            .collect();
        self.cursor = cursor;
        self.extra_carets = extra;
        self.selection = selection;
        self.occurrences = occurrences;
    }

    fn mapped_col(&self, before: &[String], line: usize, col: usize) -> usize {
        let Some(old) = before.get(line) else {
            return col;
        };
        let Some(new) = self.lines.get(line) else {
            return col;
        };
        if old == new {
            return col.min(new.chars().count());
        }
        crate::docx::column_at_content(new, crate::docx::content_index(old, col)).min(new.chars().count())
    }

    fn clamp_carets(&mut self) {
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        let last = self.lines.len() - 1;
        self.cursor.0 = self.cursor.0.min(last);
        self.cursor.1 = self.cursor.1.min(self.lines[self.cursor.0].chars().count());
        self.extra_carets.retain(|c| c.0 < self.lines.len());
        for caret in &mut self.extra_carets {
            caret.1 = caret.1.min(self.lines[caret.0].chars().count());
        }
    }

    /// Write `path`. A `.docx` path saves the package; anything else saves the lines as text.
    pub fn write_to(&mut self, path: &str) -> std::io::Result<Option<String>> {
        let note = if !crate::docx::is_path(path) {
            std::fs::write(path, self.full_content().as_bytes())?;
            self.docx = None;
            None
        } else {
            if self.docx.is_none() {
                let (lines, session) = crate::docx::from_lines(&self.lines)?;
                self.lines = lines;
                self.docx = Some(session);
                self.clamp_carets();
            }
            let settled = self.settle_docx();
            let (bytes, note) = self.docx.as_mut().unwrap().save(&mut self.lines)?;
            self.clamp_carets();
            std::fs::write(path, &bytes)?;
            note.or(settled)
        };
        self.disk = Some(stamp_file(path));
        Ok(note)
    }

    pub(crate) fn note_disk(&mut self) {
        if let Some(path) = self.path.clone() {
            self.disk = Some(stamp_file(&path));
        }
    }

    pub(crate) fn disk_change(&self) -> Option<DiskKind> {
        let path = self.path.as_deref()?;
        let was = self.disk?;
        let now = stamp_file(path);
        if now == was {
            return None;
        }
        Some(match (was, now) {
            (DiskStamp::Present { .. }, DiskStamp::Absent) => DiskKind::Deleted,
            _ => DiskKind::Changed,
        })
    }

    /// Replace the buffer from the file at `path`. The open text is one undo step.
    pub fn reload_from_disk(&mut self) -> std::io::Result<()> {
        let path = self.path.clone().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no path"))?;
        let fresh = Self::from_file(&path)?;
        self.undo.push(self.capture());
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.lines = fresh.lines;
        self.docx = fresh.docx;
        self.selection = None;
        self.occurrences.clear();
        self.extra_carets.clear();
        self.expansions.clear();
        self.expanded_to = None;
        self.rev += 1;
        self.dirty = false;
        self.clamp_carets();
        self.disk = fresh.disk;
        Ok(())
    }
}

fn stamp_file(path: &str) -> DiskStamp {
    match std::fs::metadata(path) {
        Ok(meta) => DiskStamp::Present { modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), len: meta.len() },
        Err(_) => DiskStamp::Absent,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_with(text: &str) -> Document {
        let mut d = Document::new("test");
        d.lines = text.split('\n').map(|s| s.to_string()).collect();
        d.cursor = (0, 0);
        d
    }

    #[test]
    fn insert_collapses_selection() {
        let mut d = doc_with("hello");
        d.selection = Some(((0, 1), (0, 3)));
        d.cursor = (0, 3);
        d.insert_char('X');
        assert_eq!(d.lines[0], "hXlo");
        assert_eq!(d.cursor, (0, 2));
        assert_eq!(d.selection, None);
    }

    #[test]
    fn backspace_joins_lines() {
        let mut d = doc_with("ab\ncd");
        d.cursor = (1, 0);
        d.backspace();
        assert_eq!(d.lines.len(), 1);
        assert_eq!(d.lines[0], "abcd");
        assert_eq!(d.cursor, (0, 2));
    }

    #[test]
    fn backspace_deletes_selection() {
        let mut d = doc_with("hello");
        d.selection = Some(((0, 1), (0, 4)));
        d.cursor = (0, 4);
        d.backspace();
        assert_eq!(d.lines[0], "ho");
        assert_eq!(d.cursor, (0, 1));
        assert_eq!(d.selection, None);
    }

    #[test]
    fn full_content_joins_lines_as_saved() {
        let mut d = Document::new("t");
        d.lines = vec!["a".into(), "b".into()];
        assert_eq!(d.full_content(), "a\nb");
        let mut d2 = Document::new("t");
        d2.lines = vec!["a".into(), "b".into(), String::new()];
        assert_eq!(d2.full_content(), "a\nb\n");
        assert_eq!(Document::new("t").full_content(), "");
    }

    #[test]
    fn newline_splits_line() {
        let mut d = doc_with("hello");
        d.cursor = (0, 2);
        d.newline();
        assert_eq!(d.lines, vec!["he", "llo"]);
        assert_eq!(d.cursor, (1, 0));
    }

    #[test]
    fn word_deletes() {
        type Delete = fn(&mut Document);
        let cases: &[(&str, usize, Delete, &str)] = &[
            ("hello world", 0, Document::delete_word_forward, " world"),
            (" hello world", 0, Document::delete_word_forward, " world"),
            ("hello world", 11, Document::delete_word_backward, "hello "),
            ("hello world", 10, Document::delete_word_backward, "hello d"),
        ];
        for &(text, col, delete, expected) in cases {
            let mut d = doc_with(text);
            d.cursor = (0, col);
            delete(&mut d);
            assert_eq!(d.lines[0], expected, "{:?} from col {}", text, col);
        }
    }

    #[test]
    fn shift_move_selects_from_anchor_and_crosses_it() {
        let mut d = doc_with("hello");
        d.cursor = (0, 2);
        d.move_by(Document::move_right, true);
        d.move_by(Document::move_right, true);
        assert_eq!(d.selection, Some(((0, 2), (0, 4))));
        d.move_by(Document::move_left, true);
        d.move_by(Document::move_left, true);
        assert_eq!(d.selection, None, "back at the anchor selects nothing");
        d.move_by(Document::move_left, true);
        assert_eq!(d.selection, Some(((0, 1), (0, 2))));
        assert_eq!(d.cursor, (0, 1));
        d.move_by(Document::end, true);
        assert_eq!(d.selection, Some(((0, 2), (0, 5))), "shift+end from the anchor");
        d.move_by(Document::home, true);
        assert_eq!((d.selection, d.cursor), (Some(((0, 0), (0, 2))), (0, 0)), "shift+home crosses back over it");
    }

    #[test]
    fn plain_move_clears_selection_and_moves_every_caret() {
        let mut d = doc_with("abc\nabc\nabc");
        d.cursor = (0, 1);
        d.extra_carets = vec![(1, 1), (2, 1)];
        d.selection = Some(((0, 0), (0, 1)));
        d.occurrences = vec![(1, 0, 1), (2, 0, 1)];
        d.move_by(Document::move_right, false);
        assert_eq!((d.selection, d.occurrences.clone()), (None, vec![]), "plain movement deselects every caret");
        assert_eq!(d.cursor, (0, 2));
        assert_eq!(d.extra_carets, vec![(1, 2), (2, 2)]);
        d.move_by(Document::home, false);
        d.move_by(Document::move_up, false);
        assert_eq!(d.cursor, (0, 0));
        assert_eq!(d.extra_carets, vec![(1, 0)], "carets that land together merge");
    }

    #[test]
    fn undo_redo_round_trip() {
        let mut d = doc_with("hello");
        d.cursor = (0, 5);
        d.insert_char('!');
        assert_eq!(d.lines[0], "hello!");
        d.undo();
        assert_eq!(d.lines[0], "hello");
        assert_eq!(d.cursor, (0, 5));
        d.redo();
        assert_eq!(d.lines[0], "hello!");
        assert_eq!(d.cursor, (0, 6));
    }

    #[test]
    fn undo_restores_selection() {
        let mut d = doc_with("hello");
        d.selection = Some(((0, 1), (0, 3)));
        d.cursor = (0, 3);
        d.insert_char('X');
        d.undo();
        assert_eq!(d.lines[0], "hello");
        assert_eq!(d.selection, Some(((0, 1), (0, 3))));
        assert_eq!(d.cursor, (0, 3));
    }

    #[test]
    fn undo_cap_at_100() {
        let mut d = doc_with("");
        for _ in 0..105 {
            d.insert_char('a');
        }
        assert_eq!(d.undo.len(), 100);
        d.undo();
        assert_eq!(d.lines[0], "a".repeat(104));
    }

    #[test]
    fn redo_cleared_on_new_edit() {
        let mut d = doc_with("a");
        d.insert_char('b');
        d.undo();
        d.insert_char('c');
        d.redo();
        assert_eq!(d.lines[0], "ca", "redo after a new edit does nothing");
    }

    #[test]
    fn insert_multibyte_in_middle() {
        let mut d = doc_with("ab");
        d.cursor = (0, 1);
        d.insert_char('中');
        assert_eq!(d.lines[0], "a中b");
        assert_eq!(d.cursor, (0, 2));
        assert_eq!(d.lines[0].chars().count(), 3);
    }

    #[test]
    fn backspace_multibyte_deletes_whole_char() {
        let mut d = doc_with("a中b");
        d.cursor = (0, 2);
        d.backspace();
        assert_eq!(d.lines[0], "ab");
        assert_eq!(d.cursor, (0, 1));
    }

    #[test]
    fn delete_char_forward_multibyte() {
        let mut d = doc_with("a中b");
        d.cursor = (0, 2);
        d.delete_char_forward();
        assert_eq!(d.lines[0], "a中");
    }

    #[test]
    fn movement_primitives() {
        type Move = fn(&mut Document);
        let cases: &[(&str, (usize, usize), Move, (usize, usize))] = &[
            ("hello", (0, 3), Document::move_left, (0, 2)),
            ("", (0, 0), Document::move_left, (0, 0)),
            ("ab\ncd", (1, 0), Document::move_left, (0, 2)),
            ("hello", (0, 3), Document::move_right, (0, 4)),
            ("", (0, 0), Document::move_right, (0, 0)),
            ("ab\ncd", (0, 2), Document::move_right, (1, 0)),
            ("a中b", (0, 1), Document::move_right, (0, 2)),
            ("a中b", (0, 3), Document::move_right, (0, 3)),
            ("ab\ncdef", (1, 4), Document::move_up, (0, 2)),
            ("ab\ncd", (0, 0), Document::move_up, (0, 0)),
            ("cdef\nab", (0, 3), Document::move_down, (1, 2)),
            ("ab\ncd", (1, 2), Document::move_down, (1, 2)),
            ("hello", (0, 3), Document::home, (0, 0)),
            ("  hi", (0, 4), Document::home, (0, 2)),
            ("  hi", (0, 2), Document::home, (0, 0)),
            ("  hi", (0, 0), Document::home, (0, 2)),
            ("  hi", (0, 4), Document::line_start, (0, 0)),
            ("hello", (0, 2), Document::end, (0, 5)),
            ("a中b", (0, 0), Document::end, (0, 3)),
            ("ab\ncd", (1, 2), Document::beginning_of_file, (0, 0)),
            ("ab\ncd", (0, 0), Document::end_of_file, (1, 2)),
            ("hello world", (0, 0), Document::word_right, (0, 5)),
            ("hello world", (0, 5), Document::word_right, (0, 11)),
            ("a.b", (0, 0), Document::word_right, (0, 1)),
            ("foo   bar", (0, 3), Document::word_right, (0, 9)),
            ("ab\ncd", (0, 2), Document::word_right, (1, 0)),
            ("hello world", (0, 11), Document::word_left, (0, 6)),
            ("hello world", (0, 6), Document::word_left, (0, 0)),
            ("a.b", (0, 3), Document::word_left, (0, 2)),
            ("ab\ncd", (1, 0), Document::word_left, (0, 2)),
            ("é tè", (0, 0), Document::word_right, (0, 1)),
        ];
        for (i, &(text, start, movement, expected)) in cases.iter().enumerate() {
            let mut d = doc_with(text);
            d.cursor = start;
            movement(&mut d);
            assert_eq!(d.cursor, expected, "case {}: {:?} from {:?}", i, text, start);
        }
    }

    #[test]
    fn newline_multibyte() {
        let mut d = doc_with("a中b");
        d.cursor = (0, 2);
        d.newline();
        assert_eq!(d.lines[0], "a中");
        assert_eq!(d.lines[1], "b");
        assert_eq!(d.cursor, (1, 0));
    }

    #[test]
    fn multi_caret_insert_on_two_lines() {
        let mut d = doc_with("ab\ncd");
        d.cursor = (0, 1);
        d.extra_carets.push((1, 1));
        d.insert_char('X');
        assert_eq!(d.lines[0], "aXb");
        assert_eq!(d.lines[1], "cXd");
    }

    #[test]
    fn multi_caret_backspace() {
        let mut d = doc_with("ab\ncd");
        d.cursor = (0, 2);
        d.extra_carets.push((1, 2));
        d.backspace();
        assert_eq!(d.lines[0], "a");
        assert_eq!(d.lines[1], "c");
        assert_eq!(d.cursor, (0, 1));
        assert_eq!(d.extra_carets, vec![(1, 1)]);
    }

    #[test]
    fn multi_caret_delete_char_forward() {
        let mut d = doc_with("ab\ncd");
        d.cursor = (0, 1);
        d.extra_carets.push((1, 1));
        d.delete_char_forward();
        assert_eq!(d.lines[0], "a");
        assert_eq!(d.lines[1], "c");
        assert_eq!(d.cursor, (0, 1));
        assert_eq!(d.extra_carets, vec![(1, 1)]);
    }

    #[test]
    fn multi_caret_delete_word_forward() {
        let mut d = doc_with("hello\nworld");
        d.cursor = (0, 1);
        d.extra_carets.push((1, 1));
        d.delete_word_forward();
        assert_eq!(d.lines[0], "h");
        assert_eq!(d.lines[1], "w");
    }

    #[test]
    fn add_caret_up_leaves_caret_behind() {
        let mut d = doc_with("aa\nbb\ncc");
        d.cursor = (1, 0);
        d.add_caret_up();
        assert_eq!(d.cursor, (0, 0));
        assert_eq!(d.extra_carets, vec![(1, 0)]);
    }

    #[test]
    fn add_caret_down_accumulates() {
        let mut d = doc_with("aa\nbb\ncc");
        d.cursor = (0, 0);
        d.add_caret_down();
        assert_eq!(d.cursor, (1, 0));
        assert_eq!(d.extra_carets, vec![(0, 0)]);
        d.add_caret_down();
        assert_eq!(d.cursor, (2, 0));
        assert_eq!(d.extra_carets, vec![(0, 0), (1, 0)]);
    }

    #[test]
    fn wrapped_lines_move_by_visual_rows_and_home_and_end_stay_on_the_source_line() {
        let mut d = doc_with("0123456789abcdefghij\nab\n0123456789abcdefghij");
        d.wrap_width.set(10);
        d.cursor = (0, 15);
        d.move_lines(3);
        assert_eq!(d.cursor, (2, 5), "one move keeps column 5 across the short line");
        d.cursor = (0, 15);
        d.move_lines(1);
        assert_eq!(d.cursor, (0, 20), "the first row down lands on the end of the line");
        d.move_lines(1);
        assert_eq!(d.cursor, (1, 0), "the next move starts from column 0, where that caret landed");
        d.cursor = (0, 15);
        d.home();
        assert_eq!(d.cursor, (0, 0), "Home is the start of the source line");
        d.cursor = (0, 15);
        d.end();
        assert_eq!(d.cursor, (0, 20), "End is the end of the source line");
        d.selection = Some(((0, 18), (1, 2)));
        assert_eq!(d.selected_text().as_deref(), Some("ij\nab"));
    }

    #[test]
    fn several_carets_select_by_visual_rows_until_one_crosses_a_line() {
        let mut d = doc_with("0123456789abcdefghij\nshort");
        d.wrap_width.set(10);
        d.cursor = (0, 3);
        d.add_caret_down();
        assert_eq!(d.cursor, (0, 13));
        assert_eq!(d.extra_carets, vec![(0, 3)], "the caret left behind stays on the previous wrap row");
        d.extra_carets.clear();
        d.cursor = (0, 3);
        d.move_by(Document::move_down, true);
        assert_eq!(d.cursor, (0, 13));
        assert_eq!(d.selection, Some(((0, 3), (0, 13))), "shift+down inside a wrap stays one selection");
        d.selection = None;
        d.cursor = (0, 3);
        d.extra_carets = vec![(0, 14)];
        d.move_by(Document::move_down, true);
        assert_eq!(d.cursor, (0, 13));
        assert_eq!(d.extra_carets, vec![(0, 20)]);
        assert_eq!(d.occurrences, vec![(0, 3, 13), (0, 14, 20)], "both carets stay on the line, so both ranges count");
        d.move_by(Document::move_down, true);
        assert_eq!(d.cursor, (0, 20));
        assert_eq!(d.extra_carets, vec![(1, 0)], "the caret that was at the end steps onto the next line");
        assert_eq!(d.occurrences, vec![(0, 3, 20)], "a caret that crosses a source line keeps no range");
    }

    #[test]
    fn add_caret_at_accumulates_and_toggles() {
        let mut d = doc_with("aa\nbb");
        d.cursor = (0, 0);
        d.add_caret_at(1, 1);
        assert_eq!(d.cursor, (1, 1));
        assert_eq!(d.extra_carets, vec![(0, 0)]);
        d.add_caret_at(0, 0); // remove the secondary caret
        assert_eq!(d.cursor, (1, 1));
        assert_eq!(d.extra_carets, vec![]);
        d.add_caret_at(1, 1); // clicking active caret: no-op
        assert_eq!(d.cursor, (1, 1));
        assert_eq!(d.extra_carets, vec![]);
    }

    #[test]
    fn duplicate_line_inserts_copy_below() {
        let mut d = doc_with("one\ntwo");
        d.cursor = (0, 1);
        d.duplicate_line();
        assert_eq!(d.lines, vec!["one".to_string(), "one".to_string(), "two".to_string()]);
        assert_eq!(d.cursor, (1, 1));
    }

    #[test]
    fn duplicate_line_bumps_lower_carets() {
        let mut d = doc_with("one\ntwo\nthree");
        d.cursor = (0, 1);
        d.extra_carets.push((2, 2));
        d.duplicate_line();
        assert_eq!(d.cursor, (1, 1));
        assert_eq!(d.extra_carets, vec![(3, 2)]);
    }

    #[test]
    fn delete_line_removes() {
        let mut d = doc_with("one\ntwo\nthree");
        d.cursor = (0, 1);
        d.delete_line();
        assert_eq!(d.lines, vec!["two".to_string(), "three".to_string()]);
        assert_eq!(d.cursor, (0, 1));
    }

    #[test]
    fn delete_line_keeps_single_line() {
        let mut d = doc_with("only");
        d.cursor = (0, 2);
        d.delete_line();
        assert_eq!(d.lines, vec!["only".to_string()]);
        assert_eq!(d.cursor, (0, 2));
    }

    #[test]
    fn move_line_up_swaps() {
        let mut d = doc_with("a\nb\nc");
        d.cursor = (2, 0);
        d.move_line_up();
        assert_eq!(d.lines, vec!["a".to_string(), "c".to_string(), "b".to_string()]);
        assert_eq!(d.cursor, (1, 0));
    }

    #[test]
    fn move_line_down_swaps() {
        let mut d = doc_with("a\nb\nc");
        d.cursor = (0, 0);
        d.move_line_down();
        assert_eq!(d.lines, vec!["b".to_string(), "a".to_string(), "c".to_string()]);
        assert_eq!(d.cursor, (1, 0));
    }

    #[test]
    fn clear_extra_carets_keeps_active_cursor_and_text() {
        let mut d = doc_with("a\nb\nc");
        d.cursor = (1, 1);
        d.extra_carets.push((0, 1));
        d.extra_carets.push((2, 2));
        assert!(!d.dirty);
        d.clear_extra_carets();
        assert_eq!(d.extra_carets, vec![]);
        assert_eq!(d.cursor, (1, 1));
        assert_eq!(d.lines, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        assert!(!d.dirty);
    }

    #[test]
    fn insert_text_multi_line_splits_around_caret() {
        let mut d = doc_with("abXcd");
        d.cursor = (0, 2);
        d.selection = Some(((0, 2), (0, 3)));
        d.insert_text("1\n22\n333");
        assert_eq!(d.lines, vec!["ab1", "22", "333cd"]);
        assert_eq!(d.cursor, (2, 3));
        d.undo();
        assert_eq!(d.lines, vec!["abXcd"]);
    }

    #[test]
    fn insert_text_single_line_goes_to_every_caret() {
        let mut d = doc_with("a\nb");
        d.cursor = (0, 1);
        d.extra_carets = vec![(1, 1)];
        d.insert_text("xy");
        assert_eq!(d.lines, vec!["axy", "bxy"]);
        assert_eq!(d.cursor, (0, 3));
        assert_eq!(d.extra_carets, vec![(1, 3)]);
    }

    #[test]
    fn copy_and_cut_take_the_selection_or_the_whole_line() {
        let mut d = doc_with("one\ntwo\nthree");
        d.cursor = (1, 1);
        assert_eq!(d.copy_text(), ("two\n".to_string(), true));
        d.selection = Some(((0, 1), (2, 2)));
        assert_eq!(d.copy_text(), ("ne\ntwo\nth".to_string(), false));
        d.cut();
        assert_eq!(d.lines, vec!["oree"]);
        d.cut();
        assert_eq!(d.lines, vec![""], "cutting the only line empties it");
    }

    #[test]
    fn insert_lines_above_keeps_caret_on_its_text() {
        let mut d = doc_with("a\nb");
        d.cursor = (1, 1);
        d.insert_lines_above("x\ny\n");
        assert_eq!(d.lines, vec!["a", "x", "y", "b"]);
        assert_eq!(d.cursor, (3, 1));
    }

    #[test]
    fn join_lines_keeps_whitespace_and_undoes() {
        let mut d = doc_with("foo \n  bar\nbaz");
        d.cursor = (0, 1);
        d.extra_carets = vec![(1, 2), (2, 0)];
        d.join_lines();
        assert_eq!(d.lines, vec!["foo   bar", "baz"]);
        assert_eq!(d.cursor, (0, 4));
        assert_eq!(d.extra_carets, vec![(0, 6), (1, 0)]);
        d.cursor = (1, 0);
        d.extra_carets.clear();
        d.join_lines();
        assert_eq!(d.lines, vec!["foo   bar", "baz"], "last line has nothing to join");
        d.undo();
        assert_eq!(d.lines, vec!["foo ", "  bar", "baz"]);
    }

    #[test]
    fn reformat_trims_and_collapses_blank_lines() {
        let mut d = doc_with("a  \n\n \n\t\n\nb\t\n");
        d.cursor = (5, 2);
        assert!(d.reformat());
        assert_eq!(d.full_content(), "a\n\n\nb\n");
        assert_eq!(d.cursor, (3, 1));
        assert!(!d.reformat(), "already clean");
        d.undo();
        assert_eq!(d.full_content(), "a  \n\n \n\t\n\nb\t\n", "only one undo step was recorded");
    }

    #[test]
    fn typing_over_occurrences_replaces_them_all() {
        let mut d = doc_with("foo bar foo\nfoo");
        d.cursor = (0, 9);
        assert_eq!(d.occurrence_target().as_deref(), Some("foo"));
        d.set_occurrences(vec![(0, 0, 3), (0, 8, 11), (1, 0, 3)]);
        assert_eq!(d.cursor, (0, 11));
        d.insert_char('x');
        d.insert_char('y');
        assert_eq!(d.lines, vec!["xy bar xy", "xy"]);
        assert!(d.occurrences.is_empty());
        d.undo();
        d.undo();
        assert_eq!(d.lines, vec!["foo bar foo", "foo"]);
        assert_eq!(d.occurrences.len(), 3);
        d.clear_extra_carets();
        assert!(d.occurrences.is_empty() && d.extra_carets.is_empty());
        assert_eq!(d.cursor, (0, 11));
    }

    #[test]
    fn caret_lines_are_unique_and_in_document_order() {
        let mut d = doc_with("ls\npwd\necho hi");
        d.cursor = (2, 1);
        d.extra_carets = vec![(0, 0), (2, 3)];
        assert_eq!(d.caret_lines(), vec!["ls", "echo hi"]);
        d.extra_carets.clear();
        assert_eq!(d.caret_lines(), vec!["echo hi"]);
        d.cursor = (1, 0);
        d.extra_carets = vec![(2, 0), (0, 1)];
        assert_eq!(d.caret_lines(), vec!["ls", "pwd", "echo hi"], "top to bottom whatever the caret order");
    }

    #[test]
    fn shift_moves_select_from_each_carets_anchor() {
        type Move = fn(&mut Document);
        struct Case<'a> {
            name: &'static str,
            text: &'static str,
            carets: &'static [(usize, usize)],
            before: &'static [(usize, usize, usize)],
            moves: &'a [Move],
            selected: &'static [(usize, usize, usize)],
            after: &'static [(usize, usize)],
        }
        let (end, home, down, left): (Move, Move, Move, Move) = (Document::end, Document::home, Document::move_down, Document::move_left);
        let (word_right, word_left, file_start, file_end): (Move, Move, Move, Move) = (Document::word_right, Document::word_left, Document::beginning_of_file, Document::end_of_file);
        let cases = [
            Case { name: "shift+end at every caret", text: "abc def\n  ghi jkl", carets: &[(0, 3), (1, 5)], before: &[], moves: &[end], selected: &[(0, 3, 7), (1, 5, 9)], after: &[(0, 7), (1, 9)] },
            Case { name: "shift+home goes back past each anchor to the first non-blank", text: "abc def\n  ghi jkl", carets: &[(0, 3), (1, 5)], before: &[], moves: &[end, home], selected: &[(0, 0, 3), (1, 2, 5)], after: &[(0, 0), (1, 2)] },
            Case { name: "shift+home twice reaches column 0", text: "  ab\n  cd", carets: &[(0, 4), (1, 4)], before: &[], moves: &[home, home], selected: &[(0, 0, 4), (1, 0, 4)], after: &[(0, 0), (1, 0)] },
            Case { name: "shift+end then shift+home shrinks back to nothing", text: "ab\ncd", carets: &[(0, 0), (1, 0)], before: &[], moves: &[end, home], selected: &[], after: &[(0, 0), (1, 0)] },
            Case { name: "shift+left back onto the anchor deselects", text: "abc\nabc", carets: &[(0, 1), (1, 1)], before: &[], moves: &[end, left, left], selected: &[], after: &[(0, 1), (1, 1)] },
            Case { name: "carets that meet share one selection", text: "abcdef", carets: &[(0, 1), (0, 3)], before: &[], moves: &[end], selected: &[(0, 1, 6)], after: &[(0, 6)] },
            Case { name: "a caret on an empty line selects nothing", text: "ab\n\ncd", carets: &[(0, 0), (1, 0), (2, 0)], before: &[], moves: &[end], selected: &[(0, 0, 2), (2, 0, 2)], after: &[(0, 2), (1, 0), (2, 2)] },
            Case { name: "shift+left over a line start moves the carets but keeps no selection", text: "ab\ncd\nef", carets: &[(1, 0), (2, 0)], before: &[], moves: &[left], selected: &[], after: &[(0, 2), (1, 2)] },
            Case { name: "shift+down moves the carets but keeps no selection", text: "abcd\nab\nabcd", carets: &[(0, 3), (1, 1)], before: &[], moves: &[down], selected: &[], after: &[(1, 2), (2, 1)] },
            Case { name: "a single caret keeps extending its occurrence", text: "one two", carets: &[(0, 3)], before: &[(0, 0, 3)], moves: &[end], selected: &[(0, 0, 7)], after: &[(0, 7)] },
            Case { name: "shift+word-right selects each word", text: "abc def\nxyz uvw", carets: &[(0, 0), (1, 0)], before: &[], moves: &[word_right], selected: &[(0, 0, 3), (1, 0, 3)], after: &[(0, 3), (1, 3)] },
            Case { name: "shift+word-left selects back to the word start", text: "abc def\nxyz uvw", carets: &[(0, 3), (1, 3)], before: &[], moves: &[word_left], selected: &[(0, 0, 3), (1, 0, 3)], after: &[(0, 0), (1, 0)] },
            Case { name: "shift+file-start selects only a caret already on the first line", text: "abc def\nxyz uvw", carets: &[(0, 4), (1, 3)], before: &[], moves: &[file_start], selected: &[(0, 0, 4)], after: &[(0, 0)] },
            Case { name: "shift+file-end selects only a caret already on the last line", text: "abc\ndef", carets: &[(1, 1), (0, 2)], before: &[], moves: &[file_end], selected: &[(1, 1, 3)], after: &[(1, 3)] },
        ];
        for case in cases {
            let mut d = doc_with(case.text);
            d.cursor = case.carets[0];
            d.extra_carets = case.carets[1..].to_vec();
            d.occurrences = case.before.to_vec();
            for movement in case.moves {
                d.move_by(movement, true);
            }
            let mut carets = vec![d.cursor];
            carets.extend(d.extra_carets.iter().copied());
            carets.sort();
            assert_eq!((d.occurrences.as_slice(), carets.as_slice()), (case.selected, case.after), "{}", case.name);
            assert_eq!(d.selection, None, "{}: several carets use per-caret ranges", case.name);
        }

        let mut d = doc_with("abc\ndef");
        d.cursor = (0, 1);
        d.move_by(Document::end, true);
        d.add_caret_down();
        d.move_by(Document::home, true);
        assert_eq!(d.occurrences, vec![(0, 0, 1), (1, 0, 3)], "a selection made before adding a caret keeps its anchor");
    }

    #[cfg(feature = "lang-rust")]
    #[test]
    fn syntax_colours_follow_typing_paste_undo_and_redo() {
        use crate::syntax::Role;
        let kw = |d: &Document, line: usize, col: usize| d.syntax_roles(line..line + 1)[0].iter().any(|(r, role)| r.contains(&col) && *role == Role::Keyword);
        let mut d = Document::with_content("t.rs", "let a = 1;\nlet b = 1;");
        assert!(kw(&d, 0, 0) && kw(&d, 1, 0));
        d.extra_carets = vec![(1, 0)];
        d.insert_char(' ');
        assert_eq!(d.lines, vec![" let a = 1;".to_string(), " let b = 1;".to_string()]);
        assert!(kw(&d, 0, 1) && kw(&d, 1, 1), "typing at every caret moves the keyword");
        assert!(!kw(&d, 0, 0));
        d.undo();
        assert!(kw(&d, 0, 0) && kw(&d, 1, 0));
        d.redo();
        assert!(kw(&d, 0, 1) && kw(&d, 1, 1));
        d.undo();
        d.clear_extra_carets();
        d.insert_text("fn f() {\n    let s = \"hi\";\n}\n");
        assert!(kw(&d, 0, 0), "a multi-line paste is highlighted");
        assert!(d.syntax_roles(1..2)[0].iter().any(|(_, role)| *role == Role::String), "the pasted string is coloured");
    }

    #[cfg(feature = "lang-bash")]
    #[test]
    fn a_shebang_turns_an_untitled_file_into_bash() {
        use crate::syntax::Role;
        let mut d = Document::new("untitled");
        assert!(d.syntax_roles(0..1)[0].is_empty());
        for c in "#!/bin/bash".chars() {
            d.insert_char(c);
        }
        d.newline();
        d.insert_text("echo hi");
        assert!(d.syntax_roles(1..2)[0].iter().any(|(_, role)| *role == Role::Function), "echo is a command once the shebang is there");
    }

    #[cfg(feature = "syntax")]
    #[test]
    fn dotenv_files_colour_keys_values_comments_and_expansions() {
        use crate::syntax::Role;
        let text = "# note\nexport PORT=3000\nHOST=\"local # keep\"\nURL=https://x/${HOST}/é\nSECRET='plain $NO'\nhello\nCERT=\"BEGIN\n# still\nEND\"\n";
        let doc = Document::with_content(".env.local", text);
        let role = |line: usize, col: usize| {
            doc.syntax_roles(line..line + 1)[0].iter().find(|(span, _)| span.contains(&col)).map(|(_, role)| *role).unwrap_or(Role::Plain)
        };
        let at = |line: usize, needle: char| {
            let col = doc.lines[line].chars().position(|c| c == needle).unwrap();
            role(line, col)
        };
        assert_eq!(at(0, '#'), Role::Comment);
        assert_eq!(role(1, 0), Role::Keyword, "export");
        assert_eq!(role(1, 6), Role::Plain, "the space after export");
        assert_eq!(at(1, 'P'), Role::Function);
        assert_eq!(at(1, '='), Role::Punctuation);
        assert_eq!(at(1, '3'), Role::String);
        assert_eq!(at(2, '#'), Role::String, "a hash inside quotes stays in the value");
        assert_eq!(at(3, '$'), Role::Escape);
        assert_eq!(at(3, '{'), Role::Escape);
        assert_eq!(at(3, 'é'), Role::String);
        assert_eq!(at(4, '$'), Role::String, "single quotes do not expand");
        assert!(doc.syntax_roles(5..6)[0].is_empty(), "a line that is not an assignment stays plain");
        assert_eq!(doc.syntax_roles(7..8), vec![vec![(0.."# still".chars().count(), Role::String)]], "a continued quote is a string on a later line");
        assert!(Document::with_content("notes.txt", text).syntax_roles(1..2)[0].is_empty());
        let app = Document::with_content("dir/app.env", "PORT=1");
        assert!(app.syntax_roles(0..1)[0].iter().any(|(_, role)| *role == Role::Function));
    }

    #[test]
    fn copy_cut_and_typing_use_every_selection() {
        let select = || {
            let mut d = doc_with("abc def\n  ghi jkl");
            d.cursor = (0, 3);
            d.extra_carets = vec![(1, 5)];
            d.move_by(Document::end, true);
            d
        };
        assert_eq!(select().copy_text(), (" def\n jkl".to_string(), false), "copy takes every selection, one per line");
        let mut d = select();
        d.insert_char('X');
        assert_eq!(d.lines, vec!["abcX", "  ghiX"], "typing replaces every selection");
        d.undo();
        assert_eq!(d.occurrences, vec![(0, 3, 7), (1, 5, 9)], "undo brings the selections back");
        d.cut();
        assert_eq!(d.lines, vec!["abc", "  ghi"]);
        assert!(d.occurrences.is_empty());
        d.undo();
        assert_eq!(d.lines, vec!["abc def", "  ghi jkl"], "cut is one undo step");
    }

    #[test]
    fn multi_line_pastes_go_to_every_caret() {
        let mut d = doc_with("abc def\n  ghi jkl");
        d.cursor = (0, 7);
        d.extra_carets = vec![(1, 9)];
        d.insert_text(" def\n jkl");
        assert_eq!(d.lines, vec!["abc def def", "  ghi jkl jkl"], "as many lines as carets: one line each");
        assert_eq!((d.cursor, d.extra_carets.clone()), ((0, 11), vec![(1, 13)]));

        let mut d = doc_with("ab");
        d.cursor = (0, 1);
        d.extra_carets = vec![(0, 2)];
        d.insert_text("X\nY");
        assert_eq!((d.lines.clone(), d.cursor, d.extra_carets.clone()), (vec!["aXbY".to_string()], (0, 2), vec![(0, 4)]), "one line each, carets on one line");

        let mut d = doc_with("ab\ncd");
        d.cursor = (1, 1);
        d.extra_carets = vec![(0, 1)];
        d.insert_text("X\nY\nZ");
        assert_eq!(d.lines, vec!["aX", "Y", "Zb", "cX", "Y", "Zd"]);
        assert_eq!((d.cursor, d.extra_carets.clone()), ((5, 1), vec![(2, 1)]), "each caret lands after its paste");
        d.undo();
        assert_eq!(d.lines, vec!["ab", "cd"], "one undo step");

        let mut d = doc_with("abc");
        d.cursor = (0, 1);
        d.extra_carets = vec![(0, 2)];
        d.insert_text("X\nY\nZ");
        assert_eq!(d.full_content(), "aX\nY\nZbX\nY\nZc", "two carets on one line");
        assert_eq!((d.cursor, d.extra_carets.clone()), ((2, 1), vec![(4, 1)]));

        let mut d = doc_with("a\nb\nc");
        d.cursor = (2, 1);
        d.extra_carets = vec![(0, 0)];
        d.insert_lines_above("new\n");
        assert_eq!(d.lines, vec!["new", "a", "b", "new", "c"], "a copied line goes above each caret's line");
        assert_eq!((d.cursor, d.extra_carets.clone()), ((4, 1), vec![(1, 0)]));
    }

    #[test]
    fn removing_caret_lines_takes_every_caret_line_in_one_undo_step() {
        let mut d = doc_with("ls\nkeep\npwd\nmake\nend");
        d.cursor = (2, 2);
        d.extra_carets = vec![(0, 1), (3, 0)];
        d.selection = Some(((0, 0), (0, 1)));
        d.remove_caret_lines();
        assert_eq!(d.lines, vec!["keep", "end"]);
        assert_eq!((d.cursor, d.extra_carets.clone(), d.selection), ((0, 2), vec![], None), "one caret where the first line was");
        assert!(d.dirty);
        d.undo();
        assert_eq!(d.lines, vec!["ls", "keep", "pwd", "make", "end"]);

        let mut d = doc_with("only");
        d.remove_caret_lines();
        assert_eq!((d.lines.clone(), d.cursor), (vec![String::new()], (0, 0)), "the last line leaves an empty document");
        let mut d = doc_with("a\nlast");
        d.cursor = (1, 3);
        d.remove_caret_lines();
        assert_eq!((d.lines.clone(), d.cursor), (vec!["a".to_string()], (0, 1)), "removing the bottom line lands on the one above");
    }

    #[test]
    fn markdown_view_is_rebuilt_after_edits_and_skips_other_files() {
        let mut d = Document::with_content("notes.md", "# one\ntext");
        assert!(d.markdown_view().is_some());
        let first = d.markdown_view().unwrap();
        d.cursor = (1, 0);
        d.insert_char('#');
        d.insert_char(' ');
        let second = d.markdown_view().unwrap();
        assert!(!Rc::ptr_eq(&first, &second), "an edit invalidates the cached view");
        assert!(Rc::ptr_eq(&second, &d.markdown_view().unwrap()), "no edit, same view");
        d.undo();
        assert!(!Rc::ptr_eq(&second, &d.markdown_view().unwrap()), "undo invalidates too");
        assert!(Document::with_content("notes.txt", "# one").markdown_view().is_none());
        let before = d.markdown_view().unwrap();
        crate::theme::set(crate::theme::Palette { hot: ratatui::style::Color::Rgb(1, 2, 3), ..crate::theme::NEON_PALETTE });
        let recoloured = d.markdown_view().unwrap();
        crate::theme::set(crate::theme::NEON_PALETTE);
        assert!(!Rc::ptr_eq(&before, &recoloured), "a theme change rebuilds the view in the new colours");
    }

    #[test]
    fn caret_selection_and_occurrence_lines_stay_raw() {
        let mut d = doc_with("a\nb\nc\nd\ne");
        d.cursor = (0, 0);
        d.extra_carets = vec![(1, 0)];
        d.selection = None;
        assert!(d.is_raw_line(0) && d.is_raw_line(1) && !d.is_raw_line(2));
        d.selection = Some(((2, 0), (3, 1)));
        assert!(d.is_raw_line(2) && d.is_raw_line(3) && !d.is_raw_line(4));
        d.selection = None;
        d.occurrences = vec![(4, 0, 1)];
        assert!(d.is_raw_line(4));
    }

    #[test]
    fn start_new_line_opens_an_indented_line_below_every_caret() {
        let mut d = doc_with("  first line\nsecond\n\tthird");
        d.cursor = (0, 4);
        d.extra_carets = vec![(2, 3)];
        d.start_new_line();
        assert_eq!(d.lines, vec!["  first line", "  ", "second", "\tthird", "\t"]);
        assert_eq!(d.cursor, (1, 2));
        assert_eq!(d.extra_carets, vec![(4, 1)]);
        d.undo();
        assert_eq!(d.lines, vec!["  first line", "second", "\tthird"]);
    }

    #[test]
    fn extend_selection_grows_step_by_step_and_shrink_walks_back() {
        let mut d = doc_with("intro\n    call(\"hello world\") now\nmore\n\nnext");
        d.cursor = (1, 18);
        let mut steps = Vec::new();
        for _ in 0..9 {
            d.extend_selection();
            steps.push(d.selected_text().unwrap());
        }
        assert_eq!(
            steps,
            [
                "world",
                "hello world",
                "\"hello world\"",
                "(\"hello world\")",
                "call(\"hello world\")",
                "call(\"hello world\") now",
                "    call(\"hello world\") now",
                "intro\n    call(\"hello world\") now\nmore",
                "intro\n    call(\"hello world\") now\nmore\n\nnext",
            ]
        );
        d.shrink_selection();
        d.shrink_selection();
        assert_eq!(d.selected_text().as_deref(), Some("    call(\"hello world\") now"));
        d.extend_selection();
        assert_eq!(d.selected_text().as_deref(), Some("intro\n    call(\"hello world\") now\nmore"), "extend resumes from there");
        for _ in 0..10 {
            d.shrink_selection();
        }
        assert_eq!((d.selection, d.cursor), (None, (1, 18)), "back to the caret where it started");
        assert!(!d.dirty, "selection changes are not edits");
    }

    #[test]
    fn tab_indents_to_the_next_stop_at_every_caret_and_shift_tab_unindents() {
        let mut d = doc_with("ab\ncdef");
        d.cursor = (0, 1);
        d.extra_carets = vec![(1, 3)];
        d.indent();
        assert_eq!(d.lines, vec!["a   b", "cde f"]);
        assert_eq!((d.cursor, d.extra_carets.clone()), ((0, 4), vec![(1, 4)]));
        d.undo();
        assert_eq!(d.lines, vec!["ab", "cdef"]);

        let mut d = doc_with("one\n\ntwo\nthree");
        d.selection = Some(((0, 1), (3, 0)));
        d.cursor = (3, 0);
        d.indent();
        assert_eq!(d.lines, vec!["    one", "", "    two", "three"], "empty lines and a last line at col 0 are left alone");
        assert_eq!(d.selection, Some(((0, 5), (3, 0))));
        d.unindent();
        assert_eq!(d.lines, vec!["one", "", "two", "three"]);
        assert_eq!(d.selection, Some(((0, 1), (3, 0))));

        let mut d = doc_with("\ttabbed\n  two\nnone");
        d.cursor = (0, 3);
        d.extra_carets = vec![(1, 4), (2, 2)];
        d.unindent();
        assert_eq!(d.lines, vec!["tabbed", "two", "none"]);
        assert_eq!((d.cursor, d.extra_carets.clone()), ((0, 2), vec![(1, 2), (2, 2)]));
        let rev = d.rev();
        d.unindent();
        assert_eq!(d.rev(), rev, "nothing left to remove records no undo step");

        let mut d = doc_with("éa");
        d.cursor = (0, 1);
        d.indent();
        assert_eq!(d.lines[0], "é   a", "columns are characters, not bytes");
    }

    #[test]
    fn select_all_new_line_above_and_toggle_case() {
        let mut d = doc_with("ab\n  cd");
        d.select_all();
        assert_eq!(d.selected_text().as_deref(), Some("ab\n  cd"));
        assert!(!d.dirty);
        d.cursor = (1, 3);
        d.selection = None;
        d.start_new_line_above();
        assert_eq!((d.lines.clone(), d.cursor), (vec!["ab".to_string(), "  ".into(), "  cd".into()], (1, 2)));
        d.cursor = (2, 3);
        assert!(d.toggle_case());
        assert_eq!(d.lines[2], "  CD", "the word under the caret");
        assert!(d.toggle_case());
        assert_eq!(d.lines[2], "  cd", "all upper goes back to lower");
        d.cursor = (1, 0);
        assert!(!d.toggle_case(), "nothing under the caret");
        let mut d = doc_with("Hello");
        d.toggle_case();
        assert_eq!(d.lines[0], "HELLO", "mixed case goes to upper");
        let mut d = doc_with("abc def\nghi");
        d.selection = Some(((0, 4), (1, 1)));
        d.toggle_case();
        assert_eq!(d.lines, vec!["abc DEF", "Ghi"], "only the selected range");
        let mut d = doc_with("ab cd ab");
        d.set_occurrences(vec![(0, 0, 2), (0, 6, 8)]);
        d.toggle_case();
        assert_eq!(d.lines[0], "AB cd AB", "every occurrence");
    }

    #[test]
    fn toggle_comment_comments_the_group_at_its_indent_and_back() {
        let mut d = doc_with("    let a = 1;\n\n  let b = 2;");
        d.selection = Some(((0, 0), (2, 3)));
        d.cursor = (2, 3);
        d.toggle_comment("//", "");
        assert_eq!(d.lines, vec!["  //   let a = 1;", "", "  // let b = 2;"]);
        d.toggle_comment("//", "");
        assert_eq!(d.lines, vec!["    let a = 1;", "", "  let b = 2;"]);
        let mut d = doc_with("note");
        d.toggle_comment("<!--", "-->");
        assert_eq!(d.lines[0], "<!-- note -->");
        d.toggle_comment("<!--", "-->");
        assert_eq!(d.lines[0], "note");
        let mut d = doc_with("// a\nb");
        d.selection = Some(((0, 0), (1, 1)));
        d.toggle_comment("//", "");
        assert_eq!(d.lines, vec!["// // a", "// b"], "a partly commented group gets commented");
        let mut d = doc_with("a\nb\nc");
        d.extra_carets = vec![(2, 0)];
        d.toggle_comment("#", "");
        assert_eq!(d.lines, vec!["# a", "b", "# c"], "every caret line, and only those");
    }

    #[test]
    fn clicks_select_words_and_lines_and_drags_extend_from_the_anchor() {
        let mut d = doc_with("hello world\nsecond");
        d.click_at(0, 8, 1);
        assert_eq!((d.cursor, d.selection), ((0, 8), None));
        d.click_at(0, 8, 2);
        assert_eq!(d.selected_text().as_deref(), Some("world"));
        d.click_at(0, 8, 3);
        assert_eq!(d.selected_text().as_deref(), Some("hello world\n"));
        d.click_at(0, 3, 1);
        let anchor = d.selection_anchor();
        d.select_to(anchor, 1, 3);
        assert_eq!(d.selected_text().as_deref(), Some("lo world\nsec"));
        d.select_to(anchor, 0, 1);
        assert_eq!((d.selected_text().as_deref(), d.cursor), (Some("el"), (0, 1)), "dragging back past the anchor");
        d.select_to(anchor, 5, usize::MAX);
        assert_eq!(d.cursor, (1, 6), "clamped to the end of the document");
    }
}
