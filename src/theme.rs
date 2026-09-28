use std::cell::Cell;
use std::path::PathBuf;
use std::time::SystemTime;

use ratatui::style::{Color, Modifier, Style};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub ice: Color,
    pub hot: Color,
    pub amber: Color,
    pub error: Color,
    pub ghost: Color,
    pub text: Color,
    pub void: Color,
    pub selection: Color,
    pub find_match: Color,
    pub cursor_line: Color,
    pub code_bg: Color,
    pub table_head: Color,
    pub table_stripe: Color,
    pub neon: Color,
    pub violet: Color,
    pub keycap: Color,
}

pub const NEON_PALETTE: Palette = Palette {
    ice: Color::Rgb(0x29, 0xF0, 0xFF),
    hot: Color::Rgb(0xFF, 0x2A, 0x6D),
    amber: Color::Rgb(0xFF, 0xB6, 0x27),
    error: Color::Rgb(0xFF, 0x3B, 0x3B),
    ghost: Color::Rgb(0x5C, 0x5A, 0x8A),
    text: Color::Rgb(0xD6, 0xDC, 0xF5),
    void: Color::Rgb(0x0B, 0x06, 0x20),
    selection: Color::Rgb(0x3A, 0x1A, 0x5E),
    find_match: Color::Rgb(0x0F, 0x4C, 0x58),
    cursor_line: Color::Rgb(0x14, 0x0C, 0x2E),
    code_bg: Color::Rgb(0x1A, 0x12, 0x38),
    table_head: Color::Rgb(0x24, 0x14, 0x52),
    table_stripe: Color::Rgb(0x12, 0x0B, 0x26),
    neon: Color::Rgb(0x3D, 0xFF, 0xA2),
    violet: Color::Rgb(0xB1, 0x8C, 0xFF),
    keycap: Color::Rgb(0x2E, 0x26, 0x56),
};

thread_local! {
    static CURRENT: Cell<Palette> = const { Cell::new(NEON_PALETTE) };
}

pub fn pal() -> Palette {
    CURRENT.with(|p| p.get())
}

pub fn set(palette: Palette) {
    CURRENT.with(|p| p.set(palette));
}

fn hex(value: &toml::Value) -> Option<(u8, u8, u8)> {
    let s = value.as_str()?.strip_prefix('#')?;
    if s.len() != 6 {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).ok();
    Some((byte(0)?, byte(2)?, byte(4)?))
}

fn mix(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    (m(a.0, b.0), m(a.1, b.1), m(a.2, b.2))
}

fn distance(a: (u8, u8, u8), b: (u8, u8, u8)) -> f32 {
    let d = |x: u8, y: u8| (x as f32 - y as f32).powi(2);
    (d(a.0, b.0) + d(a.1, b.1) + d(a.2, b.2)).sqrt()
}

fn rgb(c: (u8, u8, u8)) -> Color {
    Color::Rgb(c.0, c.1, c.2)
}

pub fn from_omarchy(colors_toml: &str) -> Option<Palette> {
    let table: toml::Table = colors_toml.parse().ok()?;
    let get = |key: &str| table.get(key).and_then(hex);
    let background = get("background")?;
    let foreground = get("foreground")?;
    let pick = |keys: &[&str], avoid: &[(u8, u8, u8)], fallback: (u8, u8, u8)| {
        let found: Vec<(u8, u8, u8)> = keys.iter().filter_map(|k| get(k)).collect();
        let apart = |c: &(u8, u8, u8)| avoid.iter().map(|a| distance(*c, *a)).fold(f32::MAX, f32::min);
        found
            .iter()
            .copied()
            .find(|c| apart(c) >= 64.0)
            .or_else(|| found.iter().copied().max_by(|a, b| apart(a).total_cmp(&apart(b))))
            .unwrap_or(fallback)
    };
    let tint = |t: f32| mix(background, foreground, t);
    let ice = pick(&["accent", "cyan", "blue"], &[], tint(0.7));
    let hot = pick(&["magenta", "red", "bright_magenta"], &[ice], tint(0.8));
    let violet = pick(&["blue", "bright_magenta", "magenta", "cyan"], &[ice, hot], ice);
    let neon = pick(&["green", "bright_green"], &[ice], ice);
    let amber = pick(&["yellow", "orange", "bright_yellow"], &[], hot);
    let error = pick(&["red", "bright_red"], &[], hot);
    let cursor_line = tint(0.06);
    let selection = (7..=11)
        .map(|step| mix(background, hot, step as f32 * 0.05))
        .find(|s| distance(*s, background) >= 72.0 && distance(*s, cursor_line) >= 56.0)
        .unwrap_or(mix(background, hot, 0.55));
    Some(Palette {
        ice: rgb(ice),
        hot: rgb(hot),
        amber: rgb(amber),
        error: rgb(error),
        ghost: rgb(get("muted").unwrap_or(tint(0.45))),
        text: rgb(foreground),
        void: rgb(background),
        selection: rgb(selection),
        find_match: rgb(mix(background, ice, 0.3)),
        cursor_line: rgb(cursor_line),
        code_bg: rgb(tint(0.09)),
        table_head: rgb(mix(background, violet, 0.25)),
        table_stripe: rgb(tint(0.04)),
        neon: rgb(neon),
        violet: rgb(violet),
        keycap: rgb(tint(0.2)),
    })
}

