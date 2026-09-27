use ratatui::style::{Color, Modifier, Style};

pub const ICE: Color = Color::Rgb(0x29, 0xF0, 0xFF);
pub const HOT: Color = Color::Rgb(0xFF, 0x2A, 0x6D);
pub const AMBER: Color = Color::Rgb(0xFF, 0xB6, 0x27);
pub const ERROR: Color = Color::Rgb(0xFF, 0x3B, 0x3B);
pub const GHOST: Color = Color::Rgb(0x5C, 0x5A, 0x8A);
pub const TEXT: Color = Color::Rgb(0xD6, 0xDC, 0xF5);
pub const VOID: Color = Color::Rgb(0x0B, 0x06, 0x20);
pub const SELECTION: Color = Color::Rgb(0x3A, 0x1A, 0x5E);
pub const MATCH: Color = Color::Rgb(0x0F, 0x4C, 0x58);
pub const CURSOR_LINE: Color = Color::Rgb(0x14, 0x0C, 0x2E);
pub const CODE_BG: Color = Color::Rgb(0x1A, 0x12, 0x38);
pub const TABLE_HEAD: Color = Color::Rgb(0x24, 0x14, 0x52);
pub const TABLE_STRIPE: Color = Color::Rgb(0x12, 0x0B, 0x26);

pub fn text() -> Style {
    Style::default().fg(TEXT)
}

pub fn dim() -> Style {
    Style::default().fg(GHOST)
}

pub fn block(bg: Color) -> Style {
    Style::default().fg(VOID).bg(bg).add_modifier(Modifier::BOLD)
}

pub fn caret() -> Style {
    Style::default().fg(VOID).bg(HOT)
}

pub fn extra_caret() -> Style {
    Style::default().fg(VOID).bg(ICE)
}

pub fn selection() -> Style {
    Style::default().fg(TEXT).bg(SELECTION)
}

pub fn find_match() -> Style {
    Style::default().fg(ICE).bg(MATCH)
}
