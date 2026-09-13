//! Headless mode: one prompt in, one answer out.
//!
//! Stream separation is the contract here. The answer goes to **stdout** so that
//! `truecode -p "…" > answer.md` produces exactly the answer. Everything else —
//! tool activity, accounting, warnings — goes to **stderr**, so it is visible in a
//! terminal but never contaminates a pipe.

use std::io::Write as _;

use tc_agent::{Agent, AgentEvent, FinishReason};
use tc_core::{Cost, Usage};
use tokio::sync::mpsc;

/// Exit code used when a guard rail stopped the run before it finished.
///
/// Distinct from success so a CI step can tell "answered" from "gave up".
pub const EXIT_INCOMPLETE: i32 = 2;

/// Runs a single prompt and prints the answer. Returns the process exit code.
pub fn run(agent: Agent, prompt: &str) -> anyhow::Result<i32> {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    runtime.block_on(run_async(agent, prompt))
}

/// Async body of [`run`].
async fn run_async(mut agent: Agent, prompt: &str) -> anyhow::Result<i32> {
    let (tx, mut rx) = mpsc::channel::<AgentEvent>(256);

    // The agent runs in its own task so that printing can keep pace with the
    // stream instead of stalling it. The prompt is owned by the task because a
    // spawned task outlives this function's borrows.
    let owned_prompt = prompt.to_owned();
    let worker = tokio::spawn(async move {
        agent.run(&owned_prompt, &tx).await;
        agent
    });

    let mut stdout = std::io::stdout().lock();
    let mut usage = Usage::default();
    let mut cost = Cost::default();
    let mut exit = 0;

    while let Some(event) = rx.recv().await {
        match event {
            AgentEvent::Started { .. } => {}

            AgentEvent::Text { text } => {
                // Flushed per chunk so a piped consumer sees output as it arrives
                // instead of waiting for the whole run.
                write!(stdout, "{text}")?;
                stdout.flush()?;
            }

            AgentEvent::ToolStarted { tool, summary } => {
                eprintln!("· {tool} {summary}");
            }

            AgentEvent::ToolFinished { tool, is_error } => {
                if is_error {
                    eprintln!("· {tool} failed");
                }
            }

            AgentEvent::ToolDeclined { tool, summary } => {
                // Without --yes there is nobody to ask, so this is the expected
                // outcome rather than a fault — but it must be loud, or a script
                // will report success for a change that never happened.
                eprintln!("· {tool} declined: {summary} (pass --yes to allow changes)");
                exit = EXIT_INCOMPLETE;
            }

            AgentEvent::TurnCompleted { usage: turn, cost: turn_cost } => {
                usage = usage.saturating_add(turn);
                cost = cost.add(turn_cost);
            }

            AgentEvent::Finished { reason } => {
                if reason != FinishReason::Completed {
                    eprintln!("— {}", reason.message());
                    exit = EXIT_INCOMPLETE;
                }
            }

            // Goes to stderr with everything else that is not the answer, so a
            // piped run still yields exactly the answer.
            AgentEvent::Notice { text } => eprintln!("· {text}"),

            AgentEvent::Failed { message } => {
                eprintln!("— failed: {message}");
                exit = 1;
            }
        }
    }
    writeln!(stdout)?;

    let agent = worker.await?;
    report(&agent, usage, cost);
    Ok(exit)
}

/// Writes the accounting summary to stderr.
fn report(agent: &Agent, usage: Usage, cost: Cost) {
    let cache = usage
        .cache_hit_rate()
        .map_or_else(|| "n/a".to_owned(), |rate| format!("{:.0} %", rate * 100.0));

    eprintln!(
        "— {} · in {} / out {} · cache {} · {}",
        agent.model_id(),
        usage.total_input(),
        usage.output_tokens,
        cache,
        cost.display(),
    );

    if let Some(path) = agent.log().path() {
        eprintln!("— session: {}", path.display());
    }
}
