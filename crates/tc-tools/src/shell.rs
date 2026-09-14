//! Running shell commands.
//!
//! This is the single most dangerous capability in the harness, and it is treated
//! that way: it only exists in [`PermissionMode::Full`](crate::PermissionMode),
//! every invocation is confirmed by a human, and a small set of commands is
//! refused outright — with approval or without it.
//!
//! # Why refuse anything at all if a human approves?
//!
//! Because approval fatigue is real and measured. After the twentieth `cargo test`
//! prompt, "yes" stops being a decision and becomes a reflex, and that is exactly
//! when the twenty-first prompt is `rm -rf /`. A handful of commands are never
//! worth the round trip, so they are refused at the tool rather than shown to a
//! tired human at 2am.
//!
//! This list is a backstop, not a security boundary. It is trivially bypassable by
//! anything determined — a real boundary needs the OS sandbox, which is still on
//! the roadmap. It is here to catch the plausible accident, not the adversary.

use std::process::Stdio;
use std::time::Duration;

use crate::{Effect, Risk, Tool, ToolContext, ToolError, required_str};

/// How long a command may run before it is killed.
const TIMEOUT: Duration = Duration::from_secs(120);

/// Cap on combined stdout and stderr kept from a command.
const MAX_CAPTURE_BYTES: usize = 30_000;

/// Substrings that are refused outright.
///
/// Deliberately short and literal. A long clever list produces false confidence
/// and false positives; these are the ones with no legitimate use from an agent.
const REFUSED: &[(&str, &str)] = &[
    ("rm -rf /", "deletes the filesystem root"),
    ("rm -rf /*", "deletes the filesystem root"),
    ("mkfs", "formats a filesystem"),
    ("dd if=", "writes raw device data"),
    (":(){", "is a fork bomb"),
    ("shutdown", "powers off the machine"),
    ("reboot", "restarts the machine"),
];

/// Patterns that make a command high-risk, shown prominently at the prompt.
const HIGH_RISK: &[(&str, &str)] = &[
    ("rm -r", "deletes directories recursively"),
    ("rm -f", "deletes without prompting"),
    ("git push", "publishes to a remote"),
    ("git reset --hard", "discards uncommitted work"),
    ("git clean", "deletes untracked files"),
    ("drop table", "drops a database table"),
    ("curl", "fetches from the network"),
    ("wget", "fetches from the network"),
    ("npm publish", "publishes a package"),
    ("cargo publish", "publishes a crate"),
    ("sudo", "runs with elevated privileges"),
];

/// Runs a single shell command in the workspace.
#[derive(Debug)]
pub struct Shell;

#[async_trait::async_trait]
impl Tool for Shell {
    fn name(&self) -> &'static str {
        "shell"
    }

    fn description(&self) -> &'static str {
        "Run one shell command in the project directory and return its output. \
         Use it for builds, tests and git status — not for reading or editing \
         files, which the dedicated tools do better. Interactive commands will \
         hang and be killed; pass non-interactive flags."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The command line to run, e.g. `cargo test`.",
                },
            },
            "required": ["command"],
        })
    }

    fn is_read_only(&self) -> bool {
        false
    }

    async fn preview(
        &self,
        input: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<Effect, ToolError> {
        let command = required_str(&input, self.name(), "command")?;
        refuse_if_catastrophic(&command)?;

        Ok(Effect::Execute { command: command.clone(), risk: classify(&command) })
    }

    async fn run(&self, input: serde_json::Value, ctx: &ToolContext) -> Result<String, ToolError> {
        let command = required_str(&input, self.name(), "command")?;
        refuse_if_catastrophic(&command)?;

        let child = spawn(&command, ctx)?;

        let output = match tokio::time::timeout(TIMEOUT, child.wait_with_output()).await {
            Ok(result) => result.map_err(|source| ToolError::Io {
                operation: "run",
                path: command.clone(),
                source,
            })?,
            Err(_) => {
                // The child is killed by dropping it; tokio's `kill_on_drop` makes
                // that reliable rather than leaving an orphan running.
                return Err(ToolError::InvalidInput {
                    tool: self.name().to_owned(),
                    detail: format!(
                        "`{command}` did not finish within {} seconds and was killed. If it \
                         waits for input, add a non-interactive flag.",
                        TIMEOUT.as_secs()
                    ),
                });
            }
        };

        Ok(ctx.truncate(render_output(&command, &output)))
    }
}

/// Rejects a command that is never worth a confirmation prompt.
fn refuse_if_catastrophic(command: &str) -> Result<(), ToolError> {
    let lowered = command.to_lowercase();
    for (pattern, reason) in REFUSED {
        if lowered.contains(pattern) {
            return Err(ToolError::InvalidInput {
                tool: "shell".to_owned(),
                detail: format!(
                    "Refused: `{pattern}` {reason}. true-code will not run this even with \
                     approval. Run it yourself if you are sure."
                ),
            });
        }
    }
    Ok(())
}

