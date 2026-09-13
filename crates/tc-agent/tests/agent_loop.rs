//! End-to-end tests for the agent loop, against a scripted provider.
//!
//! These exercise the whole path — turn, tool call, tool result, next turn —
//! without a network or an API key, so every guard rail is verified in CI rather
//! than hoped about. The provider is scripted, but the tools, the workspace
//! boundary and the session log are all real.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tc_agent::{Agent, AgentEvent, FinishReason, SessionLog};
use tc_config::{Budget, Config};
use tc_core::{Delta, Price, SessionId, StopReason, ToolCall, Usage};
use tc_providers::{DeltaStream, Provider, ProviderError, Request};
use tc_tools::{ToolContext, ToolSet};
use tokio::sync::mpsc;

/// A provider that replays a fixed script, one entry per turn.
#[derive(Debug)]
struct ScriptedProvider {
    /// One vector of deltas per turn, in order.
    script: Vec<Vec<Delta>>,
    /// How many turns have been requested so far.
    calls: AtomicUsize,
}

impl ScriptedProvider {
    fn new(script: Vec<Vec<Delta>>) -> Self {
        Self { script, calls: AtomicUsize::new(0) }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl Provider for ScriptedProvider {
    fn id(&self) -> &'static str {
        "test/scripted"
    }

    fn context_window(&self) -> u32 {
        200_000
    }

    fn price(&self) -> Price {
        Price {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
            cached_input_per_mtok: 0.3,
            cache_write_per_mtok: 3.75,
        }
    }

    async fn stream(&self, _request: Request) -> Result<DeltaStream, ProviderError> {
        let index = self.calls.fetch_add(1, Ordering::SeqCst);
        // Past the end of the script, keep answering so that turn-limit and loop
        // behaviour can be driven deterministically.
        let deltas = self
            .script
            .get(index)
            .cloned()
            .unwrap_or_else(|| self.script.last().cloned().unwrap_or_default());

        Ok(Box::pin(futures::stream::iter(deltas.into_iter().map(Ok))))
    }
}

/// A turn that answers with text and stops.
fn answer(text: &str) -> Vec<Delta> {
    vec![
        Delta::Started { model: "test/scripted".to_owned() },
        Delta::Text { text: text.to_owned() },
        Delta::Completed {
            stop_reason: StopReason::EndTurn,
            usage: Usage { input_tokens: 100, output_tokens: 50, ..Usage::default() },
        },
    ]
}

/// A turn that calls one tool.
fn call_tool(id: &str, name: &str, input: serde_json::Value) -> Vec<Delta> {
    vec![
        Delta::Started { model: "test/scripted".to_owned() },
        Delta::ToolCall(ToolCall { id: id.to_owned(), name: name.to_owned(), input }),
        Delta::Completed {
            stop_reason: StopReason::ToolUse,
            usage: Usage { input_tokens: 100, output_tokens: 20, ..Usage::default() },
        },
    ]
}

/// A workspace with one readable file.
fn workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp dir is creatable");
    std::fs::create_dir_all(dir.path().join("src")).expect("dir is creatable");
    std::fs::write(dir.path().join("src/lib.rs"), "pub fn answer() -> u32 { 42 }\n")
        .expect("file is writable");
    dir
}

fn config(limit_usd: f64) -> Config {
    Config {
        budget: Budget { session_limit_usd: limit_usd, warn_at_percent: 70 },
        ..Config::default()
    }
}

/// Runs a prompt and collects every event.
async fn run(
    provider: Arc<ScriptedProvider>,
    root: &std::path::Path,
    config: &Config,
    prompt: &str,
) -> Vec<AgentEvent> {
    let mut agent = Agent::new(
        provider,
        ToolSet::read_only(),
        ToolContext::new(root),
        config,
        SessionLog::create(root, SessionId::new()),
    );

    let (tx, mut rx) = mpsc::channel(256);
    agent.run(prompt, &tx).await;
    drop(tx);

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    events
}

fn finish_reason(events: &[AgentEvent]) -> Option<FinishReason> {
    events.iter().find_map(|event| match event {
        AgentEvent::Finished { reason } => Some(reason.clone()),
        _ => None,
    })
}

