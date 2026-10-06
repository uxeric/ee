use ratatui::layout::{Alignment, Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::document::Document;
use crate::motion::Areas;
use crate::state::{Alert, EditorState, Mode, PickerKind, PromptKind, MENU};
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
    render_editor(frame, doc, &highlights, matches!(state.mode, Mode::Normal | Mode::Menu), editor_area);
    if let Some(fb) = find_bar {
        match state.mode {
            Mode::Find | Mode::Replace => render_find_bar(frame, state, &hits, fb),
            Mode::Prompt(kind) => render_prompt(frame, kind, &state.prompt_input, fb),
            _ => {}
        }
    }
    render_status_bar(frame, state, status_bar);
    if matches!(state.mode, Mode::Picker) {
        render_picker(frame, state, area);
    }
    if state.reload.is_some() {
        render_reload_modal(frame, state, area);
    } else {
        render_update_modal(frame, state, area);
    }
    if state.reload.is_none() && state.update.is_none() {
        render_menu(frame, state, area);
    }
}

pub fn popup_rect(area: Rect, state: &EditorState) -> Option<Rect> {
    let picker = state.picker.as_ref()?;
    let editor = editor_rect_for(area, state.mode);
    let width = area.width.saturating_sub(4).min(84);
    let rows = (picker.shown.len().max(1) as u16 + 5).min(20).min(editor.height.max(3));
    Some(Rect::new(area.x + (area.width - width) / 2, editor.y + editor.height.min(1), width, rows))
}

struct PickerView {
    list: Rect,
    start: usize,
}

fn picker_view(area: Rect, state: &EditorState) -> Option<PickerView> {
    let picker = state.picker.as_ref()?;
    let rect = popup_rect(area, state)?;
    let inner = Rect::new(rect.x + 1, rect.y + 1, rect.width.saturating_sub(2), rect.height.saturating_sub(2));
    let list = Rect::new(inner.x, inner.y + 2, inner.width, inner.height.saturating_sub(3));
    let visible = list.height as usize;
    Some(PickerView { list, start: picker.selected.saturating_sub(visible.saturating_sub(1)) })
}

pub fn picker_row(area: Rect, state: &EditorState) -> Option<Rect> {
    let picker = state.picker.as_ref().filter(|p| !p.shown.is_empty())?;
    let view = picker_view(area, state)?;
    let offset = picker.selected.checked_sub(view.start)? as u16;
    (offset < view.list.height).then(|| Rect::new(view.list.x, view.list.y + offset, view.list.width, 1))
}

fn render_picker(frame: &mut Frame, state: &EditorState, area: Rect) {
    let (Some(picker), Some(rect), Some(view)) = (state.picker.as_ref(), popup_rect(area, state), picker_view(area, state)) else {
        return;
    };
    let (title, enter) = match picker.kind {
        PickerKind::Actions => ("command palette", "run"),
        PickerKind::Structure => ("file structure", "jump"),
        PickerKind::Files => ("open or create a file", "open"),
        PickerKind::Themes => ("theme", "keep"),
    };
    let p = theme::pal();
    let tag = format!("{}/{}", picker.shown.len(), picker.total());
    let inner = crate::hud::draw(frame.buffer_mut(), rect, &crate::hud::Chrome { title, tag: Some(tag), hazard: false });
    if inner.height == 0 {
        return;
    }
    let input = Line::from(vec![
        Span::styled(" ❯ ", Style::default().fg(p.hot).add_modifier(Modifier::BOLD)),
        Span::styled(picker.input.clone(), theme::text().add_modifier(Modifier::BOLD)),
        Span::styled(" ", theme::caret()),
    ]);
    frame.render_widget(Paragraph::new(input), Rect::new(inner.x, inner.y, inner.width, 1));
    if inner.height > 1 {
        let rule = "╌".repeat(inner.width.saturating_sub(2) as usize);
        frame.render_widget(Paragraph::new(Span::styled(rule, Style::default().fg(p.ghost))), Rect::new(inner.x + 1, inner.y + 1, inner.width.saturating_sub(2), 1));
    }
    if inner.height > 3 {
        let esc = if picker.kind == PickerKind::Themes { "put back" } else { "close" };
        let row = Rect::new(inner.x + 1, inner.y + inner.height - 1, inner.width.saturating_sub(2), 1);
        crate::hud::hints(frame.buffer_mut(), row, &[("↑↓", "choose"), ("Enter", enter), ("Esc", esc)]);
    }
    if picker.shown.is_empty() {
        let hint = match picker.kind {
            PickerKind::Files if !picker.input.trim().is_empty() => format!("Enter opens {}, creating it if it doesn't exist", picker.input.trim()),
            _ => "nothing matches".to_string(),
        };
        if view.list.height > 0 {
            frame.render_widget(Paragraph::new(Span::styled(hint, theme::dim())), Rect::new(view.list.x + 3, view.list.y, view.list.width.saturating_sub(3), 1));
        }
        return;
    }
    for (row, (index, hits)) in picker.shown.iter().enumerate().skip(view.start).take(view.list.height as usize) {
        let y = view.list.y + (row - view.start) as u16;
        let selected = row == picker.selected;
        let item = picker.item(*index);
        let row_rect = Rect::new(view.list.x, y, view.list.width, 1);
        let base = if selected { Style::default().bg(p.selection) } else { Style::default() };
        if selected {
            frame.buffer_mut().set_style(row_rect, base);
        }
        let mut spans = vec![Span::styled(if selected { " ▶ " } else { "   " }, base.fg(p.hot).add_modifier(Modifier::BOLD))];
        for (i, c) in item.label.chars().enumerate() {
            let style = if hits.contains(&i) {
                base.fg(p.hot).add_modifier(Modifier::BOLD)
            } else if selected {
                base.fg(p.text).add_modifier(Modifier::BOLD)
            } else {
                base.fg(p.text)
            };
            spans.push(Span::styled(c.to_string(), style));
        }
        let used = Line::from(spans.clone()).width() as u16;
        frame.render_widget(Paragraph::new(Line::from(spans)), row_rect);
        if !item.detail.is_empty() {
            render_right(frame, row_rect, used, &format!("{} ", item.detail), base.fg(if selected { p.ice } else { p.ghost }));
        }
    }
}

pub fn areas(area: Rect, state: &EditorState) -> Areas {
    let (_, editor, bar, status) = layout(area, state.mode);
    let doc = &state.tabs[state.active];
    let height = editor.height as usize;
    let (top, _) = visible_origin(doc, editor);
    let gutter = gutter(doc);
    let width = text_width(doc, editor);
    let map = screen_map(doc, width);
    let selection = doc.selection.and_then(|((sl, sc), (el, ec))| {
        if sl != el || width == 0 {
            return None;
        }
        let (r1, c1) = (sc / width, sc % width);
        let (r2, c2) = if ec > sc && ec % width == 0 {
            (ec / width - 1, width)
        } else {
            (ec / width, ec % width)
        };
        if r1 != r2 || c1 >= c2 {
            return None;
        }
        let visual = map.starts.get(sl).copied()? + r1;
        if visual < top || visual >= top + height {
            return None;
        }
        Some(Rect::new(editor.x + (gutter + c1) as u16, editor.y + (visual - top) as u16, (c2 - c1) as u16, 1))
    });
    let mut lines: Vec<usize> = doc.extra_carets.iter().map(|&(l, _)| l).collect();
    lines.push(doc.cursor.0);
    lines.sort();
    lines.dedup();
    let mut caret_rows = Vec::new();
    for line in lines {
        let Some(&start) = map.starts.get(line) else {
            continue;
        };
        let len = doc.lines[line].chars().count();
        for local in 0..map.rows_of(line) {
            let visual = start + local;
            if visual < top || visual >= top + height {
                continue;
            }
            let shown = if width == 0 {
                1
            } else {
                let at = local * width;
                if at >= len { 1 } else { (len - at).min(width).max(1) }
            };
            caret_rows.push(Rect::new(editor.x + gutter as u16, editor.y + (visual - top) as u16, shown as u16, 1));
        }
    }
    Areas {
        screen: area,
        editor,
        bar,
        status,
        selection,
        caret_rows,
        popup: popup_rect(area, state),
        top,
        picker_row: picker_row(area, state),
        modal: modal_rect(area, state),
    }
}

