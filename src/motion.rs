use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration as StdDuration, Instant};

use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use tachyonfx::fx::{self, Glitch};
use tachyonfx::pattern::SweepPattern;
use tachyonfx::{Duration, Effect, EffectManager, Interpolation, IntoEffect, Motion as Dir};

use crate::state::{Cue, Departure, Toward};
use crate::theme;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
enum Slot {
    #[default]
    Screen,
    Editor,
    Bar,
    Status,
    Match,
    Sent,
    Popup,
    Modal,
    Lock,
    Depart,
}

pub struct Areas {
    pub screen: Rect,
    pub editor: Rect,
    pub bar: Option<Rect>,
    pub status: Rect,
    pub selection: Option<Rect>,
    pub caret_rows: Vec<Rect>,
    pub popup: Option<Rect>,
    pub modal: Option<Rect>,
    pub picker_row: Option<Rect>,
    pub top: usize,
}

pub struct Motion {
    effects: EffectManager<Slot>,
    enabled: bool,
    last_frame: Instant,
    shown: Option<Buffer>,
    grounded: Option<Arc<AtomicBool>>,
}

impl Motion {
    pub fn new(enabled: bool) -> Self {
        Self {
            effects: EffectManager::default(),
            enabled,
            last_frame: Instant::now(),
            shown: None,
            grounded: None,
        }
    }

    pub fn from_env() -> Self {
        Self::new(std::env::var_os("EOE_NO_FX").is_none())
    }

    pub fn is_animating(&self) -> bool {
        self.effects.is_running()
    }

    pub fn cue(&mut self, cue: Cue, areas: &Areas) {
        if !self.enabled {
            return;
        }
        let editor = areas.editor;
        match cue {
            Cue::TabNext | Cue::TabPrev => {
                let dir = if cue == Cue::TabNext { Dir::RightToLeft } else { Dir::LeftToRight };
                let sweep = fx::fade_from_fg(theme::pal().hot, (320, Interpolation::QuadOut))
                    .with_pattern(SweepPattern::new(dir, 16));
                self.add(Slot::Editor, sweep.with_area(editor));
            }
            Cue::Opened => {
                let decode = fx::parallel(&[
                    fx::coalesce((380, Interpolation::QuadOut)),
                    fx::fade_from_fg(theme::pal().ice, (520, Interpolation::QuadOut)),
                ]);
                self.add(Slot::Editor, decode.with_area(editor));
            }
            Cue::FindOpened => {
                if let Some(bar) = areas.bar {
                    let sweep = fx::sweep_in(Dir::LeftToRight, 10, 0, theme::pal().void, (200, Interpolation::QuadOut));
                    self.add(Slot::Bar, sweep.with_area(bar));
                }
            }
            Cue::Matched => {
                if let Some(sel) = areas.selection {
                    let glow = fx::fade_from(theme::pal().void, theme::pal().hot, (420, Interpolation::QuadOut));
                    self.add(Slot::Match, glow.with_area(sel));
                }
            }
            Cue::Moved(leaving) => {
                let grounded = Arc::new(AtomicBool::new(false));
                let flight = self.shown.as_ref().filter(|s| s.area == areas.screen).and_then(|s| depart(&leaving, s, editor, areas.top, grounded.clone()));
                if let Some(flight) = flight {
                    self.interrupt();
                    self.grounded = Some(grounded);
                    self.add(Slot::Depart, flight.with_area(editor));
                }
            }
            Cue::Saved => {
                let beam = fx::sweep_in(Dir::LeftToRight, 24, 0, theme::pal().ice, (480, Interpolation::QuadOut));
                self.add(Slot::Status, beam.with_area(areas.status));
            }
            Cue::Sent => {
                let beam = fx::sweep_in(Dir::LeftToRight, 24, 0, theme::pal().ice, (480, Interpolation::QuadOut));
                self.add(Slot::Status, beam.with_area(areas.status));
                if !areas.caret_rows.is_empty() {
                    let packets: Vec<Effect> = areas.caret_rows.iter().map(|&row| transmit().with_area(row)).collect();
                    self.add(Slot::Sent, fx::parallel(&packets));
                }
            }
            Cue::UpdateOffered => {
                if let Some(modal) = areas.modal {
                    self.add(Slot::Modal, decrypt(620).with_area(modal));
                }
            }
            Cue::PickerOpened => {
                if let Some(popup) = areas.popup {
                    self.add(Slot::Popup, decrypt(420).with_area(popup));
                }
            }
            Cue::PickerMoved => {
                if let Some(row) = areas.picker_row {
                    self.add(Slot::Lock, lock_on().with_area(row));
                }
            }
            Cue::Warn => {
                let flash = fx::fade_from_fg(theme::pal().amber, (600, Interpolation::QuadOut));
                self.add(Slot::Status, flash.with_area(areas.status));
            }
            Cue::Error => {
                let glitch = Glitch::builder()
                    .cell_glitch_ratio(0.15)
                    .action_start_delay_ms(0..120)
                    .action_ms(40..140)
                    .build()
                    .into_effect();
                let fail = fx::parallel(&[
                    fx::with_duration(Duration::from_millis(380), glitch),
                    fx::fade_from_fg(theme::pal().error, (600, Interpolation::QuadOut)),
                ]);
                self.add(Slot::Status, fail.with_area(areas.status));
            }
            Cue::Quit => {
                let fade = fx::parallel(&[
                    fx::dissolve((300, Interpolation::QuadIn)),
                    fx::fade_to_fg(theme::pal().void, (300, Interpolation::QuadIn)),
                ]);
                self.add(Slot::Screen, fade.with_area(areas.screen));
            }
        }
    }

