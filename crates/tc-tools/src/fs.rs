//! Filesystem tools: reading, listing, and finding files by pattern.
//!
//! All three respect `.gitignore` where they walk, via the `ignore` crate — the
//! same rules the developer's own tooling uses. Without that, the first `glob`
//! on a Rust project returns forty thousand paths from `target/` and the session
//! is over before it starts.

use std::fmt::Write as _;

use globset::{Glob as GlobPattern, GlobSetBuilder};
use ignore::WalkBuilder;

use crate::{Tool, ToolContext, ToolError, required_str};

/// Reads a file, optionally a line range.
#[derive(Debug)]
pub struct ReadFile;

/// Maximum number of entries any listing tool returns.
///
/// Beyond this a listing stops being information and becomes noise that crowds
/// out the actual task.
const MAX_ENTRIES: usize = 500;

#[async_trait::async_trait]
impl Tool for ReadFile {
    fn name(&self) -> &'static str {
        "read_file"
    }

    fn description(&self) -> &'static str {
        "Read a text file from the project. Optionally pass start_line and end_line \
         (1-based, inclusive) to read only part of a large file."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path relative to the project root, e.g. src/main.rs",
                },
                "start_line": { "type": "integer", "minimum": 1 },
                "end_line": { "type": "integer", "minimum": 1 },
            },
            "required": ["path"],
        })
    }

    fn is_read_only(&self) -> bool {
        true
    }

    async fn run(&self, input: serde_json::Value, ctx: &ToolContext) -> Result<String, ToolError> {
        let requested = required_str(&input, self.name(), "path")?;
        let path = ctx.resolve(&requested)?;

        let content = std::fs::read(&path).map_err(|source| ToolError::Io {
            operation: "read",
            path: requested.clone(),
            source,
        })?;

        // Binary content in the context window is garbage that costs real money
        // and teaches the model nothing.
        let Ok(text) = String::from_utf8(content) else {
            return Err(ToolError::InvalidInput {
                tool: self.name().to_owned(),
                detail: format!("`{requested}` is not a UTF-8 text file"),
            });
        };

        let start = input.get("start_line").and_then(serde_json::Value::as_u64);
        let end = input.get("end_line").and_then(serde_json::Value::as_u64);

        Ok(ctx.truncate(number_lines(&text, start, end)))
    }
}

/// Renders `text` with line numbers, limited to the requested range.
///
/// Line numbers are not decoration: without them the model cannot refer to a
/// location precisely, and every later patch becomes guesswork.
fn number_lines(text: &str, start: Option<u64>, end: Option<u64>) -> String {
    let start = start.unwrap_or(1).max(1);
    let end = end.unwrap_or(u64::MAX);

    let mut out = String::with_capacity(text.len() + text.len() / 8);
    for (index, line) in text.lines().enumerate() {
        let number = index as u64 + 1;
        if number < start {
            continue;
        }
        if number > end {
            break;
        }
        let _ = writeln!(out, "{number:>6}  {line}");
    }

    if out.is_empty() {
        return "[the requested range contains no lines]".to_owned();
    }
    out
}

/// Lists the direct contents of a directory.
#[derive(Debug)]
pub struct ListDir;

#[async_trait::async_trait]
impl Tool for ListDir {
    fn name(&self) -> &'static str {
        "list_dir"
    }

    fn description(&self) -> &'static str {
        "List the files and directories directly inside a project directory. \
         Ignored files (.gitignore) are omitted."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Directory relative to the project root. Defaults to the root.",
                },
            },
        })
    }

    fn is_read_only(&self) -> bool {
        true
    }

    async fn run(&self, input: serde_json::Value, ctx: &ToolContext) -> Result<String, ToolError> {
        let requested =
            input.get("path").and_then(serde_json::Value::as_str).unwrap_or(".").to_owned();
        let path = ctx.resolve(&requested)?;

        let mut entries: Vec<String> = WalkBuilder::new(&path)
            .max_depth(Some(1))
            .hidden(false)
            .git_ignore(true)
            // Without this, ignore rules apply only inside a git repository.
            // A project folder is a project folder either way.
            .require_git(false)
            .build()
            .filter_map(Result::ok)
            // The walk yields the directory itself first; it is not a child.
            .filter(|entry| entry.path() != path)
            .map(|entry| {
                let name = ctx.display_path(entry.path());
                if entry.file_type().is_some_and(|kind| kind.is_dir()) {
                    format!("{name}/")
                } else {
                    name
                }
            })
            .take(MAX_ENTRIES)
            .collect();

        entries.sort();

        if entries.is_empty() {
            return Ok(format!("`{requested}` is empty."));
        }
        Ok(ctx.truncate(entries.join("\n")))
    }
}

