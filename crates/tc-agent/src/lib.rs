//! The agent loop.
//!
//! Roughly two hundred lines that decide eighty percent of how the tool behaves.
//! The model proposes; this loop decides what actually runs, when to stop, and
//! what the user is told about it.
//!
//! ```text
//! prompt ─► provider.stream ─► text + tool calls
//!              ▲                      │
//!              │                      ▼
//!         tool results ◄────── run tools (workspace-scoped)
//! ```
//!
//! # Guard rails
//!
//! Three limits, each protecting against a documented, expensive failure mode:
//!
//! * [`MAX_TURNS`] — a model that never converges cannot spin forever.
//! * **Budget** — checked *before* each turn, so the limit stops the next request
//!   rather than being noticed after the money is gone.
//! * [`LOOP_THRESHOLD`] — an agent repeating one identical tool call is stuck.
//!   Detecting that is the difference between a wasted minute and a wasted night.
//!
//! Every one of these ends the run with a stated reason. The loop never stops
//! quietly, because a silent stop is indistinguishable from success.

pub mod approval;
pub mod checkpoint;
pub mod proof;
pub mod session;

pub use approval::{ApprovalRequest, Approver, Decision, DenyAll};
pub use checkpoint::UndoError;
pub use proof::{Proof, Verdict};
pub use tc_tools::{Effect, Ledger, PermissionMode, Risk, Violation};

use std::collections::HashMap;
use std::fmt::Write as _;

use futures::StreamExt as _;
use tc_config::Config;
use tc_core::{Content, Cost, Delta, EventKind, Message, Role, StopReason, ToolResult, Usage};
use tc_providers::{Provider, Request, ToolSchema};
use tc_tools::{ToolContext, ToolSet};

use tokio::sync::mpsc;

pub use session::SessionLog;

/// Hard ceiling on model turns for a single prompt.
///
/// Reaching it is a signal that the task was underspecified, not that the limit
/// is too low — which is why the message says so.
pub const MAX_TURNS: usize = 25;

/// How many identical tool calls count as a loop.
///
/// Public for the same reason as [`MAX_TURNS`]: it is a limit users hit and need
/// to be able to reason about, not an implementation detail.
pub const LOOP_THRESHOLD: usize = 3;

/// The default system prompt.
///
/// Kept as a single `const` and never interpolated with anything dynamic. The
/// moment a date or a file list is prepended, the provider's prompt cache misses
/// on every request and input costs multiply.
pub const SYSTEM_PROMPT: &str = "\
You are true-code, a coding assistant working inside the user's project directory.

Use the tools to look at the real code before answering. Do not guess at file
contents, and do not describe code you have not read.

Two things matter more than being fast:

1. Say what you actually verified. If you did not check something, say so rather
   than implying you did.
2. Explain your reasoning briefly as you go, so the user understands the codebase
   better after your answer than before it.

";

/// Mode-specific instructions, appended to [`SYSTEM_PROMPT`].
///
/// Appended rather than interpolated, so the cached prefix stays byte-identical.
/// The mode cannot change within a session, so this is stable too.
#[must_use]
pub const fn mode_prompt(mode: PermissionMode) -> &'static str {
    match mode {
        PermissionMode::ReadOnly => {
            "\nYou have read-only tools and cannot modify anything. If the user asks for a \
             change, explain exactly what you would change and why, and tell them to restart \
             with --permission-mode write to apply it."
        }
        PermissionMode::Write => {
            "\nYou can create and edit files in this project. Every change is shown to the user \
             as a diff and applied only if they agree, so propose the change you actually mean \
             rather than asking for permission in prose.\n\n\
             Prefer `patch` over `write_file`: it changes one exact region, so a mistake costs \
             one hunk instead of a whole file. Read a file before patching it.\n\n\
             You cannot run commands, so you cannot run the tests. Do not claim a change is \
             verified when you have not verified it — say what you checked and what you did not."
        }
        PermissionMode::Full => {
            "\nYou can create and edit files in this project and run shell commands. Every \
             change and every command is shown to the user and applied only if they agree.\n\n\
             Prefer `patch` over `write_file`: it changes one exact region, so a mistake costs \
             one hunk instead of a whole file. Read a file before patching it.\n\n\
             After changing code, run the project's tests and report what actually happened, \
             including the exit code. A claim of success without evidence is worse than no \
             claim at all."
        }
    }
}