    pub fn jack_in(&mut self, areas: &Areas) {
        if !self.enabled {
            return;
        }
        let jack_in = fx::parallel(&[
            fx::coalesce((360, Interpolation::QuadOut)),
            fx::sweep_in(Dir::UpToDown, 8, 4, theme::pal().ice, (480, Interpolation::QuadOut)),
        ]);
        self.add(Slot::Screen, jack_in.with_area(areas.screen));
    }

    pub fn interrupt(&mut self) {
        if let Some(grounded) = self.grounded.take() {
            grounded.store(true, Ordering::Relaxed);
            self.effects.cancel_unique_effect(Slot::Depart);
        }
    }

    pub fn render(&mut self, buf: &mut Buffer, area: Rect) {
        self.shown = Some(buf.clone());
        let now = Instant::now();
        let elapsed = now - self.last_frame;
        self.last_frame = now;
        self.advance(elapsed, buf, area);
    }

    fn advance(&mut self, elapsed: StdDuration, buf: &mut Buffer, area: Rect) {
        self.effects.process_effects(elapsed.into(), buf, area);
    }

    fn add(&mut self, slot: Slot, effect: Effect) {
        if !self.effects.is_running() {
            self.last_frame = Instant::now();
        }
        self.effects.add_unique_effect(slot, effect);
    }
}

fn mix(a: ratatui::style::Color, b: ratatui::style::Color, t: f32) -> ratatui::style::Color {
    use ratatui::style::Color::Rgb;
    match (a, b) {
        (Rgb(ar, ag, ab), Rgb(br, bg, bb)) => {
            let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
            Rgb(m(ar, br), m(ag, bg), m(ab, bb))
        }
        _ => b,
    }
}

const FLIGHT_MS: f32 = 680.0;
const CHARGE: f32 = 0.10;
const IMPLODE: f32 = 0.34;
const FIRE: f32 = 0.70;
const IMPACT: f32 = 0.80;
const CLOSE: f32 = 0.82;

#[derive(Clone)]
struct Band {
    at: usize,
    cells: Vec<Cell>,
    delay: f32,
}

#[derive(Clone)]
struct Flight {
    bands: Vec<Band>,
    gutter: usize,
    toward: Toward,
    total: f32,
    grounded: Arc<AtomicBool>,
}

