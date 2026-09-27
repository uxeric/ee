use std::io::Write;
use std::process::{Command, Stdio};

pub enum CopyTarget {
    System,
    Terminal,
}

pub struct Clipboard {
    text: String,
    whole_line: bool,
}

impl Clipboard {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            whole_line: false,
        }
    }

    pub fn copy(&mut self, text: String, whole_line: bool) -> CopyTarget {
        let target = if system_copy(&text) {
            CopyTarget::System
        } else {
            terminal_copy(&text);
            CopyTarget::Terminal
        };
        self.text = text;
        self.whole_line = whole_line;
        target
    }

    pub fn paste(&self) -> (String, bool) {
        match system_paste() {
            Some(t) if !t.is_empty() && t != self.text => (t, false),
            _ => (self.text.clone(), self.whole_line),
        }
    }
}

#[cfg(target_os = "macos")]
fn copy_commands() -> Vec<Vec<&'static str>> {
    vec![vec!["pbcopy"]]
}

#[cfg(target_os = "macos")]
fn paste_commands() -> Vec<Vec<&'static str>> {
    vec![vec!["pbpaste"]]
}

#[cfg(windows)]
fn copy_commands() -> Vec<Vec<&'static str>> {
    vec![vec!["clip"]]
}

#[cfg(windows)]
fn paste_commands() -> Vec<Vec<&'static str>> {
    vec![vec![
        "powershell",
        "-NoProfile",
        "-Command",
        "[Console]::OutputEncoding=[Text.Encoding]::UTF8; Get-Clipboard -Raw",
    ]]
}

#[cfg(not(any(windows, target_os = "macos")))]
fn copy_commands() -> Vec<Vec<&'static str>> {
    let mut cmds = Vec::new();
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        cmds.push(vec!["wl-copy"]);
    }
    if std::env::var_os("DISPLAY").is_some() {
        cmds.push(vec!["xclip", "-selection", "clipboard"]);
        cmds.push(vec!["xsel", "--clipboard", "--input"]);
    }
    cmds
}

#[cfg(not(any(windows, target_os = "macos")))]
fn paste_commands() -> Vec<Vec<&'static str>> {
    let mut cmds = Vec::new();
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        cmds.push(vec!["wl-paste", "--no-newline"]);
    }
    if std::env::var_os("DISPLAY").is_some() {
        cmds.push(vec!["xclip", "-selection", "clipboard", "-o"]);
        cmds.push(vec!["xsel", "--clipboard", "--output"]);
    }
    cmds
}

fn encode_for_copy(text: &str) -> Vec<u8> {
    if cfg!(windows) {
        let mut out = vec![0xFF, 0xFE];
        for unit in text.replace('\n', "\r\n").encode_utf16() {
            out.extend_from_slice(&unit.to_le_bytes());
        }
        out
    } else {
        text.as_bytes().to_vec()
    }
}

fn system_copy(text: &str) -> bool {
    if cfg!(test) {
        return false;
    }
    let data = encode_for_copy(text);
    for cmd in copy_commands() {
        let child = Command::new(cmd[0])
            .args(&cmd[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        let Ok(mut child) = child else { continue };
        let written = child
            .stdin
            .take()
            .map(|mut stdin| stdin.write_all(&data).is_ok())
            .unwrap_or(false);
        if written && child.wait().map(|s| s.success()).unwrap_or(false) {
            return true;
        }
    }
    false
}

fn system_paste() -> Option<String> {
    if cfg!(test) {
        return None;
    }
    for cmd in paste_commands() {
        let output = Command::new(cmd[0])
            .args(&cmd[1..])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output();
        if let Ok(out) = output {
            if out.status.success() {
                let mut text = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
                if cfg!(windows) && text.ends_with('\n') {
                    text.pop();
                }
                return Some(text);
            }
        }
    }
    None
}

fn terminal_copy(text: &str) {
    if cfg!(test) {
        return;
    }
    let mut out = std::io::stdout();
    let _ = write!(out, "\x1b]52;c;{}\x07", base64(text.as_bytes()));
    let _ = out.flush();
}

fn base64(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(TABLE[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_rfc4648_vectors() {
        let cases = [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foobar", "Zm9vYmFy")];
        for (input, expected) in cases {
            assert_eq!(base64(input.as_bytes()), expected);
        }
    }
}
