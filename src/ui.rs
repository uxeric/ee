use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::document::Document;
use crate::motion::Areas;
use crate::state::{Alert, EditorState, Mode, PromptKind};
use crate::theme;

pub fn render(frame: &mut Frame, state: &EditorState) {
    let area = frame.area();
    let doc = &state.tabs[state.active];

    let (tab_bar, editor_area, find_bar, status_bar) = layout(area, state.mode);

    let hits = match state.mode {
        Mode::Find | Mode::Replace => state.find_hits(),
        _ => Vec::new(),
    };
    let highlights: Vec<(usize, usize, usize)> = hits
        .iter()
        .filter(|h| h.0 == state.active)
        .map(|&(_, l, s, e)| (l, s, e))
        .collect();

    render_tab_bar(frame, state, tab_bar);
    render_editor(frame, doc, &highlights, matches!(state.mode, Mode::Normal), editor_area);
    if let Some(fb) = find_bar {
        match state.mode {
            Mode::Find | Mode::Replace => render_find_bar(frame, state, &hits, fb),
            Mode::Prompt(kind) => render_prompt(frame, kind, &state.prompt_input, fb),
            _ => {}
        }
    }
    render_status_bar(frame, state, status_bar);
}

pub fn areas(area: Rect, state: &EditorState) -> Areas {
    let (_, editor, bar, status) = layout(area, state.mode);
    let doc = &state.tabs[state.active];
    let selection = doc.selection.and_then(|((sl, sc), (el, ec))| {
        let height = editor.height as usize;
        let top = visible_top(doc, height);
        if sl != el || sl < top || sl >= top + height {
            return None;
        }
        let gutter = doc.lines.len().to_string().len() + 1;
        let x = editor.x as usize + gutter + sc;
        let right = (editor.x + editor.width) as usize;
        if x >= right {
            return None;
        }
        let width = (ec - sc).min(right - x);
        Some(Rect::new(x as u16, editor.y + (sl - top) as u16, width as u16, 1))
    });
    Areas {
        screen: area,
        editor,
        bar,
        status,
        selection,
    }
}