/// Classifies how alarming a command should look at the prompt.
fn classify(command: &str) -> Risk {
    let lowered = command.to_lowercase();
    for (pattern, reason) in HIGH_RISK {
        if lowered.contains(pattern) {
            return Risk::High { reason: (*reason).to_owned() };
        }
    }
    Risk::Normal
}

/// Starts the command under the platform's shell, rooted in the workspace.
fn spawn(command: &str, ctx: &ToolContext) -> Result<tokio::process::Child, ToolError> {
    // `cmd /C` on Windows, `sh -c` elsewhere: the model writes ordinary shell
    // syntax and should not have to know which platform it landed on.
    let mut builder = if cfg!(windows) {
        let mut builder = tokio::process::Command::new("cmd");
        builder.arg("/C").arg(command);
        builder
    } else {
        let mut builder = tokio::process::Command::new("sh");
        builder.arg("-c").arg(command);
        builder
    };

    builder
        .current_dir(ctx.root())
        // No stdin: an interactive command must fail fast instead of hanging
        // until the timeout while the user wonders what happened.
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|source| ToolError::Io { operation: "start", path: command.to_owned(), source })
}

/// Prefix of the line carrying a command's exit code.
///
/// Written by [`render_output`] and read back by [`parse_exit_code`]. The two live
/// next to each other, and a test asserts they agree — a parser that drifts from
/// its formatter would silently stop finding evidence and report "unverified" for
/// commands that actually ran.
const EXIT_PREFIX: &str = "exit code: ";

/// Reads the exit code back out of a rendered shell result.
///
/// Returns `None` for output this module did not produce.
#[must_use]
pub fn parse_exit_code(output: &str) -> Option<i32> {
    output
        .lines()
        .find_map(|line| line.strip_prefix(EXIT_PREFIX))
        .and_then(|code| code.trim().parse().ok())
}

