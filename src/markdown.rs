use std::ops::Range;

use pulldown_cmark::{Alignment, BlockQuoteKind, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};

use crate::theme::{AMBER, CODE_BG, GHOST, HOT, ICE, KEYCAP, NEON, TABLE_HEAD, TABLE_STRIPE, TEXT, VIOLET, VOID};

#[derive(Clone, Debug, PartialEq)]
pub struct Seg {
    pub text: String,
    pub style: Style,
    pub src: Range<usize>,
    pub verbatim: bool,
    pub link: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum LineKind {
    #[default]
    Plain,
    Heading(u8),
    SetextUnderline(u8),
    FenceOpen(String),
    FenceClose,
    Code,
    Rule,
    AlertTitle(usize),
}

#[derive(Clone, Debug, Default)]
pub struct RenderedLine {
    pub segs: Vec<Seg>,
    pub fill: Option<(char, Style)>,
    pub bg: Option<Color>,
    pub centered: bool,
    pub anchor: Option<String>,
}

#[derive(Clone, Debug)]
struct Piece {
    start: usize,
    end: usize,
    text: String,
    style: Style,
    verbatim: bool,
    marker: bool,
    link: Option<String>,
}

struct Table {
    aligns: Vec<Alignment>,
    rows: Vec<usize>,
}

pub struct MdView {
    pub lines: Vec<RenderedLine>,
}

fn heading_style(level: u8) -> Style {
    let s = Style::default().add_modifier(Modifier::BOLD);
    match level {
        1 => s.fg(HOT).add_modifier(Modifier::UNDERLINED),
        2 => s.fg(AMBER),
        3 => s.fg(ICE),
        _ => s.fg(TEXT),
    }
}

fn level_num(l: HeadingLevel) -> u8 {
    l as u8
}

const ALERTS: [(&str, &str, Color); 5] = [
    ("●", "Note", ICE),
    ("◆", "Tip", NEON),
    ("◉", "Important", VIOLET),
    ("▲", "Warning", AMBER),
    ("⊘", "Caution", HOT),
];

fn alert_index(kind: BlockQuoteKind) -> usize {
    match kind {
        BlockQuoteKind::Note => 0,
        BlockQuoteKind::Tip => 1,
        BlockQuoteKind::Important => 2,
        BlockQuoteKind::Warning => 3,
        BlockQuoteKind::Caution => 4,
    }
}

pub fn slug(text: &str) -> String {
    text.trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
        .map(|c| if c == ' ' { '-' } else { c })
        .collect()
}

enum Frag {
    Tag { range: Range<usize>, name: String, closing: bool, attrs: String },
    Text(Range<usize>),
    Comment(Range<usize>),
}

fn html_frags(src: &str, r: Range<usize>) -> Vec<Frag> {
    let mut out = Vec::new();
    let mut i = r.start;
    while i < r.end {
        let rest = &src[i..r.end];
        if rest.starts_with("<!--") {
            let end = rest.find("-->").map_or(r.end, |e| i + e + 3);
            out.push(Frag::Comment(i..end));
            i = end;
        } else if rest.starts_with('<') {
            let Some(close) = rest.find('>') else {
                out.push(Frag::Text(i..r.end));
                break;
            };
            let inner = &rest[1..close];
            let closing = inner.starts_with('/');
            let body = inner.trim_start_matches('/');
            let name_len = body.find(|c: char| !c.is_ascii_alphanumeric()).unwrap_or(body.len());
            out.push(Frag::Tag {
                range: i..i + close + 1,
                name: body[..name_len].to_ascii_lowercase(),
                closing,
                attrs: body[name_len..].to_string(),
            });
            i += close + 1;
        } else {
            let next = rest.find('<').map_or(r.end, |n| i + n);
            out.push(Frag::Text(i..next));
            i = next;
        }
    }
    out
}

fn attr(attrs: &str, key: &str) -> Option<String> {
    let lower = attrs.to_ascii_lowercase();
    let mut from = 0;
    while let Some(pos) = lower[from..].find(key) {
        let at = from + pos;
        let before_ok = at == 0 || !lower.as_bytes()[at - 1].is_ascii_alphanumeric();
        let rest = attrs[at + key.len()..].trim_start();
        if before_ok && rest.starts_with('=') {
            let value = rest[1..].trim_start();
            let quote = value.chars().next()?;
            if quote == '"' || quote == '\'' {
                let body = &value[1..];
                return Some(body[..body.find(quote)?].to_string());
            }
            return Some(value.split_whitespace().next().unwrap_or("").to_string());
        }
        from = at + key.len();
    }
    None
}

fn decode_entities(text: &str) -> String {
    text.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

fn badge_color(value: &str) -> Color {
    let named = match value {
        "brightgreen" => "44cc11",
        "green" => "97ca00",
        "yellow" => "dfb317",
        "orange" => "fe7d37",
        "red" => "e05d44",
        "blue" => "007ec6",
        "lightgrey" | "lightgray" => "9f9f9f",
        "grey" | "gray" => "555555",
        other => other,
    };
    let hex = named.trim_start_matches('#');
    match (hex.len(), u32::from_str_radix(hex, 16)) {
        (6, Ok(v)) => Color::Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8),
        _ => Color::Rgb(0x55, 0x55, 0x55),
    }
}

fn on(bg: Color) -> Style {
    let Color::Rgb(r, g, b) = bg else {
        return Style::default().bg(bg);
    };
    let light = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32 > 150.0;
    Style::default().bg(bg).fg(if light { VOID } else { TEXT }).add_modifier(Modifier::BOLD)
}

fn shields_badge(src: &str) -> Option<(String, Color, String, Color)> {
    let after = src.split_once("img.shields.io/badge/")?.1;
    let (path, query) = after.split_once('?').unwrap_or((after, ""));
    let unescape = |t: &str| t.replace("__", "\u{1}").replace('_', " ").replace('\u{1}', "_").replace("%20", " ");
    let parts: Vec<String> = path
        .replace("--", "\u{2}")
        .split('-')
        .map(|p| unescape(&p.replace('\u{2}', "-")))
        .collect();
    let (label, message, color) = match parts.len() {
        0 | 1 => return None,
        2 => (String::new(), parts[0].clone(), parts[1].clone()),
        n => (parts[..n - 2].join("-"), parts[n - 2].clone(), parts[n - 1].clone()),
    };
    let label_color = query
        .split('&')
        .find_map(|kv| kv.strip_prefix("labelColor="))
        .map(badge_color)
        .unwrap_or(Color::Rgb(0x55, 0x55, 0x55));
    Some((label, label_color, message, badge_color(&color)))
}

const HASH_COMMENTS: &[&str] = &[
    "sh", "bash", "zsh", "shell", "console", "fish", "toml", "python", "py", "yaml", "yml", "ruby", "rb", "conf", "ini",
    "make", "makefile", "dockerfile",
];
const SLASH_COMMENTS: &[&str] = &[
    "rust", "rs", "js", "javascript", "ts", "typescript", "jsx", "tsx", "c", "cpp", "c++", "h", "go", "java", "kotlin",
    "swift", "jsonc", "json5", "zig", "css", "scss",
];

fn keywords(lang: &str) -> &'static [&'static str] {
    match lang {
        "sh" | "bash" | "zsh" | "shell" | "console" | "fish" => &[
            "if", "then", "else", "elif", "fi", "for", "in", "do", "done", "while", "case", "esac", "function", "export",
            "local", "return", "set", "source",
        ],
        "rust" | "rs" => &[
            "fn", "let", "mut", "pub", "use", "mod", "struct", "enum", "impl", "trait", "match", "if", "else", "for",
            "while", "loop", "return", "self", "Self", "crate", "const", "static", "as", "in", "where", "async", "await",
            "move", "ref", "type", "dyn", "unsafe", "true", "false",
        ],
        "python" | "py" => &[
            "def", "class", "import", "from", "return", "if", "elif", "else", "for", "while", "in", "not", "and", "or",
            "with", "as", "try", "except", "None", "True", "False", "lambda", "yield",
        ],
        "js" | "javascript" | "ts" | "typescript" | "jsx" | "tsx" => &[
            "const", "let", "var", "function", "return", "if", "else", "for", "while", "import", "export", "from",
            "class", "new", "async", "await", "true", "false", "null", "undefined", "type", "interface",
        ],
        "toml" | "yaml" | "yml" | "json" | "jsonc" | "json5" => &["true", "false", "null"],
        _ => &[],
    }
}

