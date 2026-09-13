//! Application state.
//!
//! Deliberately free of `ratatui` and `crossterm` types so that the state machine
//! can be unit-tested without a terminal. Rendering reads this; it never owns it.

use tc_agent::{AgentEvent, FinishReason};
use tc_config::Budget;
use tc_core::{Cost, Price, Usage};

/// How a tool call is going.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolState {
    /// Still running.
    Running,
    /// Finished successfully.
    Done,
    /// Failed. The model sees the error and usually recovers, so this is
    /// informational rather than fatal.
    Failed,
}

/// One entry in the visible transcript.
///
/// Tool activity is a first-class entry rather than a hidden detail: showing what
/// the agent read, and when, is the transparency the whole project is premised on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// Something the user typed.
    User(String),
    /// Text from the model.
    Assistant(String),
    /// A tool the agent ran.
    Tool {
        /// Tool name.
        tool: String,
        /// One-line rendering of the arguments.
        summary: String,
        /// How it is going.
        state: ToolState,
    },
}

/// What the app is currently doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Waiting for input.
    Idle,
    /// A turn is in flight; `Esc` aborts it.
    Working,
    /// The budget is exhausted; no further turns are started.
    BudgetExhausted,
}

/// Complete UI state.
#[derive(Debug)]
pub struct App {
    /// Visible conversation.
    pub entries: Vec<Entry>,
    /// Current input line.
    pub input: String,
    /// Cursor position within [`Self::input`], as a byte index.
    pub cursor: usize,
    /// What the app is doing right now.
    pub status: Status,
    /// Model identifier shown in the status bar.
    pub model: String,
    /// Context window of the active model, in tokens.
    pub context_window: u32,
    /// Price of the active model.
    pub price: Price,
    /// Accumulated token usage for the session.
    pub usage: Usage,
    /// Accumulated cost for the session.
    pub cost: Cost,
    /// Spending limits.
    pub budget: Budget,
    /// Lines scrolled up from the bottom of the transcript.
    pub scroll: u16,
    /// A message to show in place of the hint line, e.g. an error.
    pub notice: Option<String>,
    /// Set once the user asked to quit.
    pub should_quit: bool,
}

impl App {
    /// Creates an app for the given model.
    #[must_use]
    pub fn new(model: String, context_window: u32, price: Price, budget: Budget) -> Self {
        Self {
            entries: Vec::new(),
            input: String::new(),
            cursor: 0,
            status: Status::Idle,
            model,
            context_window,
            price,
            usage: Usage::default(),
            cost: Cost::default(),
            budget,
            scroll: 0,
            notice: None,
            should_quit: false,
        }
    }

