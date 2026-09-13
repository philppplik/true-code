//! Rendering.
//!
//! Layout, top to bottom:
//!
//! ```text
//! ┌ true-code · CODE · anthropic/claude-sonnet-4-5 ····· 12.4k/200k ▓▓▓░░ $0.31 ┐
//! │ you  › refactor the auth module                                             │
//! │ tc   › I changed the error handling in three places…                        │
//! ├─────────────────────────────────────────────────────────────────────────────┤
//! │ › refactor the auth module▏                                                 │
//! └ Enter send · Esc abort · Ctrl+C quit · PgUp/PgDn scroll ────────────────────┘
//! ```
//!
//! Accessibility rules that are not negotiable here: no blinking, never colour as
//! the only carrier of meaning (roles also have text gutters), and every state is
//! legible on a monochrome terminal.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};
use tc_core::Role;

use crate::app::{App, Status};

/// Accent colour of the brand.
const ACCENT: Color = Color::Cyan;
/// Colour used for secondary, non-essential information.
const MUTED: Color = Color::DarkGray;
/// Colour used for warnings and aborts.
const WARN: Color = Color::Yellow;

/// Number of segments in the context gauge.
const GAUGE_WIDTH: usize = 10;

/// Width of one gauge segment as a fraction of the whole.
const GAUGE_STEP: f64 = 1.0 / 10.0;

/// Draws the whole frame.
pub fn draw(frame: &mut Frame, app: &App) {
    let [transcript_area, input_area] =
        Layout::vertical([Constraint::Min(3), Constraint::Length(3)]).areas(frame.area());

    draw_transcript(frame, transcript_area, app);
    draw_input(frame, input_area, app);
}

/// Draws the transcript, framed by the status bar.
fn draw_transcript(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(MUTED))
        .title(status_line(app))
        .title_bottom(context_line(app));

    let lines: Vec<Line<'_>> = if app.entries.is_empty() {
        welcome_lines()
    } else {
        app.entries.iter().flat_map(entry_lines).collect()
    };

    let paragraph =
        Paragraph::new(lines).block(block).wrap(Wrap { trim: false }).scroll((app.scroll, 0));

    frame.render_widget(paragraph, area);
}

/// Builds the status bar shown in the top border.
fn status_line(app: &App) -> Line<'static> {
    Line::from(vec![
        Span::styled(" true-code ", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Span::styled("· ", Style::default().fg(MUTED)),
        Span::styled(app.model.clone(), Style::default().fg(Color::White)),
        Span::styled(" · ", Style::default().fg(MUTED)),
        Span::styled(status_label(app.status), Style::default().fg(status_colour(app.status))),
        Span::raw(" "),
    ])
}

/// Builds the accounting line shown in the bottom border of the transcript.
fn context_line(app: &App) -> Line<'static> {
    let fill = app.context_fill();
    Line::from(vec![
        Span::styled(" ctx ", Style::default().fg(MUTED)),
        Span::styled(gauge(fill), Style::default().fg(gauge_colour(fill))),
        Span::styled(
            format!(" {} ", format_tokens(app.usage.total_input())),
            Style::default().fg(MUTED),
        ),
        Span::styled("· ", Style::default().fg(MUTED)),
        Span::styled(app.cost.display(), Style::default().fg(cost_colour(app))),
        Span::styled(format!("/{:.2} ", app.budget.session_limit_usd), Style::default().fg(MUTED)),
    ])
}

/// Renders a fill ratio as a text gauge.
///
/// Text rather than colour blocks, so the state survives a monochrome terminal.
/// Built by comparing against segment thresholds rather than casting a scaled
/// float to an index — no rounding mode to get wrong, no truncation to guard.
fn gauge(fill: f64) -> String {
    let clamped = fill.clamp(0.0, 1.0);
    let mut bar = String::with_capacity(GAUGE_WIDTH * '▓'.len_utf8());
    let mut threshold = 0.0_f64;

    for _ in 0..GAUGE_WIDTH {
        threshold += GAUGE_STEP;
        bar.push(if clamped >= threshold - GAUGE_STEP / 2.0 { '▓' } else { '░' });
    }
    bar
}

/// Formats a token count compactly, e.g. `12.4k`.
fn format_tokens(tokens: u32) -> String {
    if tokens < 1_000 {
        return format!("{tokens}");
    }
    format!("{:.1}k", f64::from(tokens) / 1_000.0)
}