fn highlight(line: &str, lang: &str) -> Vec<Seg> {
    let chars: Vec<char> = line.chars().collect();
    let n = chars.len();
    let lang = lang.to_ascii_lowercase();
    let text = Style::default().fg(TEXT);
    let seg = |a: usize, b: usize, style: Style| Seg {
        text: chars[a..b].iter().collect(),
        style,
        src: a..b,
        verbatim: true,
        link: None,
    };
    if n == 0 {
        return Vec::new();
    }
    if lang == "diff" || lang == "patch" {
        let style = match chars[0] {
            '+' => Style::default().fg(ICE),
            '-' => Style::default().fg(HOT),
            '@' => Style::default().fg(AMBER),
            _ => text,
        };
        return vec![seg(0, n, style)];
    }
    if lang == "toml" && line.trim_start().starts_with('[') {
        return vec![seg(0, n, Style::default().fg(ICE).add_modifier(Modifier::BOLD))];
    }
    let hash = HASH_COMMENTS.contains(&lang.as_str());
    let slash = SLASH_COMMENTS.contains(&lang.as_str());
    let words = keywords(&lang);
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = Vec::new();
    let mut i = 0;
    if lang == "console" && line.starts_with("$ ") {
        out.push(seg(0, 1, Style::default().fg(HOT)));
        i = 1;
    }
    while i < n {
        let c = chars[i];
        let comment = (hash && c == '#' && (i == 0 || chars[i - 1].is_whitespace()))
            || (slash && c == '/' && chars.get(i + 1) == Some(&'/'));
        if comment {
            out.push(seg(i, n, Style::default().fg(GHOST).add_modifier(Modifier::ITALIC)));
            break;
        }
        if c == '"' || c == '`' || (c == '\'' && lang != "rust" && lang != "rs") {
            let mut j = i + 1;
            while j < n && chars[j] != c {
                j += if chars[j] == '\\' { 2 } else { 1 };
            }
            let end = (j + 1).min(n);
            out.push(seg(i, end, Style::default().fg(AMBER)));
            i = end;
            continue;
        }
        if c.is_ascii_digit() && (i == 0 || !is_word(chars[i - 1])) {
            let mut j = i;
            while j < n && (chars[j].is_ascii_alphanumeric() || chars[j] == '.' || chars[j] == '_') {
                j += 1;
            }
            out.push(seg(i, j, Style::default().fg(HOT)));
            i = j;
            continue;
        }
        if is_word(c) {
            let mut j = i;
            while j < n && is_word(chars[j]) {
                j += 1;
            }
            let word: String = chars[i..j].iter().collect();
            let style = if words.contains(&word.as_str()) { Style::default().fg(ICE) } else { text };
            out.push(seg(i, j, style));
            i = j;
            continue;
        }
        out.push(seg(i, i + 1, text));
        i += 1;
    }
    out
}


