//! Terminal lifecycle and the event loop.
//!
//! # Two rules this module exists to enforce
//!
//! 1. **The event loop never blocks.** The agent runs in a separate task and
//!    reaches the UI through a channel. A slow provider or a long-running tool
//!    must never freeze the keyboard — that is what makes `Esc` trustworthy.
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
use tc_agent::{Agent, AgentEvent};
use tc_config::Config;
use tokio::sync::{Mutex, mpsc};

use crate::app::{App, Status};

/// Frame budget. Rendering per token would flicker and burn CPU for no gain.
const FRAME_INTERVAL: Duration = Duration::from_millis(33);

/// Lines moved by one page-scroll key press.
const PAGE_SCROLL: u16 = 10;

/// Runs the interactive TUI until the user quits.
pub fn run(agent: Agent, config: &Config) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    runtime.block_on(run_async(agent, config))
}

/// Async body of [`run`], kept separate so the terminal guard has a clear scope.
async fn run_async(agent: Agent, config: &Config) -> anyhow::Result<()> {
    let mut app =
        App::new(agent.model_id().to_owned(), agent.context_window(), agent.price(), config.budget);

    let agent = Arc::new(Mutex::new(agent));
    let mut terminal = TerminalGuard::enter()?;

    let (tx, mut rx) = mpsc::channel::<AgentEvent>(256);
    let mut run: Option<tokio::task::JoinHandle<()>> = None;

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

            Some(event) = rx.recv() => {
                let ends_run = matches!(
                    event,
                    AgentEvent::Finished { .. } | AgentEvent::Failed { .. }
                );
                app.apply(event);
                if ends_run {
                    run = None;
                }
                dirty = true;
            }

            Some(Ok(event)) = keys.next() => {
                match event {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        handle_key(key, &mut app, &mut run, &agent, &tx);
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

    if let Some(handle) = run {
        handle.abort();
    }
    Ok(())
}

/// Applies one key press.
fn handle_key(
    key: KeyEvent,
    app: &mut App,
    run: &mut Option<tokio::task::JoinHandle<()>>,
    agent: &Arc<Mutex<Agent>>,
    tx: &mpsc::Sender<AgentEvent>,
) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    match key.code {
        KeyCode::Char('c' | 'd') if ctrl => app.should_quit = true,

        // Dropping the task drops the HTTP stream, which aborts the request.
        // One cancellation mechanism, no half-cancelled state to reason about.
        KeyCode::Esc => {
            if let Some(handle) = run.take() {
                handle.abort();
            }
            app.abort();
        }

        KeyCode::Enter => {
            if let Some(prompt) = app.submit() {
                *run = Some(spawn_run(agent.clone(), prompt, tx.clone()));
                app.status = Status::Working;
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

/// Spawns the agent run for one prompt.
fn spawn_run(
    agent: Arc<Mutex<Agent>>,
    prompt: String,
    tx: mpsc::Sender<AgentEvent>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // Uncontended in practice: the UI refuses a second prompt while one runs.
        // The lock exists so an aborted task cannot leave the agent half-updated.
        let mut agent = agent.lock().await;
        agent.run(&prompt, &tx).await;
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
