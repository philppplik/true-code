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
pub mod setup;
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
use tc_agent::{Agent, AgentEvent, ApprovalRequest, Approver, Decision, PermissionMode};
use tc_config::Config;
use tokio::sync::{Mutex, mpsc, oneshot};

use crate::app::{App, Status, Submission};

/// One question from the agent, and the channel its answer goes back on.
type ApprovalMessage = (ApprovalRequest, oneshot::Sender<Decision>);

/// Routes approval requests to the TUI and waits for the user.
///
/// The agent knows nothing about the terminal (ADR 0002): it awaits a `Decision`,
/// and whether that comes from a modal, a policy or a test is not its concern.
#[derive(Debug)]
pub struct TuiApprover {
    requests: mpsc::Sender<ApprovalMessage>,
}

#[async_trait::async_trait]
impl Approver for TuiApprover {
    async fn approve(&self, request: &ApprovalRequest) -> Decision {
        let (tx, rx) = oneshot::channel();

        if self.requests.send((request.clone(), tx)).await.is_err() {
            // The UI is gone. Refusing is the only safe reading of silence.
            return Decision::Deny;
        }
        rx.await.unwrap_or(Decision::Deny)
    }
}

/// Frame budget. Rendering per token would flicker and burn CPU for no gain.
const FRAME_INTERVAL: Duration = Duration::from_millis(33);

/// Lines moved by one page-scroll key press.
const PAGE_SCROLL: u16 = 10;

/// Creates the approver the TUI answers, together with its request channel.
///
/// Returned as a pair because the agent is constructed before the event loop
/// starts, so both ends have to exist by then.
#[must_use]
pub fn approver() -> (Arc<TuiApprover>, mpsc::Receiver<ApprovalMessage>) {
    let (tx, rx) = mpsc::channel(8);
    (Arc::new(TuiApprover { requests: tx }), rx)
}

/// Runs the interactive TUI until the user quits.
pub fn run(
    agent: Agent,
    config: &Config,
    mode: PermissionMode,
    approvals: mpsc::Receiver<ApprovalMessage>,
) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    runtime.block_on(run_async(agent, config, mode, approvals))
}

