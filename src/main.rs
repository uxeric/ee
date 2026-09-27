mod clipboard;
mod config;
mod document;
mod find;
mod herdr;
mod keys;
mod markdown;
mod motion;
mod state;
mod theme;
mod ui;

use std::env;
use std::io::{stdout, Stdout};
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{
    poll, read, DisableBracketedPaste, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture, Event, KeyEventKind, KeyModifiers, MouseButton,
    MouseEventKind,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, supports_keyboard_enhancement, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;

use document::Document;
use keys::{Action, DoubleShift, Keymap};
use motion::Motion;
use state::{Cue, EditorState};

fn main() -> std::io::Result<()> {
    // Load documents from any file path arguments (non-flag args).
    let args: Vec<String> = env::args().collect();
    let file_args: Vec<&str> = args[1..]
        .iter()
        .map(|a| a.as_str())
        .filter(|a| !a.starts_with('-'))
        .collect();

    let mut state = EditorState::new();
    if !file_args.is_empty() {
        let mut tabs: Vec<Document> = Vec::with_capacity(file_args.len());
        let mut new_file = None;
        for path in &file_args {
            match Document::open_or_new(path) {
                Ok((doc, is_new)) => {
                    if is_new && new_file.is_none() {
                        new_file = Some(doc.name.clone());
                    }
                    tabs.push(doc);
                }
                Err(e) => {
                    eprintln!("ee: cannot open {}: {}", path, state::plain(&e));
                    std::process::exit(1);
                }
            }
        }
        state.tabs = tabs;
        state.active = 0;
        if let Some(name) = new_file {
            state.note_new_file(&name);
        }
    }

    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    execute!(stdout(), EnterAlternateScreen, EnableMouseCapture, EnableBracketedPaste)?;
    enable_raw_mode()?;
    let enhanced =
        env::var_os("EOE_LEGACY_KEYS").is_none() && supports_keyboard_enhancement().unwrap_or(false);
    if enhanced {
        execute!(
            stdout(),
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                    | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
                    | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
            )
        )?;
    }

    let result = run(&mut terminal, &mut state);

    if enhanced {
        execute!(stdout(), PopKeyboardEnhancementFlags)?;
    }
    disable_raw_mode()?;
    execute!(stdout(), DisableBracketedPaste, DisableMouseCapture, LeaveAlternateScreen)?;
    result
}

fn screen(terminal: &Terminal<CrosstermBackend<Stdout>>) -> std::io::Result<Rect> {
    let size = terminal.size()?;
    Ok(Rect::new(0, 0, size.width, size.height))
}

fn run(terminal: &mut Terminal<CrosstermBackend<Stdout>>, state: &mut EditorState) -> std::io::Result<()> {
    let (config, warning) = config::load();
    let keymap = Keymap::new(config);
    if let Some(warning) = warning {
        state.warn(warning);
    }
    let mut double_shift = DoubleShift::default();
    let mut motion = Motion::from_env();
    motion.start_splash(&ui::areas(screen(terminal)?, state));
    let mut quitting = false;

    loop {
        let area = screen(terminal)?;
        motion.tick(&ui::areas(area, state));
        let splash = motion.splash_progress();
        terminal.draw(|f| {
            match splash {
                Some(progress) => ui::render_splash(f, progress),
                None => ui::render(f, state),
            }
            let area = f.area();
            motion.render(f.buffer_mut(), area);
        })?;

        if quitting && !motion.is_animating() {
            return Ok(());
        }
        if motion.is_animating() && !poll(Duration::from_millis(16))? {
            continue;
        }
        let event = read()?;
        if quitting {
            continue;
        }
        let keep_running = match event {
            Event::Key(key_event) => {
                let search = double_shift.feed(&key_event, Instant::now());
                let action = if search {
                    Some(Action::FindInFiles)
                } else if key_event.kind == KeyEventKind::Release {
                    None
                } else {
                    keymap.dispatch_in(state.mode, &key_event)
                };
                match action {
                    Some(action) => {
                        motion.end_splash(&ui::areas(area, state));
                        state.apply(action)
                    }
                    None => true,
                }
            }
            Event::Mouse(mouse)
                if mouse.kind == MouseEventKind::Down(MouseButton::Left)
                    && mouse.modifiers.contains(KeyModifiers::ALT) =>
            {
                let editor_area = ui::editor_rect_for(area, state.mode);
                if let Some((line, col)) =
                    ui::mouse_to_doc(editor_area, mouse.column, mouse.row, &state.tabs[state.active])
                {
                    state.apply(Action::AddCaretAt(line, col));
                }
                true
            }
            Event::Paste(text) => {
                motion.end_splash(&ui::areas(area, state));
                let text = text.replace("\r\n", "\n").replace('\r', "\n");
                state.apply(Action::InsertText(text))
            }
            _ => true,
        };
        let areas = ui::areas(area, state);
        for cue in state.take_cues() {
            motion.cue(cue, &areas);
        }
        if !keep_running {
            quitting = true;
            motion.cue(Cue::Quit, &areas);
        }
    }
}
