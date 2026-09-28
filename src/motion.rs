use std::time::{Duration as StdDuration, Instant};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use tachyonfx::fx::{self, Glitch};
use tachyonfx::pattern::SweepPattern;
use tachyonfx::{Duration, Effect, EffectManager, Interpolation, IntoEffect, Motion as Dir};

use crate::state::Cue;
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
}

pub struct Motion {
    effects: EffectManager<Slot>,
    enabled: bool,
    last_frame: Instant,
}

impl Motion {
    pub fn new(enabled: bool) -> Self {
        Self {
            effects: EffectManager::default(),
            enabled,
            last_frame: Instant::now(),
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
            Cue::Saved | Cue::Moved => {
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

    pub fn render(&mut self, buf: &mut Buffer, area: Rect) {
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
        }
    }

    const ALL: [Cue; 14] = [
        Cue::TabNext,
        Cue::TabPrev,
        Cue::Opened,
        Cue::FindOpened,
        Cue::Matched,
        Cue::Saved,
        Cue::Sent,
        Cue::Moved,
        Cue::Warn,
        Cue::Error,
        Cue::Quit,
        Cue::PickerOpened,
        Cue::PickerMoved,
        Cue::UpdateOffered,
    ];

    #[test]
    fn every_effect_finishes_so_the_loop_can_go_idle() {
        let a = areas();
        let mut buf = Buffer::empty(a.screen);
        for cue in ALL {
            let mut m = Motion::new(true);
            m.cue(cue, &a);
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

    #[test]
    fn disabled_motion_never_animates() {
        let a = areas();
        let mut m = Motion::new(false);
        m.jack_in(&a);
        for cue in ALL {
            m.cue(cue, &a);
        }
        assert!(!m.is_animating());
    }
}
