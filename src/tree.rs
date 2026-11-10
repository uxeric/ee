use std::cell::Cell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub const WIDTH: u16 = 32;

pub fn width(screen: u16) -> u16 {
    let w = WIDTH.min(screen * 2 / 5);
    if w < 14 { 0 } else { w }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub path: PathBuf,
    pub name: String,
    pub dir: bool,
    pub open: bool,
    pub depth: usize,
    pub guides: Vec<bool>,
    pub last: bool,
}

impl Row {
    pub fn prefix(&self) -> String {
        let mut out = String::new();
        for &more in &self.guides {
            out.push_str(if more { "│ " } else { "  " });
        }
        out.push(if self.last { '└' } else { '├' });
        out.push(match (self.dir, self.open) {
            (true, true) => '▾',
            (true, false) => '▸',
            _ => '─',
        });
        out.push(' ');
        out
    }
}

#[derive(Default)]
pub struct Tree {
    pub root: PathBuf,
    pub visible: bool,
    open: HashSet<PathBuf>,
    pub rows: Vec<Row>,
    pub selected: usize,
    scroll: Cell<usize>,
    follow: Cell<bool>,
}

impl Tree {
    #[cfg(test)]
    pub fn new(root: PathBuf) -> Self {
        let mut tree = Self { root, ..Self::default() };
        tree.follow.set(true);
        tree.rebuild();
        tree
    }

    pub fn name(&self) -> String {
        self.root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| self.root.to_string_lossy().to_string())
    }

    pub fn rebuild(&mut self) {
        let keep = self.rows.get(self.selected).map(|r| r.path.clone());
        self.open.retain(|p| p.is_dir());
        let mut rows = Vec::new();
        walk(&self.root, &self.open, &mut Vec::new(), &mut rows);
        self.rows = rows;
        self.selected = keep.and_then(|p| self.index_of(&p)).unwrap_or(self.selected.min(self.rows.len().saturating_sub(1)));
    }

    pub fn index_of(&self, path: &Path) -> Option<usize> {
        self.rows.iter().position(|r| r.path == path)
    }

    pub fn selected_row(&self) -> Option<&Row> {
        self.rows.get(self.selected)
    }

    pub fn select(&mut self, index: usize) -> bool {
        let index = index.min(self.rows.len().saturating_sub(1));
        self.follow.set(true);
        let moved = index != self.selected;
        self.selected = index;
        moved
    }

    pub fn subtree_len(&self, index: usize) -> usize {
        let Some(row) = self.rows.get(index) else { return 0 };
        self.rows[index + 1..].iter().take_while(|r| r.depth > row.depth).count()
    }

    /// Opens or closes the folder at `index` and returns how many rows appeared
    /// (positive) or went away (negative).
    pub fn toggle(&mut self, index: usize) -> isize {
        let Some(row) = self.rows.get(index).filter(|r| r.dir) else { return 0 };
        let path = row.path.clone();
        let before = self.rows.len() as isize;
        if !self.open.remove(&path) {
            self.open.insert(path);
        }
        self.rebuild();
        self.rows.len() as isize - before
    }

    pub fn parent_of(&self, index: usize) -> Option<usize> {
        let depth = self.rows.get(index)?.depth;
        (0..index).rev().find(|&i| self.rows[i].depth + 1 == depth)
    }

    /// Opens every folder between the root and `path` and selects it.
    pub fn reveal(&mut self, path: &Path) -> bool {
        let Ok(rel) = path.strip_prefix(&self.root) else { return false };
        let mut at = self.root.clone();
        let parts: Vec<_> = rel.components().collect();
        for part in &parts[..parts.len().saturating_sub(1)] {
            at.push(part);
            self.open.insert(at.clone());
        }
        self.rebuild();
        match self.index_of(path) {
            Some(i) => {
                self.select(i);
                true
            }
            None => false,
        }
    }

    pub fn jump_to_letter(&mut self, c: char) -> bool {
        let c = c.to_lowercase().next().unwrap_or(c);
        let n = self.rows.len();
        let hit = (1..=n).map(|k| (self.selected + k) % n).find(|&i| self.rows[i].name.to_lowercase().starts_with(c));
        hit.is_some_and(|i| self.select(i))
    }

    pub fn scroll_by(&self, rows: isize, height: usize) {
        let max = self.rows.len().saturating_sub(height.max(1));
        let top = (self.scroll.get() as isize + rows).clamp(0, max as isize) as usize;
        self.scroll.set(top);
        self.follow.set(false);
    }

    pub fn top(&self) -> usize {
        self.scroll.get()
    }

    /// The first row on screen for a list `height` rows tall, scrolled so the
    /// selection shows after a keyboard move.
    pub fn view_top(&self, height: usize) -> usize {
        let height = height.max(1);
        let mut top = self.scroll.get();
        if self.follow.get() {
            if self.selected < top {
                top = self.selected;
            } else if self.selected >= top + height {
                top = self.selected + 1 - height;
            }
        }
        top = top.min(self.rows.len().saturating_sub(1));
        self.scroll.set(top);
        top
    }
}