/// Why a run ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinishReason {
    /// The model finished its answer.
    Completed,
    /// [`MAX_TURNS`] was reached.
    TurnLimit,
    /// The session budget was exhausted.
    BudgetExhausted,
    /// The same tool call repeated [`LOOP_THRESHOLD`] times.
    Loop {
        /// Name of the tool that was repeating.
        tool: String,
    },
    /// The answer was cut off by the output token limit.
    OutputTruncated,
    /// The user declined a change and ended the run.
    Declined,
}

impl FinishReason {
    /// A sentence explaining the outcome, written for the user.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::Completed => "Done.".to_owned(),
            Self::TurnLimit => format!(
                "Stopped after {MAX_TURNS} steps without finishing — the task is probably too \
                 broad. Try narrowing it to one file or one question."
            ),
            Self::BudgetExhausted => {
                "Stopped: the session budget is used up. Raise `budget.session_limit_usd` to \
                 continue."
                    .to_owned()
            }
            Self::Loop { tool } => format!(
                "Stopped: `{tool}` was called with identical arguments {LOOP_THRESHOLD} times, \
                 which means no progress is being made."
            ),
            Self::OutputTruncated => {
                "The answer was cut off at the output limit — ask for a narrower scope.".to_owned()
            }
            Self::Declined => "Stopped: you declined the change. Nothing was written.".to_owned(),
        }
    }
}

/// What the agent reports while it works.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentEvent {
    /// The provider accepted the request.
    Started {
        /// Model reported by the provider.
        model: String,
    },
    /// A chunk of assistant text.
    Text {
        /// The fragment.
        text: String,
    },
    /// A tool is about to run.
    ToolStarted {
        /// Name of the tool.
        tool: String,
        /// One-line rendering of the arguments, for display.
        summary: String,
    },
    /// A change was proposed and refused by the user.
    ToolDeclined {
        /// Name of the tool.
        tool: String,
        /// What it would have done.
        summary: String,
    },
    /// A tool finished.
    ToolFinished {
        /// Name of the tool.
        tool: String,
        /// Whether it failed.
        is_error: bool,
    },
    /// A turn's accounting is final.
    TurnCompleted {
        /// Tokens used by the turn.
        usage: Usage,
        /// Money spent on the turn.
        cost: Cost,
    },
    /// What the run was observed to do. Sent just before [`AgentEvent::Finished`].
    Proven {
        /// The evidence.
        proof: Proof,
    },
    /// The run ended.
    Finished {
        /// Why it ended.
        reason: FinishReason,
    },
    /// Something worth telling the user, outside a run.
    ///
    /// Undo reports through here, so the UI needs no special case for it.
    Notice {
        /// The message.
        text: String,
    },
    /// The run failed. Errors are reported, never swallowed.
    Failed {
        /// What went wrong.
        message: String,
    },
}

/// Runs prompts against a provider, with tools and guard rails.
#[derive(Debug)]
pub struct Agent {
    provider: std::sync::Arc<dyn Provider>,
    tools: ToolSet,
    tool_ctx: ToolContext,
    schemas: Vec<ToolSchema>,
    system_prompt: String,
    budget: tc_config::Budget,
    /// Decides whether a mutating tool call may proceed.
    approver: std::sync::Arc<dyn Approver>,
    /// Tools the user has approved for the rest of the session.
    approved_tools: std::collections::HashSet<String>,
    /// Saves file contents before a tool overwrites them.
    checkpoints: checkpoint::Checkpoints,
    /// The project's rules, checked before any change is applied.
    ledger: Ledger,
    /// What the current run has been observed to do.
    proof: Proof,
    /// Conversation so far, carried across prompts within a session.
    history: Vec<Message>,
    /// Running session total.
    spent: Cost,
    log: SessionLog,
}

/// Everything an [`Agent`] needs to exist.
///
/// A struct rather than seven positional arguments: at this length a call site
/// stops being readable, and swapping two of them would compile.
#[derive(Debug)]
pub struct AgentSetup {
    /// Where model turns come from.
    pub provider: std::sync::Arc<dyn Provider>,
    /// What the agent may call.
    pub tools: ToolSet,
    /// Which directory it may touch.
    pub tool_ctx: ToolContext,
    /// The project's rules.
    pub ledger: Ledger,
    /// Where the session is recorded.
    pub log: SessionLog,
    /// Who decides whether a change may be applied.
    pub approver: std::sync::Arc<dyn Approver>,
    /// How much the agent is allowed to do.
    pub mode: PermissionMode,
}