pub fn build(lines: &[String]) -> MdView {
    let mut starts = Vec::with_capacity(lines.len());
    let mut src = String::new();
    for l in lines {
        starts.push(src.len());
        src.push_str(l);
        src.push('\n');
    }
    let line_of = |b: usize| starts.partition_point(|&s| s <= b).saturating_sub(1);

    let mut kinds: Vec<LineKind> = vec![LineKind::Plain; lines.len()];
    let mut pieces: Vec<Vec<Piece>> = vec![Vec::new(); lines.len()];
    let mut quote_lines = vec![false; lines.len()];

    let mut style_stack: Vec<Style> = vec![Style::default().fg(TEXT)];
    let mut list_stack: Vec<Option<u64>> = Vec::new();
    let mut pending_bullet: Option<(usize, usize)> = None;
    let mut tables: Vec<Table> = Vec::new();
    let mut cells: Vec<Vec<(usize, usize)>> = vec![Vec::new(); lines.len()];
    let mut current_row: Option<usize> = None;
    let mut quote_kind: Vec<Option<usize>> = vec![None; lines.len()];
    let mut code_lang: Vec<Option<String>> = vec![None; lines.len()];
    let mut centered = vec![false; lines.len()];
    let mut decor = vec![false; lines.len()];
    let mut links: Vec<String> = Vec::new();
    let mut html: Vec<(String, Style)> = Vec::new();
    let mut center_stack: Vec<Option<usize>> = Vec::new();

    let push = |pieces: &mut Vec<Vec<Piece>>, r: Range<usize>, text: Option<&str>, style: Style, marker: bool, link: Option<&String>| {
        let (mut b, e) = (r.start, r.end);
        while b < e {
            let li = line_of(b);
            let ls = starts[li];
            let le = ls + lines[li].len();
            let pe = e.min(le);
            if pe > b {
                let slice = &src[b..pe];
                let (t, verbatim) = match text {
                    Some(t) if t != slice && pe == e && b == r.start => (t.to_string(), false),
                    _ => (slice.to_string(), true),
                };
                pieces[li].push(Piece { start: b - ls, end: pe - ls, text: t, style, verbatim, marker, link: link.cloned() });
            }
            b = le + 1;
        }
    };

    let at = |pieces: &mut Vec<Vec<Piece>>, byte: usize, text: String, style: Style, link: Option<&String>| {
        let li = line_of(byte);
        let col = (byte - starts[li]).min(lines[li].len());
        pieces[li].push(Piece { start: col, end: col, text, style, verbatim: false, marker: false, link: link.cloned() });
    };

    let opts = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS | Options::ENABLE_GFM;
    for (ev, r) in Parser::new_ext(&src, opts).into_offset_iter() {
        let base = *style_stack.last().unwrap();
        let top = html.iter().fold(base, |s, (_, o)| s.patch(*o));
        match ev {
            Event::Start(tag) => {
                let s = match &tag {
                    Tag::Heading { level, .. } => {
                        let lv = level_num(*level);
                        let (a, z) = (line_of(r.start), line_of(r.end.saturating_sub(1)));
                        for k in &mut kinds[a..=z] {
                            *k = LineKind::Heading(lv);
                        }
                        if z > a && !lines[z].trim_start().starts_with('#') {
                            kinds[z] = LineKind::SetextUnderline(lv);
                        }
                        heading_style(lv)
                    }
                    Tag::Emphasis => top.add_modifier(Modifier::ITALIC),
                    Tag::Strong => top.add_modifier(Modifier::BOLD),
                    Tag::Strikethrough => top.add_modifier(Modifier::CROSSED_OUT).fg(GHOST),
                    Tag::Link { dest_url, .. } => {
                        links.push(dest_url.to_string());
                        top.fg(ICE).add_modifier(Modifier::UNDERLINED)
                    }
                    Tag::Image { .. } => top.fg(AMBER).add_modifier(Modifier::ITALIC),
                    Tag::BlockQuote(kind) => {
                        let (a, z) = (line_of(r.start), line_of(r.end.saturating_sub(1)));
                        let alert = kind.map(alert_index);
                        for li in a..=z {
                            quote_lines[li] = true;
                            if alert.is_some() {
                                quote_kind[li] = alert;
                            }
                        }
                        match alert {
                            Some(i) => {
                                kinds[a] = LineKind::AlertTitle(i);
                                top.fg(TEXT)
                            }
                            None => top.fg(GHOST).add_modifier(Modifier::ITALIC),
                        }
                    }
                    Tag::CodeBlock(kind) => {
                        let (a, z) = (line_of(r.start), line_of(r.end.saturating_sub(1)));
                        let lang = match kind {
                            CodeBlockKind::Fenced(lang) => lang.split_whitespace().next().unwrap_or("").to_string(),
                            CodeBlockKind::Indented => String::new(),
                        };
                        for li in a..=z {
                            kinds[li] = LineKind::Code;
                            code_lang[li] = Some(lang.clone());
                        }
                        if let CodeBlockKind::Fenced(lang) = kind {
                            kinds[a] = LineKind::FenceOpen(lang.to_string());
                            let t = lines[z].trim_start();
                            if z > a && (t.starts_with("```") || t.starts_with("~~~")) {
                                kinds[z] = LineKind::FenceClose;
                            }
                        }
                        Style::default().fg(TEXT)
                    }
                    Tag::List(start) => {
                        list_stack.push(*start);
                        top
                    }
                    Tag::Item => {
                        let li = line_of(r.start);
                        let col = r.start - starts[li];
                        let line = &lines[li];
                        let mk_len = line[col..].find(|c: char| !(c.is_ascii_digit() || matches!(c, '-' | '*' | '+' | '.' | ')'))).unwrap_or(line.len() - col);
                        let ws = line[col + mk_len..].len() - line[col + mk_len..].trim_start().len();
                        let depth = list_stack.len();
                        let (glyph, st) = match list_stack.last().copied().flatten() {
                            Some(_) => (format!("{} ", &line[col..col + mk_len]), Style::default().fg(AMBER)),
                            None => (
                                format!("{} ", ["•", "◦", "▪"][(depth - 1) % 3]),
                                Style::default().fg(HOT),
                            ),
                        };
                        push(&mut pieces, r.start..r.start + mk_len + ws.min(1), Some(&glyph), st, true, None);
                        pending_bullet = Some((li, pieces[li].len() - 1));
                        top
                    }
                    Tag::Table(aligns) => {
                        tables.push(Table { aligns: aligns.clone(), rows: Vec::new() });
                        top
                    }
                    Tag::TableCell => {
                        if let Some(li) = current_row {
                            let len = lines[li].len();
                            let from = r.start.saturating_sub(starts[li]).min(len);
                            let to = r.end.saturating_sub(starts[li]).min(len).max(from);
                            cells[li].push((from, to));
                        }
                        top
                    }
                    Tag::TableHead | Tag::TableRow => {
                        let li = line_of(r.start);
                        current_row = Some(li);
                        if let Some(t) = tables.last_mut() {
                            t.rows.push(li);
                        }
                        if matches!(tag, Tag::TableHead) {
                            top.add_modifier(Modifier::BOLD).fg(ICE)
                        } else {
                            top
                        }
                    }
                    _ => top,
                };
                style_stack.push(s);
            }
            Event::End(end) => {
                style_stack.pop();
                if matches!(
                    end,
                    TagEnd::Paragraph | TagEnd::Item | TagEnd::HtmlBlock | TagEnd::Heading(_) | TagEnd::TableCell | TagEnd::BlockQuote(_)
                ) {
                    html.clear();
                }
                match end {
                    TagEnd::Link => {
                        links.pop();
                    }
                    TagEnd::List(_) => {
                        list_stack.pop();
                    }
                    TagEnd::Item => pending_bullet = None,
                    _ => {}
                }
            }
            Event::TaskListMarker(done) => {
                if let Some((li, pi)) = pending_bullet.take() {
                    pieces[li][pi].text.clear();
                    pieces[li][pi].verbatim = false;
                }
                let (g, st) = if done {
                    ("✓", Style::default().fg(GHOST))
                } else {
                    ("□", Style::default().fg(AMBER))
                };
                push(&mut pieces, r, Some(g), st, true, None);
            }
            Event::Text(t) => push(&mut pieces, r, Some(&t), top, false, links.last()),
            Event::Code(_) => {
                let s = &src[r.clone()];
                let n = s.len() - s.trim_start_matches('`').len();
                let mut inner = r.start + n..r.end - n;
                let is = &src[inner.clone()];
                if is.len() >= 2 && is.starts_with([' ', '\n']) && is.ends_with([' ', '\n']) && !is.trim().is_empty() {
                    inner = inner.start + 1..inner.end - 1;
                }
                push(&mut pieces, inner, None, top.fg(AMBER).bg(CODE_BG), false, links.last());
            }
            Event::Html(_) | Event::InlineHtml(_) => {
                for frag in html_frags(&src, r.clone()) {
                    let top = html.iter().fold(base, |s, (_, o)| s.patch(*o));
                    let hide = |pieces: &mut Vec<Vec<Piece>>, range: Range<usize>| push(pieces, range, Some(""), top, false, None);
                    match frag {
                        Frag::Comment(range) => hide(&mut pieces, range),
                        Frag::Text(range) => {
                            let raw = &src[range.clone()];
                            if raw.trim().is_empty() {
                                continue;
                            }
                            let shown = decode_entities(raw);
                            let text = if shown == raw { None } else { Some(shown.as_str()) };
                            push(&mut pieces, range, text, top, false, links.last());
                        }
                        Frag::Tag { range, name, closing, attrs } => {
                            let li = line_of(range.start);
                            let pop = |html: &mut Vec<(String, Style)>, name: &str| {
                                if let Some(i) = html.iter().rposition(|(n, _)| n == name) {
                                    html.truncate(i);
                                    true
                                } else {
                                    false
                                }
                            };
                            let overlay = match name.as_str() {
                                "b" | "strong" => Some(Style::default().add_modifier(Modifier::BOLD)),
                                "i" | "em" => Some(Style::default().add_modifier(Modifier::ITALIC)),
                                "code" | "tt" => Some(Style::default().fg(AMBER).bg(CODE_BG)),
                                "sub" | "sup" | "small" => Some(Style::default().fg(GHOST)),
                                "s" | "del" | "strike" => Some(Style::default().add_modifier(Modifier::CROSSED_OUT)),
                                "u" | "ins" => Some(Style::default().add_modifier(Modifier::UNDERLINED)),
                                _ => None,
                            };
                            match (name.as_str(), closing) {
                                ("kbd", false) => {
                                    push(&mut pieces, range, Some("▐"), Style::default().fg(KEYCAP), false, links.last());
                                    html.push((name, Style::default().bg(KEYCAP).fg(TEXT)));
                                }
                                ("kbd", true) => {
                                    pop(&mut html, "kbd");
                                    push(&mut pieces, range, Some("▌"), Style::default().fg(KEYCAP), false, links.last());
                                }
                                ("summary", false) => {
                                    let st = Style::default().fg(ICE).add_modifier(Modifier::BOLD);
                                    push(&mut pieces, range, Some("▸ "), st, false, None);
                                    html.push((name, st));
                                }
                                ("a", false) => {
                                    hide(&mut pieces, range);
                                    links.push(attr(&attrs, "href").unwrap_or_default());
                                    html.push((name, Style::default().fg(ICE).add_modifier(Modifier::UNDERLINED)));
                                }
                                ("a", true) => {
                                    hide(&mut pieces, range);
                                    if pop(&mut html, "a") {
                                        links.pop();
                                    }
                                }
                                ("img", _) => {
                                    let alt = attr(&attrs, "alt").unwrap_or_default();
                                    let src_attr = attr(&attrs, "src").unwrap_or_default();
                                    if let Some((label, label_bg, message, message_bg)) = shields_badge(&src_attr) {
                                        let end = range.end;
                                        if label.is_empty() {
                                            hide(&mut pieces, range);
                                        } else {
                                            let shown = format!(" {} ", label);
                                            push(&mut pieces, range, Some(&shown), on(label_bg), false, links.last());
                                        }
                                        at(&mut pieces, end, format!(" {} ", message), on(message_bg), links.last());
                                    } else if alt.trim().is_empty() {
                                        decor[li] = true;
                                        hide(&mut pieces, range);
                                    } else {
                                        let mut shown: String = alt.chars().take(48).collect();
                                        if alt.chars().count() > 48 {
                                            shown.push('…');
                                        }
                                        let st = Style::default().fg(GHOST).add_modifier(Modifier::ITALIC);
                                        let shown = format!("◩ {}", shown);
                                        push(&mut pieces, range, Some(&shown), st, false, links.last());
                                    }
                                }
                                ("div" | "p", false) => {
                                    let center = attr(&attrs, "align").is_some_and(|a| a.eq_ignore_ascii_case("center"));
                                    center_stack.push(center.then_some(li));
                                    hide(&mut pieces, range);
                                }
                                ("div" | "p", true) => {
                                    if let Some(Some(from)) = center_stack.pop() {
                                        for c in &mut centered[from..=li] {
                                            *c = true;
                                        }
                                    }
                                    hide(&mut pieces, range);
                                }
                                (_, true) if overlay.is_some() || name == "summary" => {
                                    hide(&mut pieces, range);
                                    pop(&mut html, &name);
                                }
                                (_, false) if overlay.is_some() => {
                                    hide(&mut pieces, range);
                                    html.push((name, overlay.unwrap()));
                                }
                                _ => hide(&mut pieces, range),
                            }
                        }
                    }
                }
            }
            Event::Rule => kinds[line_of(r.start)] = LineKind::Rule,
            _ => {}
        }
    }

    let mut rendered: Vec<Option<RenderedLine>> = vec![None; lines.len()];
    for table in &tables {
        let contents: Vec<Vec<Vec<Seg>>> = table
            .rows
            .iter()
            .map(|&li| {
                let mut ps = std::mem::take(&mut pieces[li]);
                ps.sort_by_key(|p| p.start);
                cells[li].iter().map(|&cell| cell_content(&lines[li], cell, &ps)).collect()
            })
            .collect();
        let aligns: Vec<Alignment> = table
            .aligns
            .iter()
            .enumerate()
            .map(|(c, &align)| {
                let body: Vec<String> = contents[1..]
                    .iter()
                    .filter_map(|row| row.get(c))
                    .map(|cell| cell.iter().map(|s| s.text.as_str()).collect())
                    .filter(|t: &String| !t.trim().is_empty())
                    .collect();
                if align == Alignment::None && !body.is_empty() && body.iter().all(|t| is_number(t)) {
                    Alignment::Right
                } else {
                    align
                }
            })
            .collect();
        let mut widths = vec![1; table.aligns.len()];
        for row in &contents {
            for (c, content) in row.iter().enumerate().take(widths.len()) {
                widths[c] = widths[c].max(seg_width(content));
            }
        }
        for (ri, (&li, row)) in table.rows.iter().zip(contents).enumerate() {
            let band = match ri {
                0 => Some(TABLE_HEAD),
                _ if ri % 2 == 0 => Some(TABLE_STRIPE),
                _ => None,
            };
            rendered[li] = Some(table_row(&lines[li], &cells[li], row, &widths, &aligns, band));
        }
        if let Some(&head) = table.rows.first() {
            if head + 1 < lines.len() {
                rendered[head + 1] = Some(table_rule(&widths, lines[head + 1].chars().count()));
            }
        }
    }

    let out = lines
        .iter()
        .enumerate()
        .map(|(li, line)| {
            if let Some(row) = rendered[li].take() {
                return row;
            }
            let mut kind = std::mem::take(&mut kinds[li]);
            let mut ps = std::mem::take(&mut pieces[li]);
            ps.sort_by_key(|p| p.start);
            if decor[li] && ps.iter().all(|p| p.text.is_empty()) {
                kind = LineKind::Rule;
            }
            let heading = matches!(kind, LineKind::Heading(_));
            let mut rl = compose(line, kind, ps, quote_lines[li], quote_kind[li], code_lang[li].as_deref());
            rl.centered = centered[li];
            if heading {
                rl.anchor = Some(slug(&rl.segs.iter().map(|s| s.text.as_str()).collect::<String>()));
            }
            rl
        })
        .collect();
    MdView { lines: out }
}

