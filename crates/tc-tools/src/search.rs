//! Content search.
//!
//! Structure-aware search beats semantic search for code: "where is `handle_event`
//! defined?" is a symbol question, not a similarity question. A regex over
//! gitignore-filtered files answers it exactly, instantly, and for free — which
//! is why this lands long before any embedding index.

use std::fmt::Write as _;

use ignore::WalkBuilder;
use regex::RegexBuilder;

use crate::{Tool, ToolContext, ToolError, required_str};

/// Maximum number of matching lines returned.
const MAX_MATCHES: usize = 200;

/// Files larger than this are skipped: they are generated, vendored or binary,
/// and searching them costs more than it returns.
const MAX_FILE_BYTES: u64 = 2_000_000;

/// Searches file contents by regular expression.
#[derive(Debug)]
pub struct Grep;

#[async_trait::async_trait]
impl Tool for Grep {
    fn name(&self) -> &'static str {
        "grep"
    }

    fn description(&self) -> &'static str {
        "Search file contents in the project with a regular expression. Returns \
         matching lines with their file and line number. Use this to find where \
         something is defined or used."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Regular expression to search for.",
                },
                "path": {
                    "type": "string",
                    "description": "Directory to search in. Defaults to the project root.",
                },
                "case_sensitive": {
                    "type": "boolean",
                    "description": "Defaults to false.",
                },
            },
            "required": ["pattern"],
        })
    }

    fn is_read_only(&self) -> bool {
        true
    }

    async fn run(&self, input: serde_json::Value, ctx: &ToolContext) -> Result<String, ToolError> {
        let pattern = required_str(&input, self.name(), "pattern")?;
        let case_sensitive =
            input.get("case_sensitive").and_then(serde_json::Value::as_bool).unwrap_or(false);

        let regex = RegexBuilder::new(&pattern)
            .case_insensitive(!case_sensitive)
            // A pathological pattern must fail to compile rather than hang the
            // agent loop for minutes on a large repository.
            .size_limit(1 << 20)
            .build()
            .map_err(|source| ToolError::InvalidInput {
                tool: self.name(),
                detail: format!("`{pattern}` is not a valid regular expression: {source}"),
            })?;

        let search_root =
            ctx.resolve(input.get("path").and_then(serde_json::Value::as_str).unwrap_or("."))?;

        let mut out = String::new();
        let mut found = 0usize;

        'files: for entry in WalkBuilder::new(&search_root)
            .hidden(false)
            .git_ignore(true)
            // Without this, ignore rules apply only inside a git repository.
            // A project folder is a project folder either way.
            .require_git(false)
            .build()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))
        {
            if entry.metadata().is_ok_and(|meta| meta.len() > MAX_FILE_BYTES) {
                continue;
            }
            // Unreadable or non-UTF-8 files are skipped silently: a permission
            // error on one vendored file should not abort an otherwise good search.
            let Ok(content) = std::fs::read_to_string(entry.path()) else {
                continue;
            };

            let shown = ctx.display_path(entry.path());
            for (index, line) in content.lines().enumerate() {
                if regex.is_match(line) {
                    let _ = writeln!(out, "{}:{}: {}", shown, index + 1, line.trim_end());
                    found += 1;
                    if found >= MAX_MATCHES {
                        let _ = writeln!(
                            out,
                            "\n[stopped at {MAX_MATCHES} matches — narrow the pattern]"
                        );
                        break 'files;
                    }
                }
            }
        }

        if found == 0 {
            return Ok(format!("No matches for `{pattern}`."));
        }
        Ok(ctx.truncate(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> (tempfile::TempDir, ToolContext) {
        let dir = tempfile::tempdir().expect("temp dir is creatable");
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).expect("dir is creatable");
        std::fs::create_dir_all(root.join("target")).expect("dir is creatable");
        std::fs::write(root.join(".gitignore"), "target/\n").expect("file is writable");
        std::fs::write(root.join("src/main.rs"), "fn main() {\n    handle_event();\n}\n")
            .expect("file is writable");
        std::fs::write(root.join("src/lib.rs"), "pub fn handle_event() {}\n")
            .expect("file is writable");
        std::fs::write(root.join("target/gen.rs"), "fn handle_event() {}\n")
            .expect("file is writable");
        let ctx = ToolContext::new(root);
        (dir, ctx)
    }

    #[tokio::test]
    async fn grep_reports_file_and_line_for_each_match() {
        let (_guard, ctx) = project();
        let output = Grep
            .run(serde_json::json!({ "pattern": "fn handle_event" }), &ctx)
            .await
            .expect("the pattern is valid");

        assert!(output.contains("src/lib.rs:1:"), "unexpected output: {output}");
        assert!(output.contains("pub fn handle_event"));
    }

    #[tokio::test]
    async fn grep_skips_ignored_files() {
        let (_guard, ctx) = project();
        let output = Grep
            .run(serde_json::json!({ "pattern": "handle_event" }), &ctx)
            .await
            .expect("the pattern is valid");

        assert!(!output.contains("target/gen.rs"), "ignored files must not appear: {output}");
    }

    #[tokio::test]
    async fn grep_is_case_insensitive_by_default() {
        let (_guard, ctx) = project();
        let output = Grep
            .run(serde_json::json!({ "pattern": "HANDLE_EVENT" }), &ctx)
            .await
            .expect("the pattern is valid");

        assert!(output.contains("src/lib.rs"), "unexpected output: {output}");
    }

    #[tokio::test]
    async fn grep_honours_an_explicit_case_sensitive_flag() {
        let (_guard, ctx) = project();
        let output = Grep
            .run(serde_json::json!({ "pattern": "HANDLE_EVENT", "case_sensitive": true }), &ctx)
            .await
            .expect("the pattern is valid");

        assert!(output.contains("No matches"), "unexpected output: {output}");
    }

    #[tokio::test]
    async fn grep_can_be_scoped_to_a_directory() {
        let (guard, ctx) = project();
        std::fs::create_dir_all(guard.path().join("docs")).expect("dir is creatable");
        std::fs::write(guard.path().join("docs/notes.md"), "handle_event is here\n")
            .expect("file is writable");

        let output = Grep
            .run(serde_json::json!({ "pattern": "handle_event", "path": "docs" }), &ctx)
            .await
            .expect("the pattern is valid");

        assert!(output.contains("docs/notes.md"));
        assert!(!output.contains("src/"), "the search must stay in the given directory");
    }

    #[tokio::test]
    async fn grep_refuses_to_search_outside_the_workspace() {
        let (_guard, ctx) = project();
        let error = Grep
            .run(serde_json::json!({ "pattern": "x", "path": "../.." }), &ctx)
            .await
            .expect_err("the path escapes");

        assert!(matches!(error, ToolError::Path(_)));
    }

    #[tokio::test]
    async fn grep_rejects_a_malformed_regex() {
        let (_guard, ctx) = project();
        let error = Grep
            .run(serde_json::json!({ "pattern": "fn (" }), &ctx)
            .await
            .expect_err("the regex is malformed");

        assert!(matches!(error, ToolError::InvalidInput { .. }));
    }

    #[tokio::test]
    async fn grep_says_so_when_nothing_matches() {
        let (_guard, ctx) = project();
        let output = Grep
            .run(serde_json::json!({ "pattern": "zzzz_not_present" }), &ctx)
            .await
            .expect("the pattern is valid");

        assert!(output.contains("No matches"));
    }
}
