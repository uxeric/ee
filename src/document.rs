use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::markdown::{self, MdView};

const MAX_UNDO: usize = 100;

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
}

pub struct Document {
    pub name: String,
    pub lines: Vec<String>,
    pub cursor: (usize, usize),
    pub extra_carets: Vec<(usize, usize)>,
    pub selection: Option<((usize, usize), (usize, usize))>,
    pub occurrences: Vec<(usize, usize, usize)>,
    pub path: Option<String>,
    pub dirty: bool,
    pub scroll_top: Cell<usize>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    rev: u64,
    markdown: RefCell<Option<(u64, Rc<MdView>)>>,
}

impl Document {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            lines: vec![String::new()],
            cursor: (0, 0),
            selection: None,
            occurrences: Vec::new(),
            extra_carets: Vec::new(),
            path: None,
            dirty: false,
            scroll_top: Cell::new(0),
            undo: Vec::new(),
            redo: Vec::new(),
            rev: 0,
            markdown: RefCell::new(None),
        }
    }

    /// Create a document from pre-loaded `content`, split into lines by '\n'.
    pub fn with_content(name: &str, content: &str) -> Self {
        Self {
            name: name.to_string(),
            lines: content.split('\n').map(|s| s.to_string()).collect(),
            cursor: (0, 0),
            selection: None,
            occurrences: Vec::new(),
            extra_carets: Vec::new(),
            path: None,
            dirty: false,
            scroll_top: Cell::new(0),
            undo: Vec::new(),
            redo: Vec::new(),
            rev: 0,
            markdown: RefCell::new(None),
        }
    }

    pub fn from_file(path: &str) -> std::io::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let mut doc = Self::with_content(&file_name(path), &content);
        doc.path = Some(path.to_string());
        Ok(doc)
    }

    pub fn open_or_new(path: &str) -> std::io::Result<(Self, bool)> {
        match Self::from_file(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let mut doc = Self::new(&file_name(path));
                doc.path = Some(path.to_string());
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
        }
    }

    fn snapshot(&mut self) {
        self.rev += 1;
        self.dirty = true;
        self.undo.push(self.capture());
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn undo(&mut self) {
        if let Some(snap) = self.undo.pop() {
            self.redo.push(self.capture());
            self.lines = snap.lines;
            self.cursor = snap.cursor;
            self.extra_carets = snap.extra_carets;
            self.selection = snap.selection;
            self.occurrences = snap.occurrences;
            self.dirty = true;
            self.rev += 1;
        }
    }

    pub fn redo(&mut self) {
        if let Some(snap) = self.redo.pop() {
            self.undo.push(self.capture());
            self.lines = snap.lines;
            self.cursor = snap.cursor;
            self.extra_carets = snap.extra_carets;
            self.selection = snap.selection;
            self.occurrences = snap.occurrences;
            self.dirty = true;
            self.rev += 1;
        }
    }

    pub fn has_selection(&self) -> bool {
        self.selection.is_some() || !self.occurrences.is_empty()
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

    pub fn move_by(&mut self, movement: fn(&mut Document), select: bool) {
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
        let (line, col) = self.cursor;
        if line > 0 {
            let new_line = line - 1;
            let max_col = self.lines[new_line].chars().count();
            let new = (new_line, col.min(max_col));
                self.cursor = new;
        }
    }

    pub fn move_down(&mut self) {
        let (line, col) = self.cursor;
        if line + 1 < self.lines.len() {
            let new_line = line + 1;
            let max_col = self.lines[new_line].chars().count();
            let new = (new_line, col.min(max_col));
                self.cursor = new;
        }
    }

    /// Remove a caret position from `extra_carets` so the invariant holds:
    /// the active `cursor` is never also listed in `extra_carets`.
    fn remove_extra(&mut self, pos: (usize, usize)) {
        self.extra_carets.retain(|p| *p != pos);
    }

    /// Move the active caret up, leaving a caret behind at the previous line.
    pub fn add_caret_up(&mut self) {
        let (line, col) = self.cursor;
        if line == 0 {
            return;
        }
        let new_line = line - 1;
        let nc = col.min(self.lines[new_line].chars().count());
        let new_pos = (new_line, nc);
        let old_pos = self.cursor;
        self.remove_extra(new_pos);
        if old_pos != new_pos {
            self.extra_carets.push(old_pos);
        }
        self.cursor = new_pos;
    }

    /// Move the active caret down, leaving a caret behind at the previous line.
    pub fn add_caret_down(&mut self) {
        let (line, col) = self.cursor;
        if line + 1 >= self.lines.len() {
            return;
        }
        let new_line = line + 1;
        let nc = col.min(self.lines[new_line].chars().count());
        let new_pos = (new_line, nc);
        let old_pos = self.cursor;
        self.remove_extra(new_pos);
        if old_pos != new_pos {
            self.extra_carets.push(old_pos);
        }
        self.cursor = new_pos;
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

    pub fn home(&mut self) {
        let (line, _) = self.cursor;
        let new = (line, 0);
        self.cursor = new;
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

    pub fn is_markdown(&self) -> bool {
        let name = self.path.as_deref().unwrap_or(&self.name).to_lowercase();
        name.ends_with(".md") || name.ends_with(".markdown")
    }

    pub fn markdown_view(&self) -> Option<Rc<MdView>> {
        if !self.is_markdown() {
            return None;
        }
        let mut cache = self.markdown.borrow_mut();
        match cache.as_ref() {
            Some((rev, view)) if *rev == self.rev && view.lines.len() == self.lines.len() => Some(view.clone()),
            _ => {
                let view = Rc::new(markdown::build(&self.lines));
                *cache = Some((self.rev, view.clone()));
                Some(view)
            }
        }
    }

    pub fn is_raw_line(&self, line: usize) -> bool {
        self.cursor.0 == line
            || self.extra_carets.iter().any(|&(l, _)| l == line)
            || self.occurrences.iter().any(|&(l, _, _)| l == line)
            || self.selection.is_some_and(|((sl, _), (el, _))| sl <= line && line <= el)
    }

    pub fn caret_lines(&self) -> Vec<String> {
        let mut lines: Vec<usize> = self.extra_carets.iter().map(|&(l, _)| l).collect();
        lines.push(self.cursor.0);
        lines.sort();
        lines.dedup();
        lines.into_iter().map(|l| self.lines[l].clone()).collect()
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
        match self.selected_text() {
            Some(t) => (t, false),
            None => (format!("{}\n", self.lines[self.cursor.0]), true),
        }
    }

    pub fn cut(&mut self) {
        if self.selection.is_some() {
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
        self.extra_carets.clear();
        let (line, col) = self.cursor;
        let chars: Vec<char> = self.lines[line].chars().collect();
        let left: String = chars[..col].iter().collect();
        let right: String = chars[col..].iter().collect();
        let pieces: Vec<&str> = text.split('\n').collect();
        let last = pieces.len() - 1;
        let mut new_lines: Vec<String> = Vec::with_capacity(pieces.len());
        for (i, piece) in pieces.iter().enumerate() {
            let mut l = String::new();
            if i == 0 {
                l.push_str(&left);
            }
            l.push_str(piece);
            if i == last {
                l.push_str(&right);
            }
            new_lines.push(l);
        }
        self.lines.splice(line..=line, new_lines);
        self.cursor = (line + last, pieces[last].chars().count());
    }

    pub fn insert_lines_above(&mut self, text: &str) {
        let body = text.strip_suffix('\n').unwrap_or(text);
        self.snapshot();
        self.selection = None;
        self.occurrences.clear();
        self.extra_carets.clear();
        let line = self.cursor.0;
        let new: Vec<String> = body.split('\n').map(|l| l.to_string()).collect();
        let n = new.len();
        self.lines.splice(line..line, new);
        self.cursor.0 = line + n;
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
    }

    #[test]
    fn plain_move_clears_selection_and_moves_every_caret() {
        let mut d = doc_with("abc\nabc\nabc");
        d.cursor = (0, 1);
        d.extra_carets = vec![(1, 1), (2, 1)];
        d.selection = Some(((0, 0), (0, 1)));
        d.move_by(Document::move_right, false);
        assert_eq!(d.selection, None);
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
            ("hello", (0, 2), Document::end, (0, 5)),
            ("a中b", (0, 0), Document::end, (0, 3)),
            ("ab\ncd", (1, 2), Document::beginning_of_file, (0, 0)),
            ("ab\ncd", (0, 0), Document::end_of_file, (1, 2)),
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
}