fn compose(line: &str, kind: LineKind, ps: Vec<Piece>, in_quote: bool, alert: Option<usize>, lang: Option<&str>) -> RenderedLine {
    let cc = |b: usize| line[..b].chars().count();
    let n = line.chars().count();
    let dim = Style::default().fg(GHOST);
    let whole = |text: String, style: Style| Seg { text, style, src: 0..n, verbatim: false, link: None };
    let mut rl = RenderedLine::default();
    match &kind {
        LineKind::FenceOpen(lang) => {
            let label = if lang.is_empty() { "──".to_string() } else { format!("── {lang} ") };
            rl.segs.push(whole(label, dim));
            rl.fill = Some(('─', dim));
            return rl;
        }
        LineKind::FenceClose | LineKind::Rule => {
            rl.segs.push(Seg { text: String::new(), style: dim, src: 0..n, verbatim: false, link: None });
            rl.fill = Some(('─', dim));
            return rl;
        }
        LineKind::SetextUnderline(lv) => {
            rl.segs.push(Seg { text: String::new(), style: dim, src: 0..n, verbatim: false, link: None });
            rl.fill = Some((if *lv == 1 { '═' } else { '─' }, heading_style(*lv).remove_modifier(Modifier::UNDERLINED)));
            return rl;
        }
        LineKind::Code => {
            rl.bg = Some(CODE_BG);
            rl.segs = highlight(line, lang.unwrap_or(""));
            return rl;
        }
        LineKind::AlertTitle(i) => {
            let (glyph, label, color) = ALERTS[*i];
            let st = Style::default().fg(color).add_modifier(Modifier::BOLD);
            rl.segs.push(whole(format!("▌ {} {}", glyph, label), st));
            return rl;
        }
        LineKind::Plain if ps.is_empty() && !line.trim().is_empty() => {
            rl.segs.push(Seg { text: line.to_string(), style: dim, src: 0..n, verbatim: true, link: None });
            return rl;
        }
        _ => {}
    }
    let heading = matches!(kind, LineKind::Heading(_));
    let mut leading = true;
    let mut pos = 0;
    let gap = |rl: &mut RenderedLine, from: usize, to: usize, leading: bool| {
        for (off, c) in line[from..to].char_indices() {
            let b = from + off;
            let col = cc(b);
            let (t, st) = match c {
                '>' if leading && in_quote => ("▌", Style::default().fg(alert.map_or(HOT, |i| ALERTS[i].2))),
                c if c.is_whitespace() && leading && !heading => (" ", Style::default()),
                _ => ("", Style::default()),
            };
            rl.segs.push(Seg { text: t.to_string(), style: st, src: col..col + 1, verbatim: t.len() == 1 && c == ' ', link: None });
        }
    };
    for p in ps {
        if p.start < pos {
            continue;
        }
        gap(&mut rl, pos, p.start, leading);
        if !p.marker {
            leading = false;
        }
        rl.segs.push(Seg { text: p.text, style: p.style, src: cc(p.start)..cc(p.end), verbatim: p.verbatim, link: p.link });
        pos = p.end;
    }
    gap(&mut rl, pos, line.len(), false);
    rl.segs.retain(|s| !(s.text.is_empty() && s.src.is_empty()));
    rl
}