fn depart(leaving: &Departure, shown: &Buffer, editor: Rect, top_after: usize, grounded: Arc<AtomicBool>) -> Option<Effect> {
    let height = editor.height as usize;
    let n = leaving.lines.len();
    let stagger = if n > 1 { 45f32.min(300.0 / (n - 1) as f32) } else { 0.0 };
    let bands: Vec<Band> = leaving
        .lines
        .iter()
        .enumerate()
        .filter_map(|(i, &line)| {
            let row = line.checked_sub(leaving.top).filter(|r| *r < height)?;
            let at = (line - i).checked_sub(top_after).filter(|a| *a <= height)?;
            let y = editor.y + row as u16;
            let cells = (editor.x..editor.right()).map(|x| shown.cell((x, y)).cloned().unwrap_or_default()).collect();
            Some(Band { at, cells, delay: i as f32 * stagger })
        })
        .collect();
    if bands.is_empty() {
        return None;
    }
    let total = FLIGHT_MS + stagger * (n - 1) as f32;
    let flight = Flight { bands, gutter: leaving.gutter, toward: leaving.toward, total, grounded };
    Some(fx::effect_fn_buf(flight, (total as u32, Interpolation::Linear), |flight, ctx, buf| fly(flight, ctx.alpha(), ctx.area, buf)))
}

enum Row {
    Kept(usize),
    Leaving(usize, f32),
}

fn fly(flight: &Flight, alpha: f32, area: Rect, buf: &mut Buffer) {
    if flight.grounded.load(Ordering::Relaxed) {
        return;
    }
    let (w, h) = (area.width as usize, area.height as usize);
    let now = alpha * flight.total;
    let kept: Vec<Vec<Cell>> = (0..h)
        .map(|r| (0..w).map(|c| buf.cell((area.x + c as u16, area.y + r as u16)).cloned().unwrap_or_default()).collect())
        .collect();
    let mut rows = Vec::with_capacity(h + flight.bands.len());
    let mut next = 0;
    for (i, band) in flight.bands.iter().enumerate() {
        while next < band.at.min(h) {
            rows.push(Row::Kept(next));
            next += 1;
        }
        let t = ((now - band.delay) / FLIGHT_MS).clamp(0.0, 1.0);
        if t < CLOSE {
            rows.push(Row::Leaving(i, t));
        }
    }
    rows.extend((next..h).map(Row::Kept));
    rows.truncate(h);
    let vertical = matches!(flight.toward, Toward::Up | Toward::Down);
    let mut beams = Vec::new();
    for (r, row) in rows.iter().enumerate() {
        let y = area.y + r as u16;
        match *row {
            Row::Kept(k) => {
                for (c, cell) in kept[k].iter().enumerate() {
                    if let Some(dst) = buf.cell_mut((area.x + c as u16, y)) {
                        *dst = cell.clone();
                    }
                }
            }
            Row::Leaving(i, t) => {
                leave(buf, area, y, &flight.bands[i], flight.gutter, flight.toward, t);
                if vertical {
                    beams.push((y, i, t));
                }
            }
        }
    }
    for (y, i, t) in beams {
        climb(buf, area, y, &flight.bands[i], flight.gutter, flight.toward, t);
    }
}

fn put(buf: &mut Buffer, x: u16, y: u16, symbol: &str, style: Style) {
    if let Some(cell) = buf.cell_mut((x, y)) {
        cell.reset();
        cell.set_symbol(symbol).set_style(style);
    }
}

fn letters(band: &Band, gutter: usize) -> Vec<(usize, &Cell)> {
    band.cells.iter().enumerate().skip(gutter).filter(|(_, c)| c.symbol() != " ").collect()
}

fn focal(band: &Band, gutter: usize, toward: Toward) -> usize {
    let text = letters(band, gutter);
    match (text.first(), text.last()) {
        (Some(&(first, _)), Some(&(last, _))) => match toward {
            Toward::Right => first,
            Toward::Left => last,
            Toward::Up | Toward::Down => (first + last) / 2,
        },
        _ => gutter.min(band.cells.len().saturating_sub(1)),
    }
}

fn packet(absorbed: f32) -> &'static str {
    match absorbed {
        a if a < 0.25 => "∙",
        a if a < 0.5 => "•",
        a if a < 0.8 => "●",
        _ => "◉",
    }
}

