//! Shell commands run at fixed points in a session.
//!
//! Configured in `.truecode/hooks.toml`:
//!
//! ```toml
//! [[hook]]
//! event = "pre-tool"        # before a tool runs — can refuse it
//! matches = "write_file|patch"
//! command = "cargo fmt --check"
//!
//! [[hook]]
//! event = "post-tool"       # after a tool ran
//! matches = "write_file"
//! command = "cargo clippy -q"
//!
//! [[hook]]
//! event = "stop"            # once, when the run ends
//! command = "cargo test -q"
//! ```
//!
//! # A pre-tool hook can say no
//!
//! Exit code 2 refuses the tool call, and the hook's stderr is given to the
//! model as the reason. Any other non-zero code is reported but does not block:
//! a hook that is merely broken must not make the agent unusable, while a hook
//! that deliberately refuses must be obeyed. Two codes, one clear distinction.
//!
//! # Hooks are not a security boundary
//!
//! They run with the user's full privileges, before the workspace check, and
//! `.truecode/hooks.toml` is a file in the repository. Anyone who can land a
//! commit can run a command on the next session. That is the same trust model as
//! a `Makefile` or a git hook, and it is stated here rather than discovered.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use serde::Deserialize;

/// File name inside `.truecode/`.
pub const HOOKS_FILE: &str = "hooks.toml";

/// How long a hook may run before it is killed.
///
/// A hook that hangs would hang the agent, and the most common cause is a
/// command that decided to wait for input nobody is going to type.
pub const HOOK_TIMEOUT: Duration = Duration::from_secs(60);

/// Exit code by which a pre-tool hook refuses the call.
pub const REFUSE: i32 = 2;

/// When a hook runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Event {
    /// Before a tool runs. May refuse it with exit code [`REFUSE`].
    PreTool,
    /// After a tool ran successfully.
    PostTool,
    /// Once, when the run ends.
    Stop,
}

/// One configured hook.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hook {
    /// When to run it.
    pub event: Event,

    /// Regex the tool name must match. Absent means every tool.
    ///
    /// Ignored for `stop`, which has no tool.
    #[serde(default)]
    pub matches: Option<String>,

    /// The command line, run through the platform shell.
    pub command: String,
}

/// Everything read from `hooks.toml`.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hooks {
    /// The configured hooks, in file order.
    #[serde(default, rename = "hook")]
    pub hooks: Vec<Hook>,
}

/// Why hooks could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum HookError {
    /// The file exists but could not be read.
    #[error("cannot read {path}: {source}")]
    Read {
        /// The path involved.
        path: String,
        /// The underlying error.
        source: std::io::Error,
    },

    /// The file is not valid TOML, or does not match the expected shape.
    #[error("{path} is not a valid hook list: {source}")]
    Parse {
        /// The path involved.
        path: String,
        /// What TOML said.
        source: toml::de::Error,
    },

    /// A `matches` pattern is not a valid regex.
    #[error("hook for `{command}` has an invalid `matches` pattern: {source}")]
    Pattern {
        /// The hook's command, to identify it.
        command: String,
        /// What the regex engine said.
        source: regex::Error,
    },
}

/// What a hook did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// It ran and was happy.
    Passed,
    /// A pre-tool hook refused the call, with this reason.
    Refused(String),
    /// It failed, but not in a way that blocks. Reported, not enforced.
    Failed(String),
}

/// The hooks for a project, with their patterns already compiled.
#[derive(Debug, Default)]
pub struct HookSet {
    entries: Vec<(Hook, Option<regex::Regex>)>,
}

impl HookSet {
    /// Loads `.truecode/hooks.toml`, or an empty set if there is none.
    ///
    /// Patterns are compiled here rather than at match time, so a bad regex is a
    /// startup error instead of a surprise halfway through a run.
    ///
    /// # Errors
    ///
    /// Returns an error if the file exists but cannot be read, parsed, or if a
    /// `matches` pattern is not a valid regex.
    pub fn load(root: &Path) -> Result<Self, HookError> {
        let path = root.join(tc_config::PROJECT_DIR).join(HOOKS_FILE);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => {
                return Err(HookError::Read { path: path.display().to_string(), source });
            }
        };

        let hooks: Hooks = toml::from_str(&text)
            .map_err(|source| HookError::Parse { path: path.display().to_string(), source })?;

        Self::compile(hooks)
    }

    /// Compiles an already-parsed hook list.
    ///
    /// # Errors
    ///
    /// Returns an error if a `matches` pattern is not a valid regex.
    pub fn compile(hooks: Hooks) -> Result<Self, HookError> {
        let mut entries = Vec::new();
        for hook in hooks.hooks {
            let pattern = match &hook.matches {
                Some(pattern) => Some(regex::Regex::new(pattern).map_err(|source| {
                    HookError::Pattern { command: hook.command.clone(), source }
                })?),
                None => None,
            };
            entries.push((hook, pattern));
        }
        Ok(Self { entries })
    }

    /// How many hooks are configured.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether there are no hooks at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Every configured hook, for `truecode hooks`.
    pub fn iter(&self) -> impl Iterator<Item = &Hook> {
        self.entries.iter().map(|(hook, _)| hook)
    }

    /// The hooks that apply to an event and, for tool events, a tool name.
    fn matching(&self, event: Event, tool: &str) -> impl Iterator<Item = &Hook> {
        self.entries
            .iter()
            .filter(move |(hook, pattern)| {
                hook.event == event
                    && (event == Event::Stop
                        || pattern.as_ref().is_none_or(|pattern| pattern.is_match(tool)))
            })
            .map(|(hook, _)| hook)
    }

    /// Runs the hooks for an event, stopping at the first refusal.
    ///
    /// Stops at the first refusal because the tool is not going to run anyway;
    /// continuing would spend time on checks whose result nobody will read.
    pub async fn run(&self, event: Event, tool: &str, root: &Path) -> Vec<Outcome> {
        let mut outcomes = Vec::new();
        for hook in self.matching(event, tool) {
            let outcome = execute(hook, root).await;
            let refused = matches!(outcome, Outcome::Refused(_));
            outcomes.push(outcome);
            if refused {
                break;
            }
        }
        outcomes
    }
}