fn is_number(text: &str) -> bool {
    let t = text.trim().trim_start_matches(['$', '€', '£', '¥']).trim_end_matches('%');
    let digits: String = t.chars().filter(|c| *c != ',' && *c != '_').collect();
    !digits.is_empty() && digits.parse::<f64>().is_ok()
}

fn seg_width(segs: &[Seg]) -> usize {
    segs.iter().map(|s| s.text.chars().count()).sum()
}

fn cell_content(line: &str, (from, to): (usize, usize), pieces: &[Piece]) -> Vec<Seg> {
    let cc = |b: usize| line[..b].chars().count();
    let mut segs = Vec::new();
    let mut pos: Option<usize> = None;
    for p in pieces.iter().filter(|p| p.start >= from && p.end <= to) {
        if let Some(prev) = pos {
            for (off, c) in line[prev..p.start].char_indices() {
                if c.is_whitespace() {
                    let col = cc(prev + off);
                    segs.push(Seg { text: " ".into(), style: Style::default(), src: col..col + 1, verbatim: true, link: None });
                }
            }
        }
        segs.push(Seg { text: p.text.clone(), style: p.style, src: cc(p.start)..cc(p.end), verbatim: p.verbatim, link: p.link.clone() });
        pos = Some(p.end);
    }
    segs
}

