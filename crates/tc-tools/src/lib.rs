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

pub mod fs;
pub mod path;
pub mod search;

use std::path::{Path, PathBuf};

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
    fn display_paths_are_relative_and_use_forward_slashes() {
        let dir = tempfile::tempdir().expect("temp dir is creatable");
        std::fs::create_dir_all(dir.path().join("src")).expect("dir is creatable");
        let ctx = ToolContext::new(dir.path());

        let shown = ctx.display_path(&dir.path().canonicalize().unwrap().join("src").join("a.rs"));
        assert_eq!(shown, "src/a.rs");
    }
}
