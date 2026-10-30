use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

/// Rows a local PNG occupies when the terminal can show it. One row is a sliver.
pub const IMAGE_ROWS: usize = 6;

/// True when this process is Ghostty itself, and not a pane inside herdr.
/// herdr does not pass kitty graphics through, so a band there would be a hole.
pub fn detect(term: &str, program: &str, herdr: bool) -> bool {
    if herdr {
        return false;
    }
    program.eq_ignore_ascii_case("ghostty") || term == "xterm-ghostty"
}

/// A local `.png` reference, still carrying any query or hash. Remote and
/// `data:` URLs are not files this process can hand to the terminal.
pub fn local_png(src: &str) -> Option<&str> {
    if src.bytes().any(|b| b == b'\n' || b == b'\r' || b == 0) {
        return None;
    }
    let src = src.trim();
    if src.is_empty() {
        return None;
    }
    let lower = src.to_ascii_lowercase();
    if lower.starts_with("data:") || lower.contains("://") {
        return None;
    }
    let path = src.split(['?', '#']).next().unwrap_or(src);
    path.to_ascii_lowercase().ends_with(".png").then_some(src)
}

/// Resolve `src` against the document's directory. Missing files stay text.
pub fn png_file(doc_path: Option<&str>, src: &str) -> Option<PathBuf> {
    let src = local_png(src)?;
    let bare = src.split(['?', '#']).next().unwrap_or(src);
    let rel = Path::new(bare);
    let full = if rel.is_absolute() {
        rel.to_path_buf()
    } else {
        let dir = doc_path.and_then(|p| Path::new(p).parent())?;
        dir.join(rel)
    };
    full.is_file().then_some(full)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Place {
    pub x: u16,
    pub y: u16,
    pub cols: u16,
    pub rows: u16,
    pub path: PathBuf,
}

pub fn clear_command() -> &'static str {
    "\x1b_Ga=d,d=A,q=2\x1b\\"
}

/// Kitty graphics placements. `q=2` asks for no reply, so the bytes never come
/// back through crossterm as keypresses. Cursor save and restore are `ESC 7`
/// and `ESC 8`: asking the terminal where the cursor is would steal the next input.
pub fn show_command(places: &[Place]) -> String {
    let places: Vec<&Place> = places
        .iter()
        .filter(|place| {
            let bytes = place.path.as_os_str().as_bytes();
            !bytes.is_empty() && !bytes.contains(&0) && !bytes.contains(&b'\n') && !bytes.contains(&b'\r')
        })
        .collect();
    if places.is_empty() {
        return clear_command().to_string();
    }
    let mut out = String::from("\x1b7");
    out.push_str(clear_command());
    for (i, place) in places.iter().enumerate() {
        let encoded = base64(place.path.as_os_str().as_bytes());
        out.push_str(&format!(
            "\x1b[{};{}H\x1b_Ga=T,f=100,t=f,i={},c={},r={},q=2;{}\x1b\\",
            place.y + 1,
            place.x + 1,
            i + 1,
            place.cols,
            place.rows,
            encoded,
        ));
    }
    out.push_str("\x1b8");
    out
}

fn base64(data: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut i = 0;
    while i + 3 <= data.len() {
        let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8) | data[i + 2] as u32;
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(T[((n >> 6) & 63) as usize] as char);
        out.push(T[(n & 63) as usize] as char);
        i += 3;
    }
    let rest = data.len() - i;
    if rest == 1 {
        let n = (data[i] as u32) << 16;
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rest == 2 {
        let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8);
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(T[((n >> 6) & 63) as usize] as char);
        out.push('=');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graphics_are_on_only_in_ghostty_outside_herdr() {
        assert!(detect("xterm-ghostty", "", false));
        assert!(detect("xterm-256color", "Ghostty", false));
        assert!(!detect("xterm-ghostty", "Ghostty", true));
        assert!(!detect("xterm-kitty", "kitty", false));
        assert!(!detect("", "", false));
    }

    #[test]
    fn a_png_resolves_next_to_the_document_and_urls_do_not() {
        let dir = std::env::temp_dir().join(format!("ee-png-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("pic.png");
        let upper = dir.join("shot.PNG");
        std::fs::write(&png, b"png").unwrap();
        std::fs::write(&upper, b"png").unwrap();
        let doc = dir.join("note.md");
        let doc_path = doc.to_str().unwrap();

        assert_eq!(png_file(Some(doc_path), "pic.png").as_deref(), Some(png.as_path()));
        assert_eq!(png_file(Some(doc_path), "pic.png?x=1#y").as_deref(), Some(png.as_path()));
        assert_eq!(png_file(Some(doc_path), "shot.PNG").as_deref(), Some(upper.as_path()));
        assert_eq!(png_file(Some(doc_path), png.to_str().unwrap()).as_deref(), Some(png.as_path()));
        assert_eq!(png_file(None, png.to_str().unwrap()).as_deref(), Some(png.as_path()));
        assert!(png_file(Some(doc_path), "https://example.com/pic.png").is_none());
        assert!(png_file(Some(doc_path), "data:image/png;base64,aaaa").is_none());
        assert!(png_file(Some(doc_path), "pic.svg").is_none());
        assert!(png_file(Some(doc_path), "missing.png").is_none());
        assert!(png_file(Some(doc_path), "pic.png\n").is_none());
        assert!(png_file(None, "pic.png").is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_placement_command_is_a_quiet_file_transmit_and_an_empty_list_only_clears() {
        assert_eq!(show_command(&[]), "\x1b_Ga=d,d=A,q=2\x1b\\");
        let bad = Place { x: 0, y: 0, cols: 1, rows: 1, path: PathBuf::from("a\nb.png") };
        assert_eq!(show_command(&[bad]), "\x1b_Ga=d,d=A,q=2\x1b\\");
        let place = Place { x: 2, y: 3, cols: 10, rows: 6, path: PathBuf::from("a.png") };
        assert_eq!(
            show_command(&[place]),
            "\x1b7\x1b_Ga=d,d=A,q=2\x1b\\\x1b[4;3H\x1b_Ga=T,f=100,t=f,i=1,c=10,r=6,q=2;YS5wbmc=\x1b\\\x1b8"
        );
    }
}