fn beam(distance: f32, horizontal: bool) -> Option<(&'static str, Style)> {
    let p = theme::pal();
    let (near, mid, far) = if horizontal { ("━", "─", "╌") } else { ("┃", "│", "╎") };
    match distance {
        d if d < 2.5 => Some((near, Style::default().fg(p.ice).add_modifier(Modifier::BOLD))),
        d if d < 9.0 => Some((mid, Style::default().fg(p.ice))),
        d if d < 18.0 => Some((far, Style::default().fg(p.ghost))),
        _ => None,
    }
}

fn flash(t: f32) -> Color {
    if t < (FIRE + IMPACT) / 2.0 { theme::pal().hot } else { theme::pal().ice }
}

fn leave(buf: &mut Buffer, area: Rect, y: u16, band: &Band, gutter: usize, toward: Toward, t: f32) {
    let p = theme::pal();
    let w = area.width as usize;
    for x in area.left()..area.right() {
        put(buf, x, y, " ", Style::default());
    }
    if t < CHARGE {
        let k = t / CHARGE;
        for (c, cell) in band.cells.iter().enumerate() {
            if let Some(dst) = buf.cell_mut((area.x + c as u16, y)) {
                *dst = cell.clone();
                let fg = if c < gutter { p.hot } else { mix(if cell.fg == Color::Reset { p.text } else { cell.fg }, p.ice, k) };
                dst.set_fg(fg);
            }
        }
        return;
    }
    let text = letters(band, gutter);
    let aim = focal(band, gutter, toward);
    let hot = Style::default().fg(p.hot).add_modifier(Modifier::BOLD);
    if t < IMPLODE {
        let e = ((t - CHARGE) / (IMPLODE - CHARGE)).powi(2);
        let mut absorbed = 0;
        for &(c, cell) in &text {
            let at = c as f32 + (aim as f32 - c as f32) * e;
            if (at - aim as f32).abs() < 1.0 {
                absorbed += 1;
            } else {
                put(buf, area.x + at.round() as u16, y, cell.symbol(), Style::default().fg(p.ice));
            }
        }
        put(buf, area.x + aim as u16, y, packet(absorbed as f32 / text.len().max(1) as f32), hot);
        return;
    }
    if matches!(toward, Toward::Up | Toward::Down) {
        return;
    }
    let exit = if toward == Toward::Right { w as f32 } else { -1.0 };
    if t < FIRE {
        let e = ((t - IMPLODE) / (FIRE - IMPLODE)).powf(1.8);
        let at = aim as f32 + (exit - aim as f32) * e;
        let (from, to) = if toward == Toward::Right { (aim as f32, at) } else { (at, aim as f32) };
        for c in (from.ceil().max(0.0) as usize)..=(to.floor().min(w as f32 - 1.0).max(0.0) as usize) {
            if let Some((glyph, style)) = beam((c as f32 - at).abs(), true) {
                put(buf, area.x + c as u16, y, glyph, style);
            }
        }
        if at >= 0.0 && at < w as f32 {
            put(buf, area.x + at.round().min(w as f32 - 1.0) as u16, y, "◉", hot);
        }
    } else if t < IMPACT {
        let edge = if toward == Toward::Right { (area.right() - 1, "▐") } else { (area.left(), "▌") };
        put(buf, edge.0, y, edge.1, Style::default().fg(flash(t)).add_modifier(Modifier::BOLD));
    }
}

