//! Tools that change files.
//!
//! Both compute their result *before* applying it, so the agent can show a diff
//! and ask. The preview is a dry run over the same code path as the real thing —
//! not a second implementation that can drift from it.
//!
//! # Why `patch` is the important one
//!
//! `write_file` replaces a whole file, which means the model must reproduce every
//! line it does not want to change. On a large file that is expensive, slow, and
//! the most common way for an agent to silently delete code it forgot to repeat.
//! `patch` replaces one exact region and refuses anything ambiguous, so the blast
//! radius of a mistake is one hunk instead of one file.

use crate::diff::FileDiff;
use crate::{Effect, Tool, ToolContext, ToolError, required_str};

/// Replaces the entire contents of a file, creating it if needed.
#[derive(Debug)]
pub struct WriteFile;

#[async_trait::async_trait]
impl Tool for WriteFile {
    fn name(&self) -> &'static str {
        "write_file"
    }

    fn description(&self) -> &'static str {
        "Create a file, or replace its entire contents. Prefer `patch` for changing \
         part of an existing file — write_file requires you to reproduce every line \
         you are not changing, and anything you omit is deleted."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path relative to the project root.",
                },
                "content": {
                    "type": "string",
                    "description": "The complete new contents of the file.",
                },
            },
            "required": ["path", "content"],
        })
    }

    fn is_read_only(&self) -> bool {
        false
    }

    async fn preview(
        &self,
        input: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<Effect, ToolError> {
        let requested = required_str(&input, self.name(), "path")?;
        let content = required_str(&input, self.name(), "content")?;
        let path = ctx.resolve(&requested)?;

        let before = read_existing(&path, &requested)?;
        Ok(Effect::Write(FileDiff::new(ctx.display_path(&path), before.as_deref(), &content)))
    }

    async fn run(&self, input: serde_json::Value, ctx: &ToolContext) -> Result<String, ToolError> {
        let requested = required_str(&input, self.name(), "path")?;
        let content = required_str(&input, self.name(), "content")?;
        let path = ctx.resolve(&requested)?;

        let before = read_existing(&path, &requested)?;
        let verb = if before.is_some() { "Updated" } else { "Created" };

        write_atomically(&path, &requested, &content)?;

        Ok(format!("{verb} {} ({} bytes).", ctx.display_path(&path), content.len()))
    }
}

/// Replaces one exact region of a file.
#[derive(Debug)]
pub struct Patch;

#[async_trait::async_trait]
impl Tool for Patch {
    fn name(&self) -> &'static str {
        "patch"
    }

    fn description(&self) -> &'static str {
        "Replace an exact snippet in a file. `old_string` must match the file \
         character for character, including indentation, and must be unique — \
         include surrounding lines until it is. Set replace_all to change every \
         occurrence instead."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path relative to the project root.",
                },
                "old_string": {
                    "type": "string",
                    "description": "Exact text to replace, including indentation.",
                },
                "new_string": {
                    "type": "string",
                    "description": "Replacement text. Use an empty string to delete.",
                },
                "replace_all": {
                    "type": "boolean",
                    "description": "Replace every occurrence. Defaults to false.",
                },
            },
            "required": ["path", "old_string", "new_string"],
        })
    }

    fn is_read_only(&self) -> bool {
        false
    }

    async fn preview(
        &self,
        input: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<Effect, ToolError> {
        let (path, before, after, _) = self.compute(&input, ctx)?;
        Ok(Effect::Write(FileDiff::new(ctx.display_path(&path), Some(&before), &after)))
    }

    async fn run(&self, input: serde_json::Value, ctx: &ToolContext) -> Result<String, ToolError> {
        // Recomputed rather than carried over from the preview: the file may have
        // changed in between, and applying a stale patch is worse than failing.
        let (path, _, after, replacements) = self.compute(&input, ctx)?;
        let requested = required_str(&input, self.name(), "path")?;

        write_atomically(&path, &requested, &after)?;

        let plural = if replacements == 1 { "occurrence" } else { "occurrences" };
        Ok(format!("Replaced {replacements} {plural} in {}.", ctx.display_path(&path)))
    }
}

