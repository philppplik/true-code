//! The constraint ledger.
//!
//! The single most expensive measured failure of CLI coding agents is not that
//! they cannot code — it is that they violate a rule the user stated explicitly.
//! Roughly half of CLI agent failures are instruction-following failures, and
//! saying "no new dependencies" once in a prompt does not survive ten turns of
//! context.
//!
//! Telling the model harder does not fix this. Two things do:
//!
//! 1. **Persist the rules** in `.truecode/constraints.toml`, so they are restated
//!    in full on every request instead of decaying with the conversation.
//! 2. **Check them mechanically** against the change, before it is applied. A rule
//!    that is only in the prompt is a wish; a rule that is checked is a rule.
//!
//! # What can actually be checked
//!
//! Only three shapes, deliberately:
//!
//! * `forbid_added` — a regular expression that must not appear in any **added**
//!   line, optionally scoped with `in_files` / `except_files`.
//! * `forbid_files` — globs the change must not touch at all.
//! * `forbid_command` — a regular expression a shell command must not match.
//!
//! Anything that cannot be expressed that way belongs in `[[reminder]]`, which is
//! sent to the model and **labelled as unchecked**. Pretending a wish is a check
//! is worse than having no ledger: it produces a green tick nobody earned.

use globset::{Glob, GlobSet, GlobSetBuilder};
use regex::Regex;
use serde::Deserialize;

use crate::diff::FileDiff;

/// File, relative to the project root, holding the rules.
pub const CONSTRAINTS_FILE: &str = ".truecode/constraints.toml";

/// Errors raised while loading the ledger.
#[derive(Debug, thiserror::Error)]
pub enum ConstraintError {
    /// The file exists but is not valid TOML, or has unknown keys.
    #[error("{CONSTRAINTS_FILE} is invalid: {0}")]
    Parse(Box<toml::de::Error>),

    /// A rule's pattern could not be compiled.
    #[error("constraint `{description}` has an invalid {field}: {detail}")]
    Pattern {
        /// The rule that is wrong.
        description: String,
        /// Which field failed — `forbid_added`, `forbid_files`, …
        field: &'static str,
        /// What was wrong with it.
        detail: String,
    },

    /// The file could not be read.
    #[error("cannot read {CONSTRAINTS_FILE}: {0}")]
    Read(std::io::Error),
}

/// One rule as written in the file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConstraint {
    description: String,
    #[serde(default)]
    forbid_added: Option<String>,
    #[serde(default)]
    forbid_files: Vec<String>,
    #[serde(default)]
    forbid_command: Option<String>,
    #[serde(default)]
    in_files: Vec<String>,
    #[serde(default)]
    except_files: Vec<String>,
}

/// An unchecked note to the model.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawReminder {
    text: String,
}

/// The whole file.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLedger {
    #[serde(default)]
    constraint: Vec<RawConstraint>,
    #[serde(default)]
    reminder: Vec<RawReminder>,
}

/// A compiled, checkable rule.
#[derive(Debug)]
pub struct Constraint {
    /// What the rule says, in the user's words. Shown to the model and the user.
    pub description: String,
    added: Option<Regex>,
    files: Option<GlobSet>,
    command: Option<Regex>,
    scope: Option<GlobSet>,
    except: Option<GlobSet>,
}

/// A rule that was broken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// The rule's description.
    pub description: String,
    /// What specifically broke it — an added line, a path, or a command.
    pub evidence: String,
}

impl Violation {
    /// One line for a prompt, a log or a tool result.
    #[must_use]
    pub fn summary(&self) -> String {
        format!("{} — {}", self.description, self.evidence)
    }
}

/// Every rule in force for a project.
#[derive(Debug, Default)]
pub struct Ledger {
    constraints: Vec<Constraint>,
    reminders: Vec<String>,
}