/// Runs one hook.
async fn execute(hook: &Hook, root: &Path) -> Outcome {
    let mut command = if cfg!(windows) {
        let mut command = tokio::process::Command::new("cmd");
        command.arg("/C").arg(&hook.command);
        command
    } else {
        let mut command = tokio::process::Command::new("sh");
        command.arg("-c").arg(&hook.command);
        command
    };
    command.current_dir(root).stdin(Stdio::null());

    let run = tokio::time::timeout(HOOK_TIMEOUT, command.output()).await;

    let output = match run {
        Ok(Ok(output)) => output,
        Ok(Err(err)) => {
            return Outcome::Failed(format!("hook `{}` could not start: {err}", hook.command));
        }
        Err(_) => {
            return Outcome::Failed(format!(
                "hook `{}` was killed after {} seconds",
                hook.command,
                HOOK_TIMEOUT.as_secs()
            ));
        }
    };

    if output.status.success() {
        return Outcome::Passed;
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let reason = if stderr.is_empty() {
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    } else {
        stderr
    };

    if output.status.code() == Some(REFUSE) && hook.event == Event::PreTool {
        return Outcome::Refused(if reason.is_empty() {
            format!("refused by the hook `{}`", hook.command)
        } else {
            reason
        });
    }

    Outcome::Failed(format!("hook `{}` failed: {reason}", hook.command))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hooks(toml: &str) -> HookSet {
        HookSet::compile(toml::from_str(toml).expect("parses")).expect("compiles")
    }

    fn root() -> std::path::PathBuf {
        std::env::current_dir().expect("cwd")
    }

    #[test]
    fn a_project_with_no_hook_file_is_not_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");

        assert!(HookSet::load(dir.path()).expect("missing is fine").is_empty());
    }

    #[test]
    fn an_invalid_pattern_is_caught_at_load_rather_than_mid_run() {
        let broken: Hooks =
            toml::from_str("[[hook]]\nevent = \"pre-tool\"\nmatches = \"[\"\ncommand = \"x\"")
                .expect("parses as TOML");

        assert!(HookSet::compile(broken).is_err());
    }

    #[tokio::test]
    async fn a_hook_only_runs_for_a_tool_its_pattern_matches() {
        let set =
            hooks("[[hook]]\nevent = \"pre-tool\"\nmatches = \"write_file\"\ncommand = \"exit 0\"");

        assert_eq!(set.run(Event::PreTool, "read_file", &root()).await.len(), 0);
        assert_eq!(set.run(Event::PreTool, "write_file", &root()).await.len(), 1);
    }

    #[tokio::test]
    async fn a_hook_with_no_pattern_runs_for_every_tool() {
        let set = hooks("[[hook]]\nevent = \"pre-tool\"\ncommand = \"exit 0\"");

        assert_eq!(set.run(Event::PreTool, "anything", &root()).await, [Outcome::Passed]);
    }

    #[tokio::test]
    async fn exit_code_two_refuses_the_call() {
        let set = hooks("[[hook]]\nevent = \"pre-tool\"\ncommand = \"exit 2\"");

        let outcomes = set.run(Event::PreTool, "write_file", &root()).await;

        assert!(matches!(outcomes[0], Outcome::Refused(_)), "got {outcomes:?}");
    }

    #[tokio::test]
    async fn any_other_failure_is_reported_but_does_not_block() {
        // A hook that is merely broken must not make the agent unusable.
        let set = hooks("[[hook]]\nevent = \"pre-tool\"\ncommand = \"exit 1\"");

        let outcomes = set.run(Event::PreTool, "write_file", &root()).await;

        assert!(matches!(outcomes[0], Outcome::Failed(_)), "got {outcomes:?}");
    }

    #[tokio::test]
    async fn exit_code_two_after_the_fact_is_a_failure_not_a_refusal() {
        // There is nothing left to refuse once the tool has run.
        let set = hooks("[[hook]]\nevent = \"post-tool\"\ncommand = \"exit 2\"");

        let outcomes = set.run(Event::PostTool, "write_file", &root()).await;

        assert!(matches!(outcomes[0], Outcome::Failed(_)), "got {outcomes:?}");
    }

    #[tokio::test]
    async fn a_refusal_stops_the_hooks_behind_it() {
        // The tool is not going to run; the rest would be wasted time.
        let set = hooks(
            "[[hook]]\nevent = \"pre-tool\"\ncommand = \"exit 2\"\n\
             [[hook]]\nevent = \"pre-tool\"\ncommand = \"exit 0\"",
        );

        assert_eq!(set.run(Event::PreTool, "write_file", &root()).await.len(), 1);
    }

    #[tokio::test]
    async fn a_stop_hook_ignores_the_tool_pattern() {
        let set =
            hooks("[[hook]]\nevent = \"stop\"\nmatches = \"never_matches\"\ncommand = \"exit 0\"");

        assert_eq!(set.run(Event::Stop, "", &root()).await, [Outcome::Passed]);
    }
}