/// Async body of [`run`], kept separate so the terminal guard has a clear scope.
async fn run_async(
    agent: Agent,
    config: &Config,
    mode: PermissionMode,
    mut approvals: mpsc::Receiver<ApprovalMessage>,
) -> anyhow::Result<()> {
    let mut app = App::new(
        agent.model_id().to_owned(),
        agent.context_window(),
        agent.price(),
        config.budget,
        mode,
        agent.rule_count(),
    );

    let agent = Arc::new(Mutex::new(agent));
    let mut terminal = TerminalGuard::enter()?;

    let (tx, mut rx) = mpsc::channel::<AgentEvent>(256);
    let mut run: Option<tokio::task::JoinHandle<()>> = None;
    // The reply channel for the change currently on screen.
    let mut answer: Option<oneshot::Sender<Decision>> = None;

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

            Some((request, reply)) = approvals.recv() => {
                app.ask(request);
                answer = Some(reply);
                dirty = true;
            }

            Some(Ok(event)) = keys.next() => {
                match event {
                    // While a change is on screen it owns the keyboard. Typing a
                    // prompt into a confirmation is how people approve things
                    // they never read.
                    Event::Key(key)
                        if key.kind == KeyEventKind::Press && app.question.is_some() =>
                    {
                        answer_question(key, &mut app, &agent, &tx);
                    }
                    Event::Key(key) if key.kind == KeyEventKind::Press && answer.is_some() => {
                        if let Some(decision) = decide(key, &mut app) {
                            if let Some(reply) = answer.take() {
                                let _ = reply.send(decision);
                            }
                            app.clear_pending();
                        }
                    }
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

/// Interprets a key press while a comprehension question is up.
///
/// Answering is one keystroke, and skipping is one keystroke. A check that is
/// laborious to dismiss is a check people disable.
fn answer_question(
    key: KeyEvent,
    app: &mut App,
    agent: &Arc<Mutex<Agent>>,
    tx: &mpsc::Sender<AgentEvent>,
) {
    let Some(question) = app.question.clone() else {
        return;
    };

    match key.code {
        KeyCode::Esc => {
            // Skipping records nothing. A skipped question is not a wrong answer,
            // and counting it as one would make the profile lie.
            app.clear_question();
        }
        KeyCode::Char(ch) if ch.is_ascii_digit() => {
            let chosen = ch.to_digit(10).unwrap_or(0) as usize;
            if chosen == 0 || chosen > question.options.len() {
                return;
            }
            app.clear_question();

            let agent = agent.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                let feedback = agent.lock().await.answer(&question, chosen - 1);
                // Into the transcript, not a dialog: an explanation that scrolls
                // away is learning effort thrown out.
                let _ = tx.send(AgentEvent::Notice { text: feedback }).await;
            });
        }
        _ => {}
    }
}

/// Interprets a key press while a change is awaiting confirmation.
///
/// Returns the decision, or `None` for keys that only scroll the diff.
///
/// There is deliberately no "approve on Enter": Enter is the send key everywhere
/// else in this UI, and muscle memory must not be able to apply a change.
fn decide(key: KeyEvent, app: &mut App) -> Option<Decision> {
    match key.code {
        KeyCode::Char('y' | 'Y') => Some(Decision::Approve),
        KeyCode::Char('a' | 'A') => Some(Decision::ApproveToolForSession),
        KeyCode::Char('n' | 'N') => Some(Decision::Deny),
        KeyCode::Esc => Some(Decision::Abort),
        KeyCode::Char('c' | 'd') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
            Some(Decision::Abort)
        }
        KeyCode::Up => {
            app.scroll_pending_up(1);
            None
        }
        KeyCode::Down => {
            app.scroll_pending_down(1);
            None
        }
        KeyCode::PageUp => {
            app.scroll_pending_up(PAGE_SCROLL);
            None
        }
        KeyCode::PageDown => {
            app.scroll_pending_down(PAGE_SCROLL);
            None
        }
        // Every other key is ignored rather than guessed at.
        _ => None,
    }
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

        KeyCode::Enter => match app.submit() {
            Some(Submission::Prompt(prompt)) => {
                *run = Some(spawn_run(agent.clone(), prompt, tx.clone()));
                app.status = Status::Working;
            }
            // Undo touches only the filesystem, but the agent is behind a lock,
            // so it runs as a task like any other and reports over the same channel.
            Some(Submission::Undo) => {
                tokio::spawn(undo(agent.clone(), tx.clone()));
            }
            Some(Submission::Handoff) => {
                tokio::spawn(handoff(agent.clone(), tx.clone()));
            }
            Some(Submission::Help) => app.note(App::help_text()),
            None => {}
        },

        KeyCode::Backspace => app.backspace(),
        KeyCode::Left => app.cursor_left(),
        KeyCode::Right => app.cursor_right(),
        KeyCode::PageUp => app.scroll_up(PAGE_SCROLL),
        KeyCode::PageDown => app.scroll_down(PAGE_SCROLL),
        KeyCode::Char(ch) if !ctrl => app.insert_char(ch),
        _ => {}
    }
}

/// Undoes the last change and reports the outcome.
async fn undo(agent: Arc<Mutex<Agent>>, tx: mpsc::Sender<AgentEvent>) {
    let text = match agent.lock().await.undo_last() {
        Ok(message) => message,
        // A failed undo is reported in full. "Nothing happened" with no reason is
        // the worst possible answer when someone is trying to take a change back.
        Err(err) => err.to_string(),
    };
    let _ = tx.send(AgentEvent::Notice { text }).await;
}

/// Writes a session summary and reports where it went.
async fn handoff(agent: Arc<Mutex<Agent>>, tx: mpsc::Sender<AgentEvent>) {
    let text = match agent.lock().await.write_handoff() {
        Ok(path) => format!("Handoff written to {}", path.display()),
        Err(err) => format!("Could not write the handoff: {err}"),
    };
    let _ = tx.send(AgentEvent::Notice { text }).await;
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