fn hidden(name: &str) -> bool {
    name == ".git"
}

fn walk(dir: &Path, open: &HashSet<PathBuf>, guides: &mut Vec<bool>, rows: &mut Vec<Row>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<(String, PathBuf, bool)> = entries
        .flatten()
        .map(|e| {
            let path = e.path();
            (e.file_name().to_string_lossy().to_string(), path.clone(), path.is_dir())
        })
        .filter(|(name, _, _)| !hidden(name))
        .collect();
    entries.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase())).then_with(|| a.0.cmp(&b.0)));
    let n = entries.len();
    for (i, (name, path, dir)) in entries.into_iter().enumerate() {
        let last = i + 1 == n;
        let is_open = dir && open.contains(&path);
        rows.push(Row { path: path.clone(), name, dir, open: is_open, depth: guides.len(), guides: guides.clone(), last });
        if is_open {
            guides.push(!last);
            walk(&path, open, guides, rows);
            guides.pop();
        }
    }
}

pub fn project_root(files: &[&str]) -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_default();
    let first = files.first().and_then(|f| std::fs::canonicalize(f).ok()).and_then(|p| p.parent().map(Path::to_path_buf));
    match first {
        Some(dir) if !dir.starts_with(&cwd) => dir,
        _ => cwd,
    }
}

#[cfg(test)]
pub(crate) mod scratch {
    pub struct Dir(pub std::path::PathBuf);

    impl Dir {
        pub fn new() -> Self {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static N: AtomicUsize = AtomicUsize::new(0);
            let p = std::env::temp_dir().join(format!("ee-tree-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }

        pub fn project() -> Self {
            let dir = Self::new();
            std::fs::create_dir_all(dir.0.join("src/deep")).unwrap();
            std::fs::create_dir_all(dir.0.join(".git")).unwrap();
            for f in ["Cargo.toml", "README.md", ".env", "src/main.rs", "src/ui.rs", "src/deep/x.rs"] {
                std::fs::write(dir.0.join(f), "").unwrap();
            }
            dir
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> (scratch::Dir, Tree) {
        let dir = scratch::Dir::project();
        let tree = Tree::new(dir.0.clone());
        (dir, tree)
    }

    fn shown(tree: &Tree) -> Vec<String> {
        tree.rows.iter().map(|r| format!("{}{}", r.prefix(), r.name)).collect()
    }

    #[test]
    fn folders_come_first_and_open_and_close_with_their_guides() {
        let (_dir, mut tree) = sample();
        assert_eq!(shown(&tree), vec!["├▸ src", "├─ .env", "├─ Cargo.toml", "└─ README.md"], ".git stays out, dotfiles stay in");
        assert_eq!(tree.toggle(0), 3);
        assert_eq!(shown(&tree), vec!["├▾ src", "│ ├▸ deep", "│ ├─ main.rs", "│ └─ ui.rs", "├─ .env", "├─ Cargo.toml", "└─ README.md"]);
        assert_eq!(tree.toggle(1), 1);
        assert_eq!(tree.rows[2].prefix(), "│ │ └─ ");
        assert_eq!(tree.subtree_len(0), 4);
        assert_eq!(tree.toggle(0), -4);
        assert_eq!(tree.toggle(0), 4, "a folder remembers which of its folders were open");
        assert_eq!(tree.parent_of(2), Some(1));
    }

    #[test]
    fn reveal_opens_the_folders_above_a_file_and_selects_it() {
        let (dir, mut tree) = sample();
        assert!(tree.reveal(&dir.0.join("src/deep/x.rs")));
        assert_eq!(tree.selected_row().unwrap().name, "x.rs");
        assert!(!tree.reveal(Path::new("/nowhere/else.rs")));
    }

    #[test]
    fn the_view_follows_the_selection_until_the_wheel_moves_it() {
        let (_dir, mut tree) = sample();
        tree.toggle(0);
        tree.toggle(1);
        tree.select(7);
        assert_eq!(tree.view_top(3), 5);
        tree.scroll_by(-10, 3);
        assert_eq!(tree.view_top(3), 0, "the wheel moves the view alone");
        tree.select(6);
        assert_eq!(tree.view_top(3), 4);
    }

    #[test]
    fn a_letter_jumps_to_the_next_name_that_starts_with_it() {
        let (_dir, mut tree) = sample();
        assert!(tree.jump_to_letter('C'));
        assert_eq!(tree.selected_row().unwrap().name, "Cargo.toml");
        assert!(!tree.jump_to_letter('C'), "the only match stays selected");
        assert!(!tree.jump_to_letter('q'));
    }

    #[test]
    fn the_panel_needs_room() {
        assert_eq!(width(200), WIDTH);
        assert_eq!(width(50), 20);
        assert_eq!(width(30), 0);
    }
}
