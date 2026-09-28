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
mod update;
mod ui;

use std::env;
use std::io::{stdout, Stdout};
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{
    poll, read, DisableBracketedPaste, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste, EnableFocusChange, EnableMouseCapture, Event, KeyEventKind, KeyModifiers, MouseButton,
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
    match args.get(1).map(String::as_str) {
        Some("--version" | "-V") => {
            let rev = update::installed_rev().unwrap_or_else(|| "not installed by install.sh".to_string());
            println!("ee {} ({})", env!("CARGO_PKG_VERSION"), rev);
            return Ok(());
        }
        Some("--update") => {
            let status = update::run_installer()?;
            std::process::exit(status.code().unwrap_or(1));
        }
        Some("--help" | "-h") => {
            println!("ee: Eric's Own Editor\n\n  ee [file ...]     open files (a missing file opens empty; saving creates it)\n  ee --update       download and install the latest version\n  ee --version      show the installed version\n\nKeys, config and more: https://github.com/uxeric/ee");
            return Ok(());
        }
        _ => {}
    }
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

    execute!(stdout(), EnterAlternateScreen, EnableMouseCapture, EnableBracketedPaste, EnableFocusChange)?;
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
    execute!(stdout(), DisableFocusChange, DisableBracketedPaste, DisableMouseCapture, LeaveAlternateScreen)?;
    result?;
    if state.restart_for_update {
        let (message, code) = update::install_and_restart(&args[1..]);
        eprintln!("ee: {}", message);
        std::process::exit(code);
    }
    Ok(())
}

fn screen(terminal: &Terminal<CrosstermBackend<Stdout>>) -> std::io::Result<Rect> {
    let size = terminal.size()?;
    Ok(Rect::new(0, 0, size.width, size.height))
}

fn run(terminal: &mut Terminal<CrosstermBackend<Stdout>>, state: &mut EditorState) -> std::io::Result<()> {
    let (config, warning) = config::load();
    state.theme_roots = theme::Roots::from_env();
    state.config_path = config::path();
    let theme_found = state.use_theme(&config.theme);
    let missing_theme = config.theme.clone();
    let keymap = Keymap::new(config);
    if let Some(warning) = warning {
        state.warn(warning);
    } else if !theme_found {
        state.warn(format!("theme `{}` isn't installed; using neon (Ctrl+` picks another)", missing_theme));
    }
    let mut double_shift = DoubleShift::default();
    let mut motion = Motion::from_env();
    motion.jack_in(&ui::areas(screen(terminal)?, state));
    let mut last_click: Option<(Instant, (usize, usize), u8)> = None;
    let mut quitting = false;
    let mut update_check = update::spawn_check();

    loop {
        let area = screen(terminal)?;
        if !motion.is_animating() {
            state.theme.refresh();
        }
        if let Some(rx) = &update_check {
            match rx.try_recv() {
                Ok((from, to)) => {
                    state.offer_update(from, to);
                    update_check = None;
                    let areas = ui::areas(area, state);
                    for cue in state.take_cues() {
                        motion.cue(cue, &areas);
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => update_check = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        let was_animating = motion.is_animating();
        terminal.draw(|f| {
            ui::render(f, state);
            let area = f.area();
            motion.render(f.buffer_mut(), area);
        })?;
        if was_animating && !motion.is_animating() && !quitting {
            continue;
        }

        if quitting && !motion.is_animating() {
            return Ok(());
        }
        let wait = if motion.is_animating() {
            Some(Duration::from_millis(16))
        } else if update_check.is_some() {
            Some(Duration::from_millis(200))
        } else {
            None
        };
        if let Some(wait) = wait {
            if !poll(wait)? {
                continue;
            }
        }
        state.page_rows = ui::page_rows(area, state);
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
                    Some(action) => state.apply(action),
                    None => true,
                }
            }
            Event::Mouse(mouse)
                if mouse.kind == MouseEventKind::Down(MouseButton::Left)
                    && ui::help_button(area, state).is_some_and(|b| b.contains(ratatui::layout::Position::new(mouse.column, mouse.row))) =>
            {
                state.apply(Action::ShowHelp)
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
            Event::Mouse(mouse)
                if mouse.kind == MouseEventKind::Down(MouseButton::Left)
                    && mouse.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                let editor_area = ui::editor_rect_for(area, state.mode);
                match ui::link_at(editor_area, mouse.column, mouse.row, &state.tabs[state.active]) {
                    Some(url) => state.apply(Action::FollowLink(url)),
                    None => true,
                }
            }
            Event::Mouse(mouse)
                if matches!(state.mode, state::Mode::Normal)
                    && matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left)) =>
            {
                let editor_area = ui::editor_rect_for(area, state.mode);
                let doc = &state.tabs[state.active];
                let below = mouse.row >= editor_area.y + editor_area.height;
                let at = ui::mouse_to_doc(editor_area, mouse.column, mouse.row, doc).or_else(|| {
                    (mouse.row >= editor_area.y && !below || mouse.kind != MouseEventKind::Down(MouseButton::Left))
                        .then(|| (doc.lines.len() - 1, usize::MAX))
                });
                match (mouse.kind, at) {
                    (MouseEventKind::Drag(_), Some((line, col))) => state.apply(Action::DragTo(line, col)),
                    (_, Some((line, col))) if mouse.modifiers.contains(KeyModifiers::SHIFT) => {
                        state.apply(Action::ShiftClickAt(line, col))
                    }
                    (_, Some((line, col))) => {
                        let now = Instant::now();
                        let clicks = match last_click {
                            Some((t, (l, _), n)) if l == line && now.duration_since(t) < Duration::from_millis(400) => (n % 3) + 1,
                            _ => 1,
                        };
                        last_click = Some((now, (line, col), clicks));
                        state.apply(Action::ClickAt(line, col, clicks))
                    }
                    _ => true,
                }
            }
            Event::Mouse(mouse) if matches!(mouse.kind, MouseEventKind::ScrollUp | MouseEventKind::ScrollDown) => {
                let lines = if mouse.kind == MouseEventKind::ScrollUp { -3 } else { 3 };
                if mouse.modifiers.contains(KeyModifiers::SHIFT) {
                    ui::scroll_sideways(area, state, lines * 2);
                } else {
                    ui::scroll_view(area, state, lines);
                }
                true
            }
            Event::Mouse(mouse) if matches!(mouse.kind, MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight) => {
                ui::scroll_sideways(area, state, if mouse.kind == MouseEventKind::ScrollLeft { -6 } else { 6 });
                true
            }
            Event::Paste(text) => {
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
