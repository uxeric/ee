use std::time::{Duration as StdDuration, Instant};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use tachyonfx::fx::{self, Glitch};
use tachyonfx::pattern::SweepPattern;
use tachyonfx::{Duration, Effect, EffectManager, Interpolation, IntoEffect, Motion as Dir};

use crate::state::Cue;
use crate::theme;

const SPLASH: StdDuration = StdDuration::from_millis(900);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
enum Slot {
    #[default]
    Screen,
    Editor,
    Bar,
    Status,
    Match,
}

pub struct Areas {
    pub screen: Rect,
    pub editor: Rect,
    pub bar: Option<Rect>,
    pub status: Rect,
    pub selection: Option<Rect>,
}

pub struct Motion {
    effects: EffectManager<Slot>,
    enabled: bool,
    last_frame: Instant,
    splash_until: Option<Instant>,
}

impl Motion {
    pub fn splash_progress(&self) -> Option<f32> {
        let until = self.splash_until?;
        let left = until.saturating_duration_since(Instant::now()).as_secs_f32();
        Some(1.0 - left / SPLASH.as_secs_f32())
    }

    pub fn new(enabled: bool) -> Self {
        Self {
            effects: EffectManager::default(),
            enabled,
            last_frame: Instant::now(),
            splash_until: None,
        }
    }

    pub fn from_env() -> Self {
        Self::new(std::env::var_os("EOE_NO_FX").is_none())
    }

    pub fn start_splash(&mut self, areas: &Areas) {
        if !self.enabled {
            return;
        }
        self.splash_until = Some(Instant::now() + SPLASH);
        let decode = fx::sequence(&[
            fx::parallel(&[
                fx::coalesce((380, Interpolation::QuadOut)),
                fx::fade_from_fg(theme::VOID, (380, Interpolation::QuadOut)),
            ]),
            fx::sleep(220),
            fx::dissolve((300, Interpolation::QuadIn)),
        ]);
        self.add(Slot::Screen, decode.with_area(areas.screen));
    }

    pub fn end_splash(&mut self, areas: &Areas) {
        if self.splash_until.take().is_some() {
            let jack_in = fx::parallel(&[
                fx::coalesce((420, Interpolation::QuadOut)),
                fx::sweep_in(Dir::UpToDown, 8, 4, theme::ICE, (600, Interpolation::QuadOut)),
            ]);
            self.add(Slot::Screen, jack_in.with_area(areas.screen));
        }
    }

    pub fn tick(&mut self, areas: &Areas) {
        if self.splash_until.is_some_and(|t| Instant::now() >= t) {
            self.end_splash(areas);
        }
    }

    pub fn is_animating(&self) -> bool {
        self.splash_until.is_some() || self.effects.is_running()
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
            Cue::Saved | Cue::Sent => {
                let beam = fx::sweep_in(Dir::LeftToRight, 24, 0, theme::ICE, (480, Interpolation::QuadOut));
                self.add(Slot::Status, beam.with_area(areas.status));
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
        }
    }

    const ALL: [Cue; 10] = [
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
        m.start_splash(&a);
        m.end_splash(&a);
        for _ in 0..200 {
            m.advance(StdDuration::from_millis(16), &mut buf, a.screen);
        }
        assert!(!m.is_animating(), "splash and boot must end");
    }

    #[test]
    fn disabled_motion_never_animates() {
        let a = areas();
        let mut m = Motion::new(false);
        m.start_splash(&a);
        for cue in ALL {
            m.cue(cue, &a);
        }
        assert!(!m.is_animating());
    }
}