    /// Applies one event from the agent.
    pub fn apply(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::Started { model } => {
                self.model = model;
                self.status = Status::Working;
            }

            AgentEvent::Text { text } => self.append_assistant_text(&text),

            AgentEvent::ToolStarted { tool, summary } => {
                self.entries.push(Entry::Tool { tool, summary, state: ToolState::Running });
            }

            AgentEvent::ToolFinished { tool, is_error } => {
                self.finish_tool(&tool, is_error);
            }

            AgentEvent::TurnCompleted { usage, cost } => {
                self.usage = self.usage.saturating_add(usage);
                self.cost = self.cost.add(cost);
            }

            AgentEvent::Finished { reason } => self.finish(&reason),

            AgentEvent::Failed { message } => {
                self.status = Status::Idle;
                self.notice = Some(message);
            }
        }
    }

    /// Appends streamed text to the current assistant entry, or starts one.
    fn append_assistant_text(&mut self, text: &str) {
        match self.entries.last_mut() {
            Some(Entry::Assistant(existing)) => existing.push_str(text),
            _ => self.entries.push(Entry::Assistant(text.to_owned())),
        }
    }

    /// Marks the most recent running instance of `tool` as finished.
    ///
    /// Searches from the back because parallel tool calls can be in flight at
    /// once, and the newest matching one is the one that just reported.
    fn finish_tool(&mut self, tool: &str, is_error: bool) {
        let state = if is_error { ToolState::Failed } else { ToolState::Done };
        for entry in self.entries.iter_mut().rev() {
            if let Entry::Tool { tool: name, state: existing, .. } = entry
                && name == tool
                && *existing == ToolState::Running
            {
                *existing = state;
                return;
            }
        }
    }

    /// Closes out a run.
    fn finish(&mut self, reason: &FinishReason) {
        self.status = if *reason == FinishReason::BudgetExhausted
            || self.budget.is_exhausted(self.cost.usd)
        {
            Status::BudgetExhausted
        } else {
            Status::Idle
        };

        // "Done." is the expected outcome and does not need announcing; every
        // other reason tells the user something they need to act on.
        self.notice = match reason {
            FinishReason::Completed => None,
            other => Some(other.message()),
        };
    }

    /// Records that the user aborted the run.
    pub fn abort(&mut self) {
        if self.status == Status::Working {
            self.status = Status::Idle;
            self.notice = Some("Aborted.".to_owned());
            for entry in &mut self.entries {
                if let Entry::Tool { state, .. } = entry
                    && *state == ToolState::Running
                {
                    *state = ToolState::Failed;
                }
            }
        }
    }

    /// Takes the current input as a user turn, if it is non-empty and allowed.
    ///
    /// Returns the prompt to send, or `None` when nothing should be sent.
    pub fn submit(&mut self) -> Option<String> {
        if self.status != Status::Idle {
            return None;
        }
        let prompt = self.input.trim().to_owned();
        if prompt.is_empty() {
            return None;
        }
        self.input.clear();
        self.cursor = 0;
        self.notice = None;
        self.scroll = 0;
        self.entries.push(Entry::User(prompt.clone()));
        Some(prompt)
    }

    /// Inserts a character at the cursor.
    pub fn insert_char(&mut self, ch: char) {
        self.input.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
    }

    /// Deletes the character before the cursor.
    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let previous = self.input[..self.cursor]
            .chars()
            .next_back()
            .map_or(0, |ch| self.cursor - ch.len_utf8());
        self.input.replace_range(previous..self.cursor, "");
        self.cursor = previous;
    }

    /// Moves the cursor one character left.
    pub fn cursor_left(&mut self) {
        if let Some(ch) = self.input[..self.cursor].chars().next_back() {
            self.cursor -= ch.len_utf8();
        }
    }

    /// Moves the cursor one character right.
    pub fn cursor_right(&mut self) {
        if let Some(ch) = self.input[self.cursor..].chars().next() {
            self.cursor += ch.len_utf8();
        }
    }

    /// Scrolls the transcript up by `lines`.
    pub fn scroll_up(&mut self, lines: u16) {
        self.scroll = self.scroll.saturating_add(lines);
    }

    /// Scrolls the transcript down by `lines`.
    pub fn scroll_down(&mut self, lines: u16) {
        self.scroll = self.scroll.saturating_sub(lines);
    }

    /// Share of the context window currently occupied, in `0.0..=1.0`.
    ///
    /// Quality degrades long before a window is full, so this is shown as a health
    /// indicator rather than only as a limit warning.
    #[must_use]
    pub fn context_fill(&self) -> f64 {
        if self.context_window == 0 {
            return 0.0;
        }
        f64::from(self.usage.total_input()) / f64::from(self.context_window)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> App {
        App::new(
            "test/model".to_owned(),
            1_000,
            Price {
                input_per_mtok: 3.0,
                output_per_mtok: 15.0,
                cached_input_per_mtok: 0.3,
                cache_write_per_mtok: 3.75,
            },
            Budget { session_limit_usd: 1.0, warn_at_percent: 70 },
        )
    }

    fn tool_started(tool: &str) -> AgentEvent {
        AgentEvent::ToolStarted { tool: tool.to_owned(), summary: "path=a.rs".to_owned() }
    }

    #[test]
    fn streaming_text_accumulates_into_one_entry() {
        let mut app = test_app();
        app.apply(AgentEvent::Started { model: "test/model".to_owned() });
        app.apply(AgentEvent::Text { text: "Hal".to_owned() });
        app.apply(AgentEvent::Text { text: "lo".to_owned() });

        assert_eq!(app.entries, vec![Entry::Assistant("Hallo".to_owned())]);
        assert_eq!(app.status, Status::Working);
    }

    #[test]
    fn a_tool_call_appears_in_the_transcript_while_it_runs() {
        let mut app = test_app();
        app.apply(tool_started("read_file"));

        assert_eq!(
            app.entries[0],
            Entry::Tool {
                tool: "read_file".to_owned(),
                summary: "path=a.rs".to_owned(),
                state: ToolState::Running,
            }
        );
    }

    #[test]
    fn finishing_a_tool_marks_that_entry_done() {
        let mut app = test_app();
        app.apply(tool_started("read_file"));
        app.apply(AgentEvent::ToolFinished { tool: "read_file".to_owned(), is_error: false });

        assert!(matches!(app.entries[0], Entry::Tool { state: ToolState::Done, .. }));
    }

    #[test]
    fn a_failed_tool_is_marked_but_does_not_end_the_run() {
        let mut app = test_app();
        app.apply(AgentEvent::Started { model: "test/model".to_owned() });
        app.apply(tool_started("read_file"));
        app.apply(AgentEvent::ToolFinished { tool: "read_file".to_owned(), is_error: true });

        assert!(matches!(app.entries[0], Entry::Tool { state: ToolState::Failed, .. }));
        assert_eq!(
            app.status,
            Status::Working,
            "a tool error is context for the model, not the end of the run"
        );
    }

    #[test]
    fn parallel_calls_of_the_same_tool_finish_independently() {
        let mut app = test_app();
        app.apply(tool_started("grep"));
        app.apply(tool_started("grep"));
        app.apply(AgentEvent::ToolFinished { tool: "grep".to_owned(), is_error: false });

        // The most recent running call is the one that reported.
        assert!(matches!(app.entries[0], Entry::Tool { state: ToolState::Running, .. }));
        assert!(matches!(app.entries[1], Entry::Tool { state: ToolState::Done, .. }));
    }

    #[test]
    fn text_after_a_tool_starts_a_new_entry_instead_of_appending_to_the_tool() {
        let mut app = test_app();
        app.apply(AgentEvent::Text { text: "reading".to_owned() });
        app.apply(tool_started("read_file"));
        app.apply(AgentEvent::Text { text: "found it".to_owned() });

        assert_eq!(app.entries.len(), 3);
        assert_eq!(app.entries[2], Entry::Assistant("found it".to_owned()));
    }

    #[test]
    fn completing_a_turn_accumulates_usage_and_cost() {
        let mut app = test_app();
        app.apply(AgentEvent::TurnCompleted {
            usage: Usage { input_tokens: 1_000, output_tokens: 1_000, ..Usage::default() },
            cost: Cost { usd: 0.018 },
        });

        assert_eq!(app.usage.output_tokens, 1_000);
        assert!((app.cost.usd - 0.018).abs() < 1e-9);
    }

    #[test]
    fn a_successful_finish_shows_no_notice() {
        let mut app = test_app();
        app.apply(AgentEvent::Finished { reason: FinishReason::Completed });

        assert_eq!(app.status, Status::Idle);
        assert!(app.notice.is_none(), "'Done.' is noise the user does not need");
    }

    #[test]
    fn a_guard_rail_finish_explains_itself() {
        let mut app = test_app();
        app.apply(AgentEvent::Finished { reason: FinishReason::TurnLimit });

        assert!(app.notice.expect("a notice is set").contains("narrowing"));
    }

    #[test]
    fn exceeding_the_budget_stops_the_session_instead_of_spending_on() {
        let mut app = test_app();
        app.apply(AgentEvent::Finished { reason: FinishReason::BudgetExhausted });

        assert_eq!(app.status, Status::BudgetExhausted);
        app.input = "another prompt".to_owned();
        assert!(app.submit().is_none(), "no further turn may be started");
    }

    #[test]
    fn a_failure_is_reported_rather_than_swallowed() {
        let mut app = test_app();
        app.apply(AgentEvent::Failed { message: "network unreachable".to_owned() });

        assert_eq!(app.status, Status::Idle);
        assert_eq!(app.notice.as_deref(), Some("network unreachable"));
    }

    #[test]
    fn submit_ignores_blank_input() {
        let mut app = test_app();
        app.input = "   ".to_owned();
        assert!(app.submit().is_none());
        assert!(app.entries.is_empty());
    }

    #[test]
    fn submit_moves_the_prompt_into_the_transcript() {
        let mut app = test_app();
        for ch in "hi".chars() {
            app.insert_char(ch);
        }
        assert_eq!(app.submit().as_deref(), Some("hi"));
        assert!(app.input.is_empty());
        assert_eq!(app.entries[0], Entry::User("hi".to_owned()));
    }

    #[test]
    fn submit_is_refused_while_a_turn_is_in_flight() {
        let mut app = test_app();
        app.status = Status::Working;
        app.input = "second prompt".to_owned();
        assert!(app.submit().is_none());
    }

    #[test]
    fn aborting_marks_running_tools_as_failed() {
        let mut app = test_app();
        app.status = Status::Working;
        app.apply(tool_started("grep"));
        app.abort();

        assert_eq!(app.status, Status::Idle);
        assert!(matches!(app.entries[0], Entry::Tool { state: ToolState::Failed, .. }));
    }

    #[test]
    fn aborting_when_nothing_runs_is_a_no_op() {
        let mut app = test_app();
        app.abort();
        assert!(app.notice.is_none());
    }

    #[test]
    fn editing_handles_multi_byte_characters() {
        let mut app = test_app();
        for ch in "äöü".chars() {
            app.insert_char(ch);
        }
        app.backspace();
        assert_eq!(app.input, "äö");

        app.cursor_left();
        app.insert_char('x');
        assert_eq!(app.input, "äxö");
    }

    #[test]
    fn backspace_on_empty_input_is_a_no_op() {
        let mut app = test_app();
        app.backspace();
        assert!(app.input.is_empty());
        assert_eq!(app.cursor, 0);
    }

    #[test]
    fn context_fill_reports_the_used_share() {
        let mut app = test_app();
        app.usage = Usage { input_tokens: 250, ..Usage::default() };
        assert!((app.context_fill() - 0.25).abs() < 1e-9);
    }

    #[test]
    fn scrolling_down_never_goes_below_the_bottom() {
        let mut app = test_app();
        app.scroll_down(5);
        assert_eq!(app.scroll, 0);
    }
}