impl Ledger {
    /// Loads the ledger for a project.
    ///
    /// A missing file means no rules, which is the common case and not an error.
    /// A *malformed* file is an error: silently ignoring a rule the user wrote
    /// down is the exact failure this module exists to prevent.
    pub fn load(root: &std::path::Path) -> Result<Self, ConstraintError> {
        let path = root.join(CONSTRAINTS_FILE);

        let raw = match std::fs::read_to_string(&path) {
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(err) => return Err(ConstraintError::Read(err)),
            Ok(raw) => raw,
        };

        let parsed: RawLedger =
            toml::from_str(&raw).map_err(|err| ConstraintError::Parse(Box::new(err)))?;

        let constraints = parsed
            .constraint
            .into_iter()
            .map(Constraint::compile)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self { constraints, reminders: parsed.reminder.into_iter().map(|r| r.text).collect() })
    }

    /// Whether there is anything to state or check.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.constraints.is_empty() && self.reminders.is_empty()
    }

    /// The checkable rules.
    #[must_use]
    pub fn constraints(&self) -> &[Constraint] {
        &self.constraints
    }

    /// The unchecked notes.
    #[must_use]
    pub fn reminders(&self) -> &[String] {
        &self.reminders
    }

    /// Rules broken by a file change.
    #[must_use]
    pub fn check_diff(&self, diff: &FileDiff) -> Vec<Violation> {
        self.constraints.iter().filter_map(|rule| rule.check_diff(diff)).collect()
    }

    /// Rules broken by a shell command.
    #[must_use]
    pub fn check_command(&self, command: &str) -> Vec<Violation> {
        self.constraints.iter().filter_map(|rule| rule.check_command(command)).collect()
    }

    /// The ledger as the model sees it.
    ///
    /// Restated in full on every request, which is the point: a rule mentioned
    /// once in turn one has no force by turn ten. Checked and unchecked rules are
    /// labelled differently, because the model should know which ones the harness
    /// will actually catch it on.
    #[must_use]
    pub fn prompt_section(&self) -> String {
        if self.is_empty() {
            return String::new();
        }

        let mut out = String::from("\n\nThe user has set rules for this project.");

        if !self.constraints.is_empty() {
            out.push_str(
                "\nThese are checked automatically before any change is applied, and a \
                 violation is shown to the user:\n",
            );
            for rule in &self.constraints {
                out.push_str("  - ");
                out.push_str(&rule.description);
                out.push('\n');
            }
        }

        if !self.reminders.is_empty() {
            out.push_str("\nThese are not checked automatically. Following them is on you:\n");
            for note in &self.reminders {
                out.push_str("  - ");
                out.push_str(note);
                out.push('\n');
            }
        }

        out.push_str(
            "\nIf a rule makes the task impossible, say so and stop. Do not work around it.",
        );
        out
    }
}

impl Constraint {
    /// Compiles one rule, reporting which field was wrong rather than just that
    /// something was.
    fn compile(raw: RawConstraint) -> Result<Self, ConstraintError> {
        let pattern_error = |field: &'static str, detail: String| ConstraintError::Pattern {
            description: raw.description.clone(),
            field,
            detail,
        };