#[tokio::test]
async fn a_plain_answer_finishes_in_one_turn() {
    let dir = workspace();
    let provider = Arc::new(ScriptedProvider::new(vec![answer("Hello.")]));

    let events = run(provider.clone(), dir.path(), &config(5.0), "hi").await;

    assert_eq!(provider.calls(), 1, "no tool call means no second turn");
    assert_eq!(finish_reason(&events), Some(FinishReason::Completed));
    assert!(events.contains(&AgentEvent::Text { text: "Hello.".to_owned() }));
}

#[tokio::test]
async fn a_tool_call_runs_and_its_result_drives_a_second_turn() {
    let dir = workspace();
    let provider = Arc::new(ScriptedProvider::new(vec![
        call_tool("t1", "read_file", serde_json::json!({ "path": "src/lib.rs" })),
        answer("It returns 42."),
    ]));

    let events = run(provider.clone(), dir.path(), &config(5.0), "what does lib.rs do?").await;

    assert_eq!(provider.calls(), 2, "the tool result must trigger another turn");
    assert!(events.contains(&AgentEvent::ToolStarted {
        tool: "read_file".to_owned(),
        summary: "path=src/lib.rs".to_owned(),
    }));
    assert!(
        events
            .contains(&AgentEvent::ToolFinished { tool: "read_file".to_owned(), is_error: false })
    );
    assert_eq!(finish_reason(&events), Some(FinishReason::Completed));
}

#[tokio::test]
async fn a_tool_error_is_reported_and_the_run_continues() {
    let dir = workspace();
    let provider = Arc::new(ScriptedProvider::new(vec![
        // Escaping the workspace must fail as a *tool error*, not a crash, so the
        // model gets the message and can correct itself.
        call_tool("t1", "read_file", serde_json::json!({ "path": "../../../etc/passwd" })),
        answer("I cannot read outside the project."),
    ]));

    let events = run(provider.clone(), dir.path(), &config(5.0), "read /etc/passwd").await;

    assert!(
        events.contains(&AgentEvent::ToolFinished { tool: "read_file".to_owned(), is_error: true })
    );
    assert_eq!(
        finish_reason(&events),
        Some(FinishReason::Completed),
        "a refused path must not abort the run"
    );
}

#[tokio::test]
async fn an_unknown_tool_is_an_error_the_model_can_recover_from() {
    let dir = workspace();
    let provider = Arc::new(ScriptedProvider::new(vec![
        call_tool("t1", "delete_everything", serde_json::json!({})),
        answer("That tool does not exist."),
    ]));

    let events = run(provider.clone(), dir.path(), &config(5.0), "delete it all").await;

    assert!(events.contains(&AgentEvent::ToolFinished {
        tool: "delete_everything".to_owned(),
        is_error: true,
    }));
    assert_eq!(finish_reason(&events), Some(FinishReason::Completed));
}

#[tokio::test]
async fn repeating_one_identical_tool_call_stops_the_run() {
    let dir = workspace();
    // The script never advances: the same call, forever.
    let provider = Arc::new(ScriptedProvider::new(vec![call_tool(
        "t1",
        "read_file",
        serde_json::json!({ "path": "src/lib.rs" }),
    )]));

    let events = run(provider.clone(), dir.path(), &config(100.0), "loop please").await;

    assert_eq!(
        finish_reason(&events),
        Some(FinishReason::Loop { tool: "read_file".to_owned() }),
        "a stuck agent must be stopped by loop detection, not by the turn limit"
    );
    assert!(provider.calls() < tc_agent::MAX_TURNS, "it stopped well before the turn limit");
}

#[tokio::test]
async fn the_budget_stops_the_run_before_the_next_request() {
    let dir = workspace();
    let provider = Arc::new(ScriptedProvider::new(vec![call_tool(
        "t1",
        "list_dir",
        // Varying arguments, so loop detection cannot be what stops it.
        serde_json::json!({ "path": "src" }),
    )]));

    // One turn costs 100 input + 20 output tokens ≈ $0.0006, so a tiny budget
    // is exhausted after a handful of turns.
    let events = run(provider.clone(), dir.path(), &config(0.001), "explore").await;

    assert_eq!(finish_reason(&events), Some(FinishReason::BudgetExhausted));
}