#[derive(Clone, Debug, Default)]
pub struct Roots {
    pub user: Option<PathBuf>,
    pub system: Option<PathBuf>,
    pub current: Option<PathBuf>,
}

impl Roots {
    pub fn from_env() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let omarchy = std::env::var_os("OMARCHY_PATH").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/usr/share/omarchy"));
        Self {
            user: home.as_ref().map(|h| h.join(".config/omarchy/themes")),
            system: Some(omarchy.join("themes")),
            current: home.map(|h| h.join(".local/state/omarchy/current")),
        }
    }

    pub fn colors(&self, choice: &str) -> Option<PathBuf> {
        match choice {
            "neon" => None,
            "omarchy" => self.current.as_ref().map(|c| c.join("theme/colors.toml")),
            name => [&self.user, &self.system].into_iter().flatten().map(|d| d.join(name).join("colors.toml")).find(|p| p.is_file()),
        }
    }

    pub fn installed(&self) -> Vec<(String, bool)> {
        let mut found: Vec<(String, bool)> = Vec::new();
        for (dir, yours) in [(&self.user, true), (&self.system, false)] {
            let Some(entries) = dir.as_ref().and_then(|d| std::fs::read_dir(d).ok()) else { continue };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if entry.path().join("colors.toml").is_file() && !found.iter().any(|(n, _)| *n == name) {
                    found.push((name, yours));
                }
            }
        }
        found.sort();
        found
    }

    pub fn current_name(&self) -> Option<String> {
        let name = std::fs::read_to_string(self.current.as_ref()?.join("theme.name")).ok()?;
        Some(name.trim().to_string()).filter(|n| !n.is_empty())
    }
}

pub fn load(path: Option<&std::path::Path>) -> Palette {
    path.and_then(|p| std::fs::read_to_string(p).ok()).as_deref().and_then(from_omarchy).unwrap_or(NEON_PALETTE)
}

fn modified(path: Option<&std::path::Path>) -> Option<SystemTime> {
    std::fs::metadata(path?).and_then(|m| m.modified()).ok()
}

#[derive(Default)]
pub struct ThemeFile {
    path: Option<PathBuf>,
    seen: Option<SystemTime>,
}

impl ThemeFile {
    pub fn switch(&mut self, path: Option<PathBuf>) {
        self.seen = modified(path.as_deref());
        set(load(path.as_deref()));
        self.path = path;
    }

    pub fn refresh(&mut self) -> bool {
        if self.path.is_none() {
            return false;
        }
        let now = modified(self.path.as_deref());
        if now == self.seen {
            return false;
        }
        self.seen = now;
        let palette = load(self.path.as_deref());
        let changed = palette != pal();
        set(palette);
        changed
    }
}

pub fn text() -> Style {
    Style::default().fg(pal().text)
}

pub fn dim() -> Style {
    Style::default().fg(pal().ghost)
}

pub fn block(bg: Color) -> Style {
    Style::default().fg(pal().void).bg(bg).add_modifier(Modifier::BOLD)
}

pub fn caret() -> Style {
    Style::default().fg(pal().void).bg(pal().hot)
}

pub fn extra_caret() -> Style {
    Style::default().fg(pal().void).bg(pal().ice)
}

pub fn selection() -> Style {
    Style::default().fg(pal().text).bg(pal().selection)
}

