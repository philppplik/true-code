//! Shared harness for the agent integration tests.
//!
//! The provider is scripted; everything else — the tools, the workspace boundary,
//! the session log — is the real thing. That is the point: these tests exercise
//! the code that ships, with only the network replaced.
//!
//! Cargo compiles this module separately into every integration-test binary, so
//! anything only one of them uses looks dead to the others. The allow is about
//! that quirk, not about tolerating unused code.
#![allow(dead_code)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tc_agent::{Agent, AgentEvent, AgentSetup, Approver, FinishReason, SessionLog};
use tc_config::{Budget, Config};
use tc_core::{Delta, Price, SessionId, StopReason, ToolCall, Usage};
use tc_providers::{DeltaStream, Provider, ProviderError, Request};
use tc_tools::{Ledger, PermissionMode, ToolContext, ToolSet};
use tokio::sync::mpsc;

/// A provider that replays a fixed script, one entry per turn.
#[derive(Debug)]
pub struct ScriptedProvider {
    /// One vector of deltas per turn, in order.
    script: Vec<Vec<Delta>>,
    /// How many turns have been requested so far.
    calls: AtomicUsize,
}

impl ScriptedProvider {
    pub fn new(script: Vec<Vec<Delta>>) -> Self {
        Self { script, calls: AtomicUsize::new(0) }
    }

    pub fn calls(&self) -> usize {
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
pub fn answer(text: &str) -> Vec<Delta> {
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
pub fn call_tool(id: &str, name: &str, input: serde_json::Value) -> Vec<Delta> {
    vec![
        Delta::Started { model: "test/scripted".to_owned() },
        Delta::ToolCall(ToolCall { id: id.to_owned(), name: name.to_owned(), input }),
        Delta::Completed {
            stop_reason: StopReason::ToolUse,
            usage: Usage { input_tokens: 100, output_tokens: 20, ..Usage::default() },
        },
    ]
}

/// A workspace containing `src/lib.rs`.
pub fn workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp dir is creatable");
    std::fs::create_dir_all(dir.path().join("src")).expect("dir is creatable");
    std::fs::write(dir.path().join("src/lib.rs"), "pub fn answer() -> u32 { 42 }\n")
        .expect("file is writable");
    dir
}

/// Reads the workspace's `src/lib.rs`.
pub fn lib_contents(dir: &tempfile::TempDir) -> String {
    std::fs::read_to_string(dir.path().join("src/lib.rs")).expect("the file is readable")
}

/// A config with the given session budget.
pub fn config(limit_usd: f64) -> Config {
    Config {
        budget: Budget { session_limit_usd: limit_usd, warn_at_percent: 70 },
        ..Config::default()
    }
}

/// Builds an agent over a scripted provider.
pub fn agent_with(
    provider: Arc<dyn Provider>,
    mode: PermissionMode,
    root: &std::path::Path,
    config: &Config,
    approver: Arc<dyn Approver>,
) -> Agent {
    agent_with_ledger(provider, mode, root, config, approver, Ledger::default())
}

/// Builds an agent with the comprehension check on or off.
pub fn agent_with_learning(
    provider: Arc<dyn Provider>,
    mode: PermissionMode,
    root: &std::path::Path,
    config: &Config,
    approver: Arc<dyn Approver>,
    teaching: bool,
) -> Agent {
    Agent::new(
        AgentSetup {
            hooks: tc_agent::HookSet::default(),
            provider,
            tools: ToolSet::for_mode(mode),
            tool_ctx: ToolContext::new(root),
            ledger: Ledger::default(),
            log: SessionLog::create(root, SessionId::new()),
            approver,
            mode,
            teaching,
            profile: tc_agent::Profile::load(root).unwrap_or_default(),
        },
        config,
    )
}

/// Builds an agent with a specific set of project rules.
pub fn agent_with_ledger(
    provider: Arc<dyn Provider>,
    mode: PermissionMode,
    root: &std::path::Path,
    config: &Config,
    approver: Arc<dyn Approver>,
    ledger: Ledger,
) -> Agent {
    Agent::new(
        AgentSetup {
            hooks: tc_agent::HookSet::default(),
            provider,
            tools: ToolSet::for_mode(mode),
            tool_ctx: ToolContext::new(root),
            ledger,
            log: SessionLog::create(root, SessionId::new()),
            approver,
            mode,
            teaching: false,
            profile: tc_agent::Profile::default(),
        },
        config,
    )
}

/// Runs one prompt and drains every event.
pub async fn drive(agent: &mut Agent, prompt: &str) -> Vec<AgentEvent> {
    let (tx, mut rx) = mpsc::channel(256);
    agent.run(prompt, &tx).await;
    drop(tx);

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    events
}

/// Extracts the reason a run ended.
pub fn finish_reason(events: &[AgentEvent]) -> Option<FinishReason> {
    events.iter().find_map(|event| match event {
        AgentEvent::Finished { reason } => Some(reason.clone()),
        _ => None,
    })
}