fn climb(buf: &mut Buffer, area: Rect, y: u16, band: &Band, gutter: usize, toward: Toward, t: f32) {
    if !(IMPLODE..IMPACT).contains(&t) {
        return;
    }
    let p = theme::pal();
    let x = area.x + focal(band, gutter, toward) as u16;
    let (start, exit) = (y as f32, if toward == Toward::Up { area.top() as f32 - 1.0 } else { area.bottom() as f32 });
    if t < FIRE {
        let e = ((t - IMPLODE) / (FIRE - IMPLODE)).powf(1.8);
        let at = start + (exit - start) * e;
        let (from, to) = if toward == Toward::Down { (start, at) } else { (at, start) };
        for r in (from.ceil().max(area.top() as f32) as u16)..=(to.floor().min(area.bottom() as f32 - 1.0) as u16) {
            if let Some((glyph, style)) = beam((r as f32 - at).abs(), false) {
                put(buf, x, r, glyph, style);
            }
        }
        if at >= area.top() as f32 && at < area.bottom() as f32 {
            put(buf, x, at.round() as u16, "◉", Style::default().fg(p.hot).add_modifier(Modifier::BOLD));
        }
    } else {
        let (row, glyph) = if toward == Toward::Up { (area.top(), "▀") } else { (area.bottom() - 1, "▄") };
        put(buf, x, row, glyph, Style::default().fg(flash(t)).add_modifier(Modifier::BOLD));
    }
}

fn scramble(x: u16, y: u16) -> u32 {
    (x as u32).wrapping_mul(73_856_093) ^ (y as u32).wrapping_mul(19_349_663)
}

const NOISE: [&str; 22] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "a", "b", "c", "d", "e", "f", "<", ">", "/", "#", "%", "$"];

fn decrypt(ms: u32) -> Effect {
    fx::effect_fn_buf((), (ms, Interpolation::Linear), |_, ctx, buf| {
        let area = ctx.area;
        let alpha = ctx.alpha();
        let tick = (alpha * 30.0) as u32;
        let (w, h) = (area.width.max(1) as f32, area.height.max(1) as f32);
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                let Some(cell) = buf.cell_mut((x, y)) else { continue };
                let edge = x == area.left() || x + 1 == area.right() || y == area.top() || y + 1 == area.bottom();
                if edge {
                    let dx = (x - area.left()).min(area.right() - 1 - x) as f32 / w;
                    let dy = (y - area.top()).min(area.bottom() - 1 - y) as f32 / h;
                    if alpha < (dx + dy) * 1.4 {
                        cell.set_symbol(" ");
                    }
                    continue;
                }
                if cell.symbol() == " " {
                    continue;
                }
                let seed = scramble(x, y);
                let reveal = 0.25 + ((x - area.left()) as f32 / w) * 0.5 + (seed % 100) as f32 / 100.0 * 0.2;
                if alpha < reveal {
                    let n = seed.wrapping_add(tick.wrapping_mul(2_654_435_761)) as usize;
                    cell.set_symbol(NOISE[n % NOISE.len()]);
                    cell.set_fg(if n % 5 == 0 { theme::pal().hot } else { theme::pal().ice });
                } else if alpha < reveal + 0.08 {
                    cell.set_fg(theme::pal().ice);
                }
            }
        }
    })
}

fn lock_on() -> Effect {
    fx::sweep_in(Dir::LeftToRight, 12, 0, theme::pal().ice, (200, Interpolation::QuadOut))
}