impl Agent {
    /// Builds an agent from its setup.
    #[must_use]
    pub fn new(setup: AgentSetup, config: &Config) -> Self {
        let AgentSetup { provider, tools, tool_ctx, ledger, log, approver, mode } = setup;

        let schemas = tools
            .tools()
            .iter()
            .map(|tool| ToolSchema {
                name: tool.name().to_owned(),
                description: tool.description().to_owned(),
                input_schema: tool.input_schema(),
            })
            .collect();

        let mut agent = Self {
            provider,
            tools,
            tool_ctx,
            schemas,
            // The ledger goes last, after the cacheable prefix and the mode
            // section. All three are stable for the whole session, so the prompt
            // cache still hits — and the rules are restated in full on every
            // request instead of decaying with the conversation.
            system_prompt: config.system_prompt.clone().unwrap_or_else(|| {
                format!("{SYSTEM_PROMPT}{}{}", mode_prompt(mode), ledger.prompt_section())
            }),
            budget: config.budget,
            approver,
            approved_tools: std::collections::HashSet::new(),
            checkpoints: checkpoint::Checkpoints::new(
                log.directory().unwrap_or_else(|| std::path::PathBuf::from(".")),
            ),
            ledger,
            proof: Proof::default(),
            history: Vec::new(),
            spent: Cost::default(),
            log,
        };

        agent.log.append(EventKind::SessionStarted {
            model: agent.provider.id().to_owned(),
            tool_version: env!("CARGO_PKG_VERSION").to_owned(),
        });
        agent
    }

    /// Total spent so far this session.
    #[must_use]
    pub const fn spent(&self) -> Cost {
        self.spent
    }

    /// Qualified identifier of the model in use.
    #[must_use]
    pub fn model_id(&self) -> &str {
        self.provider.id()
    }

    /// Context window of the model in use, in tokens.
    #[must_use]
    pub fn context_window(&self) -> u32 {
        self.provider.context_window()
    }

    /// How many project rules are being checked.
    #[must_use]
    pub fn rule_count(&self) -> usize {
        self.ledger.constraints().len()
    }

    /// Indicative price of the model in use.
    #[must_use]
    pub fn price(&self) -> tc_core::Price {
        self.provider.price()
    }

    /// The session log.
    #[must_use]
    pub const fn log(&self) -> &SessionLog {
        &self.log
    }

    /// Runs one user prompt to completion, reporting progress over `events`.
    ///
    /// Returns when the model has nothing left to do or a guard rail fired.
    pub async fn run(&mut self, prompt: &str, events: &mpsc::Sender<AgentEvent>) {
        // Per prompt, not per session: the question the panel answers is what
        // *this* task did, not what the whole afternoon amounted to.
        self.proof = Proof::default();

        self.log.append(EventKind::UserMessage { content: prompt.to_owned() });
        self.history.push(Message::user(prompt));

        let mut repeats: HashMap<String, usize> = HashMap::new();

        for _turn in 0..MAX_TURNS {
            if self.budget.is_exhausted(self.spent.usd) {
                self.finish(events, FinishReason::BudgetExhausted).await;
                return;
            }

            let outcome = match self.one_turn(events).await {
                Ok(outcome) => outcome,
                Err(message) => {
                    self.log.append(EventKind::Error { message: message.clone() });
                    if !self.proof.is_empty() {
                        send(events, AgentEvent::Proven { proof: self.proof.clone() }).await;
                    }
                    send(events, AgentEvent::Failed { message }).await;
                    return;
                }
            };

            self.history.push(Message { role: Role::Assistant, content: outcome.content.clone() });
            self.log.append(EventKind::AssistantMessage { content: outcome.text.clone() });

            let calls = outcome.tool_calls();
            if calls.is_empty() {
                let reason = if outcome.stop_reason == StopReason::MaxTokens {
                    FinishReason::OutputTruncated
                } else {
                    FinishReason::Completed
                };
                self.finish(events, reason).await;
                return;
            }

            // A model repeating one call verbatim has stopped making progress.
            // Without this check the turn limit is the only backstop, and every
            // one of those turns is billed.
            for call in &calls {
                let fingerprint = format!("{}:{}", call.name, call.input);
                let seen = repeats.entry(fingerprint).or_insert(0);
                *seen += 1;
                if *seen >= LOOP_THRESHOLD {
                    let reason = FinishReason::Loop { tool: call.name.clone() };
                    self.log.append(EventKind::Error { message: reason.message() });
                    self.finish(events, reason).await;
                    return;
                }
            }

            match self.run_tools(&calls, events).await {
                ToolsOutcome::Continue(results) => {
                    self.history.push(Message::tool_results(results));
                }
                ToolsOutcome::Aborted => {
                    self.finish(events, FinishReason::Declined).await;
                    return;
                }
            }
        }

        self.finish(events, FinishReason::TurnLimit).await;
    }

