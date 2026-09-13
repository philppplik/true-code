//! What counts as evidence in this project.
//!
//! An agent saying "done" is a claim. An agent saying "`cargo test` exited 0" is
//! evidence — but only if something actually observed that exit code. This module
//! answers two questions:
//!
//! 1. **Which commands would verify this project?** Detected from the files that
//!    are there, so `truecode verify` works without configuration.
//! 2. **Was a command that just ran one of them?** So a `cargo test` the *model*
//!    chose to run still counts, and a `cat README.md` does not.
//!
//! The classification is keyword-based and therefore approximate. It errs toward
//! [`CheckKind::Other`], which contributes nothing to a verdict — over-claiming
//! evidence would defeat the entire point.

use std::path::Path;

/// What kind of assurance a command provides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckKind {
    /// It compiles.
    Build,
    /// It behaves — the only kind that alone justifies "verified".
    Test,
    /// It is clean by the project's own standards.
    Lint,
    /// Something else. Counts as activity, never as evidence.
    Other,
}

impl CheckKind {
    /// A short label for the proof panel.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Test => "test",
            Self::Lint => "lint",
            Self::Other => "ran",
        }
    }
}

/// A command that verifies something.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Check {
    /// What it proves.
    pub kind: CheckKind,
    /// The command line.
    pub command: &'static str,
}

/// A recognised kind of project.
#[derive(Debug, Clone, Copy)]
pub struct Profile {
    /// Human-readable name.
    pub name: &'static str,
    /// File whose presence identifies the project.
    pub marker: &'static str,
    /// Commands that verify it, cheapest first.
    pub checks: &'static [Check],
}

/// Known project types, in detection order.
///
/// First match wins, so a Rust workspace that also has a `package.json` for its
/// docs site is still verified as Rust.
static PROFILES: &[Profile] = &[
    Profile {
        name: "Rust",
        marker: "Cargo.toml",
        checks: &[
            Check { kind: CheckKind::Build, command: "cargo build --workspace" },
            Check { kind: CheckKind::Test, command: "cargo test --workspace" },
            Check { kind: CheckKind::Lint, command: "cargo clippy --workspace -- -D warnings" },
        ],
    },
    Profile {
        name: "Go",
        marker: "go.mod",
        checks: &[
            Check { kind: CheckKind::Build, command: "go build ./..." },
            Check { kind: CheckKind::Test, command: "go test ./..." },
            Check { kind: CheckKind::Lint, command: "go vet ./..." },
        ],
    },
    Profile {
        name: "Python",
        marker: "pyproject.toml",
        checks: &[
            Check { kind: CheckKind::Test, command: "pytest" },
            Check { kind: CheckKind::Lint, command: "ruff check ." },
        ],
    },
    Profile {
        name: "Node",
        marker: "package.json",
        // Only `test`: `build` and `lint` scripts are conventional but not
        // guaranteed, and a check that fails because the script is missing
        // teaches the user to ignore failures.
        checks: &[Check { kind: CheckKind::Test, command: "npm test" }],
    },
];

/// Identifies the project in `root`, if it is one we know.
#[must_use]
pub fn detect(root: &Path) -> Option<&'static Profile> {
    PROFILES.iter().find(|profile| root.join(profile.marker).exists())
}

/// Patterns that identify a lint. Checked first: `cargo clippy --tests` contains
/// "test" but proves nothing about behaviour.
const LINT_PATTERNS: &[&str] =
    &["clippy", "lint", "go vet", "ruff", "eslint", "fmt --check", "mypy"];

/// Patterns that identify a test run.
const TEST_PATTERNS: &[&str] = &["test", "pytest", "jest", "vitest", "spec"];

/// Patterns that identify a build.
const BUILD_PATTERNS: &[&str] = &["build", "compile", "cargo check", "tsc", "make"];

/// Guesses what a command proves.
///
/// Order matters: `cargo clippy --tests` contains "test" but is a lint, so lint
/// patterns are tried first.
#[must_use]
pub fn classify(command: &str) -> CheckKind {
    let lowered = command.to_lowercase();
    let matches = |patterns: &[&str]| patterns.iter().any(|needle| lowered.contains(needle));

    if matches(LINT_PATTERNS) {
        CheckKind::Lint
    } else if matches(TEST_PATTERNS) {
        CheckKind::Test
    } else if matches(BUILD_PATTERNS) {
        CheckKind::Build
    } else {
        CheckKind::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project_with(marker: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("temp dir is creatable");
        std::fs::write(dir.path().join(marker), "").expect("file is writable");
        dir
    }

    #[test]
    fn a_rust_project_is_detected_by_its_manifest() {
        let dir = project_with("Cargo.toml");
        let profile = detect(dir.path()).expect("a Rust project is recognised");

        assert_eq!(profile.name, "Rust");
        assert!(profile.checks.iter().any(|check| check.kind == CheckKind::Test));
    }

    #[test]
    fn an_unknown_project_yields_no_profile() {
        let dir = tempfile::tempdir().expect("temp dir is creatable");
        assert!(detect(dir.path()).is_none(), "guessing would produce commands that do not exist");
    }

    #[test]
    fn the_first_matching_profile_wins() {
        let dir = project_with("Cargo.toml");
        std::fs::write(dir.path().join("package.json"), "{}").expect("file is writable");

        // A Rust workspace with a docs site is still a Rust project.
        assert_eq!(detect(dir.path()).expect("detected").name, "Rust");
    }

    #[test]
    fn every_profile_offers_a_test_command() {
        for profile in PROFILES {
            assert!(
                profile.checks.iter().any(|check| check.kind == CheckKind::Test),
                "{} has no way to prove behaviour",
                profile.name
            );
        }
    }

    #[test]
    fn test_commands_are_recognised() {
        assert_eq!(classify("cargo test --workspace"), CheckKind::Test);
        assert_eq!(classify("pytest -q"), CheckKind::Test);
        assert_eq!(classify("npm test"), CheckKind::Test);
    }

    #[test]
    fn a_lint_that_mentions_tests_is_still_a_lint() {
        // `cargo clippy --all-targets` covers tests but proves nothing about
        // behaviour; counting it as a test would manufacture evidence.
        assert_eq!(classify("cargo clippy --tests -- -D warnings"), CheckKind::Lint);
    }

    #[test]
    fn build_commands_are_recognised() {
        assert_eq!(classify("cargo build --release"), CheckKind::Build);
        assert_eq!(classify("go build ./..."), CheckKind::Build);
    }

    #[test]
    fn an_unrelated_command_proves_nothing() {
        assert_eq!(classify("cat README.md"), CheckKind::Other);
        assert_eq!(classify("git status"), CheckKind::Other);
        assert_eq!(classify("ls -la"), CheckKind::Other);
    }

    #[test]
    fn classification_ignores_case() {
        assert_eq!(classify("CARGO TEST"), CheckKind::Test);
    }

    #[test]
    fn every_kind_has_a_label() {
        for kind in [CheckKind::Build, CheckKind::Test, CheckKind::Lint, CheckKind::Other] {
            assert!(!kind.label().is_empty());
        }
    }
}