fn transmit() -> Effect {
    const TRAIL: f32 = 5.0;
    const SETTLE: f32 = 12.0;
    const BLOCKS: [&str; 3] = ["▓", "▒", "░"];
    fx::effect_fn_buf((), (700, Interpolation::SineInOut), |_, ctx, buf| {
        let area = ctx.area;
        let head = ctx.alpha() * (area.width as f32 + TRAIL + SETTLE);
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                let behind = head - (x - area.left()) as f32;
                if behind < 0.0 {
                    continue;
                }
                let Some(cell) = buf.cell_mut((x, y)) else { continue };
                if behind < 1.0 {
                    cell.set_fg(theme::pal().void).set_bg(theme::pal().hot);
                } else if behind < TRAIL {
                    if cell.symbol() != " " {
                        let i = (((behind - 1.0) / (TRAIL - 1.0)) * BLOCKS.len() as f32) as usize;
                        cell.set_symbol(BLOCKS[i.min(BLOCKS.len() - 1)]);
                    }
                    cell.set_fg(theme::pal().ice);
                } else {
                    let settled = ((behind - TRAIL) / SETTLE).min(1.0);
                    let target = match cell.fg {
                        ratatui::style::Color::Rgb(..) => cell.fg,
                        _ => theme::pal().text,
                    };
                    cell.set_fg(mix(theme::pal().ice, target, settled));
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn areas() -> Areas {
        Areas {
            screen: Rect::new(0, 0, 40, 10),
            editor: Rect::new(0, 1, 40, 7),
            bar: Some(Rect::new(0, 8, 40, 1)),
            status: Rect::new(0, 9, 40, 1),
            selection: Some(Rect::new(4, 2, 3, 1)),
            caret_rows: vec![Rect::new(2, 2, 20, 1), Rect::new(2, 3, 20, 1)],
            popup: Some(Rect::new(4, 1, 30, 6)),
            modal: Some(Rect::new(2, 1, 36, 8)),
            picker_row: Some(Rect::new(5, 4, 28, 1)),
            top: 0,
        }
    }

    fn all() -> Vec<Cue> {
        vec![
            Cue::TabNext,
            Cue::TabPrev,
            Cue::Opened,
            Cue::FindOpened,
            Cue::Matched,
            Cue::Saved,
            Cue::Sent,
            Cue::Moved(Departure { lines: vec![1], top: 0, gutter: 2, toward: Toward::Right }),
            Cue::Warn,
            Cue::Error,
            Cue::Quit,
            Cue::PickerOpened,
            Cue::PickerMoved,
            Cue::UpdateOffered,
        ]
    }

    #[test]
    fn every_effect_finishes_so_the_loop_can_go_idle() {
        let a = areas();
        let mut buf = Buffer::empty(a.screen);
        for cue in all() {
            let mut m = Motion::new(true);
            m.render(&mut buf.clone(), a.screen);
            m.cue(cue.clone(), &a);
            assert!(m.is_animating(), "{:?} starts an effect", cue);
            for _ in 0..200 {
                m.advance(StdDuration::from_millis(16), &mut buf, a.screen);
            }
            assert!(!m.is_animating(), "{:?} must end", cue);
        }
        let mut m = Motion::new(true);
        m.jack_in(&a);
        assert!(m.is_animating(), "the jack-in starts");
        for _ in 0..200 {
            m.advance(StdDuration::from_millis(16), &mut buf, a.screen);
        }
        assert!(!m.is_animating(), "the jack-in must end");
    }

    #[test]
    fn sending_lines_leaves_the_text_exactly_as_it_found_it() {
        let a = areas();
        let mut base = Buffer::empty(a.screen);
        base.set_string(2, 2, "echo one && sleep 10", ratatui::style::Style::default().fg(theme::pal().text));
        base.set_string(2, 3, "echo two", ratatui::style::Style::default().fg(theme::pal().text));
        let mut m = Motion::new(true);
        m.cue(Cue::Sent, &a);
        let mut frame = base.clone();
        let mut mid_frame_changed = false;
        while m.is_animating() {
            frame = base.clone();
            m.advance(StdDuration::from_millis(16), &mut frame, a.screen);
            mid_frame_changed |= frame != base;
        }
        assert!(mid_frame_changed, "the lines visibly animate");
        let row = |buf: &Buffer, y: u16| (2..22).map(|x| buf.cell((x, y)).unwrap().symbol().to_string()).collect::<String>();
        assert_eq!(row(&frame, 2), row(&base, 2), "no data blocks left behind");
        for x in 2..22 {
            assert_eq!(frame.cell((x, 2)).unwrap().fg, base.cell((x, 2)).unwrap().fg, "colour settles at x={}", x);
        }
    }

    #[test]
    fn the_decrypt_reveal_scrambles_then_ends_on_the_real_text() {
        let a = areas();
        let popup = a.popup.unwrap();
        let mut base = Buffer::empty(a.screen);
        base.set_string(popup.x, popup.y, format!("┏{}┓", "━".repeat(popup.width as usize - 2)), ratatui::style::Style::default());
        base.set_string(popup.x + 2, popup.y + 2, "Send lines to herdr", ratatui::style::Style::default().fg(theme::pal().text));
        let mut m = Motion::new(true);
        m.cue(Cue::PickerOpened, &a);
        let mut frame = base.clone();
        m.advance(StdDuration::from_millis(40), &mut frame, a.screen);
        let text = |buf: &Buffer| (0..19).map(|i| buf.cell((popup.x + 2 + i, popup.y + 2)).unwrap().symbol().to_string()).collect::<String>();
        assert_ne!(text(&frame), "Send lines to herdr", "starts as noise");
        let top = |buf: &Buffer, x: u16| buf.cell((x, popup.y)).unwrap().symbol().to_string();
        assert_eq!((top(&frame, popup.x), top(&frame, popup.x + popup.width / 2)), ("┏".to_string(), " ".to_string()), "the frame draws out from its corners");
        while m.is_animating() {
            frame = base.clone();
            m.advance(StdDuration::from_millis(16), &mut frame, a.screen);
        }
        assert_eq!(text(&frame), "Send lines to herdr");
        assert_eq!(frame, base, "nothing is left behind");
    }

    fn screen_with(rows: &[&str]) -> Buffer {
        let mut buf = Buffer::empty(Rect::new(0, 0, 30, 6));
        for (i, row) in rows.iter().enumerate() {
            buf.set_string(0, 1 + i as u16, row, ratatui::style::Style::default().fg(theme::pal().text));
        }
        buf
    }

    fn fly_areas() -> Areas {
        Areas {
            screen: Rect::new(0, 0, 30, 6),
            editor: Rect::new(0, 1, 30, 4),
            bar: None,
            status: Rect::new(0, 5, 30, 1),
            selection: None,
            caret_rows: vec![],
            popup: None,
            modal: None,
            picker_row: None,
            top: 0,
        }
    }

    fn row(buf: &Buffer, y: u16) -> String {
        (0..buf.area.width).map(|x| buf.cell((x, y)).unwrap().symbol().to_string()).collect::<String>().trim_end().to_string()
    }

    fn packet_at(buf: &Buffer) -> Option<(u16, u16)> {
        (0..buf.area.height).find_map(|y| (0..buf.area.width).find(|&x| buf.cell((x, y)).unwrap().symbol() == "◉").map(|x| (x, y)))
    }

    fn launch(shown: &Buffer, lines: Vec<usize>, toward: Toward) -> Motion {
        let mut m = Motion::new(true);
        m.render(&mut shown.clone(), shown.area);
        m.cue(Cue::Moved(Departure { lines, top: 0, gutter: 2, toward }), &fly_areas());
        m
    }

    fn step(m: &mut Motion, new: &Buffer, ms: u64) -> Buffer {
        let mut frame = new.clone();
        m.advance(StdDuration::from_millis(ms), &mut frame, new.area);
        frame
    }

    fn settle(m: &mut Motion, new: &Buffer) -> Buffer {
        let mut frame = new.clone();
        while m.is_animating() {
            frame = step(m, new, 16);
        }
        frame
    }

    #[test]
    fn a_moved_line_is_thrown_toward_its_pane_and_the_gap_closes_behind_it() {
        let shown = screen_with(&["1 alpha", "2 bravo", "3 charlie", "4 delta"]);
        let new = screen_with(&["1 alpha", "2 charlie", "3 delta"]);
        let mut m = launch(&shown, vec![1], Toward::Right);
        let f = step(&mut m, &new, 30);
        assert_eq!((1..5).map(|y| row(&f, y)).collect::<Vec<_>>(), ["1 alpha", "2 bravo", "2 charlie", "3 delta"], "it charges in its old slot while the lines below wait");
        let f = step(&mut m, &new, 300);
        let (first, y) = packet_at(&f).expect("the line has become a packet");
        assert_eq!(y, 2, "flying through the slot it left");
        assert!(!row(&f, 2).contains("bravo") && row(&f, 2).contains('━'), "with a beam behind it: {:?}", row(&f, 2));
        assert_eq!(row(&f, 3), "2 charlie", "the gap stays open during the flight");
        let (later, _) = packet_at(&step(&mut m, &new, 60)).expect("still flying");
        assert!(later > first, "toward the pane on the right");
        assert_eq!(settle(&mut m, &new), new, "it ends on exactly the new frame");
    }

    #[test]
    fn a_pane_on_the_left_gets_the_line_thrown_left() {
        let shown = screen_with(&["1 alpha", "2 bravo", "3 charlie"]);
        let new = screen_with(&["1 alpha", "2 charlie"]);
        let mut m = launch(&shown, vec![1], Toward::Left);
        let (first, _) = packet_at(&step(&mut m, &new, 330)).expect("a packet");
        let (later, _) = packet_at(&step(&mut m, &new, 60)).expect("still flying");
        assert!(later < first, "leftward");
        let f = step(&mut m, &new, 110);
        assert_eq!(f.cell((0, 2)).unwrap().symbol(), "▌", "it flashes where it leaves through the left edge");
        assert_eq!(settle(&mut m, &new), new);
    }

    #[test]
    fn several_lines_leave_in_a_volley_and_the_gaps_zip_shut_in_order() {
        let shown = screen_with(&["1 a", "2 b", "3 c", "4 d"]);
        let new = screen_with(&["1 b", "2 d"]);
        let mut m = launch(&shown, vec![0, 2], Toward::Right);
        let f = step(&mut m, &new, 30);
        assert_eq!((1..5).map(|y| row(&f, y)).collect::<Vec<_>>(), ["1 a", "1 b", "3 c", "2 d"], "both slots are still there");
        let f = step(&mut m, &new, 545);
        assert_eq!((row(&f, 1), row(&f, 3)), ("1 b".to_string(), "2 d".to_string()), "the first gap has closed, the second is still open");
        assert!(!row(&f, 2).contains('c'), "the second line is on its way out");
        assert_eq!(settle(&mut m, &new), new);
    }

    #[test]
    fn a_pane_above_gets_the_packet_flying_up_over_the_lines() {
        let shown = screen_with(&["1 alpha", "2 bravo", "3 charlie", "4 delta"]);
        let new = screen_with(&["1 alpha", "2 bravo", "3 delta"]);
        let mut m = launch(&shown, vec![2], Toward::Up);
        let (x, y) = packet_at(&step(&mut m, &new, 340)).expect("a packet");
        assert!(y < 3, "above the slot the line left (row 3), at row {}", y);
        assert_eq!(x, 5, "rising from the middle of the line");
        assert_eq!(settle(&mut m, &new), new);
    }

    #[test]
    fn any_input_ends_a_flight_at_once() {
        let shown = screen_with(&["1 alpha", "2 bravo"]);
        let new = screen_with(&["1 alpha"]);
        let mut m = launch(&shown, vec![1], Toward::Right);
        step(&mut m, &new, 100);
        m.interrupt();
        let f = step(&mut m, &new, 16);
        assert!(!m.is_animating(), "stops within a frame");
        assert_eq!(f, new);
    }

    #[test]
    fn nothing_flies_without_a_frame_to_take_it_from() {
        let new = screen_with(&["1 alpha"]);
        let leaving = Departure { lines: vec![1], top: 0, gutter: 2, toward: Toward::Right };
        let mut m = Motion::new(true);
        m.cue(Cue::Moved(leaving.clone()), &fly_areas());
        assert!(!m.is_animating(), "no frame has been shown yet");
        m.render(&mut Buffer::empty(Rect::new(0, 0, 50, 9)), Rect::new(0, 0, 50, 9));
        m.cue(Cue::Moved(leaving.clone()), &fly_areas());
        assert!(!m.is_animating(), "the terminal was resized since");
        m.render(&mut new.clone(), new.area);
        m.cue(Cue::Moved(Departure { lines: vec![1], top: 5, ..leaving }), &fly_areas());
        assert!(!m.is_animating(), "the line was above the view");
    }

    #[test]
    fn disabled_motion_never_animates() {
        let a = areas();
        let mut m = Motion::new(false);
        m.render(&mut Buffer::empty(a.screen), a.screen);
        m.jack_in(&a);
        for cue in all() {
            m.cue(cue, &a);
        }
        assert!(!m.is_animating());
    }
}