    /// Ends a run: the evidence first, then why it stopped.
    ///
    /// Every exit goes through here. A run that ends without a panel would let
    /// "it stopped" read as "it worked", which is the whole failure this guards
    /// against.
    async fn finish(&mut self, events: &mpsc::Sender<AgentEvent>, reason: FinishReason) {
        if !self.proof.is_empty() {
            send(events, AgentEvent::Proven { proof: self.proof.clone() }).await;
        }
        send(events, AgentEvent::Finished { reason }).await;
    }

    /// Streams a single model turn and collects what it produced.
    async fn one_turn(&mut self, events: &mpsc::Sender<AgentEvent>) -> Result<Turn, String> {
        let request = Request::new(Some(self.system_prompt.clone()), self.history.clone())
            .with_tools(self.schemas.clone());

        let mut stream = self.provider.stream(request).await.map_err(|err| err.to_string())?;

        let mut turn = Turn::default();

        while let Some(item) = stream.next().await {
            match item.map_err(|err| err.to_string())? {
                Delta::Started { model } => send(events, AgentEvent::Started { model }).await,

                Delta::Text { text } => {
                    turn.text.push_str(&text);
                    turn.content.push(Content::Text { text: text.clone() });
                    send(events, AgentEvent::Text { text }).await;
                }

                Delta::ToolCall(call) => turn.content.push(Content::ToolCall(call)),

                Delta::Completed { stop_reason, usage } => {
                    turn.stop_reason = stop_reason;
                    let cost = self.provider.price().cost_of(usage);
                    self.spent = self.spent.add(cost);

                    self.log.append(EventKind::TurnCompleted { stop_reason, usage, cost });
                    send(events, AgentEvent::TurnCompleted { usage, cost }).await;
                }
            }
        }

        turn.coalesce_text();
        Ok(turn)
    }

    /// Decides whether one call may run, asking the user when it would change
    /// something.
    ///
    /// Returns the effect to proceed with, or the refusal to report.
    async fn authorise(
        &mut self,
        call: &tc_core::ToolCall,
        events: &mpsc::Sender<AgentEvent>,
    ) -> Gate {
        let effect = match self.tools.preview(&call.name, call.input.clone(), &self.tool_ctx).await
        {
            Ok(effect) => effect,
            // A preview that fails is the call failing: `patch` cannot compute a
            // diff for a snippet that is not there. Report it like any tool error
            // so the model can correct itself, and never reach the real call.
            Err(err) => return Gate::Failed(err.to_string()),
        };

        let violations = match &effect {
            Effect::Write(diff) => self.ledger.check_diff(diff),
            Effect::Execute { command, .. } => self.ledger.check_command(command),
            Effect::ReadOnly => Vec::new(),
        };

        for violation in &violations {
            self.proof.record_violation(violation.clone());
            self.log.append(EventKind::ConstraintViolated {
                call_id: call.id.clone(),
                description: violation.description.clone(),
                evidence: violation.evidence.clone(),
            });
        }

        if !effect.needs_approval() {
            return Gate::Run { effect, violations };
        }
        // A write that changes nothing is not worth a prompt. Prompts people
        // learn to dismiss are prompts that have stopped protecting them.
        if let Effect::Write(diff) = &effect
            && diff.is_empty()
        {
            return Gate::Run { effect, violations };
        }
        // A blanket "always allow this tool" does not extend to a change that
        // breaks a stated rule. That is precisely the case worth interrupting for.
        if self.approved_tools.contains(&call.name) && violations.is_empty() {
            return Gate::Run { effect, violations };
        }

        let request = ApprovalRequest { tool: call.name.clone(), effect, violations };
        let summary = request.summary();
        let decision = self.approver.approve(&request).await;

        self.log.append(EventKind::ApprovalDecided {
            call_id: call.id.clone(),
            tool: call.name.clone(),
            summary: summary.clone(),
            approved: matches!(decision, Decision::Approve | Decision::ApproveToolForSession),
        });

        match decision {
            Decision::Approve => {
                Gate::Run { effect: request.effect, violations: request.violations }
            }
            Decision::ApproveToolForSession => {
                self.approved_tools.insert(call.name.clone());
                Gate::Run { effect: request.effect, violations: request.violations }
            }
            Decision::Deny | Decision::Abort => {
                send(events, AgentEvent::ToolDeclined { tool: call.name.clone(), summary }).await;
                if decision == Decision::Abort { Gate::Aborted } else { Gate::Declined }
            }
        }
    }

