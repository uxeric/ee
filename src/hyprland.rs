use std::process::Command;

use serde_json::Value;

use crate::herdr::Desktop;

pub struct Hyprland;

impl Hyprland {
    pub fn detect() -> Option<Self> {
        if cfg!(test) || std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none() {
            return None;
        }
        Some(Self)
    }

    fn hyprctl(args: &[&str]) -> Option<String> {
        let out = Command::new("hyprctl").args(args).output().ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).to_string())
    }
}

pub fn windows_titled(clients_json: &str, token: &str) -> Vec<String> {
    let Ok(Value::Array(windows)) = serde_json::from_str::<Value>(clients_json) else {
        return Vec::new();
    };
    windows
        .iter()
        .filter(|w| w["title"].as_str().is_some_and(|t| t.contains(token)))
        .filter_map(|w| w["address"].as_str().map(str::to_string))
        .collect()
}

pub fn dispatch_ok(output: Option<String>) -> bool {
    output.is_some_and(|o| o.trim() == "ok")
}

impl Desktop for Hyprland {
    fn windows_titled(&self, token: &str) -> Vec<String> {
        Self::hyprctl(&["clients", "-j"]).map(|json| windows_titled(&json, token)).unwrap_or_default()
    }

    fn focus(&self, address: &str) -> bool {
        let lua = format!("hl.dsp.focus({{ window = \"address:{}\" }})", address);
        dispatch_ok(Self::hyprctl(&["dispatch", &lua])) || dispatch_ok(Self::hyprctl(&["dispatch", "focuswindow", &format!("address:{}", address)]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_windows_showing_the_token_are_candidates_and_only_ok_counts() {
        let clients = r#"[
            {"address": "0x5f33a1", "title": "ee-focus-7-99", "class": "Alacritty", "pid": 10},
            {"address": "0x5f33b2", "title": "notes.md - ee", "class": "Alacritty", "pid": 11},
            {"address": "0x5f33c3", "title": "", "class": "firefox", "pid": 12}
        ]"#;
        assert_eq!(windows_titled(clients, "ee-focus-7-99"), vec!["0x5f33a1".to_string()]);
        assert!(windows_titled(clients, "ee-focus-8-00").is_empty());
        assert!(windows_titled("not json", "x").is_empty());
        assert!(dispatch_ok(Some("ok\n".to_string())));
        assert!(!dispatch_ok(Some("warning: hl.focus: window not found".to_string())), "Hyprland answers anything but ok when it did not focus");
        assert!(!dispatch_ok(None));
        assert!(Hyprland::detect().is_none(), "tests never touch the desktop");
    }
}