fn render_tab_bar(frame: &mut Frame, state: &EditorState, area: Rect) {
    let mut spans: Vec<Span> = vec![Span::styled(" ee ", theme::block(theme::HOT)), Span::raw(" ")];
    for (i, tab) in state.tabs.iter().enumerate() {
        if i == state.active {
            let label = if tab.dirty {
                format!(" {} ● ", tab.name)
            } else {
                format!(" {} ", tab.name)
            };
            spans.push(Span::styled(label, theme::block(theme::ICE)));
        } else {
            spans.push(Span::styled(format!(" {}", tab.name), theme::dim()));
            if tab.dirty {
                spans.push(Span::styled(" ●", Style::default().fg(theme::AMBER)));
            }
            spans.push(Span::raw(" "));
        }
        spans.push(Span::raw(" "));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_editor(
    frame: &mut Frame,
    doc: &Document,
    highlights: &[(usize, usize, usize)],
    focused: bool,
    area: Rect,
) {
    let height = area.height as usize;
    let cursor_line = doc.cursor.0;
    let cursor_col = doc.cursor.1;

    let top_line = visible_top(doc, height);

    let num_width = doc.lines.len().to_string().len();
    let text_width = (area.width as usize).saturating_sub(num_width + 1);
    let markdown = doc.markdown_view();
    let mut tinted_rows: Vec<(usize, Color)> = Vec::new();

    let mut lines: Vec<Line> = Vec::new();
        for i in 0..height {
            let line_idx = top_line + i;
            if line_idx >= doc.lines.len() {
                lines.push(Line::default());
                continue;
            }
            if let Some(view) = markdown.as_ref().filter(|_| !doc.is_raw_line(line_idx)) {
                let rendered = &view.lines[line_idx];
                let mut spans = vec![Span::styled(
                    format!("{:width$} ", line_idx + 1, width = num_width),
                    theme::dim(),
                )];
                let cells = rendered.cells();
                for &(c, style, src) in &cells {
                    let hit = highlights.iter().any(|&(l, s, e)| l == line_idx && s <= src && src < e);
                    let style = if hit { theme::find_match() } else { theme::text().patch(style) };
                    spans.push(Span::styled(c.to_string(), style));
                }
                if let Some((c, style)) = rendered.fill {
                    let rest = text_width.saturating_sub(cells.len());
                    spans.push(Span::styled(c.to_string().repeat(rest), style));
                }
                if let Some(bg) = rendered.bg {
                    tinted_rows.push((i, bg));
                }
                lines.push(Line::from(spans));
                continue;
            }
            let line = &doc.lines[line_idx];
            let chars: Vec<char> = line.chars().collect();
            // Every caret on this line, tagged with whether it is the active one.
            let mut caret_cols: Vec<(usize, bool)> = Vec::new();
            if line_idx == cursor_line {
                caret_cols.push((cursor_col, true));
            }
            for e in &doc.extra_carets {
                if e.0 == line_idx {
                    caret_cols.push((e.1, false));
                }
            }
            let caret_style = |active: bool| match (active, focused) {
                (true, true) => theme::caret(),
                (true, false) => theme::block(theme::GHOST),
                (false, _) => theme::extra_caret(),
            };
            let is_cursor_line = line_idx == cursor_line;
            let number_style = if is_cursor_line {
                Style::default().fg(theme::HOT).add_modifier(Modifier::BOLD)
            } else {
                theme::dim()
            };
            let mut spans: Vec<Span> = Vec::new();
            spans.push(Span::styled(
                format!("{:width$} ", line_idx + 1, width = num_width),
                number_style,
            ));
            for (col, c) in chars.iter().enumerate() {
                let style = match caret_cols.iter().find(|(cc, _)| *cc == col) {
                    Some((_, active)) => caret_style(*active),
                    None => {
                        if in_selection(doc, line_idx, col) {
                            theme::selection()
                        } else if highlights
                            .iter()
                            .any(|&(l, s, e)| l == line_idx && s <= col && col < e)
                        {
                            theme::find_match()
                        } else {
                            theme::text()
                        }
                    }
                };
                spans.push(Span::styled(c.to_string(), style));
            }
            // Carets sitting at the end of the line get their own marker cell.
            for (col, is_active) in &caret_cols {
                if *col == chars.len() {
                    spans.push(Span::styled(" ", caret_style(*is_active)));
                }
            }
            lines.push(Line::from(spans));
        }

    for (row, bg) in tinted_rows {
        let x = area.x + (num_width + 1) as u16;
        let tint = Rect::new(x, area.y + row as u16, area.width.saturating_sub(x - area.x), 1);
        frame.buffer_mut().set_style(tint, Style::default().bg(bg));
    }
    if cursor_line >= top_line && cursor_line - top_line < height {
        let row = Rect::new(area.x, area.y + (cursor_line - top_line) as u16, area.width, 1);
        frame.buffer_mut().set_style(row, Style::default().bg(theme::CURSOR_LINE));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn scroll_for(top: usize, cursor_line: usize, height: usize) -> usize {
    if height == 0 {
        top
    } else if cursor_line < top {
        cursor_line
    } else if cursor_line >= top + height {
        cursor_line + 1 - height
    } else {
        top
    }
}

fn visible_top(doc: &Document, height: usize) -> usize {
    let top = scroll_for(doc.scroll_top.get(), doc.cursor.0, height);
    doc.scroll_top.set(top);
    top
}

fn in_selection(doc: &Document, line_idx: usize, col: usize) -> bool {
    if doc
        .occurrences
        .iter()
        .any(|&(l, s, e)| l == line_idx && s <= col && col < e)
    {
        return true;
    }
    let Some((start, end)) = doc.selection else {
        return false;
    };
    let (sl, sc) = start;
    let (el, ec) = end;
    if line_idx < sl || line_idx > el {
        return false;
    }
    if line_idx == sl && col < sc {
        return false;
    }
    if line_idx == el && col >= ec {
        return false;
    }
    true
}

fn layout(area: Rect, mode: Mode) -> (Rect, Rect, Option<Rect>, Rect) {
    match mode {
        Mode::Normal => {
            let rects = Layout::vertical([
                Constraint::Length(1),
                Constraint::Min(0),
                Constraint::Length(1),
            ])
            .split(area);
            (rects[0], rects[1], None, rects[2])
        }
        _ => {
            let rects = Layout::vertical([
                Constraint::Length(1),
                Constraint::Min(0),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(area);
            (rects[0], rects[1], Some(rects[2]), rects[3])
        }
    }
}

pub fn editor_rect_for(area: Rect, mode: Mode) -> Rect {
    layout(area, mode).1
}

/// Map a 0-indexed mouse position to a (line, col) document position, or None if the
/// click falls outside a real line of the active document.
pub fn mouse_to_doc(editor_area: Rect, mx: u16, my: u16, doc: &Document) -> Option<(usize, usize)> {
    let ex = mx as isize;
    let ey = my as isize;
    if ex < editor_area.x as isize
        || ey < editor_area.y as isize
        || ex >= (editor_area.x + editor_area.width) as isize
        || ey >= (editor_area.y + editor_area.height) as isize
    {
        return None;
    }
    let height = editor_area.height as usize;
    if height == 0 {
        return None;
    }
    let row_in = (ey - editor_area.y as isize) as usize;
    let col_in = (ex - editor_area.x as isize) as usize;
    let top_line = visible_top(doc, height);
    let line = top_line + row_in;
    if line >= doc.lines.len() {
        return None;
    }
    let gutter = doc.lines.len().to_string().len() + 1;
    let text_col = col_in.saturating_sub(gutter);
    let col = match doc.markdown_view().filter(|_| !doc.is_raw_line(line)) {
        Some(view) => view.lines[line].display_to_source(text_col, doc.lines[line].chars().count()),
        None => text_col,
    };
    Some((line, col))
}

fn input_spans(spans: &mut Vec<Span>, label: &'static str, value: &str, focused: bool) {
    spans.push(Span::styled(label, Style::default().fg(theme::ICE)));
    spans.push(Span::styled("▸ ", Style::default().fg(theme::HOT)));
    spans.push(Span::styled(value.to_string(), theme::text()));
    if focused {
        spans.push(Span::styled(" ", theme::caret()));
    }
}

fn render_find_bar(frame: &mut Frame, state: &EditorState, hits: &[crate::state::Hit], area: Rect) {
    let find = &state.find;
    let replacing = matches!(state.mode, Mode::Replace);
    let label = if find.all_tabs { " find in files " } else { " find " };
    let mut spans: Vec<Span> = Vec::new();
    input_spans(&mut spans, label, &find.query, !(replacing && find.editing_replacement));
    if replacing {
        spans.push(Span::raw("  "));
        input_spans(&mut spans, "replace ", &find.replacement, find.editing_replacement);
    }
    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        "Aa",
        if find.case_sensitive {
            Style::default().fg(theme::AMBER).add_modifier(Modifier::BOLD)
        } else {
            theme::dim()
        },
    ));
    let count = match state.current_hit(hits) {
        _ if find.query.is_empty() => String::new(),
        _ if hits.is_empty() => "  no matches".to_string(),
        Some(i) => format!("  {}/{}", i + 1, hits.len()),
        None => format!("  {} matches", hits.len()),
    };
    let count_style = if !find.query.is_empty() && hits.is_empty() {
        Style::default().fg(theme::ERROR)
    } else {
        Style::default().fg(theme::ICE)
    };
    spans.push(Span::styled(count, count_style));
    if let Some(i) = state.current_hit(hits).filter(|_| find.all_tabs) {
        let (t, l, _, _) = hits[i];
        spans.push(Span::styled(format!("  {}:{}", state.tabs[t].name, l + 1), theme::dim()));
    }
    let used = Line::from(spans.clone()).width() as u16;
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
    let hint = if replacing {
        "Alt+P replace  Alt+A all  Tab switch field  Alt+C case  Esc close "
    } else {
        "Enter next  Up previous  Alt+C case  Esc close "
    };
    render_right(frame, area, used, hint, theme::dim());
}

fn render_right(frame: &mut Frame, area: Rect, used: u16, text: &str, style: Style) -> u16 {
    let width = text.chars().count() as u16;
    if used + width + 1 > area.width {
        return 0;
    }
    let right = Rect::new(area.x + area.width - width, area.y, width, area.height);
    frame.render_widget(Paragraph::new(text.to_string()).style(style).alignment(Alignment::Right), right);
    width
}

fn render_prompt(frame: &mut Frame, kind: PromptKind, input: &str, area: Rect) {
    let mut spans: Vec<Span> = Vec::new();
    let label = match kind {
        PromptKind::Open => " open ",
        PromptKind::Rename => " rename to ",
    };
    input_spans(&mut spans, label, input, true);
    let used = Line::from(spans.clone()).width() as u16;
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
    render_right(frame, area, used, "Enter confirm  Esc cancel ", theme::dim());
}

fn render_status_bar(frame: &mut Frame, state: &EditorState, area: Rect) {
    let doc = &state.tabs[state.active];
    let (cursor_line, cursor_col) = doc.cursor;
    let sel_len = match doc.selection {
        Some((start, end)) => selection_length(&doc.lines, start, end),
        None => 0,
    };
    let mode_color = match state.mode {
        Mode::Normal => theme::ICE,
        Mode::Find => theme::HOT,
        Mode::Replace => theme::AMBER,
        Mode::Prompt(_) => theme::ICE,
    };
    let mut spans: Vec<Span> = vec![
        Span::styled(format!(" {} ", state.mode_str().to_lowercase()), theme::block(mode_color)),
        Span::styled(format!(" {}", doc.name), theme::text()),
    ];
    if doc.dirty {
        spans.push(Span::styled(" ●", Style::default().fg(theme::AMBER)));
    }
    if !state.status.is_empty() {
        let style = match state.alert {
            Some(Alert::Warn) => Style::default().fg(theme::AMBER),
            Some(Alert::Error) => Style::default().fg(theme::ERROR).add_modifier(Modifier::BOLD),
            None => Style::default().fg(theme::ICE),
        };
        spans.push(Span::styled(format!("   {}", state.status), style));
    }

    let mut position = format!("ln {}, col {}", cursor_line + 1, cursor_col + 1);
    let carets = doc.extra_carets.len();
    if carets > 0 {
        position = format!("{} carets  {}", carets + 1, position);
    }
    if sel_len > 0 {
        position = format!("{} selected  {}", sel_len, position);
    }
    let right = render_right(frame, area, 0, &format!("{} ", position), theme::dim());
    let left = Rect::new(area.x, area.y, area.width.saturating_sub(right + 3), area.height);
    frame.render_widget(Paragraph::new(Line::from(spans)), left);
}

fn selection_length(lines: &[String], start: (usize, usize), end: (usize, usize)) -> usize {
    let (sl, sc) = start;
    let (el, ec) = end;
    if sl == el {
        return ec - sc;
    }
    let mut len = lines[sl].chars().count() - sc;
    for l in sl + 1..el {
        len += lines[l].chars().count();
    }
    len += ec;
    len
}

const LOGO: [&str; 4] = ["▄▀▀▀▀▄  ▄▀▀▀▀▄", "█▄▄▄▄█  █▄▄▄▄█", "█       █     ", "▀▄▄▄▄▀  ▀▄▄▄▄▀"];

const BOOT_LOG: [&str; 3] = ["mounting buffers", "calibrating neon", "ignoring your opinions"];

fn lerp(a: Color, b: Color, t: f32) -> Color {
    let (Color::Rgb(ar, ag, ab), Color::Rgb(br, bg, bb)) = (a, b) else {
        return a;
    };
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color::Rgb(mix(ar, br), mix(ag, bg), mix(ab, bb))
}

pub fn render_splash(frame: &mut Frame, progress: f32) {
    let area = frame.area();
    let log_width = 32;
    let mut lines: Vec<Line> = Vec::new();
    for (i, row) in LOGO.iter().enumerate() {
        let doubled: String = row.chars().flat_map(|c| [c, c]).collect();
        let color = lerp(theme::HOT, theme::ICE, i as f32 / (LOGO.len() - 1) as f32);
        lines.push(Line::styled(doubled, Style::default().fg(color)));
    }
    lines.push(Line::default());
    lines.push(Line::styled("eric's own editor", theme::text()));
    lines.push(Line::default());
    let shown = ((progress * (BOOT_LOG.len() + 1) as f32) as usize).min(BOOT_LOG.len());
    for (i, entry) in BOOT_LOG.iter().enumerate() {
        if i >= shown {
            lines.push(Line::default());
            continue;
        }
        let dots = ".".repeat(log_width - 3 - entry.len());
        lines.push(Line::from(vec![
            Span::styled("› ", Style::default().fg(theme::HOT)),
            Span::styled(format!("{} {} ", entry, dots), theme::dim()),
            Span::styled("ok", Style::default().fg(theme::ICE).add_modifier(Modifier::BOLD)),
        ]));
    }
    let height = lines.len() as u16;
    let top = area.y + area.height.saturating_sub(height) / 2;
    let body = Rect::new(area.x, top, area.width, height.min(area.height));
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), body);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;
    use crate::state::Mode;
    use ratatui::backend::TestBackend;
    use ratatui::layout::{Position, Rect};
    use ratatui::{Terminal, buffer::Buffer};

    #[test]
    fn scrolling_up_after_reaching_the_bottom_keeps_the_view_until_the_top_row() {
        let text: Vec<String> = (1..=30).map(|n| format!("line {}", n)).collect();
        let mut state = EditorState::new();
        state.tabs[0] = Document::with_content("t.txt", &text.join("\n"));
        let mut t = Terminal::new(TestBackend::new(20, 12)).unwrap();
        let first_row = |t: &mut Terminal<TestBackend>, state: &EditorState| {
            t.draw(|f| render(f, state)).unwrap();
            render_rows(t.backend().buffer())[1].trim().to_string()
        };
        for _ in 0..29 {
            state.apply(crate::keys::Action::Down);
            first_row(&mut t, &state);
        }
        assert_eq!(first_row(&mut t, &state), "21 line 21", "bottom reached, 10 rows visible");
        let mut collapsed = Terminal::new(TestBackend::new(20, 0)).unwrap();
        collapsed.draw(|f| render(f, &state)).unwrap();
        assert_eq!(first_row(&mut t, &state), "21 line 21", "a 0-row frame keeps the scroll position");
        for _ in 0..9 {
            state.apply(crate::keys::Action::Up);
            assert_eq!(first_row(&mut t, &state), "21 line 21", "view holds while the cursor climbs");
        }
        state.apply(crate::keys::Action::Up);
        assert_eq!(first_row(&mut t, &state), "20 line 20", "scrolls once the cursor passes the top row");
    }

    fn render_rows(buf: &Buffer) -> Vec<String> {
        let area = &buf.area;
        (0..area.height).map(|y| {
            (0..area.width).map(|x| {
                buf.cell(Position { x, y })
                    .map(|c| c.symbol().to_string())
                    .unwrap_or_else(|| " ".to_string())
            })
            .collect::<String>()
        })
        .collect()
    }

    #[test]
    fn renders_at_any_terminal_size_in_every_mode() {
        let mut state = EditorState::new();
        state.tabs[0] = Document::with_content("t", "alpha beta\nbeta\n");
        state.tabs[0].cursor = (1, 2);
        for mode in [Mode::Normal, Mode::Find, Mode::Replace, Mode::Prompt(PromptKind::Open)] {
            state.mode = mode;
            state.find.query = "beta".to_string();
            for (w, h) in [(10, 0), (1, 1), (20, 3), (80, 24), (120, 40), (200, 50)] {
                let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
                t.draw(|f| render(f, &state)).unwrap();
            }
        }
    }

    #[test]
    fn editor_fills_every_row_not_used_by_a_bar() {
        let mut state = EditorState::new();
        state.tabs[0] = Document::with_content("t", "x");
        for (mode, bars) in [(Mode::Normal, 2), (Mode::Find, 3), (Mode::Prompt(PromptKind::Open), 3)] {
            state.mode = mode;
            for h in [10u16, 24, 50] {
                let editor = editor_rect_for(Rect::new(0, 0, 80, h), mode);
                assert_eq!((editor.y, editor.height), (1, h - bars), "h={}", h);
                let mut t = Terminal::new(TestBackend::new(80, h)).unwrap();
                t.draw(|f| render(f, &state)).unwrap();
                let rows = render_rows(t.backend().buffer());
                assert!(rows[0].contains('t'), "tab bar on the first row");
                assert!(rows[h as usize - 1].contains("ln 1, col 1"), "status bar on the last row, h={}", h);
                if matches!(mode, Mode::Normal) {
                    assert!(rows[h as usize - 1].starts_with(" edit "), "mode badge reads edit");
                }
            }
        }
    }

    #[test]
    fn clicks_on_rendered_markdown_land_on_the_same_source_char() {
        let area = Rect::new(0, 0, 40, 10);
        let editor = editor_rect_for(area, Mode::Normal);
        let mut state = EditorState::new();
        state.tabs[0] = Document::with_content("n.md", "caret here\na **bold** word");
        let mut t = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
        t.draw(|f| render(f, &state)).unwrap();
        let row: Vec<char> = render_rows(t.backend().buffer())[editor.y as usize + 1].chars().collect();
        assert_eq!(row[2..13].iter().collect::<String>(), "a bold word", "markup hidden off the caret line");
        let doc = &state.tabs[0];
        let source: Vec<char> = doc.lines[1].chars().collect();
        for x in 2..13u16 {
            let (line, col) = mouse_to_doc(editor, x, editor.y + 1, doc).unwrap();
            assert_eq!((line, source[col]), (1, row[x as usize]), "x={}", x);
        }
    }

    #[test]
    fn selections_and_occurrences_are_highlighted_and_counted() {
        let mut state = EditorState::new();
        state.tabs[0] = Document::with_content("t", "ab\ncd\nef\ngh\nij");
        state.tabs[0].selection = Some(((0, 1), (2, 1)));
        state.tabs[0].cursor = (4, 0);
        state.tabs[0].occurrences = vec![(3, 0, 1)];
        let mut t = Terminal::new(TestBackend::new(30, 8)).unwrap();
        t.draw(|f| render(f, &state)).unwrap();
        let buf = t.backend().buffer();
        let selected = |x: u16, y: u16| buf.cell((x, y)).unwrap().bg == theme::SELECTION;
        let (gutter, top) = (2, 1);
        let expect = [
            ((0, 0), false),
            ((0, 1), true),
            ((1, 0), true),
            ((1, 1), true),
            ((2, 0), true),
            ((2, 1), false),
            ((3, 0), true),
            ((3, 1), false),
        ];
        for ((line, col), want) in expect {
            assert_eq!(selected(gutter + col, top + line), want, "cell ({}, {})", line, col);
        }
        assert!(render_rows(buf)[7].contains("4 selected"), "status counts selected chars");
    }

    #[test]
    fn mouse_clicks_map_to_the_rendered_text() {
        let area = Rect::new(0, 0, 40, 12);
        let editor = editor_rect_for(area, Mode::Normal);
        let mut state = EditorState::new();
        state.tabs[0] = Document::with_content("t", "abcdefghijk\n2\n3");
        let mut t = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
        t.draw(|f| render(f, &state)).unwrap();
        let row: Vec<char> = render_rows(t.backend().buffer())[editor.y as usize].chars().collect();
        let doc = &state.tabs[0];
        for x in 2..13u16 {
            let (line, col) = mouse_to_doc(editor, x, editor.y, doc).unwrap();
            assert_eq!((line, doc.lines[0].chars().nth(col)), (0, Some(row[x as usize])), "x={}", x);
        }
        assert_eq!(mouse_to_doc(editor, 0, editor.y + 2, doc), Some((2, 0)), "gutter click clamps to col 0");
        assert_eq!(mouse_to_doc(editor, 5, editor.y + 5, doc), None, "blank row below the text");

        let long: Vec<String> = (1..=30).map(|n| format!("line {}", n)).collect();
        state.tabs[0] = Document::with_content("t", &long.join("\n"));
        state.tabs[0].cursor = (29, 0);
        t.draw(|f| render(f, &state)).unwrap();
        assert_eq!(mouse_to_doc(editor, 3, editor.y, &state.tabs[0]), Some((20, 0)), "clicks follow the scroll");
    }
}