/// Colour of the context gauge — green while healthy, warning as it fills.
fn gauge_colour(fill: f64) -> Color {
    if fill >= 0.7 { WARN } else { Color::Green }
}

/// Colour of the cost readout, driven by the configured budget.
fn cost_colour(app: &App) -> Color {
    if app.budget.is_exhausted(app.cost.usd) {
        Color::Red
    } else if app.budget.should_warn(app.cost.usd) {
        WARN
    } else {
        Color::White
    }
}

/// Human-readable label for a status.
const fn status_label(status: Status) -> &'static str {
    match status {
        Status::Idle => "ready",
        Status::Streaming => "thinking… (Esc aborts)",
        Status::BudgetExhausted => "budget reached",
    }
}

/// Colour for a status.
const fn status_colour(status: Status) -> Color {
    match status {
        Status::Idle => Color::Green,
        Status::Streaming => ACCENT,
        Status::BudgetExhausted => Color::Red,
    }
}

/// Turns one transcript entry into renderable lines.
fn entry_lines(entry: &crate::app::Entry) -> Vec<Line<'static>> {
    let (gutter, colour) = match entry.role {
        Role::User => ("you ", Color::White),
        Role::Assistant => ("tc  ", ACCENT),
        Role::System => ("sys ", MUTED),
    };

    let mut lines = vec![Line::from(vec![
        Span::styled(gutter, Style::default().fg(colour).add_modifier(Modifier::BOLD)),
        Span::styled("› ", Style::default().fg(MUTED)),
        Span::raw(entry.text.clone()),
    ])];
    lines.push(Line::raw(""));
    lines
}

/// First-run guidance, shown while the transcript is empty.
fn welcome_lines() -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled("  Ask a question to start.", Style::default().fg(Color::White))),
        Line::raw(""),
        Line::from(Span::styled(
            "  Every turn shows what it cost. Esc aborts mid-answer, nothing is",
            Style::default().fg(MUTED),
        )),
        Line::from(Span::styled(
            "  written to your project without you seeing it first.",
            Style::default().fg(MUTED),
        )),
    ]
}

/// Draws the input box and the key hints.
fn draw_input(frame: &mut Frame, area: Rect, app: &App) {
    let hint = app
        .notice
        .clone()
        .unwrap_or_else(|| " Enter send · Esc abort · Ctrl+C quit · PgUp/PgDn scroll ".to_owned());
    let hint_colour = if app.notice.is_some() { WARN } else { MUTED };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if app.status == Status::Idle { ACCENT } else { MUTED }))
        .title_bottom(Line::from(Span::styled(hint, Style::default().fg(hint_colour))));

    let input = Paragraph::new(Line::from(vec![
        Span::styled("› ", Style::default().fg(ACCENT)),
        Span::raw(app.input.clone()),
    ]))
    .block(block);

    frame.render_widget(input, area);

    // Place the real terminal cursor so screen readers and the terminal agree
    // with what the user sees.
    let cursor_column =
        area.x + 3 + u16::try_from(app.input[..app.cursor].chars().count()).unwrap_or(0);
    frame.set_cursor_position((cursor_column.min(area.x + area.width - 2), area.y + 1));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_gauge_is_all_empty_segments() {
        assert_eq!(gauge(0.0), "░".repeat(GAUGE_WIDTH));
    }

    #[test]
    fn a_full_gauge_is_all_filled_segments() {
        assert_eq!(gauge(1.0), "▓".repeat(GAUGE_WIDTH));
    }

    #[test]
    fn gauge_values_outside_the_range_are_clamped() {
        assert_eq!(gauge(5.0), "▓".repeat(GAUGE_WIDTH));
        assert_eq!(gauge(-1.0), "░".repeat(GAUGE_WIDTH));
    }

    #[test]
    fn the_gauge_warns_once_the_context_is_mostly_full() {
        assert_eq!(gauge_colour(0.69), Color::Green);
        assert_eq!(gauge_colour(0.70), WARN);
    }

    #[test]
    fn token_counts_stay_readable_at_every_magnitude() {
        assert_eq!(format_tokens(0), "0");
        assert_eq!(format_tokens(999), "999");
        assert_eq!(format_tokens(12_400), "12.4k");
    }

    #[test]
    fn every_status_has_a_label_that_names_its_exit() {
        assert!(status_label(Status::Streaming).contains("Esc"));
        assert_eq!(status_label(Status::Idle), "ready");
    }
}