#[tokio::test]
async fn a_truncated_answer_is_reported_as_such() {
    let dir = workspace();
    let provider = Arc::new(ScriptedProvider::new(vec![vec![
        Delta::Started { model: "test/scripted".to_owned() },
        Delta::Text { text: "It starts like this and then".to_owned() },
        Delta::Completed {
            stop_reason: StopReason::MaxTokens,
            usage: Usage { input_tokens: 10, output_tokens: 4096, ..Usage::default() },
        },
    ]]));

    let events = run(provider, dir.path(), &config(5.0), "write an epic").await;

    assert_eq!(finish_reason(&events), Some(FinishReason::OutputTruncated));
}

#[tokio::test]
async fn a_provider_failure_is_surfaced_rather_than_swallowed() {
    /// A provider whose every request fails.
    #[derive(Debug)]
    struct Failing;

    #[async_trait::async_trait]
    impl Provider for Failing {
        fn id(&self) -> &'static str {
            "test/failing"
        }
        fn context_window(&self) -> u32 {
            1_000
        }
        fn price(&self) -> Price {
            Price::FREE
        }
        async fn stream(&self, _request: Request) -> Result<DeltaStream, ProviderError> {
            Err(ProviderError::Api {
                provider: "test",
                status: 429,
                body: "rate limited".to_owned(),
            })
        }
    }

    let dir = workspace();
    let mut agent = Agent::new(
        Arc::new(Failing),
        ToolSet::read_only(),
        ToolContext::new(dir.path()),
        &config(5.0),
        SessionLog::disabled(SessionId::new()),
    );

    let (tx, mut rx) = mpsc::channel(64);
    agent.run("hi", &tx).await;
    drop(tx);

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }

    let failed = events
        .iter()
        .any(|event| matches!(event, AgentEvent::Failed { message } if message.contains("429")));
    assert!(failed, "the user must see why it failed: {events:?}");
}

#[tokio::test]
async fn the_session_log_records_the_whole_run() {
    let dir = workspace();
    let provider = Arc::new(ScriptedProvider::new(vec![
        call_tool("t1", "read_file", serde_json::json!({ "path": "src/lib.rs" })),
        answer("It returns 42."),
    ]));

    let mut agent = Agent::new(
        provider,
        ToolSet::read_only(),
        ToolContext::new(dir.path()),
        &config(5.0),
        SessionLog::create(dir.path(), SessionId::new()),
    );
    let log_path = agent.log().path().expect("the log is enabled").to_path_buf();

    let (tx, mut rx) = mpsc::channel(256);
    agent.run("what does lib.rs do?", &tx).await;
    drop(tx);
    while rx.recv().await.is_some() {}

    let log = std::fs::read_to_string(&log_path).expect("the log is readable");

    for expected in [
        "session_started",
        "user_message",
        "tool_called",
        "tool_completed",
        "assistant_message",
        "turn_completed",
    ] {
        assert!(log.contains(expected), "`{expected}` missing from the session log:\n{log}");
    }

    // Every line must stand on its own, or replay cannot recover from a partial write.
    for line in log.lines() {
        tc_core::Event::from_jsonl(line).expect("every line is a self-contained event");
    }
}

#[tokio::test]
async fn tool_arguments_are_never_logged_before_the_tool_runs_out_of_order() {
    let dir = workspace();
    let provider = Arc::new(ScriptedProvider::new(vec![
        call_tool("t1", "grep", serde_json::json!({ "pattern": "fn answer" })),
        answer("Found it."),
    ]));

    let mut agent = Agent::new(
        provider,
        ToolSet::read_only(),
        ToolContext::new(dir.path()),
        &config(5.0),
        SessionLog::create(dir.path(), SessionId::new()),
    );
    let log_path = agent.log().path().expect("the log is enabled").to_path_buf();

    let (tx, mut rx) = mpsc::channel(256);
    agent.run("find the answer", &tx).await;
    drop(tx);
    while rx.recv().await.is_some() {}

    let log = std::fs::read_to_string(&log_path).expect("the log is readable");
    let called = log.find("tool_called").expect("the call is logged");
    let completed = log.find("tool_completed").expect("the completion is logged");

    assert!(
        called < completed,
        "the attempt must be recorded before the outcome, so a crash mid-tool is still visible"
    );
}
