//! The tool layer.
//!
//! Seven tools, not thirty. Tool-selection accuracy collapses as the menu grows,
//! and every schema costs context on every single request. Anything beyond the
//! core set belongs behind MCP, loaded on demand — never compiled in.
//!
//! This crate currently ships the read-only half of that set: [`fs::ReadFile`],
//! [`fs::ListDir`], [`fs::Glob`] and [`search::Grep`]. Writing and shell
//! execution land with the diff-confirmation UI, because a write the user cannot
//! inspect first is exactly the thing that destroys trust in an agent.
//!
//! # Invariants every tool upholds
//!
//! 1. **Paths never leave the workspace** — see [`path::resolve_in_workspace`].
//! 2. **Output is bounded.** An unbounded `read_file` on a 5 MB file poisons the
//!    context window and the bill. Output is truncated with a visible marker, so
//!    the model knows it saw a fragment rather than silently assuming it saw all.
//! 3. **Failure is a value, not a panic.** Tool errors go back to the model as
//!    text; that feedback loop is how an agent recovers.

pub mod diff;
pub mod edit;
pub mod fs;
pub mod path;
pub mod search;
pub mod shell;

use std::path::{Path, PathBuf};

pub use diff::{ChangeKind, FileDiff};
pub use path::PathError;

/// Errors a tool can return.
///
/// Every variant is rendered back to the model as text, so the messages are
/// written for a reader who must decide what to try next.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    /// The path was refused by the workspace boundary.
    #[error(transparent)]
    Path(#[from] PathError),

    /// The arguments did not match the tool's input schema.
    #[error("invalid arguments for `{tool}`: {detail}")]
    InvalidInput {
        /// Name of the tool that was called.
        tool: &'static str,
        /// What was wrong.
        detail: String,
    },

    /// The filesystem operation failed.
    #[error("{operation} failed for `{path}`: {source}")]
    Io {
        /// What was being attempted, e.g. `read`.
        operation: &'static str,
        /// The path involved.
        path: String,
        /// The underlying error.
        source: std::io::Error,
    },

    /// No tool with that name is registered.
    #[error("unknown tool `{0}`")]
    Unknown(String),
}

/// Default cap on how much a single tool result may contribute to the context.
///
/// Roughly 8k tokens of text. Large enough for any file worth reading in one go,
/// small enough that a runaway result cannot consume a whole context window.
pub const DEFAULT_MAX_OUTPUT_BYTES: usize = 32_000;

/// Everything a tool needs to know about where it is allowed to operate.
#[derive(Debug, Clone)]
pub struct ToolContext {
    /// Workspace root. Nothing outside it is reachable.
    root: PathBuf,
    /// Cap on the size of a single tool result.
    max_output_bytes: usize,
}

impl ToolContext {
    /// Creates a context rooted at `root` with the default output cap.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into(), max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES }
    }

    /// Overrides the output cap.
    #[must_use]
    pub fn with_max_output_bytes(mut self, bytes: usize) -> Self {
        self.max_output_bytes = bytes;
        self
    }

    /// The workspace root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolves a model-supplied path inside the workspace.
    pub fn resolve(&self, candidate: &str) -> Result<PathBuf, PathError> {
        path::resolve_in_workspace(&self.root, candidate)
    }

    /// Renders a path for display, relative to the workspace root where possible.
    ///
    /// Absolute paths leak the user's directory layout into the model's context
    /// for no benefit, and make results harder to read.
    #[must_use]
    pub fn display_path(&self, absolute: &Path) -> String {
        let root = self.root.canonicalize().unwrap_or_else(|_| self.root.clone());
        absolute.strip_prefix(&root).unwrap_or(absolute).display().to_string().replace('\\', "/")
    }

    /// Truncates `output` to the configured cap, appending a visible marker.
    #[must_use]
    pub fn truncate(&self, output: String) -> String {
        truncate_to(output, self.max_output_bytes)
    }
}