fn render_tab_bar(frame: &mut Frame, state: &EditorState, area: Rect) {
    let mut spans: Vec<Span> = vec![Span::styled(" ee ", theme::block(theme::pal().hot)), Span::raw(" ")];
    for (i, tab) in state.tabs.iter().enumerate() {
        if i == state.active {
            let label = if tab.dirty {
                format!(" {} ● ", tab.name)
            } else {
                format!(" {} ", tab.name)
            };
            spans.push(Span::styled(label, theme::block(theme::pal().ice)));
        } else {
            spans.push(Span::styled(format!(" {}", tab.name), theme::dim()));
            if tab.dirty {
                spans.push(Span::styled(" ●", Style::default().fg(theme::pal().amber)));
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
    let (top, _) = visible_origin(doc, area);
    let width = text_width(doc, area);
    let map = screen_map(doc, width);
    let source = map.source_range(top, height);
    let syntax = doc.syntax_roles(source.clone());
    let markdown = doc.rendered_view();
    let num_width = doc.lines.len().to_string().len();
    let gutter_w = gutter(doc);
    let mut tinted_rows: Vec<(usize, Color)> = Vec::new();
    let mut caret_screen: Vec<usize> = Vec::new();
    let mut lines: Vec<Line> = Vec::new();

    for i in 0..height {
        let Some((line_idx, local)) = map.line_at(top + i) else {
            lines.push(Line::default());
            continue;
        };
        if line_idx == doc.cursor.0 {
            caret_screen.push(i);
        }
        let number = if local == 0 {
            format!("{:width$} ", line_idx + 1, width = num_width)
        } else {
            " ".repeat(gutter_w)
        };
        let number_style = if line_idx == doc.cursor.0 && local == 0 {
            Style::default().fg(theme::pal().hot).add_modifier(Modifier::BOLD)
        } else {
            theme::dim()
        };
        let mut spans = vec![Span::styled(number, number_style)];
        if let Some(view) = markdown.as_ref().filter(|_| !doc.is_raw_line(line_idx)) {
            if let Some(rendered) = view.lines.get(line_idx) {
                if let Some(bg) = rendered.bg {
                    tinted_rows.push((i, bg));
                }
                let cells = rendered.cells();
                let fits_centered = rendered.centered && cells.len() <= width;
                if fits_centered || width == 0 {
                    let pad = if width == 0 { 0 } else { centered_pad(rendered, cells.len(), width) };
                    let mut shown: Vec<(char, Style)> = vec![(' ', Style::default()); pad];
                    for &(c, style, src) in &cells {
                        let hit = highlights.iter().any(|&(l, s, e)| l == line_idx && s <= src && src < e);
                        shown.push((c, if hit { theme::find_match() } else { theme::text().patch(style) }));
                    }
                    if let Some((c, style)) = rendered.fill {
                        shown.extend(std::iter::repeat_n((c, style), width.saturating_sub(shown.len())));
                    }
                    spans.extend(shown.into_iter().take(width).map(|(c, style)| Span::styled(c.to_string(), style)));
                } else {
                    let start = local * width;
                    for &(c, style, src) in cells.iter().skip(start).take(width) {
                        let hit = highlights.iter().any(|&(l, s, e)| l == line_idx && s <= src && src < e);
                        let paint = if hit { theme::find_match() } else { theme::text().patch(style) };
                        spans.push(Span::styled(c.to_string(), paint));
                    }
                    if local + 1 == map.rows_of(line_idx) {
                        if let Some((c, style)) = rendered.fill {
                            let have = cells.len().saturating_sub(start).min(width);
                            spans.extend(std::iter::repeat_n(Span::styled(c.to_string(), style), width.saturating_sub(have)));
                        }
                    }
                }
                lines.push(Line::from(spans));
                continue;
            }
        }
        let chars: Vec<char> = doc.lines[line_idx].chars().collect();
        let mut caret_cols: Vec<(usize, bool)> = Vec::new();
        if line_idx == doc.cursor.0 {
            caret_cols.push((doc.cursor.1, true));
        }
        for caret in &doc.extra_carets {
            if caret.0 == line_idx {
                caret_cols.push((caret.1, false));
            }
        }
        let caret_style = |active: bool| match (active, focused) {
            (true, true) => theme::caret(),
            (true, false) => theme::block(theme::pal().ghost),
            (false, _) => theme::extra_caret(),
        };
        let line_roles = syntax.get(line_idx.saturating_sub(source.start)).map(Vec::as_slice).unwrap_or(&[]);
        let start = if width == 0 { 0 } else { local * width };
        for (col, c) in chars.iter().enumerate().skip(start).take(width) {
            let style = match caret_cols.iter().find(|(cc, _)| *cc == col) {
                Some((_, active)) => caret_style(*active),
                None if doc.covers(line_idx, col) => theme::selection(),
                None if highlights.iter().any(|&(l, s, e)| l == line_idx && s <= col && col < e) => theme::find_match(),
                None => line_roles.iter().find(|(range, _)| range.contains(&col)).map(|(_, role)| theme::syntax(*role)).unwrap_or_else(theme::text),
            };
            spans.push(Span::styled(c.to_string(), style));
        }
        if let Some((_, active)) = caret_cols.iter().find(|(col, _)| end_caret(*col, chars.len(), local, width)) {
            spans.push(Span::styled(" ", caret_style(*active)));
        }
        lines.push(Line::from(spans));
    }

    for (row, bg) in tinted_rows {
        let x = area.x + gutter_w as u16;
        let tint = Rect::new(x, area.y + row as u16, area.width.saturating_sub(gutter_w as u16), 1);
        frame.buffer_mut().set_style(tint, Style::default().bg(bg));
    }
    for row in caret_screen {
        frame.buffer_mut().set_style(Rect::new(area.x, area.y + row as u16, area.width, 1), Style::default().bg(theme::pal().cursor_line));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn end_caret(col: usize, len: usize, local: usize, width: usize) -> bool {
    col == len && (if width == 0 { col == 0 } else { col / width == local })
}

fn scroll_for(top: usize, cursor_row: usize, height: usize) -> usize {
    if height == 0 {
        top
    } else if cursor_row < top {
        cursor_row
    } else if cursor_row >= top + height {
        cursor_row + 1 - height
    } else {
        top
    }
}

pub fn gutter(doc: &Document) -> usize {
    doc.lines.len().to_string().len() + 1
}

fn text_width(doc: &Document, area: Rect) -> usize {
    (area.width as usize).saturating_sub(gutter(doc))
}

fn screen_map(doc: &Document, width: usize) -> crate::wrap::Map {
    let view = doc.rendered_view();
    let counts: Vec<usize> = (0..doc.lines.len()).map(|line| row_count(doc, line, width, view.as_deref())).collect();
    crate::wrap::Map::from_counts(&counts)
}

fn row_count(doc: &Document, line: usize, width: usize, view: Option<&crate::markdown::MdView>) -> usize {
    if width == 0 {
        return 1;
    }
    if let Some(view) = view.filter(|_| !doc.is_raw_line(line)) {
        if let Some(rendered) = view.lines.get(line) {
            if rendered.nowrap {
                return 1;
            }
            let cells = rendered.cells().len();
            if rendered.centered && cells <= width {
                return 1;
            }
            return crate::wrap::display_rows(cells, width);
        }
    }
    crate::wrap::rows_for(doc.lines[line].chars().count(), width)
}

pub fn cursor_visual_row(doc: &Document) -> usize {
    let width = doc.wrap_width.get();
    screen_map(doc, width).cursor_row(doc.cursor.0, doc.cursor.1, width)
}

fn visible_origin(doc: &Document, area: Rect) -> (usize, usize) {
    let height = area.height as usize;
    let width = text_width(doc, area);
    doc.wrap_width.set(width);
    let map = screen_map(doc, width);
    let anchor = Some((doc.cursor, doc.rev()));
    let row = map.cursor_row(doc.cursor.0, doc.cursor.1, width);
    let top = if doc.view_anchor.get() == anchor {
        doc.scroll_top.get().min(map.max_top(height))
    } else {
        scroll_for(doc.scroll_top.get(), row, height).min(map.max_top(height))
    };
    doc.view_anchor.set(anchor);
    doc.scroll_top.set(top);
    doc.scroll_left.set(0);
    (top, 0)
}

pub fn page_rows(area: Rect, state: &EditorState) -> usize {
    let rows = match popup_rect(area, state) {
        Some(popup) => popup.height.saturating_sub(3),
        None => editor_rect_for(area, state.mode).height,
    };
    rows.max(1) as usize
}

pub fn scroll_view(area: Rect, state: &EditorState, lines: isize) {
    let doc = &state.tabs[state.active];
    let editor = editor_rect_for(area, state.mode);
    let top = visible_origin(doc, editor).0 as isize + lines;
    let map = screen_map(doc, doc.wrap_width.get());
    doc.scroll_top.set(top.clamp(0, map.max_top(editor.height as usize) as isize) as usize);
}

pub fn scroll_sideways(area: Rect, state: &EditorState, _cols: isize) {
    let doc = &state.tabs[state.active];
    let editor = editor_rect_for(area, state.mode);
    visible_origin(doc, editor);
}

pub enum MenuHit {
    Row(usize),
    Frame,
    Outside,
}

pub fn menu_rect(area: Rect, state: &EditorState) -> Option<Rect> {
    let menu = state.menu.as_ref()?;
    let inner = MENU
        .iter()
        .map(|(label, key, _)| 3 + label.chars().count() + 2 + key.chars().count() + 1)
        .max()
        .unwrap_or(8) as u16;
    let width = inner.saturating_add(2).min(area.width);
    let height = (MENU.len() as u16 + 2).min(area.height);
    if width < 2 || height < 2 {
        return None;
    }
    let x = menu.x.min(area.x.saturating_add(area.width.saturating_sub(width)));
    let room_below = area.bottom().saturating_sub(menu.y.saturating_add(1));
    let y = if menu.y < area.bottom() && room_below >= height {
        menu.y + 1
    } else if menu.y.saturating_sub(area.y) >= height {
        menu.y - height
    } else {
        area.y
    };
    let rect = Rect::new(x, y, width, height).intersection(area);
    (rect.width >= 2 && rect.height >= 2).then_some(rect)
}

pub fn menu_hit(area: Rect, state: &EditorState, x: u16, y: u16) -> MenuHit {
    let Some(rect) = menu_rect(area, state) else {
        return MenuHit::Outside;
    };
    if !rect.contains(Position::new(x, y)) {
        return MenuHit::Outside;
    }
    let top = rect.y + 1;
    let bottom = rect.bottom().saturating_sub(1);
    if y >= top && y < bottom && x > rect.x && x + 1 < rect.right() {
        let index = (y - top) as usize;
        if index < MENU.len() {
            return MenuHit::Row(index);
        }
    }
    MenuHit::Frame
}

fn render_menu(frame: &mut Frame, state: &EditorState, area: Rect) {
    let (Some(menu), Some(rect)) = (state.menu.as_ref(), menu_rect(area, state)) else {
        return;
    };
    let inner = crate::hud::draw(frame.buffer_mut(), rect, &crate::hud::Chrome { title: "", tag: None, hazard: false });
    if inner.height == 0 {
        return;
    }
    let p = theme::pal();
    for (i, (label, key, _)) in MENU.iter().enumerate().take(inner.height as usize) {
        let row = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
        let on = i == menu.selected;
        let base = if on { Style::default().bg(p.selection) } else { Style::default() };
        if on {
            frame.buffer_mut().set_style(row, base);
        }
        let spans = vec![
            Span::styled(if on { " ▶ " } else { "   " }, base.fg(p.hot).add_modifier(Modifier::BOLD)),
            Span::styled(*label, if on { base.fg(p.text).add_modifier(Modifier::BOLD) } else { base.fg(p.text) }),
        ];
        let used = Line::from(spans.clone()).width() as u16;
        frame.render_widget(Paragraph::new(Line::from(spans)), row);
        render_right(frame, row, used, &format!("{} ", key), base.fg(if on { p.ice } else { p.ghost }));
    }
}

fn layout(area: Rect, mode: Mode) -> (Rect, Rect, Option<Rect>, Rect) {
    match mode {
        Mode::Normal | Mode::Picker | Mode::Menu => {
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

fn centered_pad(rendered: &crate::markdown::RenderedLine, used: usize, width: usize) -> usize {
    if rendered.centered {
        width.saturating_sub(used) / 2
    } else {
        0
    }
}

struct Hit {
    line: usize,
    local: usize,
    visual_col: usize,
    width: usize,
}

fn locate(editor_area: Rect, mx: u16, my: u16, doc: &Document) -> Option<Hit> {
    let ex = mx as isize;
    let ey = my as isize;
    if ex < editor_area.x as isize
        || ey < editor_area.y as isize
        || ex >= (editor_area.x + editor_area.width) as isize
        || ey >= (editor_area.y + editor_area.height) as isize
        || editor_area.height == 0
    {
        return None;
    }
    let width = text_width(doc, editor_area);
    let (top, _) = visible_origin(doc, editor_area);
    let (line, local) = screen_map(doc, width).line_at(top + (ey - editor_area.y as isize) as usize)?;
    let visual_col = ((ex - editor_area.x as isize) as usize).saturating_sub(gutter(doc));
    Some(Hit { line, local, visual_col, width })
}

fn display_col(doc: &Document, hit: &Hit) -> Option<(crate::markdown::RenderedLine, usize, usize)> {
    let view = doc.rendered_view().filter(|_| !doc.is_raw_line(hit.line))?;
    let rendered = view.lines.get(hit.line)?.clone();
    let cells = rendered.cells().len();
    let pad = if rendered.centered && (hit.width == 0 || cells <= hit.width) {
        centered_pad(&rendered, cells, hit.width)
    } else {
        0
    };
    let dcol = if hit.width == 0 { hit.visual_col } else { hit.local * hit.width + hit.visual_col };
    Some((rendered, pad, dcol))
}

pub fn link_at(editor_area: Rect, mx: u16, my: u16, doc: &Document) -> Option<String> {
    let hit = locate(editor_area, mx, my, doc)?;
    let (rendered, pad, dcol) = display_col(doc, &hit)?;
    rendered.link_at(dcol.checked_sub(pad)?).map(str::to_string)
}

pub fn editor_rect_for(area: Rect, mode: Mode) -> Rect {
    layout(area, mode).1
}

/// Map a 0-indexed mouse position to a (line, col) document position, or None if the
/// click falls outside a real line of the active document.
pub fn mouse_to_doc(editor_area: Rect, mx: u16, my: u16, doc: &Document) -> Option<(usize, usize)> {
    let hit = locate(editor_area, mx, my, doc)?;
    let len = doc.lines[hit.line].chars().count();
    let col = match display_col(doc, &hit) {
        Some((rendered, pad, dcol)) => rendered.display_to_source(dcol.saturating_sub(pad), len),
        None => {
            let raw = if hit.width == 0 { hit.visual_col } else { hit.local * hit.width + hit.visual_col };
            raw.min(len)
        }
    };
    Some((hit.line, col))
}

fn input_spans(spans: &mut Vec<Span>, label: &'static str, value: &str, focused: bool) {
    spans.push(Span::styled(label, Style::default().fg(theme::pal().ice)));
    spans.push(Span::styled("▸ ", Style::default().fg(theme::pal().hot)));
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
            Style::default().fg(theme::pal().amber).add_modifier(Modifier::BOLD)
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
        Style::default().fg(theme::pal().error)
    } else {
        Style::default().fg(theme::pal().ice)
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
        PromptKind::Rename => " rename to ",
        PromptKind::GoToLine => " go to line ",
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
        Mode::Normal | Mode::Menu => theme::pal().ice,
        Mode::Picker => theme::pal().hot,
        Mode::Find => theme::pal().hot,
        Mode::Replace => theme::pal().amber,
        Mode::Prompt(_) => theme::pal().ice,
    };
    let mut spans: Vec<Span> = vec![
        Span::styled(format!(" {} ", state.mode_str().to_lowercase()), theme::block(mode_color)),
        Span::styled(format!(" {}", doc.name), theme::text()),
    ];
    if doc.dirty {
        spans.push(Span::styled(" ●", Style::default().fg(theme::pal().amber)));
    }
    if !state.status.is_empty() {
        let style = match state.alert {
            Some(Alert::Warn) => Style::default().fg(theme::pal().amber),
            Some(Alert::Error) => Style::default().fg(theme::pal().error).add_modifier(Modifier::BOLD),
            None => Style::default().fg(theme::pal().ice),
        };
        spans.push(Span::styled(format!("   {}", state.status), style));
    } else if doc.syntax_limited() {
        spans.push(Span::styled(format!("   {}", crate::syntax::TOO_LARGE), Style::default().fg(theme::pal().amber)));
    }

    let mut position = format!("ln {}, col {}", cursor_line + 1, cursor_col + 1);
    let carets = doc.extra_carets.len();
    if carets > 0 {
        position = format!("{} carets  {}", carets + 1, position);
    }
    if sel_len > 0 {
        position = format!("{} selected  {}", sel_len, position);
    }
    let (button, room) = match command_button_in(area) {
        Some(b) => {
            frame.render_widget(Paragraph::new(Line::from(vec![Span::styled(" F1 ", crate::hud::keycap()), Span::styled(" command", theme::dim())])), b);
            (b.width + 2, Rect::new(area.x, area.y, area.width - b.width - 2, area.height))
        }
        None => (0, area),
    };
    let right = render_right(frame, room, 0, &format!("{} ", position), theme::dim());
    let left = Rect::new(area.x, area.y, area.width.saturating_sub(button + right + 3), area.height);
    frame.render_widget(Paragraph::new(Line::from(spans)), left);
}

fn command_button_in(status: Rect) -> Option<Rect> {
    let width = 12;
    (status.width >= 40 && status.height > 0).then(|| Rect::new(status.x + status.width - width, status.y, width, 1))
}

pub fn command_button(area: Rect, state: &EditorState) -> Option<Rect> {
    command_button_in(layout(area, state.mode).3)
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

pub fn modal_rect(area: Rect, state: &EditorState) -> Option<Rect> {
    if state.reload.is_none() && state.update.is_none() {
        return None;
    }
    let width = 54.min(area.width);
    let height = 14.min(area.height);
    (width > 0 && height > 0).then(|| Rect::new(area.x + (area.width - width) / 2, area.y + (area.height - height) / 2, width, height))
}

fn render_reload_modal(frame: &mut Frame, state: &EditorState, area: Rect) {
    let (Some(prompt), Some(rect)) = (state.reload.as_ref(), modal_rect(area, state)) else {
        return;
    };
    let p = theme::pal();
    let doc = state.tabs.iter().find(|doc| doc.id == prompt.id);
    let name = doc.map(|doc| doc.name.as_str()).unwrap_or("this file");
    let inner = crate::hud::draw(frame.buffer_mut(), rect, &crate::hud::Chrome { title: "changed", tag: None, hazard: true });
    if inner.height < 4 || inner.width == 0 {
        return;
    }
    let reload_y = inner.y + inner.height - 3;
    let body = Rect::new(inner.x + 1, inner.y, inner.width.saturating_sub(2), reload_y.saturating_sub(inner.y));
    let mut lines = vec![
        Line::default(),
        Line::styled(format!("{name} {}", if prompt.deleted { "was deleted on disk." } else { "changed on disk." }), theme::text()),
    ];
    if doc.is_some_and(|doc| doc.dirty) && !prompt.deleted {
        lines.push(Line::styled("Reloading discards unsaved edits.", Style::default().fg(p.amber).add_modifier(Modifier::BOLD)));
    }
    if body.height > 0 {
        frame.render_widget(Paragraph::new(lines), body);
    }
    for (i, label) in ["Reload from disk", "Continue editing"].iter().enumerate() {
        let selected = prompt.choice == i;
        let row = Rect::new(inner.x, reload_y + i as u16, inner.width, 1);
        let mark = if selected { " ▶ " } else { "   " };
        if selected {
            frame.buffer_mut().set_style(row, Style::default().bg(p.selection));
        }
        let style = if selected {
            Style::default().fg(p.hot).bg(p.selection).add_modifier(Modifier::BOLD)
        } else {
            theme::text()
        };
        frame.render_widget(Paragraph::new(Span::styled(format!("{mark}{label}"), style)), row);
    }
    let hints = Rect::new(inner.x + 1, inner.y + inner.height - 1, inner.width.saturating_sub(2), 1);
    crate::hud::hints(frame.buffer_mut(), hints, &[("↑↓", "choose"), ("Enter", "confirm"), ("Esc", "keep editing")]);
}

fn render_update_modal(frame: &mut Frame, state: &EditorState, area: Rect) {
    let (Some((from, to)), Some(rect)) = (state.update.as_ref(), modal_rect(area, state)) else {
        return;
    };
    let p = theme::pal();
    let inner = crate::hud::draw(frame.buffer_mut(), rect, &crate::hud::Chrome { title: "update", tag: None, hazard: true });
    if inner.height == 0 {
        return;
    }
    let mut lines: Vec<Line> = vec![Line::default()];
    lines.extend(
        LOGO.iter()
            .enumerate()
            .map(|(i, row)| Line::styled(*row, Style::default().fg(crate::hud::mix(p.hot, p.ice, i as f32 / (LOGO.len() - 1) as f32)))),
    );
    lines.push(Line::default());
    lines.push(Line::styled("A new version of ee is ready.", theme::text()));
    lines.push(Line::from(vec![
        Span::styled(from.clone(), theme::dim()),
        Span::styled(" ━━▶ ", Style::default().fg(p.hot).add_modifier(Modifier::BOLD)),
        Span::styled(to.clone(), Style::default().fg(p.ice).add_modifier(Modifier::BOLD)),
    ]));
    lines.push(Line::default());
    if state.has_unsaved() {
        lines.push(Line::styled("save your changes first (Ctrl+S)", Style::default().fg(p.amber).add_modifier(Modifier::BOLD)));
    }
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), Rect::new(inner.x, inner.y, inner.width, inner.height.saturating_sub(1)));
    let items = [("Enter", "restart to update"), ("Esc", "keep working")];
    let need: u16 = items.iter().map(|(k, w)| (k.chars().count() + w.chars().count() + 5) as u16).sum::<u16>() - 2;
    let x = inner.x + inner.width.saturating_sub(need) / 2;
    crate::hud::hints(frame.buffer_mut(), Rect::new(x, inner.y + inner.height - 1, inner.width.saturating_sub(x - inner.x), 1), &items);
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
        state.tabs[0] = Document::with_content("t.md", "# alpha beta\n## beta\n");
        state.tabs[0].cursor = (1, 2);
        for mode in [Mode::Normal, Mode::Find, Mode::Replace, Mode::Prompt(PromptKind::GoToLine), Mode::Picker, Mode::Menu] {
            state.mode = Mode::Normal;
            state.picker = None;
            state.menu = None;
            state.update = None;
            if matches!(mode, Mode::Picker) {
                state.apply(crate::keys::Action::FileStructure);
                state.offer_update("abc1234".into(), "def5678".into());
            } else {
                state.mode = mode;
            }
            if matches!(mode, Mode::Menu) {
                state.menu = Some(crate::state::Menu { x: 200, y: 200, selected: 0 });
            }
            state.find.query = "beta".to_string();
            for (w, h) in [(10, 0), (0, 10), (1, 1), (4, 2), (5, 3), (20, 3), (20, 4), (80, 24), (120, 40), (200, 50)] {
                let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
                t.draw(|f| render(f, &state)).unwrap();
            }
        }
    }

    #[test]
    fn editor_fills_every_row_not_used_by_a_bar() {
        let mut state = EditorState::new();
        state.tabs[0] = Document::with_content("t", "x");
        for (mode, bars) in [(Mode::Normal, 2), (Mode::Find, 3), (Mode::Prompt(PromptKind::GoToLine), 3), (Mode::Menu, 2)] {
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
    fn the_menu_lists_copy_paste_and_both_herdr_rows_on_the_clicked_cell() {
        let area = Rect::new(0, 0, 80, 24);
        let mut state = EditorState::new();
        state.tabs[0] = Document::with_content("t", "alpha\nbeta");
        state.apply(crate::keys::Action::OpenMenu { line: 0, col: 0, x: 4, y: 22 });
        let mut t = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
        t.draw(|f| render(f, &state)).unwrap();
        let rows = render_rows(t.backend().buffer());
        let shown: Vec<&String> = rows.iter().filter(|row| row.contains("Copy") || row.contains("Paste") || row.contains("herdr")).collect();
        assert_eq!(shown.len(), 4, "all four rows stay on screen when the click is on the last line: {:?}", shown);
        assert!(shown[0].contains("Copy") && shown[0].contains("Ctrl+C"), "{}", shown[0]);
        assert!(shown[1].contains("Paste") && shown[1].contains("Ctrl+V"), "{}", shown[1]);
        assert!(shown[2].contains("Copy to herdr") && shown[2].contains("Alt+Shift+E"), "{}", shown[2]);
        assert!(shown[3].contains("Move to herdr") && shown[3].contains("Alt+Shift+M"), "{}", shown[3]);
        let rect = menu_rect(area, &state).unwrap();
        assert!(rect.bottom() <= area.bottom() && rect.y < 22, "the menu opens above a click on the last line");
        assert!(matches!(menu_hit(area, &state, rect.x + 2, rect.y + 2), MenuHit::Row(1)), "the second inner row is Paste");
        assert!(matches!(menu_hit(area, &state, rect.x, rect.y + 1), MenuHit::Frame));
        assert!(matches!(menu_hit(area, &state, 0, 0), MenuHit::Outside));
        state.apply(crate::keys::Action::Down);
        t.draw(|f| render(f, &state)).unwrap();
        let buf = t.backend().buffer().clone();
        let paste = (rect.y + 2, rect.x + 4);
        assert_eq!(buf.cell(Position { x: paste.1, y: paste.0 }).unwrap().bg, theme::pal().selection, "down highlights Paste");
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
    fn docx_lines_hide_markup_off_the_caret_and_clicks_land_on_the_source_char() {
        let area = Rect::new(0, 0, 40, 10);
        let editor = editor_rect_for(area, Mode::Normal);
        let mut state = EditorState::new();
        state.tabs[0] = Document::with_content("n.docx", "# Chapter\nHello **world**\na \\* b");
        state.tabs[0].cursor = (1, 0);
        let mut t = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
        t.draw(|f| render(f, &state)).unwrap();
        let rows = render_rows(t.backend().buffer());
        let heading = &rows[editor.y as usize];
        assert!(heading.contains("Chapter") && !heading.contains('#'), "{heading}");
        let escaped = &rows[editor.y as usize + 2];
        assert!(escaped.contains("a * b") && !escaped.contains('\\'), "{escaped}");
        let head = t.backend().buffer().cell((2, editor.y)).unwrap();
        assert_eq!(head.symbol(), "C");
        assert_eq!(head.fg, theme::pal().hot);
        assert!(head.modifier.contains(Modifier::BOLD));

        state.tabs[0].cursor = (0, 0);
        t.draw(|f| render(f, &state)).unwrap();
        let rows = render_rows(t.backend().buffer());
        let bold = &rows[editor.y as usize + 1];
        assert!(bold.contains("Hello world") && !bold.contains('*'), "{bold}");
        let world = t.backend().buffer().cell((8, editor.y + 1)).unwrap();
        assert_eq!(world.symbol(), "w");
        assert!(world.modifier.contains(Modifier::BOLD), "world is bold");
        let doc = &state.tabs[0];
        let source: Vec<char> = doc.lines[1].chars().collect();
        let row: Vec<char> = rows[editor.y as usize + 1].chars().collect();
        let start = row.iter().position(|c| *c == 'H').unwrap();
        for x in start..start + "Hello world".chars().count() {
            let (line, col) = mouse_to_doc(editor, x as u16, editor.y + 1, doc).unwrap();
            assert_eq!((line, source[col]), (1, row[x]), "x={}", x);
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
        let selected = |x: u16, y: u16| buf.cell((x, y)).unwrap().bg == theme::pal().selection;
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

        let mut state = EditorState::new();
        state.tabs[0] = Document::with_content("t", "echo one two\n  echo three four\nplain");
        state.tabs[0].cursor = (0, 5);
        state.apply(crate::keys::Action::AddCaretDown);
        state.apply(crate::keys::Action::SelectEnd);
        let t = draw(&state, 30, 6);
        let bg = |line: u16, col: u16| t.backend().buffer().cell((2 + col, 1 + line)).unwrap().bg;
        let p = theme::pal();
        for (line, from, to) in [(0, 5, 12), (1, 5, 17)] {
            for col in from..to {
                assert_eq!(bg(line, col), p.selection, "shift+end paints line {} col {}", line, col);
            }
            assert_ne!(bg(line, from - 1), p.selection, "line {} col {} is not selected", line, from - 1);
        }
        assert_eq!(bg(1, 0), p.cursor_line, "the caret line keeps its tint outside the selection");
        assert_eq!((bg(0, 12), bg(1, 17)), (p.ice, p.hot), "a caret at the end of each selection");
        state.apply(crate::keys::Action::SelectHome);
        let t = draw(&state, 30, 6);
        let bg = |line: u16, col: u16| t.backend().buffer().cell((2 + col, 1 + line)).unwrap().bg;
        assert_eq!((bg(0, 0), bg(1, 2)), (p.ice, p.hot), "the carets move to each line's first non-blank");
        assert_eq!((1..5).map(|c| bg(0, c)).collect::<Vec<_>>(), vec![p.selection; 4]);
        assert_eq!((3..5).map(|c| bg(1, c)).collect::<Vec<_>>(), vec![p.selection; 2], "shift+home selects back from each anchor");
        assert_eq!((bg(1, 1), bg(1, 5)), (p.cursor_line, p.cursor_line), "nothing selected outside the ranges");
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

    #[test]
    fn mouse_wheel_scrolls_the_view_and_the_caret_brings_it_back() {
        let text: Vec<String> = (1..=40).map(|n| format!("line {}", n)).collect();
        let mut state = EditorState::new();
        state.tabs[0] = Document::with_content("t.txt", &text.join("\n"));
        let area = Rect::new(0, 0, 20, 12);
        let mut t = Terminal::new(TestBackend::new(20, 12)).unwrap();
        let mut first_row = |state: &EditorState| {
            t.draw(|f| render(f, state)).unwrap();
            render_rows(t.backend().buffer())[1].trim().to_string()
        };
        assert_eq!(first_row(&state), "1 line 1");
        scroll_view(area, &state, 3);
        scroll_view(area, &state, 3);
        assert_eq!(first_row(&state), "7 line 7", "two notches down");
        assert_eq!(state.tabs[0].cursor, (0, 0), "the caret stays put");
        for _ in 0..20 {
            scroll_view(area, &state, 3);
        }
        assert_eq!(first_row(&state), "31 line 31", "stops with the last line at the bottom");
        scroll_view(area, &state, -100);
        assert_eq!(first_row(&state), "1 line 1", "stops at the top");
        scroll_view(area, &state, 9);
        assert_eq!(first_row(&state), "10 line 10");
        state.apply(crate::keys::Action::Down);
        assert_eq!(first_row(&state), " 2 line 2".trim(), "moving the caret brings the view back");
    }

    #[test]
    fn clicks_on_centred_markdown_find_the_link_under_the_mouse() {
        let area = Rect::new(0, 0, 40, 12);
        let editor = editor_rect_for(area, Mode::Normal);
        let mut state = EditorState::new();
        state.tabs[0] = Document::with_content("t.md", "<div align=\"center\">\n\n[go](#here)\n\n</div>\n\n# here");
        state.tabs[0].cursor = (6, 0);
        let mut t = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
        t.draw(|f| render(f, &state)).unwrap();
        let row: String = render_rows(t.backend().buffer())[editor.y as usize + 2].clone();
        let x = row.find("go").unwrap() as u16;
        assert_eq!(x, 2 + (38 - 2) / 2, "centred in the text area: {:?}", row);
        let doc = &state.tabs[0];
        assert_eq!(link_at(editor, x, editor.y + 2, doc).as_deref(), Some("#here"));
        assert_eq!(link_at(editor, 3, editor.y + 2, doc), None, "the padding is not a link");
        assert_eq!(mouse_to_doc(editor, x, editor.y + 2, doc), Some((2, 1)), "lands on the g in [go](#here)");
    }

    fn draw(state: &EditorState, w: u16, h: u16) -> Terminal<TestBackend> {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| render(f, state)).unwrap();
        t
    }

    #[test]
    fn the_file_structure_popup_lists_headings_and_scrolls_to_the_selection() {
        let headings: Vec<String> = (1..=30).map(|n| format!("# H{}\n", n)).collect();
        let mut state = EditorState::new();
        state.tabs[0] = Document::with_content("t.md", &headings.concat());
        state.apply(crate::keys::Action::FileStructure);
        let rows = render_rows(draw(&state, 80, 24).backend().buffer());
        assert!(rows.iter().any(|r| r.contains("╸ FILE STRUCTURE ╺")), "the title is notched into the frame, in capitals");
        assert!(rows.iter().any(|r| r.contains("╸ 30/30 ╺")), "the count sits on the bottom edge");
        assert!(rows.iter().any(|r| r.contains(" ↑↓  choose") && r.contains(" Enter  jump") && r.contains(" Esc  close")), "the hint row");
        let first = rows.iter().position(|r| r.contains("▶ H1 ")).expect("the first heading is selected");
        assert!(rows[first].contains("line 1"));
        assert_eq!(picker_row(Rect::new(0, 0, 80, 24), &state).map(|r| r.y as usize), Some(first), "the lock-on effect targets the drawn row");
        state.picker.as_mut().unwrap().selected = 25;
        let rows = render_rows(draw(&state, 80, 24).backend().buffer());
        let selected = rows.iter().position(|r| r.contains("▶ H26")).expect("the selection scrolls into view");
        assert_eq!(picker_row(Rect::new(0, 0, 80, 24), &state).map(|r| r.y as usize), Some(selected));
        assert!(!rows[selected + 1].contains(" H27"), "it sits on the last visible row");
        assert!(!rows.iter().any(|r| r.contains(" H11 ")), "rows above the window scrolled away");
    }

    #[test]
    fn the_update_modal_shows_the_versions_and_warns_about_unsaved_work() {
        let area = Rect::new(0, 0, 80, 24);
        let mut state = EditorState::new();
        assert_eq!(modal_rect(area, &state), None);
        state.offer_update("abc1234".into(), "def5678".into());
        let rect = modal_rect(area, &state).unwrap();
        assert_eq!(rect, Rect::new(13, 5, 54, 14));
        let t = draw(&state, 80, 24);
        let buf = t.backend().buffer();
        let rows = render_rows(buf);
        assert!(rows.iter().any(|r| r.contains("abc1234 ━━▶ def5678")));
        assert!(rows[rect.y as usize].contains("╱╱╱╱╱╱"), "the hazard band marks the one popup that asks for attention");
        assert!(rows.iter().any(|r| r.contains(" Enter  restart to update") && r.contains(" Esc  keep working")));
        let corner = |x: u16, y: u16| buf.cell((x, y)).unwrap().symbol().to_string();
        assert_eq!([corner(rect.x, rect.y), corner(rect.right() - 1, rect.y), corner(rect.x, rect.bottom() - 1), corner(rect.right() - 1, rect.bottom() - 1)], ["┏", "┓", "┗", "┛"]);
        assert_eq!((buf.cell((rect.right(), rect.y + 2)).unwrap().bg, buf.cell((rect.x + 3, rect.bottom())).unwrap().bg), (crate::hud::shadow_color(), crate::hud::shadow_color()), "it floats on a shadow");
        assert_ne!(buf.cell((rect.x + 3, rect.y + 1)).unwrap().bg, buf.cell((rect.x + 3, rect.y + 2)).unwrap().bg, "scanlines alternate");
        assert!(!rows.iter().any(|r| r.contains("save your changes first")));
        state.tabs[0].dirty = true;
        let rows = render_rows(draw(&state, 80, 24).backend().buffer());
        assert!(rows.iter().any(|r| r.contains("save your changes first")));
    }

    #[test]
    fn effect_areas_match_what_is_on_screen() {
        let area = Rect::new(0, 0, 40, 10);
        let mut state = EditorState::new();
        state.tabs[0] = Document::with_content("t", "hello world\nxy\nthird line");
        state.tabs[0].cursor = (2, 0);
        state.tabs[0].extra_carets = vec![(0, 8)];
        state.tabs[0].selection = Some(((0, 2), (0, 5)));
        let a = areas(area, &state);
        assert_eq!(a.caret_rows, vec![Rect::new(2, 1, 11, 1), Rect::new(2, 3, 10, 1)]);
        assert_eq!(a.selection, Some(Rect::new(4, 1, 3, 1)));
        let t = draw(&state, 40, 10);
        let buf = t.backend().buffer();
        for x in 4..7 {
            assert_eq!(buf.cell((x, 1)).unwrap().bg, theme::pal().selection, "the selection rect covers selected cells");
        }
        assert_eq!((a.popup, a.modal), (None, None));
        state.offer_update("a".into(), "b".into());
        assert_eq!(areas(area, &state).modal, modal_rect(area, &state));
    }

    #[test]
    fn find_hints_never_cover_the_query_and_matches_are_highlighted() {
        let mut state = EditorState::new();
        state.tabs[0] = Document::with_content("t", "beta alpha beta");
        state.apply(crate::keys::Action::Find);
        for c in "beta".chars() {
            state.apply(crate::keys::Action::InsertChar(c));
        }
        let t = draw(&state, 60, 8);
        let buf = t.backend().buffer();
        assert_eq!(buf.cell((13, 1)).unwrap().bg, theme::pal().find_match, "the second beta is highlighted");
        assert_ne!(buf.cell((8, 1)).unwrap().bg, theme::pal().find_match, "alpha is not");

        state.find.query = "a-very-long-query".to_string();
        let rows = render_rows(draw(&state, 60, 8).backend().buffer());
        assert!(rows[6].contains("a-very-long-query"), "the hint gives way: {:?}", rows[6]);

        let mut state = EditorState::new();
        state.tabs[0] = Document::with_content("t.md", "beta\n\nalpha **beta**");
        state.apply(crate::keys::Action::Find);
        for c in "beta".chars() {
            state.apply(crate::keys::Action::InsertChar(c));
        }
        let t = draw(&state, 60, 8);
        let row: String = render_rows(t.backend().buffer())[3].clone();
        let x = row.find("beta").unwrap() as u16;
        assert_eq!(t.backend().buffer().cell((x, 3)).unwrap().bg, theme::pal().find_match, "matches show on rendered markdown too");
    }

    #[cfg(feature = "lang-rust")]
    #[test]
    fn syntax_colours_show_on_code_and_give_way_to_the_caret_selection_and_find() {
        use crate::keys::Action;
        let mut state = EditorState::new();
        let source = "fn main() { let s = \"hi\"; let t = \"hi\"; }";
        let at = |needle: &str| source.find(needle).unwrap();
        let other = source.rfind("hi").unwrap();
        state.tabs[0] = Document::with_content("a.rs", source);
        state.tabs[0].cursor = (0, source.chars().count());
        let cell = |t: &Terminal<TestBackend>, col: usize| t.backend().buffer().cell((2 + col as u16, 1)).unwrap().clone();
        let t = draw(&state, 60, 8);
        assert_eq!(cell(&t, at("fn")).fg, theme::pal().ice, "fn");
        assert_eq!(cell(&t, at("main")).fg, theme::pal().neon, "main");
        assert_eq!(cell(&t, at("let")).fg, theme::pal().ice, "let");
        assert_eq!(cell(&t, at("hi")).fg, theme::pal().amber, "string");

        state.tabs[0].cursor = (0, at("n"));
        let t = draw(&state, 60, 8);
        let on_caret = cell(&t, at("n"));
        assert_eq!((on_caret.fg, on_caret.bg), (theme::pal().void, theme::pal().hot), "the caret covers the keyword");

        state.tabs[0].cursor = (0, source.chars().count());
        state.tabs[0].selection = Some(((0, at("fn")), (0, at("fn") + 2)));
        let t = draw(&state, 60, 8);
        let selected = cell(&t, at("fn"));
        assert_eq!((selected.fg, selected.bg), (theme::pal().text, theme::pal().selection), "the selection covers the keyword");

        state.tabs[0].selection = None;
        state.apply(Action::Find);
        for c in "hi".chars() {
            state.apply(Action::InsertChar(c));
        }
        let t = draw(&state, 80, 8);
        let current = cell(&t, at("hi"));
        assert_eq!((current.fg, current.bg), (theme::pal().text, theme::pal().selection), "the current match is a selection");
        let hit = cell(&t, other);
        assert_eq!((hit.fg, hit.bg), (theme::pal().ice, theme::pal().find_match), "another match covers the string colour");
    }

    #[cfg(feature = "lang-rust")]
    #[test]
    fn a_theme_switch_recolours_syntax_on_the_next_frame() {
        let mut state = EditorState::new();
        state.tabs[0] = Document::with_content("a.rs", "fn main() {}");
        state.tabs[0].cursor = (0, 12);
        let neon = draw(&state, 40, 6).backend().buffer().cell((3, 1)).unwrap().fg;
        assert_eq!(neon, theme::NEON_PALETTE.ice);
        let night = theme::from_omarchy("background = \"#150f0f\"\nforeground = \"#ffffff\"\naccent = \"#65a4d9\"\nmagenta = \"#d879bb\"\ngreen = \"#47ac3a\"\n").unwrap();
        theme::set(night);
        let mapped = draw(&state, 40, 6).backend().buffer().cell((3, 1)).unwrap().fg;
        theme::set(theme::NEON_PALETTE);
        assert_eq!(mapped, night.ice);
        assert_ne!(mapped, neon);
    }

    #[test]
    fn plain_text_is_unchanged_and_an_oversized_file_says_so() {
        let mut state = EditorState::new();
        state.tabs[0] = Document::with_content("a.txt", "fn main() {}");
        state.tabs[0].cursor = (0, 12);
        let t = draw(&state, 40, 8);
        assert_eq!(t.backend().buffer().cell((3, 1)).unwrap().fg, theme::pal().text, "a .txt file is not highlighted");

        state.tabs[0] = Document::new("big.rs");
        state.tabs[0].lines = vec![String::new(); crate::syntax::MAX_LINES + 1];
        let rows = render_rows(draw(&state, 80, 8).backend().buffer());
        if cfg!(feature = "lang-rust") {
            assert!(rows[7].contains(crate::syntax::TOO_LARGE), "{:?}", rows[7]);
            state.status = "saved".into();
            let rows = render_rows(draw(&state, 80, 8).backend().buffer());
            assert!(rows[7].contains("saved"), "{:?}", rows[7]);
            assert!(!rows[7].contains("too large"));
            state.status.clear();
            state.tabs[0].lines.truncate(3);
            let rows = render_rows(draw(&state, 80, 8).backend().buffer());
            assert!(!rows.iter().any(|r| r.contains("too large")));
        } else {
            assert!(!rows.iter().any(|r| r.contains("too large")), "{:?}", rows[7]);
        }
    }

    #[cfg(feature = "lang-bash")]
    #[test]
    fn bash_past_its_line_guard_stays_plain_and_says_so() {
        let mut state = EditorState::new();
        let mut doc = Document::new("big.sh");
        doc.lines = vec!["echo hi".into(); crate::syntax::BASH_MAX_LINES + 1];
        doc.cursor = (0, 7);
        state.tabs[0] = doc;
        let t = draw(&state, 80, 8);
        let rows = render_rows(t.backend().buffer());
        assert!(rows[7].contains(crate::syntax::TOO_LARGE), "{:?}", rows[7]);
        let echo = t.backend().buffer().cell((gutter(&state.tabs[0]) as u16, 1)).unwrap();
        assert_eq!(echo.fg, theme::pal().text, "bash past the guard is plain");

        let mut doc = Document::new("ok.sh");
        doc.lines = vec!["echo hi".into(); crate::syntax::BASH_MAX_LINES];
        doc.cursor = (0, 7);
        state.tabs[0] = doc;
        let t = draw(&state, 80, 8);
        let rows = render_rows(t.backend().buffer());
        assert!(!rows.iter().any(|r| r.contains("too large")), "{:?}", rows[7]);
        let echo = t.backend().buffer().cell((gutter(&state.tabs[0]) as u16, 1)).unwrap();
        assert_eq!(echo.fg, theme::pal().neon, "bash at the guard is still coloured");
    }

    #[test]
    fn paging_keeps_the_caret_on_its_screen_row_and_never_scrolls_past_the_end() {
        let text: Vec<String> = (0..40).map(|n| format!("row{}", n)).collect();
        let mut state = EditorState::new();
        state.tabs[0] = Document::with_content("t", &text.join("\n"));
        state.tabs[0].cursor = (3, 0);
        let area = Rect::new(0, 0, 30, 12);
        state.page_rows = page_rows(area, &state);
        let height = editor_rect_for(area, state.mode).height as usize;
        assert_eq!(state.page_rows, height);
        let caret_row = |state: &EditorState| {
            let rows = render_rows(draw(state, 30, 12).backend().buffer());
            let caret = rows.iter().position(|r| r.contains(&format!("row{} ", state.tabs[0].cursor.0))).unwrap();
            (caret, rows)
        };
        let (before, _) = caret_row(&state);
        state.apply(crate::keys::Action::PageDown);
        let (after, rows) = caret_row(&state);
        assert_eq!(state.tabs[0].cursor.0, 3 + height);
        assert_eq!(after, before, "the caret keeps its row on screen: {:?}", rows);
        assert_eq!(areas(area, &state).top, height, "effects know the view's top line");
        for _ in 0..5 {
            state.apply(crate::keys::Action::PageDown);
        }
        let rows = render_rows(draw(&state, 30, 12).backend().buffer());
        let last = rows.iter().rposition(|r| r.contains("row")).unwrap();
        assert!(rows[last].contains("row39 "), "the last line sits at the bottom: {:?}", rows);
        assert_eq!(rows.iter().filter(|r| r.contains("row")).count(), height, "no empty rows below the end");

        state.apply(crate::keys::Action::FindAction);
        let popup = popup_rect(area, &state).unwrap();
        assert_eq!(page_rows(area, &state), popup.height as usize - 3, "pickers page by their visible rows");
    }

    #[test]
    fn long_lines_wrap_so_the_caret_and_a_click_follow_the_rows() {
        let long: String = (0..200).map(|i| char::from(b'0' + (i % 10) as u8)).collect();
        let mut state = EditorState::new();
        state.tabs[0] = Document::with_content("t.txt", &long);
        state.tabs[0].cursor = (0, 150);
        let area = Rect::new(0, 0, 30, 8);
        let editor = editor_rect_for(area, state.mode);
        let t = draw(&state, 30, 8);
        let rows = render_rows(t.backend().buffer());
        assert_eq!(state.tabs[0].scroll_left.get(), 0);
        assert!(rows[1].starts_with("1 0123456789"), "the first row is the start of the line: {:?}", rows[1]);
        assert!(rows[6].starts_with("  "), "a continuation has a blank gutter: {:?}", rows[6]);
        assert_eq!(rows[6].chars().skip(2).take(10).collect::<String>(), "0123456789", "row of columns 140..: {:?}", rows[6]);
        assert_eq!(t.backend().buffer().cell((12, 6)).unwrap().bg, theme::pal().hot, "column 150 is ten cells into that row");

        state.apply(crate::keys::Action::Down);
        assert_eq!(state.tabs[0].cursor, (0, 178), "down keeps visual column 10");
        state.apply(crate::keys::Action::Home);
        let rows = render_rows(draw(&state, 30, 8).backend().buffer());
        assert!(rows[1].starts_with("1 0123456789"), "Home is the start of the source line: {:?}", rows[1]);

        state.tabs[0].cursor = (0, 150);
        scroll_sideways(area, &state, 40);
        assert_eq!((state.tabs[0].scroll_left.get(), state.tabs[0].cursor), (0, (0, 150)), "the sideways wheel does not pan");
        let doc = &state.tabs[0];
        assert_eq!(mouse_to_doc(editor, 5, editor.y + 1, doc), Some((0, 31)), "the second row starts at column 28");
        state.tabs[0].cursor = (0, 200);
        draw(&state, 30, 8);
        assert_eq!(mouse_to_doc(editor, 6, 6, &state.tabs[0]), Some((0, 200)), "the cell past the last character is the end of the line");
    }

    #[test]
    fn a_selection_and_a_second_caret_paint_on_the_row_they_wrap_onto() {
        let long: String = (0..80).map(|i| char::from(b'a' + (i % 26) as u8)).collect();
        let mut state = EditorState::new();
        state.tabs[0] = Document::with_content("t.txt", &long);
        state.tabs[0].selection = Some(((0, 20), (0, 40)));
        state.tabs[0].cursor = (0, 80);
        let area = Rect::new(0, 0, 30, 8);
        let t = draw(&state, 30, 8);
        let bg = |x, y| t.backend().buffer().cell((x, y)).unwrap().bg;
        let sel = theme::pal().selection;
        for x in 22..30 {
            assert_eq!(bg(x, 1), sel, "columns 20..28 of the first row, x={x}");
        }
        for x in 2..14 {
            assert_eq!(bg(x, 2), sel, "columns 28..40 of the next row, x={x}");
        }
        assert_ne!(bg(21, 1), sel, "the cell before the selection");
        assert_ne!(bg(14, 2), sel, "the cell after the selection");
        assert_eq!(areas(area, &state).selection, None, "a selection on two rows is not one glow rect");

        state.tabs[0].selection = None;
        state.tabs[0].cursor = (0, 30);
        state.tabs[0].extra_carets = vec![(0, 2)];
        let t = draw(&state, 30, 8);
        let bg = |x, y| t.backend().buffer().cell((x, y)).unwrap().bg;
        assert_eq!(bg(4, 1), theme::pal().ice, "the extra caret stays on the first row");
        assert_eq!(bg(4, 2), theme::pal().hot, "the active caret is on the next row, same visual column");
    }

    #[test]
    fn paging_a_wrapped_line_keeps_the_caret_on_its_screen_row() {
        let long: String = (0..400).map(|i| char::from(b'0' + (i % 10) as u8)).collect();
        let mut state = EditorState::new();
        state.tabs[0] = Document::with_content("t.txt", &long);
        let area = Rect::new(0, 0, 30, 8);
        draw(&state, 30, 8);
        state.page_rows = page_rows(area, &state);
        assert_eq!(state.page_rows, 6);
        let caret_y = |state: &EditorState| {
            let t = draw(state, 30, 8);
            (1..7).find(|&y| t.backend().buffer().cell((2, y)).unwrap().bg == theme::pal().hot).unwrap()
        };
        let before = caret_y(&state);
        state.apply(crate::keys::Action::PageDown);
        assert_eq!(state.tabs[0].cursor, (0, 168), "a page is six rows of 28 columns, still on the same line");
        assert_eq!(caret_y(&state), before);
        let t = draw(&state, 30, 8);
        assert_eq!(t.backend().buffer().cell((2, before)).unwrap().symbol(), "8");
    }

    #[test]
    fn a_wrapped_markdown_line_is_clickable_and_a_rule_stays_one_row() {
        let mut state = EditorState::new();
        let body = format!("{} [go](#target)", "x".repeat(40));
        state.tabs[0] = Document::with_content("t.md", &format!("{body}\nshort\n---\n\n# target"));
        state.tabs[0].cursor = (1, 0);
        let area = Rect::new(0, 0, 30, 12);
        let editor = editor_rect_for(area, Mode::Normal);
        let t = draw(&state, 30, 12);
        let rows = render_rows(t.backend().buffer());
        let doc = &state.tabs[0];
        assert_eq!(link_at(editor, 15, editor.y + 1, doc).as_deref(), Some("#target"));
        assert_eq!(mouse_to_doc(editor, 15, editor.y + 1, doc), Some((0, 42)), "the g in [go](#target)");
        let rule = rows.iter().find(|row| row.contains('─')).expect("the rule is on screen");
        assert_eq!(rule.chars().filter(|c| *c == '─').count(), 28, "the rule fills the text width once: {rule}");
        assert_eq!(rows.iter().filter(|row| row.contains('─')).count(), 1);

        state.tabs[0] = Document::with_content("t.md", &format!("<div align=\"center\">\n\n{}\n\n</div>", "y".repeat(40)));
        state.tabs[0].cursor = (0, 0);
        let t = draw(&state, 30, 12);
        let rows = render_rows(t.backend().buffer());
        let first = rows.iter().position(|row| row.contains('y')).unwrap();
        assert_eq!(rows[first].find('y'), Some(2), "wider than the screen, so it is not centred: {:?}", rows[first]);
        assert!(rows[first + 1].contains('y'), "the rest wraps: {:?}", rows[first + 1]);
        assert_eq!(mouse_to_doc(editor, 2, (first + 1) as u16, &state.tabs[0]), Some((2, 28)));
    }

    #[test]
    fn a_rendered_markdown_table_stays_one_row_and_the_caret_line_still_wraps() {
        let mut state = EditorState::new();
        let src = "\
| left | right |
|---|---|
| aaaaaaaa | bbbbbbbbbbbbbbbbbbbbbbbbb |

after
xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
        state.tabs[0] = Document::with_content("t.md", src);
        state.tabs[0].cursor = (4, 0);
        let area = Rect::new(0, 0, 30, 12);
        let editor = editor_rect_for(area, Mode::Normal);
        let rows = render_rows(draw(&state, 30, 12).backend().buffer());
        let body = rows.iter().position(|row| row.contains("aaaaaaaa")).unwrap();
        assert!(rows[body].contains('│'), "the table is rendered: {:?}", rows[body]);
        assert!(rows[body + 1].starts_with("4 "), "the next source line follows at once: {:?}", rows[body + 1]);
        assert!(!rows[body + 1].contains('b'), "the rest of the row is clipped: {:?}", rows[body + 1]);
        let rule = rows.iter().position(|row| row.contains('═')).unwrap();
        assert_eq!(rows.iter().filter(|row| row.contains('═')).count(), 1, "the rule is one row: {:?}", rows[rule]);
        let xrow = rows.iter().position(|row| row.contains('x')).unwrap();
        assert!(rows[xrow + 1].starts_with("  ") && rows[xrow + 1].contains('x'), "a plain line still wraps: {:?}", rows[xrow + 1]);
        assert_eq!(mouse_to_doc(editor, 4, body as u16, &state.tabs[0]), Some((2, 2)), "the first a");

        state.tabs[0].cursor = (2, 0);
        let rows = render_rows(draw(&state, 30, 12).backend().buffer());
        let raw = rows.iter().position(|row| row.contains("aaaaaaaa")).unwrap();
        assert!(rows[raw].contains('|'), "the caret line shows the source: {:?}", rows[raw]);
        assert!(rows[raw + 1].starts_with("  ") && rows[raw + 1].contains('b'), "that source still wraps: {:?}", rows[raw + 1]);
    }

    #[cfg(feature = "lang-rust")]
    #[test]
    fn a_string_keeps_its_colour_on_the_next_wrapped_row() {
        let source = "let s = \"0123456789abcdefghijklmnopqrstuvwxyz\";";
        let mut state = EditorState::new();
        state.tabs[0] = Document::with_content("a.rs", source);
        state.tabs[0].cursor = (0, source.chars().count());
        let t = draw(&state, 30, 8);
        let buf = t.backend().buffer();
        assert_eq!(buf.cell((2, 1)).unwrap().fg, theme::pal().ice, "let");
        assert_eq!(buf.cell((2, 2)).unwrap().fg, theme::pal().amber, "the string continues on the next row");
    }

    #[test]
    fn the_reload_modal_marks_the_chosen_row() {
        let mut state = EditorState::new();
        state.tabs[0].name = "notes.txt".into();
        state.reload = Some(crate::state::ReloadPrompt { id: state.tabs[0].id, choice: 0, deleted: false });
        let rows = render_rows(draw(&state, 80, 24).backend().buffer());
        assert!(rows.iter().any(|row| row.contains("CHANGED")));
        assert!(rows.iter().any(|row| row.contains("notes.txt changed on disk.")));
        assert!(rows.iter().any(|row| row.contains("╱╱╱╱╱╱")));
        let marked = rows.iter().find(|row| row.contains('▶')).unwrap();
        assert!(marked.contains("Reload from disk"), "{marked}");
        assert!(rows.iter().any(|row| row.contains("Continue editing")));
        assert!(rows.iter().any(|row| row.contains("↑↓") && row.contains("choose") && row.contains("Esc")));

        state.reload.as_mut().unwrap().choice = 1;
        state.tabs[0].dirty = true;
        let rows = render_rows(draw(&state, 80, 24).backend().buffer());
        let marked = rows.iter().find(|row| row.contains('▶')).unwrap();
        assert!(marked.contains("Continue editing"), "{marked}");
        assert!(rows.iter().any(|row| row.contains("Reloading discards unsaved edits.")));

        state.tabs[0].dirty = false;
        state.reload.as_mut().unwrap().deleted = true;
        let rows = render_rows(draw(&state, 80, 24).backend().buffer());
        assert!(rows.iter().any(|row| row.contains("notes.txt was deleted on disk.")));
        assert!(!rows.iter().any(|row| row.contains("discards")));
    }

    #[test]
    fn the_command_button_opens_the_palette_from_the_bottom_right() {
        let mut state = EditorState::new();
        let area = Rect::new(0, 0, 80, 24);
        let button = command_button(area, &state).unwrap();
        assert_eq!(button, Rect::new(68, 23, 12, 1), "bottom right of the status bar");
        let rows = render_rows(draw(&state, 80, 24).backend().buffer());
        assert_eq!(rows[23].chars().skip(68).collect::<String>(), " F1  command");
        assert!(rows[23].contains("ln 1, col 1"), "the position still shows: {:?}", rows[23]);
        assert!(command_button(Rect::new(0, 0, 30, 10), &state).is_none(), "no button when the bar is too narrow");

        state.apply(crate::keys::Action::FindAction);
        let rows = render_rows(draw(&state, 80, 24).backend().buffer());
        assert!(rows.iter().any(|r| r.contains("COMMAND PALETTE")), "F1 opens the palette: {:?}", rows);
        assert!(!rows.iter().any(|r| r.contains("any key") && r.contains("close")), "the shortcut sheet is gone");
        assert_eq!(areas(area, &state).popup, popup_rect(area, &state));
        for (w, h) in [(0, 0), (12, 3), (30, 6), (40, 8)] {
            draw(&state, w, h);
        }
    }

    #[cfg(feature = "lang-rust")]
    #[test]
    fn rust_types_hide_off_the_caret_line_and_clicks_land_on_the_source_char() {
        let source = "fn main() -> u32 {\n    let value: HashMap<&'static str, Vec<(String, u32)>> = HashMap::new();\n    let done = 1;\n}";
        let mut state = EditorState::new();
        state.tabs[0] = Document::with_content("a.rs", source);
        state.apply(crate::keys::Action::ToggleRustView);
        let area = Rect::new(0, 0, 40, 12);
        let editor = editor_rect_for(area, Mode::Normal);
        let term = draw(&state, area.width, area.height);
        let buf = term.backend().buffer();
        let rows = render_rows(buf);
        assert!(rows[editor.y as usize].contains("-> u32"), "the caret line stays raw: {}", rows[editor.y as usize]);
        let value = rows.iter().position(|row| row.contains("value")).unwrap();
        let done = rows.iter().position(|row| row.contains("done")).unwrap();
        assert_eq!(done, value + 1, "hiding the type does not leave a wrapped row: {:?}", rows);
        assert!(!rows[value].contains("static"), "{}", rows[value]);
        let row: Vec<char> = rows[value].chars().collect();
        let start = row.iter().position(|c| *c == 'l').unwrap();
        assert_eq!(buf.cell((start as u16, value as u16)).unwrap().fg, theme::pal().ice, "let stays a keyword");
        let doc = &state.tabs[0];
        let source_chars: Vec<char> = doc.lines[1].chars().collect();
        for (x, ch) in row.iter().enumerate().skip(start) {
            if !ch.is_alphanumeric() {
                continue;
            }
            let (line, col) = mouse_to_doc(editor, x as u16, value as u16, doc).unwrap();
            assert_eq!((line, source_chars[col]), (1, *ch), "x={x}");
        }

        state.apply(crate::keys::Action::ToggleRustView);
        let rows = render_rows(draw(&state, area.width, area.height).backend().buffer());
        assert!(rows.iter().any(|row| row.contains("static")), "turning it off shows the type again: {:?}", rows);
    }
}