    /// Saves a file's current contents before a tool overwrites them.
    ///
    /// A failure here is reported but does not block the change: refusing to edit
    /// because the *backup* could not be written would be a strange way to protect
    /// someone. The user is told they are working without a net.
    async fn checkpoint(
        &mut self,
        call: &tc_core::ToolCall,
        effect: &Effect,
        events: &mpsc::Sender<AgentEvent>,
    ) {
        let Effect::Write(diff) = effect else {
            return;
        };
        let Ok(absolute) = self.tool_ctx.resolve(&diff.path) else {
            return;
        };

        self.proof.record_change(diff.path.clone());

        match self.checkpoints.capture(&absolute, &diff.path) {
            Ok(backup) => self.log.append(EventKind::Checkpointed {
                path: diff.path.clone(),
                backup,
                tool: call.name.clone(),
            }),
            Err(err) => {
                send(
                    events,
                    AgentEvent::Notice {
                        text: format!("Could not save an undo copy of {}: {err}", diff.path),
                    },
                )
                .await;
            }
        }
    }

    /// Undoes the most recent change true-code made in this session.
    pub fn undo_last(&mut self) -> Result<String, UndoError> {
        let (events_path, session_dir) = match (self.log.path(), self.log.directory()) {
            (Some(path), Some(dir)) => (path.to_path_buf(), dir),
            _ => return Err(UndoError::NothingToUndo),
        };

        let Some(entry) = checkpoint::last_undoable(&events_path)? else {
            return Err(UndoError::NothingToUndo);
        };

        checkpoint::restore(&session_dir, self.tool_ctx.root(), &entry)?;
        self.log
            .append(EventKind::Reverted { path: entry.path.clone(), checkpoint_seq: entry.seq });

        Ok(match entry.backup {
            Some(_) => format!("Reverted {}.", entry.path),
            None => format!("Removed {}, which true-code had created.", entry.path),
        })
    }

    /// Runs every tool call of a turn and collects the results.
    async fn run_tools(
        &mut self,
        calls: &[tc_core::ToolCall],
        events: &mpsc::Sender<AgentEvent>,
    ) -> ToolsOutcome {
        let mut results = Vec::with_capacity(calls.len());

        for call in calls {
            self.log.append(EventKind::ToolCalled {
                call_id: call.id.clone(),
                tool: call.name.clone(),
                input: call.input.clone(),
            });

            // Nothing touches the disk before this returns.
            let broken = match self.authorise(call, events).await {
                Gate::Run { effect, violations } => {
                    self.checkpoint(call, &effect, events).await;
                    violations
                }
                Gate::Aborted => return ToolsOutcome::Aborted,
                refusal => {
                    // Every call still gets a result. A tool call left unanswered
                    // makes the next request invalid for every provider.
                    results.push(ToolResult {
                        call_id: call.id.clone(),
                        output: refusal.message(),
                        is_error: true,
                    });
                    send(
                        events,
                        AgentEvent::ToolFinished { tool: call.name.clone(), is_error: true },
                    )
                    .await;
                    continue;
                }
            };

            send(
                events,
                AgentEvent::ToolStarted {
                    tool: call.name.clone(),
                    summary: summarise(&call.input),
                },
            )
            .await;

            let started = std::time::Instant::now();
            let outcome = self.tools.run(&call.name, call.input.clone(), &self.tool_ctx).await;

            // A failed tool is context, not a crash: the model reads the error
            // and usually corrects itself on the next turn.
            let (mut output, is_error) = match outcome {
                Ok(output) => (output, false),
                Err(err) => (err.to_string(), true),
            };

            // The model is told when a change it made broke a stated rule, even
            // though the user allowed it. Otherwise it reads the approval as
            // permission to keep doing it.
            if !is_error && !broken.is_empty() {
                output.push_str("\n\nThis change broke rules the user set:\n");
                for violation in &broken {
                    let _ = writeln!(output, "  - {}", violation.summary());
                }
                output.push_str(
                    "They allowed it this time. Do not treat that as permission to break them \
                     again; mention it in your answer.",
                );
            }

            // The only evidence that counts: an exit code the harness watched,
            // not a sentence the model wrote about one.
            if call.name == "shell"
                && let Some(command) = call.input.get("command").and_then(|c| c.as_str())
                && let Some(exit_code) = tc_tools::shell::parse_exit_code(&output)
            {
                self.proof.record_check(command, exit_code);
            }

            self.log.append(EventKind::ToolCompleted {
                call_id: call.id.clone(),
                is_error,
                duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
                output_bytes: output.len(),
            });
            send(events, AgentEvent::ToolFinished { tool: call.name.clone(), is_error }).await;

            results.push(ToolResult { call_id: call.id.clone(), output, is_error });
        }
        ToolsOutcome::Continue(results)
    }
}