        let regex = |field: &'static str, source: &Option<String>| {
            source
                .as_ref()
                .map(|value| Regex::new(value).map_err(|err| pattern_error(field, err.to_string())))
                .transpose()
        };

        let globs = |field: &'static str, patterns: &[String]| {
            if patterns.is_empty() {
                return Ok(None);
            }
            let mut builder = GlobSetBuilder::new();
            for pattern in patterns {
                builder
                    .add(Glob::new(pattern).map_err(|err| pattern_error(field, err.to_string()))?);
            }
            builder.build().map(Some).map_err(|err| pattern_error(field, err.to_string()))
        };

        Ok(Self {
            added: regex("forbid_added", &raw.forbid_added)?,
            command: regex("forbid_command", &raw.forbid_command)?,
            files: globs("forbid_files", &raw.forbid_files)?,
            scope: globs("in_files", &raw.in_files)?,
            except: globs("except_files", &raw.except_files)?,
            description: raw.description,
        })
    }

    /// Whether this rule applies to the given path.
    fn applies_to(&self, path: &str) -> bool {
        if self.except.as_ref().is_some_and(|set| set.is_match(path)) {
            return false;
        }
        self.scope.as_ref().is_none_or(|set| set.is_match(path))
    }

    /// Checks a file change, returning the first violation found.
    fn check_diff(&self, diff: &FileDiff) -> Option<Violation> {
        if let Some(forbidden) = &self.files
            && forbidden.is_match(&diff.path)
        {
            return Some(Violation {
                description: self.description.clone(),
                evidence: format!("changes {}", diff.path),
            });
        }

        let pattern = self.added.as_ref()?;
        if !self.applies_to(&diff.path) {
            return None;
        }

        let line = diff.added_lines.iter().find(|line| pattern.is_match(line))?;
        Some(Violation {
            description: self.description.clone(),
            evidence: format!("{}: {}", diff.path, line.trim()),
        })
    }

    /// Checks a shell command.
    fn check_command(&self, command: &str) -> Option<Violation> {
        let pattern = self.command.as_ref()?;
        pattern.is_match(command).then(|| Violation {
            description: self.description.clone(),
            evidence: format!("runs `{command}`"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ledger_from(toml_source: &str) -> Ledger {
        let dir = tempfile::tempdir().expect("temp dir is creatable");
        std::fs::create_dir_all(dir.path().join(".truecode")).expect("dir is creatable");
        std::fs::write(dir.path().join(CONSTRAINTS_FILE), toml_source).expect("file is writable");
        Ledger::load(dir.path()).expect("the ledger is valid")
    }

    fn diff_adding(path: &str, line: &str) -> FileDiff {
        FileDiff::new(path, Some("existing\n"), &format!("existing\n{line}\n"))
    }

    #[test]
    fn a_project_without_a_ledger_has_no_rules() {
        let dir = tempfile::tempdir().expect("temp dir is creatable");
        let ledger = Ledger::load(dir.path()).expect("a missing file is not an error");

        assert!(ledger.is_empty());
        assert!(ledger.prompt_section().is_empty(), "an empty ledger costs no tokens");
    }

    #[test]
    fn a_forbidden_pattern_in_an_added_line_is_caught() {
        let ledger = ledger_from(
            r#"
            [[constraint]]
            description = "No unwrap() in production code"
            forbid_added = '\.unwrap\(\)'
            "#,
        );

        let violations = ledger.check_diff(&diff_adding("src/lib.rs", "let x = y.unwrap();"));

        assert_eq!(violations.len(), 1);
        assert!(violations[0].evidence.contains("y.unwrap()"), "the evidence must be specific");
    }

    #[test]
    fn a_forbidden_pattern_in_an_unchanged_line_is_not_a_violation() {
        let ledger = ledger_from(
            r#"
            [[constraint]]
            description = "No unwrap()"
            forbid_added = '\.unwrap\(\)'
            "#,
        );

        // The offending line is pre-existing; the change only adds a comment.
        let diff = FileDiff::new(
            "src/lib.rs",
            Some("let x = y.unwrap();\n"),
            "let x = y.unwrap();\n// a note\n",
        );

        assert!(ledger.check_diff(&diff).is_empty(), "only what the change adds is judged");
    }

    #[test]
    fn a_removed_line_never_violates_a_rule() {
        let ledger = ledger_from(
            r#"
            [[constraint]]
            description = "No unwrap()"
            forbid_added = '\.unwrap\(\)'
            "#,
        );

        let diff = FileDiff::new("src/lib.rs", Some("let x = y.unwrap();\n"), "");

        assert!(ledger.check_diff(&diff).is_empty(), "deleting the offender is a fix, not a fault");
    }

    #[test]
    fn a_rule_can_be_scoped_to_certain_files() {
        let ledger = ledger_from(
            r#"
            [[constraint]]
            description = "No new dependencies"
            forbid_added = '^\s*[a-z0-9_-]+ = '
            in_files = ["**/Cargo.toml"]
            "#,
        );

        assert_eq!(ledger.check_diff(&diff_adding("Cargo.toml", "serde = \"1\"")).len(), 1);
        assert!(
            ledger.check_diff(&diff_adding("src/lib.rs", "let x = 1;")).is_empty(),
            "the rule must not fire outside its scope"
        );
    }

    #[test]
    fn a_scoped_rule_can_carve_out_exceptions() {
        let ledger = ledger_from(
            r#"
            [[constraint]]
            description = "No unwrap() outside tests"
            forbid_added = '\.unwrap\(\)'
            in_files = ["**/*.rs"]
            except_files = ["**/tests/**"]
            "#,
        );

        assert_eq!(ledger.check_diff(&diff_adding("src/lib.rs", "a.unwrap();")).len(), 1);
        assert!(
            ledger.check_diff(&diff_adding("crates/x/tests/it.rs", "a.unwrap();")).is_empty(),
            "tests are exempt"
        );
    }

    #[test]
    fn a_forbidden_path_is_caught_whatever_the_change_contains() {
        let ledger = ledger_from(
            r#"
            [[constraint]]
            description = "Do not touch CI"
            forbid_files = [".github/workflows/**"]
            "#,
        );

        let violations = ledger.check_diff(&diff_adding(".github/workflows/ci.yml", "  - run: x"));

        assert_eq!(violations.len(), 1);
        assert!(violations[0].evidence.contains("ci.yml"));
    }

    #[test]
    fn a_forbidden_command_is_caught() {
        let ledger = ledger_from(
            r#"
            [[constraint]]
            description = "Never publish"
            forbid_command = 'cargo publish|npm publish'
            "#,
        );

        assert_eq!(ledger.check_command("cargo publish --dry-run").len(), 1);
        assert!(ledger.check_command("cargo test").is_empty());
    }

    #[test]
    fn a_file_rule_does_not_fire_on_a_command_and_vice_versa() {
        let ledger = ledger_from(
            r#"
            [[constraint]]
            description = "Do not touch CI"
            forbid_files = [".github/**"]
            "#,
        );

        assert!(ledger.check_command("ls .github").is_empty(), "a path rule is not a command rule");
    }

    #[test]
    fn every_broken_rule_is_reported_not_just_the_first() {
        let ledger = ledger_from(
            r#"
            [[constraint]]
            description = "No unwrap()"
            forbid_added = '\.unwrap\(\)'

            [[constraint]]
            description = "Do not touch the parser"
            forbid_files = ["**/parser.rs"]
            "#,
        );

        let violations = ledger.check_diff(&diff_adding("src/parser.rs", "a.unwrap();"));

        assert_eq!(violations.len(), 2, "the user must see the whole picture, not one line of it");
    }

    #[test]
    fn the_prompt_separates_checked_rules_from_unchecked_notes() {
        let ledger = ledger_from(
            r#"
            [[constraint]]
            description = "No unwrap()"
            forbid_added = '\.unwrap\(\)'

            [[reminder]]
            text = "Prefer small commits"
            "#,
        );

        let prompt = ledger.prompt_section();

        assert!(prompt.contains("checked automatically"), "unexpected prompt: {prompt}");
        assert!(prompt.contains("not checked automatically"), "honesty about what is a wish");
        assert!(prompt.contains("No unwrap()"));
        assert!(prompt.contains("Prefer small commits"));
    }

    #[test]
    fn the_prompt_tells_the_model_to_stop_rather_than_work_around_a_rule() {
        let ledger = ledger_from(
            r#"
            [[constraint]]
            description = "No new dependencies"
            forbid_added = 'x'
            "#,
        );

        assert!(ledger.prompt_section().contains("Do not work around it"));
    }

    #[test]
    fn a_malformed_pattern_names_the_rule_and_the_field() {
        let dir = tempfile::tempdir().expect("temp dir is creatable");
        std::fs::create_dir_all(dir.path().join(".truecode")).expect("dir is creatable");
        std::fs::write(
            dir.path().join(CONSTRAINTS_FILE),
            "[[constraint]]\ndescription = \"Broken\"\nforbid_added = '('\n",
        )
        .expect("file is writable");

        let error = Ledger::load(dir.path()).expect_err("the regex is invalid");
        let message = error.to_string();

        assert!(message.contains("Broken"), "unexpected message: {message}");
        assert!(message.contains("forbid_added"));
    }

    #[test]
    fn a_typo_in_a_key_fails_loudly_instead_of_dropping_the_rule() {
        let dir = tempfile::tempdir().expect("temp dir is creatable");
        std::fs::create_dir_all(dir.path().join(".truecode")).expect("dir is creatable");
        std::fs::write(
            dir.path().join(CONSTRAINTS_FILE),
            "[[constraint]]\ndescription = \"x\"\nforbid_add = 'y'\n",
        )
        .expect("file is writable");

        // Silently ignoring a rule the user wrote down is the exact failure this
        // module exists to prevent.
        assert!(matches!(Ledger::load(dir.path()), Err(ConstraintError::Parse(_))));
    }

    #[test]
    fn a_violation_summarises_the_rule_and_the_evidence() {
        let violation = Violation {
            description: "No unwrap()".to_owned(),
            evidence: "src/lib.rs: a.unwrap();".to_owned(),
        };
        assert_eq!(violation.summary(), "No unwrap() — src/lib.rs: a.unwrap();");
    }
}