fn table_row(
    line: &str,
    cells: &[(usize, usize)],
    contents: Vec<Vec<Seg>>,
    widths: &[usize],
    aligns: &[Alignment],
    band: Option<Color>,
) -> RenderedLine {
    let cc = |b: usize| line[..b.min(line.len())].chars().count();
    let base = match band {
        Some(bg) => Style::default().bg(bg),
        None => Style::default(),
    };
    let border = base.fg(GHOST);
    let filler = |text: String, col: usize| Seg { text, style: base, src: col..col + 1, verbatim: false, link: None };
    let first = cells.first().map_or(0, |c| c.0);
    let lead = if first > 0 && line.as_bytes()[first - 1] == b'|' { first - 1 } else { first };
    let mut rl = RenderedLine::default();
    rl.segs.push(Seg { text: "│".into(), style: border, src: cc(lead)..cc(lead) + 1, verbatim: false, link: None });
    let mut contents = contents.into_iter();
    for (c, &width) in widths.iter().enumerate() {
        let (from, to) = cells.get(c).copied().unwrap_or((line.len(), line.len()));
        let content = contents.next().unwrap_or_default();
        let pad = width - seg_width(&content).min(width);
        let (left, right) = match aligns.get(c) {
            Some(Alignment::Right) => (pad, 0),
            Some(Alignment::Center) => (pad / 2, pad - pad / 2),
            _ => (0, pad),
        };
        let start = content.first().map_or(cc(from), |s| s.src.start);
        let end = content.last().map_or(cc(to), |s| s.src.end);
        rl.segs.push(filler(" ".repeat(left + 1), start));
        for mut seg in content {
            seg.style = base.patch(seg.style);
            rl.segs.push(seg);
        }
        rl.segs.push(filler(" ".repeat(right + 1), end));
        rl.segs.push(Seg { text: "│".into(), style: border, src: cc(to)..cc(to) + 1, verbatim: false, link: None });
    }
    rl
}