/// Truncates `output` to `limit` bytes on a character boundary.
///
/// The marker matters as much as the truncation: a model that cannot tell a
/// fragment from a whole file will confidently reason about code that is not there.
#[must_use]
pub fn truncate_to(output: String, limit: usize) -> String {
    if output.len() <= limit {
        return output;
    }

    let mut end = limit;
    while end > 0 && !output.is_char_boundary(end) {
        end -= 1;
    }

    format!(
        "{}\n\n[truncated: showed {} of {} bytes — narrow the request to see the rest]",
        &output[..end],
        end,
        output.len()
    )
}

/// How alarming a command should look at the confirmation prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Risk {
    /// Nothing unusual — a build, a test, a status check.
    Normal,
    /// Plausibly destructive or outward-facing.
    High {
        /// Why, in a few words, e.g. "publishes to a remote".
        reason: String,
    },
}

/// What a tool call would do, computed before it does it.
///
/// This is what the confirmation prompt is built from. Producing it is a dry run
/// over the same code path as the real call, not a second implementation that can
/// drift away from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Nothing changes; no approval needed.
    ReadOnly,
    /// A file would be created or modified.
    Write(FileDiff),
    /// A command would run.
    Execute {
        /// The command line.
        command: String,
        /// How alarming it is.
        risk: Risk,
    },
}

impl Effect {
    /// Whether this effect requires a human decision.
    #[must_use]
    pub const fn needs_approval(&self) -> bool {
        !matches!(self, Self::ReadOnly)
    }

    /// A one-line description for a prompt or a log.
    #[must_use]
    pub fn summary(&self) -> String {
        match self {
            Self::ReadOnly => String::new(),
            Self::Write(diff) => diff.summary(),
            Self::Execute { command, .. } => format!("run `{command}`"),
        }
    }
}

/// How much the agent is allowed to do.
///
/// Three levels rather than a single on/off switch, because the three capabilities
/// carry genuinely different risk: reading cannot hurt you, editing is reversible
/// through version control, and running commands is neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PermissionMode {
    /// Read and search only. The default, and the only mode with no confirmations.
    #[default]
    ReadOnly,
    /// Adds file creation and editing, confined to the workspace. Every change is
    /// shown as a diff and confirmed.
    Write,
    /// Adds running shell commands. Every command is confirmed.
    Full,
}

impl PermissionMode {
    /// The name used on the command line and in the status bar.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::Write => "write",
            Self::Full => "full",
        }
    }

    /// Parses a mode from a command-line value.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_lowercase().as_str() {
            "read-only" | "readonly" | "read" => Some(Self::ReadOnly),
            "write" => Some(Self::Write),
            "full" => Some(Self::Full),
            _ => None,
        }
    }
}

/// A capability the model can invoke.
#[async_trait::async_trait]
pub trait Tool: Send + Sync + std::fmt::Debug {
    /// Name the model calls this tool by.
    fn name(&self) -> &'static str;

    /// One-line description sent to the model.
    ///
    /// This is prompt text, not documentation: it is the only thing the model has
    /// when choosing between tools, and it is paid for on every request.
    fn description(&self) -> &'static str;

    /// JSON Schema describing the accepted arguments.
    fn input_schema(&self) -> serde_json::Value;

    /// Whether the tool can modify anything.
    ///
    /// Read-only tools need no approval, which is what makes a read-only agent
    /// usable without a permission prompt on every step.
    fn is_read_only(&self) -> bool;

    /// Describes what this call would do, without doing it.
    ///
    /// Read-only tools inherit the default and are never previewed. Mutating tools
    /// override it, and the agent refuses to run them until a human has seen the
    /// result and agreed.
    async fn preview(
        &self,
        _input: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<Effect, ToolError> {
        Ok(Effect::ReadOnly)
    }

    /// Runs the tool.
    async fn run(&self, input: serde_json::Value, ctx: &ToolContext) -> Result<String, ToolError>;
}

/// The set of tools available to a session.
#[derive(Debug)]
pub struct ToolSet {
    tools: Vec<Box<dyn Tool>>,
}

impl ToolSet {
    /// Builds a set from the given tools.
    #[must_use]
    pub fn new(tools: Vec<Box<dyn Tool>>) -> Self {
        Self { tools }
    }

