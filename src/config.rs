use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::keys::Action;

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct KeyBinding {
    pub code: KeyCode,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl KeyBinding {
    pub fn matches(&self, event: &KeyEvent) -> bool {
        let shifted_char = matches!(event.code, KeyCode::Char(c) if c.is_uppercase());
        event.code == self.code
            && event.modifiers.contains(KeyModifiers::CONTROL) == self.ctrl
            && event.modifiers.contains(KeyModifiers::ALT) == self.alt
            && (event.modifiers.contains(KeyModifiers::SHIFT) || shifted_char) == self.shift
    }
}

pub struct Config {
    pub fallback_enabled: bool,
    pub theme: String,
    pub remaps: HashMap<Action, KeyBinding>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            fallback_enabled: true,
            theme: "omarchy".to_string(),
            remaps: HashMap::new(),
        }
    }
}

pub fn path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("eoe").join("config.toml"))
}

pub fn load() -> (Config, Option<String>) {
    match path() {
        Some(p) => load_from(&p),
        None => (Config::default(), None),
    }
}

pub fn load_from(path: &Path) -> (Config, Option<String>) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return (Config::default(), None);
    };
    match parse(&text) {
        Ok(config) => (config, None),
        Err(e) => (Config::default(), Some(format!("config: {} (using defaults)", e))),
    }
}

pub fn save_theme(path: &Path, choice: &str) -> std::io::Result<()> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let setting = format!("theme = {}", toml::Value::String(choice.to_string()));
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let first_table = lines.iter().position(|l| l.trim_start().starts_with('['));
    let existing = lines[..first_table.unwrap_or(lines.len())].iter().position(|l| l.split('=').next().map(str::trim) == Some("theme"));
    match (existing, first_table) {
        (Some(i), _) => lines[i] = setting,
        (None, Some(t)) => lines.splice(t..t, [setting, String::new()]).for_each(drop),
        (None, None) => lines.push(setting),
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, lines.join("\n") + "\n")
}

pub fn parse(text: &str) -> Result<Config, String> {
    let table: toml::Table = text.parse().map_err(|e: toml::de::Error| e.message().to_string())?;
    let mut config = Config::default();
    for (key, value) in &table {
        match (key.as_str(), value) {
            ("alt_fallback", toml::Value::Boolean(on)) => config.fallback_enabled = *on,
            ("keys", toml::Value::Table(keys)) => {
                for (name, binding) in keys {
                    let action = Action::from_name(name).ok_or_else(|| format!("unknown action `{}`", name))?;
                    let spec = binding.as_str().ok_or_else(|| format!("`{}` needs a key like \"ctrl+s\"", name))?;
                    config.remaps.insert(action, parse_key(spec)?);
                }
            }
            ("theme", toml::Value::String(name)) if !name.trim().is_empty() => config.theme = name.trim().to_string(),
            ("alt_fallback", _) => return Err("`alt_fallback` must be true or false".to_string()),
            ("theme", _) => return Err("`theme` must be \"omarchy\", \"neon\" or an Omarchy theme name".to_string()),
            _ => return Err(format!("unknown setting `{}`", key)),
        }
    }
    Ok(config)
}

