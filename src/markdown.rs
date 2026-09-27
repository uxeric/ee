use std::ops::Range;

use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};

use crate::theme::{AMBER, CODE_BG, GHOST, HOT, ICE, TABLE_HEAD, TABLE_STRIPE, TEXT};

#[derive(Clone, Debug, PartialEq)]
pub struct Seg {
    pub text: String,
    pub style: Style,
    pub src: Range<usize>,
    pub verbatim: bool,
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
}

#[derive(Clone, Debug, Default)]
pub struct RenderedLine {
    pub segs: Vec<Seg>,
    pub fill: Option<(char, Style)>,
    pub bg: Option<Color>,
}

#[derive(Clone, Debug)]
struct Piece {
    start: usize,
    end: usize,
    text: String,
    style: Style,
    verbatim: bool,
    marker: bool,
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

    let push = |pieces: &mut Vec<Vec<Piece>>, r: Range<usize>, text: Option<&str>, style: Style, marker: bool| {
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
                pieces[li].push(Piece { start: b - ls, end: pe - ls, text: t, style, verbatim, marker });
            }
            b = le + 1;
        }
    };

    let opts = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    for (ev, r) in Parser::new_ext(&src, opts).into_offset_iter() {
        let top = *style_stack.last().unwrap();
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
                    Tag::Link { .. } => top.fg(ICE).add_modifier(Modifier::UNDERLINED),
                    Tag::Image { .. } => top.fg(AMBER).add_modifier(Modifier::ITALIC),
                    Tag::BlockQuote(_) => {
                        for q in &mut quote_lines[line_of(r.start)..=line_of(r.end.saturating_sub(1))] {
                            *q = true;
                        }
                        top.fg(GHOST).add_modifier(Modifier::ITALIC)
                    }
                    Tag::CodeBlock(kind) => {
                        let (a, z) = (line_of(r.start), line_of(r.end.saturating_sub(1)));
                        for k in &mut kinds[a..=z] {
                            *k = LineKind::Code;
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
                        push(&mut pieces, r.start..r.start + mk_len + ws.min(1), Some(&glyph), st, true);
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
                match end {
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
                push(&mut pieces, r, Some(g), st, true);
            }
            Event::Text(t) => push(&mut pieces, r, Some(&t), top, false),
            Event::Code(_) => {
                let s = &src[r.clone()];
                let n = s.len() - s.trim_start_matches('`').len();
                let mut inner = r.start + n..r.end - n;
                let is = &src[inner.clone()];
                if is.len() >= 2 && is.starts_with([' ', '\n']) && is.ends_with([' ', '\n']) && !is.trim().is_empty() {
                    inner = inner.start + 1..inner.end - 1;
                }
                push(&mut pieces, inner, None, top.fg(AMBER).bg(CODE_BG), false);
            }
            Event::Html(_) | Event::InlineHtml(_) => {
                push(&mut pieces, r, None, Style::default().fg(GHOST), false);
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
            let kind = std::mem::take(&mut kinds[li]);
            let mut ps = std::mem::take(&mut pieces[li]);
            ps.sort_by_key(|p| p.start);
            compose(line, kind, ps, quote_lines[li])
        })
        .collect();
    MdView { lines: out }
}

fn compose(line: &str, kind: LineKind, ps: Vec<Piece>, in_quote: bool) -> RenderedLine {
    let cc = |b: usize| line[..b].chars().count();
    let n = line.chars().count();
    let dim = Style::default().fg(GHOST);
    let whole = |text: String, style: Style| Seg { text, style, src: 0..n, verbatim: false };
    let mut rl = RenderedLine::default();
    match &kind {
        LineKind::FenceOpen(lang) => {
            let label = if lang.is_empty() { "──".to_string() } else { format!("── {lang} ") };
            rl.segs.push(whole(label, dim));
            rl.fill = Some(('─', dim));
            return rl;
        }
        LineKind::FenceClose | LineKind::Rule => {
            rl.segs.push(Seg { text: String::new(), style: dim, src: 0..n, verbatim: false });
            rl.fill = Some(('─', dim));
            return rl;
        }
        LineKind::SetextUnderline(lv) => {
            rl.segs.push(Seg { text: String::new(), style: dim, src: 0..n, verbatim: false });
            rl.fill = Some((if *lv == 1 { '═' } else { '─' }, heading_style(*lv).remove_modifier(Modifier::UNDERLINED)));
            return rl;
        }
        LineKind::Code => rl.bg = Some(CODE_BG),
        LineKind::Plain if ps.is_empty() && !line.trim().is_empty() => {
            rl.segs.push(Seg { text: line.to_string(), style: dim, src: 0..n, verbatim: true });
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
                '>' if leading && in_quote => ("▌", Style::default().fg(HOT)),
                c if c.is_whitespace() && leading && !heading => (" ", Style::default()),
                _ => ("", Style::default()),
            };
            rl.segs.push(Seg { text: t.to_string(), style: st, src: col..col + 1, verbatim: t.len() == 1 && c == ' ' });
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
        rl.segs.push(Seg { text: p.text, style: p.style, src: cc(p.start)..cc(p.end), verbatim: p.verbatim });
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
                    segs.push(Seg { text: " ".into(), style: Style::default(), src: col..col + 1, verbatim: true });
                }
            }
        }
        segs.push(Seg { text: p.text.clone(), style: p.style, src: cc(p.start)..cc(p.end), verbatim: p.verbatim });
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
    let filler = |text: String, col: usize| Seg { text, style: base, src: col..col + 1, verbatim: false };
    let first = cells.first().map_or(0, |c| c.0);
    let lead = if first > 0 && line.as_bytes()[first - 1] == b'|' { first - 1 } else { first };
    let mut rl = RenderedLine::default();
    rl.segs.push(Seg { text: "│".into(), style: border, src: cc(lead)..cc(lead) + 1, verbatim: false });
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
        rl.segs.push(Seg { text: "│".into(), style: border, src: cc(to)..cc(to) + 1, verbatim: false });
    }
    rl
}

fn table_rule(widths: &[usize], line_len: usize) -> RenderedLine {
    let bars: Vec<String> = widths.iter().map(|w| "═".repeat(w + 2)).collect();
    let text = format!("╞{}╡", bars.join("╪"));
    let style = Style::default().fg(ICE);
    RenderedLine {
        segs: vec![Seg { text, style, src: 0..line_len, verbatim: false }],
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
}
