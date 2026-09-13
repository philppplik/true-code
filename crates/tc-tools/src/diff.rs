//! Diff rendering for change previews.
//!
//! A write the user cannot inspect first is the single fastest way to lose their
//! trust. Every mutating tool therefore produces a diff *before* it runs, and the
//! agent asks before applying it.
//!
//! The output is plain unified diff, which is deliberate: developers already read
//! it fluently, it pastes into a review, and it needs no legend.

use similar::{ChangeTag, TextDiff};

/// Lines of unchanged context shown around each change.
const CONTEXT_LINES: usize = 3;

/// Cap on the rendered diff, in bytes.
///
/// A confirmation prompt that does not fit on screen does not get read, and an
/// unread prompt is worse than no prompt — it trains people to press `y`.
const MAX_DIFF_BYTES: usize = 12_000;

/// How a change affects a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// The file does not exist yet.
    Create,
    /// The file exists and its contents change.
    Modify,
}

/// A rendered preview of one file change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiff {
    /// Path shown to the user, relative to the workspace root.
    pub path: String,
    /// Whether the file is being created or modified.
    pub kind: ChangeKind,
    /// Unified diff text, possibly truncated.
    pub text: String,
    /// Number of lines added.
    pub added: usize,
    /// Number of lines removed.
    pub removed: usize,
}

impl FileDiff {
    /// Builds a diff between the current and proposed contents of a file.
    #[must_use]
    pub fn new(path: impl Into<String>, before: Option<&str>, after: &str) -> Self {
        let kind = if before.is_some() { ChangeKind::Modify } else { ChangeKind::Create };
        let before = before.unwrap_or("");

        let diff = TextDiff::from_lines(before, after);

        let mut added = 0;
        let mut removed = 0;
        for change in diff.iter_all_changes() {
            match change.tag() {
                ChangeTag::Insert => added += 1,
                ChangeTag::Delete => removed += 1,
                ChangeTag::Equal => {}
            }
        }

        let text = render(&diff);
        Self { path: path.into(), kind, text, added, removed }
    }

    /// A one-line summary, e.g. `src/lib.rs  +12 −3`.
    #[must_use]
    pub fn summary(&self) -> String {
        let verb = match self.kind {
            ChangeKind::Create => "create",
            ChangeKind::Modify => "modify",
        };
        format!("{} {}  +{} −{}", verb, self.path, self.added, self.removed)
    }

    /// Whether the change would leave the file exactly as it is.
    ///
    /// A no-op write is worth catching: it wastes a confirmation prompt, and
    /// prompts the user learns to dismiss are prompts that stop protecting them.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.added == 0 && self.removed == 0
    }
}

/// Renders a unified diff with bounded context.
fn render(diff: &TextDiff<'_, '_, '_, str>) -> String {
    let mut out = String::new();

    for (index, group) in diff.grouped_ops(CONTEXT_LINES).iter().enumerate() {
        if index > 0 {
            out.push_str("…\n");
        }
        for op in group {
            for change in diff.iter_changes(op) {
                let sign = match change.tag() {
                    ChangeTag::Delete => '-',
                    ChangeTag::Insert => '+',
                    ChangeTag::Equal => ' ',
                };
                out.push(sign);
                out.push_str(change.value());
                // `similar` keeps the original line ending, which the final line
                // of a file may lack.
                if !change.value().ends_with('\n') {
                    out.push('\n');
                }
            }
        }
    }

    crate::truncate_to(out, MAX_DIFF_BYTES)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write as _;

    /// Builds `count` numbered lines, for tests that need a large file.
    fn numbered_lines(count: usize) -> String {
        let mut out = String::new();
        for n in 0..count {
            let _ = writeln!(out, "line {n}");
        }
        out
    }

    #[test]
    fn a_new_file_is_reported_as_a_creation() {
        let diff = FileDiff::new("src/new.rs", None, "fn main() {}\n");

        assert_eq!(diff.kind, ChangeKind::Create);
        assert_eq!(diff.added, 1);
        assert_eq!(diff.removed, 0);
        assert!(diff.text.contains("+fn main() {}"));
    }

    #[test]
    fn a_changed_line_shows_both_sides() {
        let diff = FileDiff::new("a.rs", Some("let x = 1;\n"), "let x = 2;\n");

        assert_eq!(diff.kind, ChangeKind::Modify);
        assert!(diff.text.contains("-let x = 1;"), "unexpected diff: {}", diff.text);
        assert!(diff.text.contains("+let x = 2;"));
        assert_eq!((diff.added, diff.removed), (1, 1));
    }

    #[test]
    fn unchanged_context_is_marked_with_a_space() {
        let before = "one\ntwo\nthree\n";
        let after = "one\nTWO\nthree\n";
        let diff = FileDiff::new("a.txt", Some(before), after);

        assert!(diff.text.contains(" one"), "context must be shown: {}", diff.text);
        assert!(diff.text.contains(" three"));
    }

    #[test]
    fn an_identical_write_produces_an_empty_diff() {
        let diff = FileDiff::new("a.rs", Some("same\n"), "same\n");

        assert!(diff.is_empty(), "a no-op write must not ask for confirmation");
    }

    #[test]
    fn distant_changes_are_separated_rather_than_dumping_the_whole_file() {
        let before = numbered_lines(100);
        let after = before.replace("line 1\n", "LINE 1\n").replace("line 90\n", "LINE 90\n");

        let diff = FileDiff::new("big.txt", Some(&before), &after);

        assert!(diff.text.contains('…'), "far-apart hunks must be separated: {}", diff.text);
        assert!(!diff.text.contains("line 50"), "untouched regions must be omitted");
    }

    #[test]
    fn a_huge_diff_is_truncated_with_a_marker() {
        let before = String::new();
        let after = numbered_lines(5_000);

        let diff = FileDiff::new("huge.txt", Some(&before), &after);

        assert!(diff.text.contains("truncated"), "an unreadable prompt is not a prompt");
        assert_eq!(diff.added, 5_000, "the counts stay exact even when the text is cut");
    }

    #[test]
    fn a_file_without_a_trailing_newline_still_renders_one_line_per_row() {
        let diff = FileDiff::new("a.txt", Some("no newline"), "still none");

        assert!(diff.text.ends_with('\n'), "unexpected diff: {:?}", diff.text);
        assert_eq!(diff.text.lines().count(), 2);
    }

    #[test]
    fn the_summary_names_the_action_and_the_counts() {
        let diff = FileDiff::new("src/lib.rs", Some("a\n"), "a\nb\n");
        assert_eq!(diff.summary(), "modify src/lib.rs  +1 −0");

        let created = FileDiff::new("src/new.rs", None, "a\n");
        assert_eq!(created.summary(), "create src/new.rs  +1 −0");
    }
}
