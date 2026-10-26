use std::fs;
use std::path::{Path, PathBuf};

pub const KEEP: usize = 20;

pub fn path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("state")))?;
    Some(base.join("eoe").join("recent"))
}

#[derive(Default)]
pub struct Recent {
    path: Option<PathBuf>,
    files: Vec<String>,
}

impl Recent {
    pub fn load(path: Option<PathBuf>) -> Self {
        let files = path.as_deref().map(read).unwrap_or_default();
        Self { path, files }
    }

    pub fn files(&self) -> &[String] {
        &self.files
    }

    pub fn record(&mut self, file: &str) {
        let Ok(full) = fs::canonicalize(file) else {
            return;
        };
        let full = full.to_string_lossy().into_owned();
        if let Some(path) = &self.path {
            self.files = read(path);
        }
        self.files.retain(|f| *f != full);
        self.files.insert(0, full);
        self.files.truncate(KEEP);
        if let Some(path) = &self.path {
            let _ = write(path, &self.files);
        }
    }
}

fn read(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .map(|text| text.lines().filter(|l| !l.is_empty()).map(String::from).collect())
        .unwrap_or_default()
}

fn write(path: &Path, files: &[String]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(&tmp, files.join("\n") + "\n")?;
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_last_twenty_opened_newest_first_across_restarts() {
        let dir = std::env::temp_dir().join(format!("ee-test-recent-store-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let dir = fs::canonicalize(&dir).unwrap();
        let file = |i: usize| dir.join(format!("f{i}.txt")).to_string_lossy().into_owned();
        for i in 0..25 {
            fs::write(file(i), "").unwrap();
        }
        let store = Some(dir.join("state").join("recent"));

        let mut recent = Recent::load(store.clone());
        for i in 0..25 {
            recent.record(&file(i));
        }
        recent.record(&file(10));
        recent.record(&dir.join("missing.txt").to_string_lossy());

        let mut expected: Vec<String> = vec![file(10)];
        expected.extend((5..25).rev().filter(|&i| i != 10).map(file));
        assert_eq!(recent.files(), expected.as_slice(), "a reopened file moves to the top and is listed once");

        let mut other = Recent::load(store.clone());
        assert_eq!(other.files(), expected.as_slice(), "a restart reads the same list");
        other.record(&file(0));
        recent.record(&file(1));
        assert_eq!(&Recent::load(store).files()[..2], &[file(1), file(0)], "two ee windows don't drop each other's opens");

        let _ = fs::remove_dir_all(&dir);
    }
}
