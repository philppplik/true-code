//! Headless mode: one prompt in, one answer out.
//!
//! Stream separation is the contract here. The answer goes to **stdout** so that
//! `true-code -p "…" > answer.md` produces exactly the answer. The accounting goes
//! to **stderr**, so it is visible in a terminal but never contaminates a pipe.

use std::io::Write as _;

use futures::StreamExt as _;
use tc_config::Config;
use tc_core::{Delta, Message, StopReason, Usage};
use tc_providers::{Provider, Request};

/// Runs a single turn and prints the answer.
pub fn run(provider: Box<dyn Provider>, config: &Config, prompt: &str) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    runtime.block_on(run_async(provider, config, prompt))
}

/// Async body of [`run`].
async fn run_async(
    provider: Box<dyn Provider>,
    config: &Config,
    prompt: &str,
) -> anyhow::Result<()> {
    let request = Request::new(config.system_prompt.clone(), vec![Message::user(prompt)]);
    let mut stream = provider.stream(request).await?;

    let mut stdout = std::io::stdout().lock();
    let mut usage = Usage::default();
    let mut stop_reason = StopReason::EndTurn;

    while let Some(delta) = stream.next().await {
        match delta? {
            Delta::Started { .. } => {}
            Delta::Text { text } => {
                // Flushed per chunk so a piped consumer sees output as it arrives
                // instead of waiting for the whole turn.
                write!(stdout, "{text}")?;
                stdout.flush()?;
            }
            Delta::Completed { stop_reason: reason, usage: turn_usage } => {
                stop_reason = reason;
                usage = turn_usage;
            }
        }
    }
    writeln!(stdout)?;

    report(provider.as_ref(), usage, stop_reason, config);
    Ok(())
}

/// Writes the accounting summary to stderr.
fn report(provider: &dyn Provider, usage: Usage, stop_reason: StopReason, config: &Config) {
    let cost = provider.price().cost_of(usage);

    let cache = usage
        .cache_hit_rate()
        .map_or_else(|| "n/a".to_owned(), |rate| format!("{:.0} %", rate * 100.0));

    eprintln!(
        "— {} · in {} / out {} · cache {} · {}",
        provider.id(),
        usage.total_input(),
        usage.output_tokens,
        cache,
        cost.display(),
    );

    if stop_reason == StopReason::MaxTokens {
        eprintln!("— warning: the answer was cut off at the output limit.");
    }
    if config.budget.is_exhausted(cost.usd) {
        eprintln!(
            "— warning: this single turn already reached the ${:.2} session budget.",
            config.budget.session_limit_usd
        );
    }
}