fn parse_key(spec: &str) -> Result<KeyBinding, String> {
    let bad = || format!("cannot read key `{}`", spec);
    let parts: Vec<String> = spec.split('+').map(|p| p.trim().to_lowercase()).collect();
    let (key, modifiers) = parts.split_last().ok_or_else(bad)?;
    let mut binding = KeyBinding { code: KeyCode::Null, ctrl: false, alt: false, shift: false };
    for m in modifiers {
        match m.as_str() {
            "ctrl" | "control" => binding.ctrl = true,
            "alt" => binding.alt = true,
            "shift" => binding.shift = true,
            _ => return Err(bad()),
        }
    }
    binding.code = match key.as_str() {
        "enter" => KeyCode::Enter,
        "esc" | "escape" => KeyCode::Esc,
        "tab" => KeyCode::Tab,
        "backspace" => KeyCode::Backspace,
        "delete" => KeyCode::Delete,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "space" => KeyCode::Char(' '),
        f if f.len() > 1 && f.starts_with('f') => KeyCode::F(f[1..].parse().map_err(|_| bad())?),
        c if c.chars().count() == 1 => {
            let c = c.chars().next().unwrap();
            KeyCode::Char(if binding.shift { c.to_ascii_uppercase() } else { c })
        }
        _ => return Err(bad()),
    };
    Ok(binding)
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

    #[test]
    fn binding_matches_exact_key() {
        let binding = KeyBinding {
            code: KeyCode::Char('s'),
            ctrl: true,
            alt: false,
            shift: true,
        };
        assert!(binding.matches(&event(KeyCode::Char('s'), true, false, true)));
        assert!(!binding.matches(&event(KeyCode::Char('s'), true, false, false)));
        assert!(!binding.matches(&event(KeyCode::Char('s'), false, false, true)));
        assert!(!binding.matches(&event(KeyCode::Char('s'), true, true, true)));
        assert!(!binding.matches(&event(KeyCode::Char('x'), true, false, true)));
    }

    #[test]
    fn config_file_sets_fallback_and_remaps_keys() {
        let config = parse("alt_fallback = false\n[keys]\nsave_all = \"ctrl+g\"\nrename = \"f7\"\nfind = \"Alt+Shift+Q\"\n").unwrap();
        let km = crate::keys::Keymap::new(config);
        assert_eq!(km.dispatch(&event(KeyCode::Char('g'), true, false, false)), Some(Action::SaveAll));
        assert_eq!(km.dispatch(&event(KeyCode::Char('Q'), false, true, true)), Some(Action::Find));
        assert_eq!(km.dispatch(&event(KeyCode::F(7), false, false, false)), Some(Action::Rename));
        let kitty = KeyEvent::new(KeyCode::Char('Q'), KeyModifiers::ALT);
        assert_eq!(km.dispatch(&kitty), Some(Action::Find), "kitty sends Alt+Shift+Q as 'Q' without the shift flag");
        assert_eq!(km.dispatch(&event(KeyCode::Char('w'), false, true, false)), None, "alt fallback off");
        assert_eq!(km.dispatch(&event(KeyCode::Char('s'), true, false, false)), Some(Action::SaveAll), "defaults still work");
    }

    #[test]
    fn bad_config_is_reported_and_falls_back_to_defaults() {
        let cases = [
            ("[keys]\nteleport = \"ctrl+t\"", "unknown action `teleport`"),
            ("[keys]\nfind = \"hyper+f\"", "cannot read key `hyper+f`"),
            ("[keys]\nfind = 3", "`find` needs a key"),
            ("alt_fallback = \"yes\"", "must be true or false"),
            ("theme = 3", "`theme` must be \"omarchy\", \"neon\" or an Omarchy theme name"),
            ("theme = \" \"", "`theme` must be"),
            ("colour = \"neon\"", "unknown setting `colour`"),
            ("alt_fallback = ", ""),
        ];
        for (text, expected) in cases {
            let err = parse(text).err().unwrap_or_else(|| panic!("accepted {:?}", text));
            assert!(err.contains(expected), "{:?} gave {:?}", text, err);
        }
        let dir = std::env::temp_dir().join(format!("ee-config-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("config.toml");
        std::fs::write(&file, "[keys]\nteleport = \"ctrl+t\"").unwrap();
        let (config, warning) = load_from(&file);
        assert!(config.fallback_enabled && config.remaps.is_empty());
        assert!(warning.unwrap().contains("unknown action"));
        assert!(load_from(&dir.join("missing.toml")).1.is_none(), "no file, no warning");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn saving_a_theme_rewrites_only_its_own_line() {
        let dir = std::env::temp_dir().join(format!("ee-save-theme-{}", std::process::id()));
        let file = dir.join("eoe/config.toml");
        save_theme(&file, "neon").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "theme = \"neon\"\n", "creates the file and its folder");
        let mine = "# my notes\nalt_fallback = false\n# theme = \"old\"\n\n[keys]\nfind = \"f2\"\n";
        std::fs::write(&file, mine).unwrap();
        save_theme(&file, "tokyo-night").unwrap();
        let saved = std::fs::read_to_string(&file).unwrap();
        assert_eq!(saved, "# my notes\nalt_fallback = false\n# theme = \"old\"\n\ntheme = \"tokyo-night\"\n\n[keys]\nfind = \"f2\"\n", "goes above the first table, comments kept");
        save_theme(&file, "omarchy").unwrap();
        let config = parse(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!((config.theme.as_str(), config.fallback_enabled, config.remaps.len()), ("omarchy", false, 1), "replaced in place, the rest untouched");
        assert_eq!(std::fs::read_to_string(&file).unwrap().matches("theme = ").count(), 2, "one setting plus the comment");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_installer_writes_a_config_ee_accepts() {
        let script = include_str!("../install.sh");
        let start = script.find("<< 'EOF'\n").expect("installer config heredoc") + "<< 'EOF'\n".len();
        let end = start + script[start..].find("\nEOF\n").expect("heredoc end");
        let config = parse(&script[start..end]).unwrap();
        assert!(config.fallback_enabled && config.theme == "omarchy" && config.remaps.is_empty());
        assert_eq!(parse("theme = \"tokyo-night\"").unwrap().theme, "tokyo-night");
    }

    #[test]
    fn named_keys_parse_to_the_keys_they_name() {
        let cases = [
            ("enter", KeyCode::Enter),
            ("esc", KeyCode::Esc),
            ("tab", KeyCode::Tab),
            ("backspace", KeyCode::Backspace),
            ("delete", KeyCode::Delete),
            ("space", KeyCode::Char(' ')),
            ("pageup", KeyCode::PageUp),
            ("pagedown", KeyCode::PageDown),
            ("home", KeyCode::Home),
            ("f12", KeyCode::F(12)),
            ("ctrl+shift+w", KeyCode::Char('W')),
        ];
        for (spec, code) in cases {
            assert_eq!(parse_key(spec).unwrap().code, code, "{}", spec);
        }
    }

    #[test]
    fn every_palette_entry_shows_the_key_that_runs_it() {
        let km = crate::keys::Keymap::new(Config::default());
        for &(name, label, keys) in crate::keys::PALETTE {
            let expected = Action::from_name(name).unwrap_or_else(|| panic!("{} is not an action name", name));
            if keys == "Shift Shift" {
                continue;
            }
            let spec = if keys == "Shift+Tab" { "backtab".to_string() } else { keys.to_lowercase() };
            let event = if spec == "backtab" {
                KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)
            } else {
                let b = parse_key(&spec).unwrap_or_else(|e| panic!("{}: {}", label, e));
                let mut m = KeyModifiers::NONE;
                if b.ctrl {
                    m |= KeyModifiers::CONTROL;
                }
                if b.alt {
                    m |= KeyModifiers::ALT;
                }
                if b.shift {
                    m |= KeyModifiers::SHIFT;
                }
                KeyEvent::new(b.code, m)
            };
            assert_eq!(km.dispatch(&event), Some(expected), "{} is labelled {}", label, keys);
        }
    }
}