pub fn find_match() -> Style {
    Style::default().fg(pal().ice).bg(pal().find_match)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NIGHT_CITY: &str = "mode = \"dark\"\naccent = \"#65a4d9\"\nselection = \"#2c2727\"\nmuted = \"#ad8f8f\"\nbackground = \"#150f0f\"\nforeground = \"#ffffff\"\nred = \"#eb4933\"\nyellow = \"#acac3a\"\ngreen = \"#47ac3a\"\ncyan = \"#4291ba\"\nblue = \"#65a4d9\"\nmagenta = \"#d879bb\"\nbright_magenta = \"#e793cd\"\nhyprland_active_border = \"rgba(26a269ee) rgba(2ec27eee) 45deg\"\n";

    #[test]
    fn an_omarchy_theme_maps_onto_every_role_and_keeps_them_apart() {
        let p = from_omarchy(NIGHT_CITY).unwrap();
        assert_eq!((p.void, p.text, p.ghost), (Color::Rgb(0x15, 0x0f, 0x0f), Color::Rgb(0xff, 0xff, 0xff), Color::Rgb(0xad, 0x8f, 0x8f)));
        assert_eq!((p.ice, p.hot, p.amber, p.error, p.neon), (
            Color::Rgb(0x65, 0xa4, 0xd9),
            Color::Rgb(0xd8, 0x79, 0xbb),
            Color::Rgb(0xac, 0xac, 0x3a),
            Color::Rgb(0xeb, 0x49, 0x33),
            Color::Rgb(0x47, 0xac, 0x3a),
        ));
        assert_eq!(p.violet, Color::Rgb(0x42, 0x91, 0xba), "blue is the accent and magenta is taken, so Important gets the colour furthest from both");
        assert_eq!(p.cursor_line, Color::Rgb(35, 29, 29), "tints are mixed from the theme's own background");
    }

    #[test]
    fn selections_stand_out_from_the_background_and_the_caret_line_in_every_palette() {
        let apart = |a: Color, b: Color| match (a, b) {
            (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => distance((r1, g1, b1), (r2, g2, b2)),
            _ => 0.0,
        };
        let light = "background = \"#eff1f5\"\nforeground = \"#4c4f69\"\naccent = \"#1e66f5\"\nmagenta = \"#ea76cb\"\n";
        for (name, p) in [("neon", NEON_PALETTE), ("night-city", from_omarchy(NIGHT_CITY).unwrap()), ("light", from_omarchy(light).unwrap())] {
            assert!(apart(p.selection, p.void) >= 70.0, "{}: selection vs background {}", name, apart(p.selection, p.void));
            assert!(apart(p.selection, p.cursor_line) >= 55.0, "{}: selection vs caret line {}", name, apart(p.selection, p.cursor_line));
        }
    }

    #[test]
    fn light_themes_tint_towards_the_foreground_and_broken_files_fall_back() {
        let light = from_omarchy("background = \"#fafafa\"\nforeground = \"#202020\"\naccent = \"#0060c0\"\n").unwrap();
        assert_eq!(light.cursor_line, Color::Rgb(0xed, 0xed, 0xed));
        assert_eq!(light.hot, Color::Rgb(76, 76, 76), "a missing colour falls back to a tint, not to neon");
        assert!(from_omarchy("foreground = \"#ffffff\"").is_none(), "no background, no theme");
        assert!(from_omarchy("not toml [").is_none());
    }

    #[test]
    fn themes_are_found_by_name_and_reload_only_when_their_file_changes() {
        let dir = std::env::temp_dir().join(format!("ee-theme-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let write = |rel: &str, text: &str| {
            let path = dir.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, text).unwrap();
            path
        };
        let mine = write("user/mine/colors.toml", "background = \"#000000\"\nforeground = \"#ffffff\"\n");
        let yours = write("user/night-city/colors.toml", NIGHT_CITY);
        write("system/night-city/colors.toml", "background = \"#ffffff\"\nforeground = \"#000000\"\n");
        let tokyo = write("system/tokyo/colors.toml", NIGHT_CITY);
        write("system/broken/readme.txt", "no colors here");
        let current = write("current/theme/colors.toml", NIGHT_CITY);
        write("current/theme.name", "night-city\n");
        let roots = Roots { user: Some(dir.join("user")), system: Some(dir.join("system")), current: Some(dir.join("current")) };

        assert_eq!(roots.installed(), vec![("mine".to_string(), true), ("night-city".to_string(), true), ("tokyo".to_string(), false)], "yours win, folders without colors.toml are skipped");
        assert_eq!(roots.colors("night-city"), Some(yours));
        assert_eq!(roots.colors("tokyo"), Some(tokyo));
        assert_eq!((roots.colors("nope"), roots.colors("neon")), (None, None));
        assert_eq!(roots.colors("omarchy"), Some(current.clone()));
        assert_eq!(roots.current_name().as_deref(), Some("night-city"));
        assert_eq!(Roots::default().installed(), vec![], "no folders, no themes");

        let mut theme = ThemeFile::default();
        theme.switch(Some(current.clone()));
        assert_eq!(pal().void, Color::Rgb(0x15, 0x0f, 0x0f), "switching applies the theme at once");
        assert!(!theme.refresh(), "an unchanged file is not reloaded");
        std::fs::write(&current, std::fs::read_to_string(&mine).unwrap()).unwrap();
        std::fs::File::options().write(true).open(&current).unwrap().set_modified(SystemTime::UNIX_EPOCH).unwrap();
        assert!(theme.refresh(), "a changed file is reloaded");
        assert_eq!(pal().void, Color::Rgb(0, 0, 0));
        std::fs::remove_file(&current).unwrap();
        assert!(theme.refresh(), "a removed theme goes back to neon");
        assert_eq!(pal(), NEON_PALETTE);
        theme.switch(Some(mine));
        theme.switch(None);
        assert_eq!(pal(), NEON_PALETTE, "neon needs no file");
        assert!(!theme.refresh());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
