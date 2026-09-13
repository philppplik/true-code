//! First-run setup: pick a provider, paste a key.
//!
//! Without this, the first run of true-code fails with a message telling you to
//! set an environment variable — which is a fine instruction for someone who
//! already knows what that means, and a dead end for everyone else. Being useful
//! to people who are not fluent in terminals is a stated goal of this project, so
//! it cannot be gated behind knowing how to make a variable persist on Windows.
//!
//! The key goes into the **OS keyring**, never into a file true-code wrote. That
//! commitment is in `SECURITY.md` and this screen does not get to break it.
//!
//! # Typing a secret in a TUI
//!
//! The input is masked, and the key is held in a `String` that lives only as long
//! as this screen. It is never rendered, logged, or put in an event. What the OS
//! keyring does with it afterwards is the OS's business, which is the point of
//! using it rather than inventing storage.

use std::io::{Stdout, stdout};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use tc_config::Vendor;

/// What the user settled on.
#[derive(Debug)]
pub struct Setup {
    /// The provider they chose.
    pub vendor: &'static Vendor,
    /// The key they entered.
    pub key: String,
}

/// Which half of the screen has focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    /// Choosing a provider.
    Choosing,
    /// Typing the key.
    Entering,
}

/// Runs the setup screen.
///
/// Returns `None` when the user backed out, which is a decision and not an error.
pub fn run(vendors: &'static [Vendor]) -> anyhow::Result<Option<Setup>> {
    let mut terminal = Guard::enter()?;

    let mut selected = 0usize;
    let mut key = String::new();
    let mut stage = Stage::Choosing;

    loop {
        terminal.inner.draw(|frame| draw(frame, vendors, selected, &key, stage))?;

        let Event::Key(pressed) = event::read()? else {
            continue;
        };
        if pressed.kind != KeyEventKind::Press {
            continue;
        }

        match (stage, pressed.code) {
            (_, KeyCode::Esc) => return Ok(None),

            (Stage::Choosing, KeyCode::Up) => selected = selected.saturating_sub(1),
            (Stage::Choosing, KeyCode::Down) => {
                selected = (selected + 1).min(vendors.len().saturating_sub(1));
            }
            (Stage::Choosing, KeyCode::Enter) => stage = Stage::Entering,

            // Back to the list rather than out of the screen: choosing the wrong
            // provider is the likeliest mistake here, and it should cost one key.
            (Stage::Entering, KeyCode::Backspace) if key.is_empty() => stage = Stage::Choosing,
            (Stage::Entering, KeyCode::Backspace) => {
                key.pop();
            }
            (Stage::Entering, KeyCode::Char(ch)) if !ch.is_control() => key.push(ch),
            (Stage::Entering, KeyCode::Enter) if !key.trim().is_empty() => {
                return Ok(Some(Setup { vendor: &vendors[selected], key: key.trim().to_owned() }));
            }
            _ => {}
        }
    }
}

/// Draws the screen.
fn draw(frame: &mut ratatui::Frame, vendors: &[Vendor], selected: usize, key: &str, stage: Stage) {
    let [header, list, entry, footer] = Layout::vertical([
        Constraint::Length(4),
        Constraint::Min(5),
        Constraint::Length(3),
        Constraint::Length(2),
    ])
    .areas(frame.area());

    frame.render_widget(
        Paragraph::new(vec![
            Line::raw(""),
            Line::from(Span::styled(
                "  Welcome to true-code",
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "  Choose where the model should come from, then paste your key.",
                Style::default().fg(Color::Gray),
            )),
        ]),
        header,
    );

    let choices: Vec<Line<'_>> = vendors
        .iter()
        .enumerate()
        .flat_map(|(index, vendor)| {
            let chosen = index == selected;
            let colour = if chosen { Color::Cyan } else { Color::White };
            vec![
                Line::from(vec![
                    Span::styled(
                        if chosen { "  ▸ " } else { "    " },
                        Style::default().fg(Color::Cyan),
                    ),
                    Span::styled(
                        vendor.name,
                        Style::default().fg(colour).add_modifier(if chosen {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                    ),
                ]),
                Line::from(Span::styled(
                    format!("      key from {}", vendor.signup_url),
                    Style::default().fg(Color::DarkGray),
                )),
            ]
        })
        .collect();

    frame.render_widget(
        Paragraph::new(choices).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(if stage == Stage::Choosing {
                    Color::Cyan
                } else {
                    Color::DarkGray
                }))
                .title(" provider "),
        ),
        list,
    );

    // Masked: a key typed here would otherwise sit in the terminal's scrollback
    // for anyone who scrolls up, including in a screen recording.
    let masked = "•".repeat(key.chars().count());
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" › ", Style::default().fg(Color::Cyan)),
            Span::raw(masked),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(if stage == Stage::Entering {
                    Color::Cyan
                } else {
                    Color::DarkGray
                }))
                .title(format!(" {} API key ", vendors[selected].name)),
        ),
        entry,
    );

    let hint = match stage {
        Stage::Choosing => "  ↑/↓ choose · Enter continue · Esc cancel",
        Stage::Entering => "  Paste the key, then Enter · Backspace goes back · Esc cancel",
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(hint, Style::default().fg(Color::DarkGray)))),
        footer,
    );
}

/// Owns the terminal's global state and restores it on drop.
struct Guard {
    inner: Terminal<CrosstermBackend<Stdout>>,
}

impl Guard {
    fn enter() -> anyhow::Result<Self> {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore();
            previous(info);
        }));

        enable_raw_mode()?;
        execute!(stdout(), EnterAlternateScreen)?;
        Ok(Self { inner: Terminal::new(CrosstermBackend::new(stdout()))? })
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        restore();
    }
}

/// Restores the terminal. Errors are ignored: this runs during shutdown and on
/// panic, where reporting one would hide the original problem.
fn restore() {
    let _ = disable_raw_mode();
    let _ = execute!(stdout(), LeaveAlternateScreen);
}
