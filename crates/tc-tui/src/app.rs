//! Application state.
//!
//! Deliberately free of `ratatui` and `crossterm` types so that the state machine
//! can be unit-tested without a terminal. Rendering reads this; it never owns it.

use tc_config::Budget;
use tc_core::{Cost, Delta, Price, Role, StopReason, Usage};

/// One entry in the visible transcript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Who said it.
    pub role: Role,
    /// What was said. Grows incrementally while the assistant streams.
    pub text: String,
}

/// What the app is currently doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Waiting for input.
    Idle,
    /// A turn is in flight; `Esc` aborts it.
    Streaming,
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

    /// Applies one streaming increment.
    pub fn apply(&mut self, delta: Delta) {
        match delta {
            Delta::Started { model } => {
                self.model = model;
                self.status = Status::Streaming;
                self.entries.push(Entry { role: Role::Assistant, text: String::new() });
            }
            Delta::Text { text } => self.append_assistant_text(&text),
            Delta::Completed { stop_reason, usage } => self.finish_turn(stop_reason, usage),
        }
    }

    /// Appends streamed text to the current assistant entry.
    ///
    /// Creates the entry if the provider sent text before announcing the model —
    /// dropping tokens because of an unexpected event order would be worse.
    fn append_assistant_text(&mut self, text: &str) {
        match self.entries.last_mut() {
            Some(entry) if entry.role == Role::Assistant => entry.text.push_str(text),
            _ => self.entries.push(Entry { role: Role::Assistant, text: text.to_owned() }),
        }
    }

    /// Closes out a turn and updates the cost accounting.
    fn finish_turn(&mut self, stop_reason: StopReason, usage: Usage) {
        self.usage = self.usage.saturating_add(usage);
        self.cost = self.cost.add(self.price.cost_of(usage));

        self.status = if self.budget.is_exhausted(self.cost.usd) {
            self.notice = Some(format!(
                "Budget of ${:.2} reached — session stopped. Raise `budget.session_limit_usd` to continue.",
                self.budget.session_limit_usd
            ));
            Status::BudgetExhausted
        } else {
            Status::Idle
        };

        if stop_reason == StopReason::MaxTokens {
            self.notice = Some(
                "Answer was cut off at the output limit — ask for a shorter scope.".to_owned(),
            );
        }
    }

    /// Records that the user aborted the turn.
    pub fn abort(&mut self) {
        if self.status == Status::Streaming {
            self.status = Status::Idle;
            self.notice = Some("Aborted.".to_owned());
        }
    }

    /// Reports an error into the transcript instead of crashing the session.
    pub fn report_error(&mut self, message: impl Into<String>) {
        self.status = Status::Idle;
        self.notice = Some(message.into());
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
        self.entries.push(Entry { role: Role::User, text: prompt.clone() });
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

    #[test]
    fn streaming_text_accumulates_into_one_entry() {
        let mut app = test_app();
        app.apply(Delta::Started { model: "test/model".to_owned() });
        app.apply(Delta::Text { text: "Hal".to_owned() });
        app.apply(Delta::Text { text: "lo".to_owned() });

        assert_eq!(app.entries.len(), 1);
        assert_eq!(app.entries[0].text, "Hallo");
        assert_eq!(app.status, Status::Streaming);
    }

    #[test]
    fn text_before_the_start_event_is_not_dropped() {
        let mut app = test_app();
        app.apply(Delta::Text { text: "orphan".to_owned() });
        assert_eq!(app.entries[0].text, "orphan");
    }

    #[test]
    fn completing_a_turn_accumulates_usage_and_cost() {
        let mut app = test_app();
        app.apply(Delta::Started { model: "test/model".to_owned() });
        app.apply(Delta::Completed {
            stop_reason: StopReason::EndTurn,
            usage: Usage { input_tokens: 1_000, output_tokens: 1_000, ..Usage::default() },
        });

        assert_eq!(app.status, Status::Idle);
        assert_eq!(app.usage.output_tokens, 1_000);
        assert!(app.cost.usd > 0.0);
    }

    #[test]
    fn exceeding_the_budget_stops_the_session_instead_of_spending_on() {
        let mut app = test_app();
        app.apply(Delta::Completed {
            stop_reason: StopReason::EndTurn,
            usage: Usage { output_tokens: 1_000_000, ..Usage::default() },
        });

        assert_eq!(app.status, Status::BudgetExhausted);
        assert!(app.notice.is_some());
        assert!(app.submit().is_none(), "no further turn may be started");
    }

    #[test]
    fn truncated_answers_are_flagged_to_the_user() {
        let mut app = test_app();
        app.apply(Delta::Completed { stop_reason: StopReason::MaxTokens, usage: Usage::default() });
        assert!(app.notice.expect("a notice is set").contains("cut off"));
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
        assert_eq!(app.entries[0].role, Role::User);
    }

    #[test]
    fn submit_is_refused_while_a_turn_is_in_flight() {
        let mut app = test_app();
        app.status = Status::Streaming;
        app.input = "second prompt".to_owned();
        assert!(app.submit().is_none());
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
    fn aborting_only_affects_a_running_turn() {
        let mut app = test_app();
        app.abort();
        assert!(app.notice.is_none(), "nothing was running");

        app.status = Status::Streaming;
        app.abort();
        assert_eq!(app.status, Status::Idle);
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
