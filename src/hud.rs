use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use crate::theme::pal;

pub struct Chrome<'a> {
    pub title: &'a str,
    pub tag: Option<String>,
    pub hazard: bool,
}

pub fn mix(a: Color, b: Color, t: f32) -> Color {
    let (Color::Rgb(ar, ag, ab), Color::Rgb(br, bg, bb)) = (a, b) else {
        return a;
    };
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color::Rgb(m(ar, br), m(ag, bg), m(ab, bb))
}

pub fn shadow_color() -> Color {
    mix(pal().void, Color::Rgb(0, 0, 0), 0.55)
}

pub fn scanline_color() -> Color {
    mix(pal().void, pal().ice, 0.04)
}

pub fn keycap() -> Style {
    Style::default().bg(pal().keycap).fg(pal().text).add_modifier(Modifier::BOLD)
}

fn put(buf: &mut Buffer, x: u16, y: u16, symbol: &str, style: Style) {
    if let Some(cell) = buf.cell_mut((x, y)) {
        cell.set_symbol(symbol).set_style(style);
    }
}

fn text(buf: &mut Buffer, x: u16, y: u16, max_x: u16, s: &str, style: Style) -> u16 {
    let mut at = x;
    for c in s.chars() {
        if at >= max_x {
            break;
        }
        put(buf, at, y, &c.to_string(), style);
        at += 1;
    }
    at
}

fn shadow(buf: &mut Buffer, rect: Rect) {
    let dark = shadow_color();
    let bounds = buf.area;
    let mut darken = |x: u16, y: u16| {
        if x < bounds.right() && y < bounds.bottom() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                let fg = mix(cell.fg, dark, 0.6);
                cell.set_bg(dark).set_fg(fg);
            }
        }
    };
    for y in rect.y + 1..=rect.bottom() {
        darken(rect.right(), y);
    }
    for x in rect.x + 1..rect.right() {
        darken(x, rect.bottom());
    }
}

pub fn draw(buf: &mut Buffer, rect: Rect, chrome: &Chrome) -> Rect {
    let rect = rect.intersection(buf.area);
    if rect.width < 2 || rect.height < 2 {
        return Rect::new(rect.x, rect.y, 0, 0);
    }
    shadow(buf, rect);
    let p = pal();
    let scan = scanline_color();
    for y in rect.top()..rect.bottom() {
        let bg = if (y - rect.y) % 2 == 0 { p.void } else { scan };
        for x in rect.left()..rect.right() {
            put(buf, x, y, " ", Style::default().bg(bg).fg(p.text));
        }
    }
    let heavy = Style::default().fg(p.ice).bg(p.void).add_modifier(Modifier::BOLD);
    let thin = Style::default().fg(p.ghost).bg(p.void);
    let (left, right, top, bottom) = (rect.left(), rect.right() - 1, rect.top(), rect.bottom() - 1);
    for x in left + 1..right {
        let near = x <= left + 2 || x + 2 >= right;
        put(buf, x, top, if near { "━" } else { "─" }, if near { heavy } else { thin });
        put(buf, x, bottom, if near { "━" } else { "─" }, if near { heavy } else { thin });
    }
    for y in top + 1..bottom {
        let near = y == top + 1 || y + 1 == bottom;
        let (glyph, style) = if near { ("┃", heavy) } else { ("│", thin) };
        let row_bg = if (y - rect.y) % 2 == 0 { p.void } else { scan };
        put(buf, left, y, glyph, style.bg(row_bg));
        put(buf, right, y, glyph, style.bg(row_bg));
    }
    put(buf, left, top, "┏", heavy);
    put(buf, right, top, "┓", heavy);
    put(buf, left, bottom, "┗", heavy);
    put(buf, right, bottom, "┛", heavy);

    if !chrome.title.is_empty() && rect.width > 8 {
        let at = text(buf, left + 3, top, right - 2, "╸", heavy);
        let at = text(buf, at, top, right - 2, &format!(" {} ", chrome.title.to_uppercase()), Style::default().fg(p.hot).bg(p.void).add_modifier(Modifier::BOLD));
        text(buf, at, top, right - 2, "╺", heavy);
    }
    if chrome.hazard && rect.width > 30 {
        let band = "╱╱╱╱╱╱";
        let start = right - 3 - band.chars().count() as u16;
        text(buf, start - 1, top, right - 2, " ", thin);
        text(buf, start, top, right - 2, band, Style::default().fg(p.amber).bg(p.void).add_modifier(Modifier::BOLD));
        text(buf, start + band.chars().count() as u16, top, right - 2, " ", thin);
    }
    if let Some(tag) = &chrome.tag {
        let label = format!(" {} ", tag);
        let width = label.chars().count() as u16 + 2;
        if rect.width > width + 8 {
            let start = right - 3 - width;
            let at = text(buf, start, bottom, right - 2, "╸", heavy);
            let at = text(buf, at, bottom, right - 2, &label, Style::default().fg(p.ghost).bg(p.void));
            text(buf, at, bottom, right - 2, "╺", heavy);
        }
    }
    Rect::new(rect.x + 1, rect.y + 1, rect.width - 2, rect.height - 2)
}

pub fn hints(buf: &mut Buffer, row: Rect, items: &[(&str, &str)]) {
    let max = row.right();
    let mut at = row.x;
    for (i, (key, what)) in items.iter().enumerate() {
        let need = key.chars().count() + what.chars().count() + 3;
        if at as usize + need > max as usize {
            break;
        }
        if i > 0 {
            at = text(buf, at, row.y, max, "  ", Style::default());
        }
        at = text(buf, at, row.y, max, &format!(" {} ", key), keycap());
        at = text(buf, at, row.y, max, &format!(" {}", what), Style::default().fg(pal().ghost));
    }
}
