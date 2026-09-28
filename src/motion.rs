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
                let sweep = fx::fade_from_fg(theme::HOT, (320, Interpolation::QuadOut))
                    .with_pattern(SweepPattern::new(dir, 16));
                self.add(Slot::Editor, sweep.with_area(editor));
            }
            Cue::Opened => {
                let decode = fx::parallel(&[
                    fx::coalesce((380, Interpolation::QuadOut)),
                    fx::fade_from_fg(theme::ICE, (520, Interpolation::QuadOut)),
                ]);
                self.add(Slot::Editor, decode.with_area(editor));
            }
            Cue::FindOpened => {
                if let Some(bar) = areas.bar {
                    let sweep = fx::sweep_in(Dir::LeftToRight, 10, 0, theme::VOID, (200, Interpolation::QuadOut));
                    self.add(Slot::Bar, sweep.with_area(bar));
                }
            }
            Cue::Matched => {
                if let Some(sel) = areas.selection {
                    let glow = fx::fade_from(theme::VOID, theme::HOT, (420, Interpolation::QuadOut));
                    self.add(Slot::Match, glow.with_area(sel));
                }
            }
            Cue::Saved => {
                let beam = fx::sweep_in(Dir::LeftToRight, 24, 0, theme::ICE, (480, Interpolation::QuadOut));
                self.add(Slot::Status, beam.with_area(areas.status));
            }
            Cue::Sent => {
                let beam = fx::sweep_in(Dir::LeftToRight, 24, 0, theme::ICE, (480, Interpolation::QuadOut));
                self.add(Slot::Status, beam.with_area(areas.status));
                if !areas.caret_rows.is_empty() {
                    let packets: Vec<Effect> = areas.caret_rows.iter().map(|&row| transmit().with_area(row)).collect();
                    self.add(Slot::Sent, fx::parallel(&packets));
                }
            }
            Cue::UpdateOffered => {
                if let Some(modal) = areas.modal {
                    let open = fx::parallel(&[
                        fx::coalesce((520, Interpolation::QuadOut)),
                        fx::sweep_in(Dir::UpToDown, 6, 3, theme::ICE, (620, Interpolation::QuadOut)),
                    ]);
                    self.add(Slot::Modal, open.with_area(modal));
                }
            }
            Cue::PickerOpened => {
                if let Some(popup) = areas.popup {
                    let open = fx::parallel(&[
                        fx::coalesce((220, Interpolation::QuadOut)),
                        fx::fade_from_fg(theme::ICE, (320, Interpolation::QuadOut)),
                    ]);
                    self.add(Slot::Popup, open.with_area(popup));
                }
            }
            Cue::Warn => {
                let flash = fx::fade_from_fg(theme::AMBER, (600, Interpolation::QuadOut));
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
                    fx::fade_from_fg(theme::ERROR, (600, Interpolation::QuadOut)),
                ]);
                self.add(Slot::Status, fail.with_area(areas.status));
            }
            Cue::Quit => {
                let fade = fx::parallel(&[
                    fx::dissolve((300, Interpolation::QuadIn)),
                    fx::fade_to_fg(theme::VOID, (300, Interpolation::QuadIn)),
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
            fx::sweep_in(Dir::UpToDown, 8, 4, theme::ICE, (480, Interpolation::QuadOut)),
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
                    cell.set_fg(theme::VOID).set_bg(theme::HOT);
                } else if behind < TRAIL {
                    if cell.symbol() != " " {
                        let i = (((behind - 1.0) / (TRAIL - 1.0)) * BLOCKS.len() as f32) as usize;
                        cell.set_symbol(BLOCKS[i.min(BLOCKS.len() - 1)]);
                    }
                    cell.set_fg(theme::ICE);
                } else {
                    let settled = ((behind - TRAIL) / SETTLE).min(1.0);
                    let target = match cell.fg {
                        ratatui::style::Color::Rgb(..) => cell.fg,
                        _ => theme::TEXT,
                    };
                    cell.set_fg(mix(theme::ICE, target, settled));
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
        }
    }

    const ALL: [Cue; 12] = [
        Cue::TabNext,
        Cue::TabPrev,
        Cue::Opened,
        Cue::FindOpened,
        Cue::Matched,
        Cue::Saved,
        Cue::Sent,
        Cue::Warn,
        Cue::Error,
        Cue::Quit,
        Cue::PickerOpened,
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
        base.set_string(2, 2, "echo one && sleep 10", ratatui::style::Style::default().fg(theme::TEXT));
        base.set_string(2, 3, "echo two", ratatui::style::Style::default().fg(theme::TEXT));
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