    /// The read-only tool set: enough to explore and explain a codebase, unable
    /// to change a single byte of it.
    #[must_use]
    pub fn read_only() -> Self {
        Self::new(vec![
            Box::new(fs::ReadFile),
            Box::new(fs::ListDir),
            Box::new(fs::Glob),
            Box::new(search::Grep),
        ])
    }

    /// The tools available in a given permission mode.
    ///
    /// A tool the mode does not permit is not offered to the model at all, rather
    /// than offered and then refused. Advertising a capability only to reject
    /// every call wastes context on the schema and turns the model's next few
    /// turns into guesswork about why it failed.
    #[must_use]
    pub fn for_mode(mode: PermissionMode) -> Self {
        let mut tools: Vec<Box<dyn Tool>> = vec![
            Box::new(fs::ReadFile),
            Box::new(fs::ListDir),
            Box::new(fs::Glob),
            Box::new(search::Grep),
        ];

        if matches!(mode, PermissionMode::Write | PermissionMode::Full) {
            tools.push(Box::new(edit::WriteFile));
            tools.push(Box::new(edit::Patch));
        }
        if mode == PermissionMode::Full {
            tools.push(Box::new(shell::Shell));
        }
        Self::new(tools)
    }

    /// Previews a named tool call.
    pub async fn preview(
        &self,
        name: &str,
        input: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<Effect, ToolError> {
        match self.get(name) {
            Some(tool) => tool.preview(input, ctx).await,
            None => Err(ToolError::Unknown(name.to_owned())),
        }
    }

    /// Every tool in the set.
    #[must_use]
    pub fn tools(&self) -> &[Box<dyn Tool>] {
        &self.tools
    }

    /// Looks up a tool by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.iter().find(|tool| tool.name() == name).map(AsRef::as_ref)
    }