/// Finds files by glob pattern.
#[derive(Debug)]
pub struct Glob;

#[async_trait::async_trait]
impl Tool for Glob {
    fn name(&self) -> &'static str {
        "glob"
    }

    fn description(&self) -> &'static str {
        "Find files in the project by glob pattern, e.g. **/*.rs or src/**/mod.rs. \
         Use this to locate files when you do not know the exact path."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern, matched against paths relative to the root.",
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

        let glob = GlobPattern::new(&pattern).map_err(|source| ToolError::InvalidInput {
            tool: self.name().to_owned(),
            detail: format!("`{pattern}` is not a valid glob: {source}"),
        })?;
        let mut builder = GlobSetBuilder::new();
        builder.add(glob);
        let set = builder.build().map_err(|source| ToolError::InvalidInput {
            tool: self.name().to_owned(),
            detail: source.to_string(),
        })?;

        let mut matches: Vec<String> = WalkBuilder::new(ctx.root())
            .hidden(false)
            .git_ignore(true)
            // Without this, ignore rules apply only inside a git repository.
            // A project folder is a project folder either way.
            .require_git(false)
            .build()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))
            .map(|entry| ctx.display_path(entry.path()))
            .filter(|relative| set.is_match(relative.as_str()))
            .take(MAX_ENTRIES)
            .collect();

        matches.sort();

        if matches.is_empty() {
            return Ok(format!("No files match `{pattern}`."));
        }
        Ok(ctx.truncate(matches.join("\n")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a small project with an ignored `target/` directory.
    fn project() -> (tempfile::TempDir, ToolContext) {
        let dir = tempfile::tempdir().expect("temp dir is creatable");
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).expect("dir is creatable");
        std::fs::create_dir_all(root.join("target")).expect("dir is creatable");
        std::fs::write(root.join(".gitignore"), "target/\n").expect("file is writable");
        std::fs::write(root.join("src/main.rs"), "fn main() {\n    todo!()\n}\n")
            .expect("file is writable");
        std::fs::write(root.join("src/lib.rs"), "pub mod a;\n").expect("file is writable");
        std::fs::write(root.join("target/huge.rs"), "generated\n").expect("file is writable");
        let ctx = ToolContext::new(root);
        (dir, ctx)
    }

    #[tokio::test]
    async fn read_file_returns_numbered_lines() {
        let (_guard, ctx) = project();
        let output = ReadFile
            .run(serde_json::json!({ "path": "src/main.rs" }), &ctx)
            .await
            .expect("the file is readable");

        assert!(output.contains("     1  fn main() {"), "unexpected output: {output}");
        assert!(output.contains("     2      todo!()"));
    }

    #[tokio::test]
    async fn read_file_honours_a_line_range() {
        let (_guard, ctx) = project();
        let output = ReadFile
            .run(serde_json::json!({ "path": "src/main.rs", "start_line": 2, "end_line": 2 }), &ctx)
            .await
            .expect("the file is readable");

        assert!(output.contains("todo!()"));
        assert!(!output.contains("fn main"), "lines outside the range must not appear");
    }

    #[tokio::test]
    async fn read_file_refuses_to_escape_the_workspace() {
        let (_guard, ctx) = project();
        let error = ReadFile
            .run(serde_json::json!({ "path": "../../../etc/passwd" }), &ctx)
            .await
            .expect_err("the path escapes");

        assert!(matches!(error, ToolError::Path(_)));
    }

    #[tokio::test]
    async fn read_file_rejects_binary_content() {
        let (guard, ctx) = project();
        std::fs::write(guard.path().join("logo.bin"), [0xFF, 0xFE, 0x00, 0x01])
            .expect("file is writable");

        let error = ReadFile
            .run(serde_json::json!({ "path": "logo.bin" }), &ctx)
            .await
            .expect_err("binary content is refused");

        assert!(matches!(error, ToolError::InvalidInput { .. }));
    }

    #[tokio::test]
    async fn read_file_reports_a_missing_file_clearly() {
        let (_guard, ctx) = project();
        let error = ReadFile
            .run(serde_json::json!({ "path": "src/nope.rs" }), &ctx)
            .await
            .expect_err("the file does not exist");

        assert!(matches!(error, ToolError::Io { operation: "read", .. }));
    }

    #[tokio::test]
    async fn read_file_requires_a_path() {
        let (_guard, ctx) = project();
        let error = ReadFile.run(serde_json::json!({}), &ctx).await.expect_err("path is required");

        assert!(matches!(error, ToolError::InvalidInput { .. }));
    }

    #[tokio::test]
    async fn read_file_truncates_oversized_content() {
        let (guard, _ctx) = project();
        let ctx = ToolContext::new(guard.path()).with_max_output_bytes(64);
        std::fs::write(guard.path().join("big.txt"), "line\n".repeat(500))
            .expect("file is writable");

        let output = ReadFile
            .run(serde_json::json!({ "path": "big.txt" }), &ctx)
            .await
            .expect("the file is readable");

        assert!(output.contains("truncated"));
    }

    #[tokio::test]
    async fn list_dir_marks_directories_and_skips_ignored_ones() {
        let (_guard, ctx) = project();
        let output = ListDir.run(serde_json::json!({}), &ctx).await.expect("the root lists");

        assert!(output.contains("src/"), "directories are marked: {output}");
        assert!(!output.contains("target"), "ignored directories must not appear: {output}");
    }

    #[tokio::test]
    async fn list_dir_defaults_to_the_workspace_root() {
        let (_guard, ctx) = project();
        let explicit = ListDir.run(serde_json::json!({ "path": "." }), &ctx).await.unwrap();
        let implicit = ListDir.run(serde_json::json!({}), &ctx).await.unwrap();
        assert_eq!(explicit, implicit);
    }

    #[tokio::test]
    async fn glob_finds_files_and_respects_gitignore() {
        let (_guard, ctx) = project();
        let output = Glob
            .run(serde_json::json!({ "pattern": "**/*.rs" }), &ctx)
            .await
            .expect("the pattern is valid");

        assert!(output.contains("src/main.rs"), "unexpected output: {output}");
        assert!(output.contains("src/lib.rs"));
        assert!(!output.contains("target/huge.rs"), "ignored files must not appear: {output}");
    }

    #[tokio::test]
    async fn glob_reports_no_matches_rather_than_an_empty_string() {
        let (_guard, ctx) = project();
        let output = Glob
            .run(serde_json::json!({ "pattern": "**/*.py" }), &ctx)
            .await
            .expect("the pattern is valid");

        assert!(output.contains("No files match"));
    }

    #[tokio::test]
    async fn glob_rejects_a_malformed_pattern() {
        let (_guard, ctx) = project();
        let error = Glob
            .run(serde_json::json!({ "pattern": "[" }), &ctx)
            .await
            .expect_err("the pattern is malformed");

        assert!(matches!(error, ToolError::InvalidInput { .. }));
    }

    #[test]
    fn an_empty_line_range_says_so_instead_of_returning_nothing() {
        assert!(number_lines("a\nb\n", Some(50), Some(60)).contains("no lines"));
    }
}
