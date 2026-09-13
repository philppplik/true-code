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

use tc_agent::{Effect, Risk};

use crate::app::{App, Entry, Status, ToolState};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};

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

    // Drawn last so it sits above everything: a confirmation that can be missed
    // behind other content is not a confirmation.
    if let Some(pending) = &app.pending {
        draw_approval(frame, frame.area(), pending);
    }
}

/// Share of the screen the confirmation modal occupies, in percent.
const MODAL_WIDTH_PERCENT: u16 = 86;
/// Share of the screen height the confirmation modal occupies, in percent.
const MODAL_HEIGHT_PERCENT: u16 = 78;

/// Draws the change-confirmation modal.
fn draw_approval(frame: &mut Frame, area: Rect, pending: &crate::app::PendingApproval) {
    let modal = centred(area, MODAL_WIDTH_PERCENT, MODAL_HEIGHT_PERCENT);
    frame.render_widget(Clear, modal);

    let (title, mut body, mut accent) = match &pending.request.effect {
        Effect::Write(diff) => (format!(" {} ", diff.summary()), diff_lines(&diff.text), ACCENT),
        Effect::Execute { command, risk } => {
            let mut lines = vec![
                Line::raw(""),
                Line::from(Span::styled(
                    format!("  $ {command}"),
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                )),
                Line::raw(""),
            ];
            let accent = match risk {
                Risk::Normal => ACCENT,
                Risk::High { reason } => {
                    lines.push(Line::from(Span::styled(
                        format!("  ! This {reason}."),
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    )));
                    lines.push(Line::raw(""));
                    Color::Red
                }
            };
            (" run a command ".to_owned(), lines, accent)
        }
        // Never shown: read-only effects are not sent for approval.
        Effect::ReadOnly => (" no change ".to_owned(), Vec::new(), MUTED),
    };

    // Broken rules go at the very top, above the diff, and turn the whole modal
    // red. A warning below a forty-line diff is a warning nobody reads.
    if pending.request.breaks_a_rule() {
        accent = Color::Red;
        let mut header = vec![Line::from(Span::styled(
            "  This breaks rules you set:",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ))];
        for violation in &pending.request.violations {
            header.push(Line::from(Span::styled(
                format!("    ! {}", violation.summary()),
                Style::default().fg(Color::Red),
            )));
        }
        header.push(Line::raw(""));
        header.append(&mut body);
        body = header;
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent))
        .title(Span::styled(title, Style::default().fg(accent).add_modifier(Modifier::BOLD)))
        .title_bottom(Line::from(Span::styled(
            " [y] apply  ·  [n] skip  ·  [a] always for this tool  ·  [Esc] stop ",
            Style::default().fg(Color::White),
        )));

    frame.render_widget(Paragraph::new(body).block(block).scroll((pending.scroll, 0)), modal);
}

/// Colours a unified diff line by line.
fn diff_lines(text: &str) -> Vec<Line<'static>> {
    text.lines()
        .map(|line| {
            // Colour *and* the leading sign, so the diff still reads correctly
            // without colour.
            let colour = match line.chars().next() {
                Some('+') => Color::Green,
                Some('-') => Color::Red,
                Some('…') => MUTED,
                _ => Color::Gray,
            };
            Line::from(Span::styled(format!(" {line}"), Style::default().fg(colour)))
        })
        .collect()
}

/// Centres a rectangle inside `area`.
fn centred(area: Rect, width_percent: u16, height_percent: u16) -> Rect {
    let [_, middle, _] = Layout::vertical([
        Constraint::Percentage((100 - height_percent) / 2),
        Constraint::Percentage(height_percent),
        Constraint::Percentage((100 - height_percent) / 2),
    ])
    .areas(area);

    let [_, centre, _] = Layout::horizontal([
        Constraint::Percentage((100 - width_percent) / 2),
        Constraint::Percentage(width_percent),
        Constraint::Percentage((100 - width_percent) / 2),
    ])
    .areas(middle);

    centre
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
        Span::styled(" · ", Style::default().fg(MUTED)),
        Span::styled(app.mode.label(), Style::default().fg(mode_colour(app.mode))),
        Span::styled(rules_label(app.rules), Style::default().fg(MUTED)),
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
        Status::Working => "working… (Esc aborts)",
        Status::BudgetExhausted => "budget reached",
    }
}

/// How the active rule count is shown, or nothing when there are none.
fn rules_label(rules: usize) -> String {
    match rules {
        0 => String::new(),
        1 => " · 1 rule".to_owned(),
        many => format!(" · {many} rules"),
    }
}

/// Colour for a permission mode.
///
/// Read-only is the safe default and stays quiet; anything that can change the
/// project is coloured so the mode is never a surprise.
const fn mode_colour(mode: tc_agent::PermissionMode) -> Color {
    match mode {
        tc_agent::PermissionMode::ReadOnly => MUTED,
        tc_agent::PermissionMode::Write => WARN,
        tc_agent::PermissionMode::Full => Color::Red,
    }
}

/// Colour for a status.
const fn status_colour(status: Status) -> Color {
    match status {
        Status::Idle => Color::Green,
        Status::Working => ACCENT,
        Status::BudgetExhausted => Color::Red,
    }
}

/// Turns one transcript entry into renderable lines.
fn entry_lines(entry: &Entry) -> Vec<Line<'static>> {
    match entry {
        Entry::User(text) => speech("you ", Color::White, text.clone()),
        Entry::Assistant(text) => speech("tc  ", ACCENT, text.clone()),
        Entry::Tool { tool, summary, state } => vec![
            Line::from(vec![
                Span::styled("    ", Style::default()),
                Span::styled(tool_marker(*state), Style::default().fg(tool_colour(*state))),
                Span::styled(format!(" {tool}"), Style::default().fg(tool_colour(*state))),
                Span::styled(format!(" {summary}"), Style::default().fg(MUTED)),
            ]),
            Line::raw(""),
        ],
    }
}

/// Renders a spoken line with its gutter.
fn speech(gutter: &'static str, colour: Color, text: String) -> Vec<Line<'static>> {
    vec![
        Line::from(vec![
            Span::styled(gutter, Style::default().fg(colour).add_modifier(Modifier::BOLD)),
            Span::styled("› ", Style::default().fg(MUTED)),
            Span::raw(text),
        ]),
        Line::raw(""),
    ]
}

/// Symbol for a tool's state.
///
/// A shape, not just a colour: the state has to survive a monochrome terminal and
/// a colour-blind reader.
const fn tool_marker(state: ToolState) -> &'static str {
    match state {
        ToolState::Running => "◌",
        ToolState::Done => "●",
        ToolState::Failed => "✗",
    }
}

/// Colour for a tool's state.
const fn tool_colour(state: ToolState) -> Color {
    match state {
        ToolState::Running => WARN,
        ToolState::Done => Color::Green,
        ToolState::Failed => Color::Red,
    }
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
        assert!(status_label(Status::Working).contains("Esc"));
        assert_eq!(status_label(Status::Idle), "ready");
    }
}