impl Patch {
    /// Resolves the arguments and computes the patched content.
    ///
    /// Returns the absolute path, the current content, the new content, and how
    /// many occurrences were replaced.
    fn compute(
        &self,
        input: &serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<(std::path::PathBuf, String, String, usize), ToolError> {
        let requested = required_str(input, self.name(), "path")?;
        let old = required_str(input, self.name(), "old_string")?;
        let new = required_str(input, self.name(), "new_string")?;
        let replace_all =
            input.get("replace_all").and_then(serde_json::Value::as_bool).unwrap_or(false);

        let path = ctx.resolve(&requested)?;

        let before = read_existing(&path, &requested)?.ok_or_else(|| ToolError::Io {
            operation: "read",
            path: requested.clone(),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "file does not exist — use write_file to create it",
            ),
        })?;

        if old.is_empty() {
            return Err(ToolError::InvalidInput {
                tool: self.name(),
                detail: "`old_string` must not be empty — use write_file to create a file"
                    .to_owned(),
            });
        }

        let matches = before.matches(old.as_str()).count();

        // Both failure modes get an instruction, not just a complaint: the model
        // reads this and has to be able to fix its own call from it.
        if matches == 0 {
            return Err(ToolError::InvalidInput {
                tool: self.name(),
                detail: format!(
                    "`old_string` does not appear in {requested}. It must match character for \
                     character, including indentation and line breaks. Read the file first."
                ),
            });
        }
        if matches > 1 && !replace_all {
            return Err(ToolError::InvalidInput {
                tool: self.name(),
                detail: format!(
                    "`old_string` appears {matches} times in {requested}. Add surrounding lines \
                     until it is unique, or set replace_all to change all of them."
                ),
            });
        }

        let (after, replacements) = if replace_all {
            (before.replace(old.as_str(), &new), matches)
        } else {
            (before.replacen(old.as_str(), &new, 1), 1)
        };

        Ok((path, before, after, replacements))
    }
}

/// Reads a file's current contents, or `None` if it does not exist.
///
/// Non-UTF-8 files are refused rather than corrupted: writing text over a binary
/// file would destroy it, and we cannot show a meaningful diff of one either.
fn read_existing(path: &std::path::Path, requested: &str) -> Result<Option<String>, ToolError> {
    match std::fs::read(path) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(ToolError::Io { operation: "read", path: requested.to_owned(), source }),
        Ok(bytes) => String::from_utf8(bytes).map(Some).map_err(|_| ToolError::InvalidInput {
            tool: "write_file",
            detail: format!("`{requested}` is not a UTF-8 text file and will not be overwritten"),
        }),
    }
}