    /// Runs a named tool, converting an unknown name into a tool error rather
    /// than a failure of the whole turn.
    pub async fn run(
        &self,
        name: &str,
        input: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<String, ToolError> {
        match self.get(name) {
            Some(tool) => tool.run(input, ctx).await,
            None => Err(ToolError::Unknown(name.to_owned())),
        }
    }
}

/// Extracts a required string argument.
fn required_str(
    input: &serde_json::Value,
    tool: &'static str,
    key: &str,
) -> Result<String, ToolError> {
    input.get(key).and_then(serde_json::Value::as_str).map(str::to_owned).ok_or_else(|| {
        ToolError::InvalidInput {
            tool,
            detail: format!("missing required string argument `{key}`"),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_output_is_left_alone() {
        assert_eq!(truncate_to("hello".to_owned(), 100), "hello");
    }

    #[test]
    fn long_output_is_truncated_and_says_so() {
        let truncated = truncate_to("x".repeat(100), 10);
        assert!(truncated.starts_with("xxxxxxxxxx"));
        assert!(truncated.contains("truncated"), "the model must know it saw a fragment");
        assert!(truncated.contains("100 bytes"));
    }

    #[test]
    fn truncation_never_splits_a_multi_byte_character() {
        // A 3-byte character cut at byte 10 would otherwise produce invalid UTF-8.
        let truncated = truncate_to("→".repeat(20), 10);
        assert!(truncated.contains("truncated"));
    }

    #[test]
    fn the_read_only_set_contains_only_read_only_tools() {
        for tool in ToolSet::read_only().tools() {
            assert!(tool.is_read_only(), "`{}` can modify state", tool.name());
        }
    }

    #[test]
    fn tool_names_are_unique() {
        let set = ToolSet::read_only();
        let mut names: Vec<&str> = set.tools().iter().map(|tool| tool.name()).collect();
        let before = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), before, "duplicate tool name");
    }

    #[test]
    fn every_tool_has_a_description_and_an_object_schema() {
        for tool in ToolSet::read_only().tools() {
            assert!(!tool.description().is_empty(), "`{}` has no description", tool.name());
            assert_eq!(
                tool.input_schema()["type"],
                "object",
                "`{}` must accept an object",
                tool.name()
            );
        }
    }

    #[tokio::test]
    async fn calling_an_unknown_tool_is_an_error_not_a_panic() {
        let ctx = ToolContext::new(".");
        let error = ToolSet::read_only()
            .run("delete_everything", serde_json::json!({}), &ctx)
            .await
            .expect_err("the tool does not exist");
        assert!(matches!(error, ToolError::Unknown(_)));
    }

    #[test]
    fn read_only_mode_offers_no_way_to_change_anything() {
        let set = ToolSet::for_mode(PermissionMode::ReadOnly);
        assert!(set.get("write_file").is_none());
        assert!(set.get("patch").is_none());
        assert!(set.get("shell").is_none());
        assert!(set.get("read_file").is_some());
    }

    #[test]
    fn write_mode_adds_editing_but_not_command_execution() {
        let set = ToolSet::for_mode(PermissionMode::Write);
        assert!(set.get("write_file").is_some());
        assert!(set.get("patch").is_some());
        assert!(set.get("shell").is_none(), "running commands needs the full mode");
    }

    #[test]
    fn full_mode_adds_the_shell() {
        assert!(ToolSet::for_mode(PermissionMode::Full).get("shell").is_some());
    }

    #[test]
    fn every_mutating_tool_is_absent_from_the_read_only_set() {
        for tool in ToolSet::for_mode(PermissionMode::ReadOnly).tools() {
            assert!(tool.is_read_only(), "`{}` can modify state", tool.name());
        }
    }

    #[test]
    fn permission_modes_round_trip_through_their_labels() {
        for mode in [PermissionMode::ReadOnly, PermissionMode::Write, PermissionMode::Full] {
            assert_eq!(PermissionMode::parse(mode.label()), Some(mode));
        }
    }

    #[test]
    fn an_unknown_permission_mode_is_rejected_rather_than_defaulted() {
        // Silently falling back to a *more* permissive mode on a typo would be a
        // security bug; falling back to a less permissive one would be confusing.
        assert_eq!(PermissionMode::parse("yolo"), None);
    }

    #[test]
    fn the_default_mode_is_the_safe_one() {
        assert_eq!(PermissionMode::default(), PermissionMode::ReadOnly);
    }

    #[test]
    fn only_mutating_effects_need_approval() {
        assert!(!Effect::ReadOnly.needs_approval());
        assert!(Effect::Execute { command: "ls".to_owned(), risk: Risk::Normal }.needs_approval());
        assert!(
            Effect::Write(FileDiff::new("a.rs", None, "x\n")).needs_approval(),
            "a write must never apply itself"
        );
    }

    #[tokio::test]
    async fn read_only_tools_preview_as_harmless() {
        let ctx = ToolContext::new(".");
        let effect = ToolSet::read_only()
            .preview("list_dir", serde_json::json!({}), &ctx)
            .await
            .expect("the preview succeeds");
        assert_eq!(effect, Effect::ReadOnly);
    }

    #[test]
    fn display_paths_are_relative_and_use_forward_slashes() {
        let dir = tempfile::tempdir().expect("temp dir is creatable");
        std::fs::create_dir_all(dir.path().join("src")).expect("dir is creatable");
        let ctx = ToolContext::new(dir.path());

        let shown = ctx.display_path(&dir.path().canonicalize().unwrap().join("src").join("a.rs"));
        assert_eq!(shown, "src/a.rs");
    }
}