/// Whether a call may proceed, and with what effect.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Gate {
    /// Cleared to run.
    Run {
        /// The previewed effect, which the checkpoint needs.
        effect: Effect,
        /// Rules this change breaks, if the user allowed it anyway.
        violations: Vec<Violation>,
    },
    /// The user said no; the run continues.
    Declined,
    /// The user said no and ended the run.
    Aborted,
    /// The preview itself failed, so the call could never have worked.
    Failed(String),
}

impl Gate {
    /// What the model is told.
    ///
    /// A declined change must read as a decision, not a malfunction — otherwise
    /// the model retries the same edit, and the user gets asked again.
    fn message(&self) -> String {
        match self {
            // `Run` never reaches here; it is not a refusal.
            Self::Run { .. } | Self::Declined | Self::Aborted => {
                "The user declined this change. Do not retry it. Ask what they would prefer \
                 instead, or continue with the rest of the task."
                    .to_owned()
            }
            Self::Failed(detail) => detail.clone(),
        }
    }
}

/// What running a turn's tools produced.
#[derive(Debug)]
enum ToolsOutcome {
    /// Results to feed back to the model.
    Continue(Vec<ToolResult>),
    /// The user ended the run.
    Aborted,
}

/// What one model turn produced.
#[derive(Debug, Default)]
struct Turn {
    /// Assistant content blocks, in order.
    content: Vec<Content>,
    /// All text, concatenated.
    text: String,
    /// Why the turn ended.
    stop_reason: StopReason,
}

impl Turn {
    /// Returns the tool calls this turn requested.
    fn tool_calls(&self) -> Vec<tc_core::ToolCall> {
        self.content
            .iter()
            .filter_map(|block| match block {
                Content::ToolCall(call) => Some(call.clone()),
                _ => None,
            })
            .collect()
    }

    /// Merges adjacent text blocks into one.
    ///
    /// Streaming produces one block per fragment. Sending hundreds of one-word
    /// blocks back to the provider is wasteful and, on some APIs, rejected.
    fn coalesce_text(&mut self) {
        let mut merged: Vec<Content> = Vec::with_capacity(self.content.len());
        for block in self.content.drain(..) {
            match (merged.last_mut(), block) {
                (Some(Content::Text { text: existing }), Content::Text { text }) => {
                    existing.push_str(&text);
                }
                (_, block) => merged.push(block),
            }
        }
        self.content = merged;
    }
}