/// Writes a file via a temporary file and a rename.
///
/// A crash halfway through a direct write leaves a truncated source file, which
/// is a far worse outcome than a failed tool call. The rename is atomic on both
/// Unix and Windows when the temporary file sits on the same volume, which is why
/// it is created beside the target rather than in the system temp directory.
fn write_atomically(
    path: &std::path::Path,
    requested: &str,
    content: &str,
) -> Result<(), ToolError> {
    let io_error = |operation: &'static str| {
        move |source: std::io::Error| ToolError::Io {
            operation,
            path: requested.to_owned(),
            source,
        }
    };

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(io_error("create directory"))?;
    }

    let temporary = path.with_extension(format!(
        "{}.tc-tmp",
        path.extension().and_then(std::ffi::OsStr::to_str).unwrap_or("")
    ));

    std::fs::write(&temporary, content).map_err(io_error("write"))?;

    if let Err(err) = std::fs::rename(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(io_error("replace")(err));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ChangeKind;

    fn project() -> (tempfile::TempDir, ToolContext) {
        let dir = tempfile::tempdir().expect("temp dir is creatable");
        std::fs::create_dir_all(dir.path().join("src")).expect("dir is creatable");
        std::fs::write(dir.path().join("src/lib.rs"), "fn one() {}\nfn two() {}\n")
            .expect("file is writable");
        let ctx = ToolContext::new(dir.path());
        (dir, ctx)
    }

    fn read(dir: &tempfile::TempDir, relative: &str) -> String {
        std::fs::read_to_string(dir.path().join(relative)).expect("the file is readable")
    }

    #[tokio::test]
    async fn write_file_creates_a_new_file() {
        let (dir, ctx) = project();
        let input = serde_json::json!({ "path": "src/new.rs", "content": "fn new() {}\n" });

        WriteFile.run(input, &ctx).await.expect("the write succeeds");

        assert_eq!(read(&dir, "src/new.rs"), "fn new() {}\n");
    }

    #[tokio::test]
    async fn write_file_creates_missing_parent_directories() {
        let (dir, ctx) = project();
        let input = serde_json::json!({ "path": "a/b/c.rs", "content": "x\n" });

        WriteFile.run(input, &ctx).await.expect("the write succeeds");

        assert_eq!(read(&dir, "a/b/c.rs"), "x\n");
    }

    #[tokio::test]
    async fn write_file_previews_a_creation_without_writing_anything() {
        let (dir, ctx) = project();
        let input = serde_json::json!({ "path": "src/new.rs", "content": "fn new() {}\n" });

        let effect = WriteFile.preview(input, &ctx).await.expect("the preview succeeds");

        match effect {
            Effect::Write(diff) => {
                assert_eq!(diff.kind, ChangeKind::Create);
                assert_eq!(diff.added, 1);
            }
            other => panic!("expected a write effect, got {other:?}"),
        }
        assert!(!dir.path().join("src/new.rs").exists(), "a preview must not touch the disk");
    }

    #[tokio::test]
    async fn write_file_refuses_to_escape_the_workspace() {
        let (_dir, ctx) = project();
        let input = serde_json::json!({ "path": "../evil.rs", "content": "x" });

        let error = WriteFile.run(input, &ctx).await.expect_err("the path escapes");

        assert!(matches!(error, ToolError::Path(_)));
    }

    #[tokio::test]
    async fn write_file_refuses_to_overwrite_a_binary_file() {
        let (dir, ctx) = project();
        std::fs::write(dir.path().join("logo.bin"), [0xFF, 0xFE, 0x00]).expect("file is writable");
        let input = serde_json::json!({ "path": "logo.bin", "content": "text" });

        let error = WriteFile.run(input, &ctx).await.expect_err("binary files are protected");

        assert!(matches!(error, ToolError::InvalidInput { .. }));
        assert_eq!(std::fs::read(dir.path().join("logo.bin")).unwrap(), [0xFF, 0xFE, 0x00]);
    }

    #[tokio::test]
    async fn write_file_leaves_no_temporary_file_behind() {
        let (dir, ctx) = project();
        let input = serde_json::json!({ "path": "src/new.rs", "content": "x\n" });

        WriteFile.run(input, &ctx).await.expect("the write succeeds");

        let leftovers: Vec<_> = std::fs::read_dir(dir.path().join("src"))
            .expect("the directory is readable")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains("tc-tmp"))
            .collect();
        assert!(leftovers.is_empty(), "temporary files must be renamed away");
    }

    #[tokio::test]
    async fn patch_replaces_a_unique_snippet() {
        let (dir, ctx) = project();
        let input = serde_json::json!({
            "path": "src/lib.rs",
            "old_string": "fn one() {}",
            "new_string": "fn uno() {}",
        });

        Patch.run(input, &ctx).await.expect("the patch applies");

        assert_eq!(read(&dir, "src/lib.rs"), "fn uno() {}\nfn two() {}\n");
    }

    #[tokio::test]
    async fn patch_refuses_an_ambiguous_match_and_says_how_to_fix_it() {
        let (dir, ctx) = project();
        std::fs::write(dir.path().join("src/dup.rs"), "same\nsame\n").expect("file is writable");
        let input = serde_json::json!({
            "path": "src/dup.rs",
            "old_string": "same",
            "new_string": "other",
        });

        let error = Patch.run(input, &ctx).await.expect_err("the match is ambiguous");

        let message = error.to_string();
        assert!(message.contains("2 times"), "unexpected message: {message}");
        assert!(message.contains("replace_all"), "the model must be told the way out");
        assert_eq!(read(&dir, "src/dup.rs"), "same\nsame\n", "nothing may change on refusal");
    }

    #[tokio::test]
    async fn patch_replaces_every_occurrence_when_asked() {
        let (dir, ctx) = project();
        std::fs::write(dir.path().join("src/dup.rs"), "same\nsame\n").expect("file is writable");
        let input = serde_json::json!({
            "path": "src/dup.rs",
            "old_string": "same",
            "new_string": "other",
            "replace_all": true,
        });

        let output = Patch.run(input, &ctx).await.expect("the patch applies");

        assert_eq!(read(&dir, "src/dup.rs"), "other\nother\n");
        assert!(output.contains('2'), "the count is reported: {output}");
    }

    #[tokio::test]
    async fn patch_reports_a_missing_snippet_with_guidance() {
        let (_dir, ctx) = project();
        let input = serde_json::json!({
            "path": "src/lib.rs",
            "old_string": "fn nowhere() {}",
            "new_string": "x",
        });

        let error = Patch.run(input, &ctx).await.expect_err("the snippet is absent");

        let message = error.to_string();
        assert!(message.contains("does not appear"), "unexpected message: {message}");
        assert!(message.contains("Read the file first"));
    }

    #[tokio::test]
    async fn patch_can_delete_a_snippet() {
        let (dir, ctx) = project();
        let input = serde_json::json!({
            "path": "src/lib.rs",
            "old_string": "fn one() {}\n",
            "new_string": "",
        });

        Patch.run(input, &ctx).await.expect("the patch applies");

        assert_eq!(read(&dir, "src/lib.rs"), "fn two() {}\n");
    }

    #[tokio::test]
    async fn patch_rejects_an_empty_old_string() {
        let (_dir, ctx) = project();
        let input = serde_json::json!({
            "path": "src/lib.rs",
            "old_string": "",
            "new_string": "x",
        });

        let error = Patch.run(input, &ctx).await.expect_err("an empty match is meaningless");

        assert!(matches!(error, ToolError::InvalidInput { .. }));
    }

    #[tokio::test]
    async fn patch_refuses_to_create_a_missing_file() {
        let (_dir, ctx) = project();
        let input = serde_json::json!({
            "path": "src/absent.rs",
            "old_string": "a",
            "new_string": "b",
        });

        let error = Patch.run(input, &ctx).await.expect_err("the file does not exist");

        assert!(error.to_string().contains("write_file"), "point at the right tool");
    }

    #[tokio::test]
    async fn patch_previews_the_change_without_applying_it() {
        let (dir, ctx) = project();
        let input = serde_json::json!({
            "path": "src/lib.rs",
            "old_string": "fn one() {}",
            "new_string": "fn uno() {}",
        });

        let effect = Patch.preview(input, &ctx).await.expect("the preview succeeds");

        match effect {
            Effect::Write(diff) => {
                assert_eq!(diff.kind, ChangeKind::Modify);
                assert!(diff.text.contains("-fn one() {}"), "unexpected diff: {}", diff.text);
                assert!(diff.text.contains("+fn uno() {}"));
            }
            other => panic!("expected a write effect, got {other:?}"),
        }
        assert_eq!(
            read(&dir, "src/lib.rs"),
            "fn one() {}\nfn two() {}\n",
            "preview writes nothing"
        );
    }

    #[test]
    fn both_editing_tools_declare_themselves_mutating() {
        assert!(!WriteFile.is_read_only());
        assert!(!Patch.is_read_only());
    }
}
