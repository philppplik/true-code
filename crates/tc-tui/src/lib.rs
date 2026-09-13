//! Terminal lifecycle and the event loop.
//!
//! # Two rules this module exists to enforce
//!
//! 1. **The event loop never blocks.** Network streaming runs in a separate task and
//!    reaches the UI through a channel. A slow provider must never freeze the
//!    keyboard — that is what makes `Esc` trustworthy.
//! 2. **The terminal is always restored.** Raw mode and the alternate screen are
//!    global terminal state; leaking them on a panic leaves the user with a broken
//!    shell and no idea why. The panic hook below is installed before raw mode is
//!    entered, on purpose.

pub mod app;
mod ui;

use std::io::{Stdout, stdout};
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{
    DisableBracketedPaste, EnableBracketedPaste, Event, EventStream, KeyCode, KeyEvent,
    KeyEventKind, KeyModifiers,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use futures::StreamExt;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tc_config::Config;
use tc_core::{Delta, Message};
use tc_providers::{Provider, Request};
use tokio::sync::mpsc;

use crate::app::{App, Status};

/// Frame budget. Rendering per token would flicker and burn CPU for no gain.
const FRAME_INTERVAL: Duration = Duration::from_millis(33);

/// Lines moved by one page-scroll key press.
const PAGE_SCROLL: u16 = 10;

/// What the streaming task sends back to the UI.
#[derive(Debug)]
enum TurnMessage {
    /// A normalised increment from the provider.
    Delta(Delta),
    /// The turn failed. Errors are shown, not swallowed.
    Failed(String),
}

/// Runs the interactive TUI until the user quits.
pub fn run(provider: Box<dyn Provider>, config: &Config) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    runtime.block_on(run_async(Arc::from(provider), config))
}

/// Async body of [`run`], kept separate so the terminal guard has a clear scope.
async fn run_async(provider: Arc<dyn Provider>, config: &Config) -> anyhow::Result<()> {
    let mut terminal = TerminalGuard::enter()?;

    let mut app = App::new(
        provider.id().to_owned(),
        provider.context_window(),
        provider.price(),
        config.budget,
    );
    let mut history: Vec<Message> = Vec::new();

    let (tx, mut rx) = mpsc::channel::<TurnMessage>(256);
    let mut turn: Option<tokio::task::JoinHandle<()>> = None;

    let mut keys = EventStream::new();
    let mut frames = tokio::time::interval(FRAME_INTERVAL);
    let mut dirty = true;

    loop {
        if dirty {
            terminal.inner.draw(|frame| ui::draw(frame, &app))?;
            dirty = false;
        }

        tokio::select! {
            _ = frames.tick() => {}

            Some(message) = rx.recv() => {
                match message {
                    TurnMessage::Delta(delta) => {
                        let completed = matches!(delta, Delta::Completed { .. });
                        app.apply(delta);
                        if completed {
                            if let Some(entry) = app.entries.last() {
                                history.push(Message::assistant(entry.text.clone()));
                            }
                            turn = None;
                        }
                    }
                    TurnMessage::Failed(error) => {
                        app.report_error(error);
                        turn = None;
                    }
                }
                dirty = true;
            }

            Some(Ok(event)) = keys.next() => {
                match event {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        handle_key(key, &mut app, &mut turn, &mut history, &provider, &tx, config);
                    }
                    Event::Paste(text) => {
                        for ch in text.chars().filter(|ch| !ch.is_control()) {
                            app.insert_char(ch);
                        }
                    }
                    Event::Resize(_, _) => {}
                    _ => continue,
                }
                dirty = true;
            }
        }

        if app.should_quit {
            break;
        }
    }

    if let Some(handle) = turn {
        handle.abort();
    }
    Ok(())
}

/// Applies one key press.
#[allow(clippy::too_many_arguments)]
fn handle_key(
    key: KeyEvent,
    app: &mut App,
    turn: &mut Option<tokio::task::JoinHandle<()>>,
    history: &mut Vec<Message>,
    provider: &Arc<dyn Provider>,
    tx: &mpsc::Sender<TurnMessage>,
    config: &Config,
) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    match key.code {
        KeyCode::Char('c' | 'd') if ctrl => app.should_quit = true,

        // Dropping the task drops the HTTP stream, which aborts the request.
        // One cancellation mechanism, no half-cancelled state to reason about.
        KeyCode::Esc => {
            if let Some(handle) = turn.take() {
                handle.abort();
            }
            app.abort();
        }

        KeyCode::Enter => {
            if let Some(prompt) = app.submit() {
                history.push(Message::user(prompt));
                *turn = Some(spawn_turn(provider.clone(), history.clone(), tx.clone(), config));
                app.status = Status::Streaming;
            }
        }

        KeyCode::Backspace => app.backspace(),
        KeyCode::Left => app.cursor_left(),
        KeyCode::Right => app.cursor_right(),
        KeyCode::PageUp => app.scroll_up(PAGE_SCROLL),
        KeyCode::PageDown => app.scroll_down(PAGE_SCROLL),
        KeyCode::Char(ch) if !ctrl => app.insert_char(ch),
        _ => {}
    }
}

/// Spawns the streaming task for one turn.
fn spawn_turn(
    provider: Arc<dyn Provider>,
    history: Vec<Message>,
    tx: mpsc::Sender<TurnMessage>,
    config: &Config,
) -> tokio::task::JoinHandle<()> {
    let request = Request::new(config.system_prompt.clone(), history);

    tokio::spawn(async move {
        let mut stream = match provider.stream(request).await {
            Ok(stream) => stream,
            Err(err) => {
                let _ = tx.send(TurnMessage::Failed(err.to_string())).await;
                return;
            }
        };

        while let Some(item) = stream.next().await {
            let message = match item {
                Ok(delta) => TurnMessage::Delta(delta),
                Err(err) => TurnMessage::Failed(err.to_string()),
            };
            // A closed receiver means the UI is gone; stop rather than spin.
            if tx.send(message).await.is_err() {
                return;
            }
        }
    })
}

/// Owns the terminal's global state and restores it on drop.
struct TerminalGuard {
    inner: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    /// Enters raw mode and the alternate screen, installing a panic hook first.
    fn enter() -> anyhow::Result<Self> {
        // Installed *before* raw mode: if anything below panics, the hook is
        // already in place to clean up.
        let previous_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore();
            previous_hook(info);
        }));

        enable_raw_mode()?;
        execute!(stdout(), EnterAlternateScreen, EnableBracketedPaste)?;
        Ok(Self { inner: Terminal::new(CrosstermBackend::new(stdout()))? })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore();
    }
}

/// Restores the terminal to a usable state.
///
/// Errors are deliberately ignored: this runs on panic and during shutdown, where
/// there is nothing useful left to do with a failure, and reporting one would hide
/// the original problem.
fn restore() {
    let _ = disable_raw_mode();
    let _ = execute!(stdout(), LeaveAlternateScreen, DisableBracketedPaste);
}