/// Renders tool arguments as one short line for display.
fn summarise(input: &serde_json::Value) -> String {
    let Some(object) = input.as_object() else {
        return String::new();
    };
    object
        .iter()
        .map(|(key, value)| match value {
            serde_json::Value::String(text) => format!("{key}={text}"),
            other => format!("{key}={other}"),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Sends an event, ignoring a closed receiver.
///
/// A closed channel means the UI is gone. That is a normal shutdown, not an error
/// worth propagating through the loop.
async fn send(events: &mpsc::Sender<AgentEvent>, event: AgentEvent) {
    let _ = events.send(event).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_finish_reason_explains_itself_to_the_user() {
        for reason in [
            FinishReason::Completed,
            FinishReason::TurnLimit,
            FinishReason::BudgetExhausted,
            FinishReason::Loop { tool: "grep".to_owned() },
            FinishReason::OutputTruncated,
        ] {
            assert!(!reason.message().is_empty(), "{reason:?} has no message");
        }
    }

    #[test]
    fn the_turn_limit_message_suggests_what_to_do_next() {
        let message = FinishReason::TurnLimit.message();
        assert!(message.contains("narrowing"), "unexpected message: {message}");
    }

    #[test]
    fn the_loop_message_names_the_offending_tool() {
        let message = FinishReason::Loop { tool: "grep".to_owned() }.message();
        assert!(message.contains("grep"), "unexpected message: {message}");
    }

    #[test]
    fn the_system_prompt_contains_nothing_dynamic() {
        // Prompt caching only pays off while this prefix is byte-identical
        // between requests. A date or a file list here would silently multiply
        // input costs.
        assert!(!SYSTEM_PROMPT.contains("{}"));
        assert!(!SYSTEM_PROMPT.contains("2026"));
    }

    #[test]
    fn each_mode_tells_the_model_what_it_can_actually_do() {
        assert!(mode_prompt(PermissionMode::ReadOnly).contains("cannot modify"));
        assert!(mode_prompt(PermissionMode::Write).contains("create and edit"));
        assert!(mode_prompt(PermissionMode::Full).contains("shell commands"));
    }

    #[test]
    fn write_mode_admits_it_cannot_run_the_tests() {
        // The mode has no shell, so a claim of "verified" would be a lie.
        assert!(mode_prompt(PermissionMode::Write).contains("cannot run the tests"));
    }

    #[test]
    fn the_mode_suffix_is_appended_so_the_cached_prefix_stays_stable() {
        for mode in [PermissionMode::ReadOnly, PermissionMode::Write, PermissionMode::Full] {
            let full = format!("{SYSTEM_PROMPT}{}", mode_prompt(mode));
            assert!(
                full.starts_with(SYSTEM_PROMPT),
                "the cacheable prefix must come first for every mode"
            );
        }
    }

    #[test]
    fn adjacent_text_blocks_are_merged() {
        let mut turn = Turn {
            content: vec![
                Content::Text { text: "Hal".to_owned() },
                Content::Text { text: "lo".to_owned() },
            ],
            ..Turn::default()
        };
        turn.coalesce_text();

        assert_eq!(turn.content, vec![Content::Text { text: "Hallo".to_owned() }]);
    }

    #[test]
    fn merging_text_does_not_reorder_tool_calls() {
        let call = tc_core::ToolCall {
            id: "t1".to_owned(),
            name: "grep".to_owned(),
            input: serde_json::json!({}),
        };
        let mut turn = Turn {
            content: vec![
                Content::Text { text: "a".to_owned() },
                Content::ToolCall(call.clone()),
                Content::Text { text: "b".to_owned() },
                Content::Text { text: "c".to_owned() },
            ],
            ..Turn::default()
        };
        turn.coalesce_text();

        assert_eq!(turn.content.len(), 3);
        assert_eq!(turn.content[1], Content::ToolCall(call));
        assert_eq!(turn.content[2], Content::Text { text: "bc".to_owned() });
    }

    #[test]
    fn tool_calls_are_extracted_in_order() {
        let turn = Turn {
            content: vec![
                Content::ToolCall(tc_core::ToolCall {
                    id: "a".to_owned(),
                    name: "read_file".to_owned(),
                    input: serde_json::json!({}),
                }),
                Content::ToolCall(tc_core::ToolCall {
                    id: "b".to_owned(),
                    name: "grep".to_owned(),
                    input: serde_json::json!({}),
                }),
            ],
            ..Turn::default()
        };

        let calls = turn.tool_calls();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "read_file");
        assert_eq!(calls[1].name, "grep");
    }

    #[test]
    fn arguments_are_summarised_without_json_noise() {
        let summary = summarise(&serde_json::json!({ "path": "src/lib.rs" }));
        assert_eq!(summary, "path=src/lib.rs");
    }

    #[test]
    fn summarising_a_non_object_yields_an_empty_line() {
        assert_eq!(summarise(&serde_json::json!("scalar")), "");
    }
}