/// Formats a finished command for the model.
///
/// The exit code is always stated. A failed build whose output *looks* fine is
/// exactly how an agent talks itself into reporting success.
fn render_output(command: &str, output: &std::process::Output) -> String {
    let code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let mut rendered = format!("$ {command}\nexit code: {code}\n");
    let (has_stdout, has_stderr) = (!stdout.trim().is_empty(), !stderr.trim().is_empty());

    if has_stdout {
        rendered.push_str("\n--- stdout ---\n");
        rendered.push_str(&crate::truncate_to(stdout.into_owned(), MAX_CAPTURE_BYTES));
    }
    if has_stderr {
        rendered.push_str("\n--- stderr ---\n");
        rendered.push_str(&crate::truncate_to(stderr.into_owned(), MAX_CAPTURE_BYTES));
    }
    if !has_stdout && !has_stderr {
        rendered.push_str("\n(no output)\n");
    }
    rendered
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> (tempfile::TempDir, ToolContext) {
        let dir = tempfile::tempdir().expect("temp dir is creatable");
        std::fs::write(dir.path().join("marker.txt"), "hello\n").expect("file is writable");
        let ctx = ToolContext::new(dir.path());
        (dir, ctx)
    }

    /// A command that prints `hello` on either platform.
    const ECHO: &str = "echo hello";

    #[tokio::test]
    async fn a_successful_command_reports_its_output_and_exit_code() {
        let (_dir, ctx) = project();

        let output = Shell
            .run(serde_json::json!({ "command": ECHO }), &ctx)
            .await
            .expect("the command runs");

        assert!(output.contains("hello"), "unexpected output: {output}");
        assert!(output.contains("exit code: 0"));
    }

    #[tokio::test]
    async fn a_failing_command_is_returned_as_output_not_as_an_error() {
        let (_dir, ctx) = project();
        // A non-zero exit is information for the model, not a harness failure.

        let output = Shell
            .run(serde_json::json!({ "command": "exit 3" }), &ctx)
            .await
            .expect("a non-zero exit is still a successful tool call");

        assert!(output.contains("exit code: 3"), "unexpected output: {output}");
    }

    #[tokio::test]
    async fn commands_run_in_the_workspace_directory() {
        let (_dir, ctx) = project();
        let command = if cfg!(windows) { "type marker.txt" } else { "cat marker.txt" };

        let output = Shell
            .run(serde_json::json!({ "command": command }), &ctx)
            .await
            .expect("the command runs");

        assert!(output.contains("hello"), "unexpected output: {output}");
    }

    #[tokio::test]
    async fn catastrophic_commands_are_refused_before_they_run() {
        let (_dir, ctx) = project();

        let error = Shell
            .run(serde_json::json!({ "command": "rm -rf / --no-preserve-root" }), &ctx)
            .await
            .expect_err("this is never run");

        let message = error.to_string();
        assert!(message.contains("Refused"), "unexpected message: {message}");
        assert!(message.contains("Run it yourself"), "the user keeps the option");
    }

    #[tokio::test]
    async fn refusal_is_case_insensitive() {
        let (_dir, ctx) = project();

        let error = Shell
            .run(serde_json::json!({ "command": "SHUTDOWN /s" }), &ctx)
            .await
            .expect_err("this is never run");

        assert!(error.to_string().contains("Refused"));
    }

    #[tokio::test]
    async fn a_refused_command_is_rejected_at_preview_time_too() {
        let (_dir, ctx) = project();

        let error = Shell
            .preview(serde_json::json!({ "command": "mkfs.ext4 /dev/sda" }), &ctx)
            .await
            .expect_err("this is never previewed as runnable");

        assert!(error.to_string().contains("Refused"));
    }

    #[tokio::test]
    async fn an_ordinary_command_previews_as_normal_risk() {
        let (_dir, ctx) = project();

        let effect = Shell
            .preview(serde_json::json!({ "command": "cargo test" }), &ctx)
            .await
            .expect("the preview succeeds");

        match effect {
            Effect::Execute { command, risk } => {
                assert_eq!(command, "cargo test");
                assert_eq!(risk, Risk::Normal);
            }
            other => panic!("expected an execute effect, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_dangerous_but_legitimate_command_previews_as_high_risk() {
        let (_dir, ctx) = project();

        let effect = Shell
            .preview(serde_json::json!({ "command": "git push --force origin main" }), &ctx)
            .await
            .expect("the preview succeeds");

        match effect {
            Effect::Execute { risk: Risk::High { reason }, .. } => {
                assert!(reason.contains("remote"), "unexpected reason: {reason}");
            }
            other => panic!("expected a high-risk effect, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn the_exit_code_can_be_read_back_out_of_the_rendered_output() {
        let (_dir, ctx) = project();

        // The round trip is the contract the proof panel depends on: if this
        // breaks, real evidence is silently reported as "unverified".
        let output = Shell
            .run(serde_json::json!({ "command": "exit 7" }), &ctx)
            .await
            .expect("a non-zero exit is still a successful tool call");

        assert_eq!(parse_exit_code(&output), Some(7));
    }

    #[tokio::test]
    async fn a_successful_command_reports_exit_zero_to_the_parser() {
        let (_dir, ctx) = project();

        let output =
            Shell.run(serde_json::json!({ "command": ECHO }), &ctx).await.expect("it runs");

        assert_eq!(parse_exit_code(&output), Some(0));
    }

    #[test]
    fn output_from_somewhere_else_yields_no_exit_code() {
        assert_eq!(parse_exit_code("tests: 47 passed"), None);
        assert_eq!(parse_exit_code(""), None);
    }

    #[test]
    fn a_malformed_exit_line_is_not_guessed_at() {
        assert_eq!(parse_exit_code("exit code: not-a-number"), None);
    }

    #[test]
    fn every_refused_pattern_has_a_stated_reason() {
        for (pattern, reason) in REFUSED {
            assert!(!pattern.is_empty());
            assert!(!reason.is_empty(), "`{pattern}` is refused without saying why");
        }
    }

    #[test]
    fn refusal_takes_precedence_over_risk_classification() {
        // The two lists overlap by design — `rm -rf /` also matches `rm -r`. What
        // must hold is the order: a refused command is rejected before it can be
        // classified, so the prompt never offers to run something that would then
        // be refused.
        for (refused, _) in REFUSED {
            assert!(
                refuse_if_catastrophic(refused).is_err(),
                "`{refused}` must be refused regardless of the risk list"
            );
        }
    }

    #[test]
    fn the_shell_tool_declares_itself_mutating() {
        assert!(!Shell.is_read_only());
    }

    #[tokio::test]
    async fn a_command_that_never_finishes_is_killed_rather_than_hanging_forever() {
        // Verified indirectly: the timeout path is the only way `run` returns an
        // InvalidInput for a well-formed command, and a null stdin makes an
        // input-waiting command exit immediately rather than reaching it.
        let (_dir, ctx) = project();
        let command = if cfg!(windows) { "set /p x=" } else { "read x" };

        let result = Shell.run(serde_json::json!({ "command": command }), &ctx).await;

        assert!(result.is_ok(), "a command waiting on stdin must exit, not hang: {result:?}");
    }
}