fn table_rule(widths: &[usize], line_len: usize) -> RenderedLine {
    let bars: Vec<String> = widths.iter().map(|w| "═".repeat(w + 2)).collect();
    let text = format!("╞{}╡", bars.join("╪"));
    let style = Style::default().fg(ICE);
    RenderedLine {
        segs: vec![Seg { text, style, src: 0..line_len, verbatim: false, link: None }],
        ..Default::default()
    }
}

impl RenderedLine {
    pub fn cells(&self) -> Vec<(char, Style, usize)> {
        let mut v = Vec::new();
        for s in &self.segs {
            let len = s.src.end - s.src.start;
            for (i, c) in s.text.chars().enumerate() {
                let col = if s.verbatim && i < len { s.src.start + i } else { s.src.start };
                v.push((c, s.style, col));
            }
        }
        v
    }

    pub fn link_at(&self, dcol: usize) -> Option<&str> {
        let mut d = 0;
        for s in &self.segs {
            let w = s.text.chars().count();
            if dcol < d + w {
                return s.link.as_deref();
            }
            d += w;
        }
        None
    }

    pub fn display_to_source(&self, dcol: usize, line_len: usize) -> usize {
        self.cells().get(dcol).map(|c| c.2).unwrap_or(line_len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(text: &str) -> Vec<String> {
        let lines: Vec<String> = text.split('\n').map(|s| s.to_string()).collect();
        build(&lines)
            .lines
            .iter()
            .map(|l| {
                let mut s: String = l.cells().iter().map(|c| c.0).collect();
                if let Some((c, _)) = l.fill {
                    s.push(c);
                }
                s
            })
            .collect()
    }

    #[test]
    fn each_element_renders_on_its_own_row() {
        let cases = [
            ("## Loadout ##", "Loadout"),
            ("A **bold** *it* ~~no~~ `code`", "A bold it no code"),
            ("See [the brief](https://x.y \"t\").", "See the brief."),
            ("- one\n  - two", "• one\n  ◦ two"),
            ("1. first", "1. first"),
            ("- [x] done\n- [ ] todo", "✓ done\n□ todo"),
            ("> quoted", "▌ quoted"),
            ("---", "─"),
            ("```rust\nlet x = 1;\n```", "── rust ─\nlet x = 1;\n─"),
            ("| a | b |\n|---|---|\n| 1 | 2 |", "│ a │ b │\n╞═══╪═══╡\n│ 1 │ 2 │"),
            ("\\*not em\\*", "*not em*"),
            ("[ref]: https://x.y", "[ref]: https://x.y"),
            ("> [!WARNING]\n> careful", "▌ ▲ Warning\n▌ careful"),
            ("Press <kbd>Ctrl</kbd>+<kbd>S</kbd>", "Press ▐Ctrl▌+▐S▌"),
            ("<b>bold</b> and <sub>small</sub><br> a <!-- hidden -->b", "bold and small a b"),
            ("<details>\n<summary><b>Keys</b></summary>", "\n▸ Keys"),
            ("<img src=\"x.svg\" alt=\"a cat\">", "◩ a cat"),
            ("<img src=\"d.svg\" width=\"100%\" alt=\"\">", "─"),
            (
                "<img alt=\"b\" src=\"https://img.shields.io/badge/built_with-Rust-ff2a6d?labelColor=0b0620\">",
                " built with  Rust ",
            ),
        ];
        for (source, expected) in cases {
            let rows = source.split('\n').count();
            let got = shown(source);
            assert_eq!(got.len(), rows, "one row per source line: {:?}", source);
            assert_eq!(got.join("\n"), expected, "source: {:?}", source);
        }
    }

    #[test]
    fn tables_align_columns_and_right_align_numbers() {
        let source = "| a | num | note |\n|:-:|---|---|\n| xyz | 5 | **hi** |\n| q | 1,200 |\n| w | $3 | ok |";
        assert_eq!(
            shown(source),
            [
                "│  a  │   num │ note │",
                "╞═════╪═══════╪══════╡",
                "│ xyz │     5 │ hi   │",
                "│  q  │ 1,200 │      │",
                "│  w  │    $3 │ ok   │",
            ]
        );
        assert_eq!(shown("x | y\n--|--\nlong | 2"), ["│ x    │ y │", "╞══════╪═══╡", "│ long │ 2 │"]);
    }

    #[test]
    fn padded_table_cells_map_back_to_their_source() {
        let lines: Vec<String> = ["| key | v |", "|---|---:|", "| k | 42 |"].iter().map(|s| s.to_string()).collect();
        let view = build(&lines);
        let row = &view.lines[2];
        let shown: String = row.cells().iter().map(|c| c.0).collect();
        assert_eq!(shown, "│ k   │ 42 │");
        let len = lines[2].chars().count();
        let source: Vec<char> = lines[2].chars().collect();
        for (d, ch) in shown.chars().enumerate() {
            if ch.is_alphanumeric() {
                assert_eq!(source[row.display_to_source(d, len)], ch, "display col {}", d);
            }
        }
    }

    #[test]
    fn display_columns_map_back_to_source_columns() {
        let lines = vec!["a *it* `c` [l](u)".to_string()];
        let view = build(&lines);
        let line = &view.lines[0];
        let len = lines[0].chars().count();
        assert_eq!(line.display_to_source(0, len), 0);
        assert_eq!(line.display_to_source(2, len), 3, "'i' of *it*");
        assert_eq!(line.display_to_source(5, len), 8, "'c' inside the backticks");
        assert_eq!(line.display_to_source(7, len), 12, "'l' link text");
        assert_eq!(line.display_to_source(99, len), len, "past the end");
    }


    fn view(text: &str) -> MdView {
        let lines: Vec<String> = text.split('\n').map(|s| s.to_string()).collect();
        build(&lines)
    }

    #[test]
    fn alerts_colour_their_bar_and_badges_use_their_colours() {
        let v = view("> [!TIP]\n> go");
        assert_eq!(v.lines[1].cells()[0].1.fg, Some(NEON));
        let v = view("<img alt=\"b\" src=\"https://img.shields.io/badge/ok-yes-ff2a6d?labelColor=0b0620\">");
        let cells = v.lines[0].cells();
        assert_eq!((cells[1].0, cells[1].1.bg), ('o', Some(VOID)));
        assert_eq!((cells[5].0, cells[5].1.bg), ('y', Some(HOT)));
    }

    #[test]
    fn centred_html_blocks_mark_only_their_lines() {
        let v = view("before\n<div align=\"center\">\n\n# Title\n\n</div>\nafter");
        let centered: Vec<bool> = v.lines.iter().map(|l| l.centered).collect();
        assert_eq!(centered, [false, true, true, true, true, true, false]);
    }

    #[test]
    fn code_blocks_are_highlighted_by_language() {
        let v = view("```sh\necho \"hi\" # note\n```\n```diff\n+ add\n- drop\n```");
        let style_of = |line: usize, ch: char| v.lines[line].cells().into_iter().find(|c| c.0 == ch).unwrap().1.fg;
        assert_eq!(style_of(1, 'e'), Some(TEXT));
        assert_eq!(style_of(1, 'i'), Some(AMBER), "string");
        assert_eq!(style_of(1, 'n'), Some(GHOST), "comment");
        assert_eq!(style_of(4, 'a'), Some(ICE), "diff addition");
        assert_eq!(style_of(5, 'd'), Some(HOT), "diff removal");
    }

    #[test]
    fn links_are_attached_to_their_text_and_headings_get_anchors() {
        let v = view("[brief](https://x.y) and <a href=\"#faq\">faq</a> <kbd>[k](#keys)</kbd>\n\n## The FAQ!");
        let line = &v.lines[0];
        let shown: String = line.cells().iter().map(|c| c.0).collect();
        let col = |needle: &str| shown.find(needle).map(|b| shown[..b].chars().count()).unwrap();
        assert_eq!(line.link_at(col("brief")), Some("https://x.y"));
        assert_eq!(line.link_at(col("faq")), Some("#faq"));
        assert_eq!(line.link_at(col("k")), Some("#keys"));
        assert_eq!(line.link_at(col(" and")), None);
        assert_eq!(v.lines[2].anchor.as_deref(), Some("the-faq"));
    }
}
