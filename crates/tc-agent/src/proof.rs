//! Evidence instead of the word "done".
//!
//! A large share of agent sessions end with the agent reporting a status that is
//! not true — not from dishonesty, but because nothing ever required it to be
//! true. "Done" is free. A build that exited 0 is not.
//!
//! So true-code does not ask the model whether it succeeded. It reports what it
//! **observed**: which files changed, which verification commands ran, and what
//! they exited with. Everything here is a fact the harness watched happen.
//!
//! # The important case is the empty one
//!
//! When files changed and nothing verified them, the verdict is **unverified**,
//! stated in red, with what to do about it. That is the whole feature. Anything
//! can print a green tick after a successful run; the value is in refusing to
//! print one that was not earned.
//!
//! What it deliberately does **not** do: read the model's prose and decide whether
//! it sounded confident. Claims are not evidence, including well-phrased ones.

use std::fmt::Write as _;

use tc_tools::{CheckKind, Violation};

/// A verification command the harness watched run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observed {
    /// The command line.
    pub command: String,
    /// What it proves.
    pub kind: CheckKind,
    /// What it exited with. Zero means it passed.
    pub exit_code: i32,
}

impl Observed {
    /// Whether the command succeeded.
    #[must_use]
    pub const fn passed(&self) -> bool {
        self.exit_code == 0
    }
}

/// What the evidence adds up to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Nothing was changed and nothing was checked.
    NothingToShow,
    /// A check ran and failed. Stated before anything else.
    Failing,
    /// Tests ran and passed.
    Verified,
    /// Something was checked, but not behaviour.
    Partial,
    /// Files changed and nothing checked them.
    Unverified,
}

impl Verdict {
    /// The one-word verdict.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::NothingToShow => "nothing to show",
            Self::Failing => "FAILING",
            Self::Verified => "verified",
            Self::Partial => "partly verified",
            Self::Unverified => "UNVERIFIED",
        }
    }

    /// Whether this should be shown as an alarm rather than as information.
    #[must_use]
    pub const fn is_alarming(self) -> bool {
        matches!(self, Self::Failing | Self::Unverified)
    }

    /// What the user should do next, if anything.
    #[must_use]
    pub const fn advice(self) -> Option<&'static str> {
        match self {
            Self::Failing => Some("A check failed. The change is applied but not working."),
            Self::Unverified => Some(
                "Nothing here has been checked. Run `truecode verify`, or use \
                 --permission-mode full so the agent can run the tests itself.",
            ),
            Self::Partial => Some("Nothing ran the tests, so behaviour is unproven."),
            Self::NothingToShow | Self::Verified => None,
        }
    }
}

/// What a run actually did, as observed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Proof {
    /// Files the run changed, in the order they were first touched.
    pub changed: Vec<String>,
    /// Verification commands that ran, in order.
    pub checks: Vec<Observed>,
    /// Project rules the run broke.
    pub violations: Vec<Violation>,
}

impl Proof {
    /// Records a file the run changed.
    ///
    /// Repeated edits to one file count once: the question the panel answers is
    /// "what did this touch", not "how many times".
    pub fn record_change(&mut self, path: impl Into<String>) {
        let path = path.into();
        if !self.changed.contains(&path) {
            self.changed.push(path);
        }
    }

    /// Records a command the harness watched run.
    ///
    /// Commands that prove nothing are dropped rather than listed: a panel padded
    /// with `git status` invites the reader to skim past the line that matters.
    pub fn record_check(&mut self, command: impl Into<String>, exit_code: i32) {
        let command = command.into();
        let kind = tc_tools::verify::classify(&command);
        if kind == CheckKind::Other {
            return;
        }
        self.checks.push(Observed { command, kind, exit_code });
    }

    /// Records a broken rule.
    pub fn record_violation(&mut self, violation: Violation) {
        self.violations.push(violation);
    }

    /// Whether there is anything worth showing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.changed.is_empty() && self.checks.is_empty() && self.violations.is_empty()
    }

    /// What the evidence adds up to.
    #[must_use]
    pub fn verdict(&self) -> Verdict {
        // A failure outranks everything: a run that changed code and broke the
        // build must not be summarised by what else went well.
        if self.checks.iter().any(|check| !check.passed()) {
            return Verdict::Failing;
        }
        if self.checks.iter().any(|check| check.kind == CheckKind::Test) {
            return Verdict::Verified;
        }
        if !self.checks.is_empty() {
            return Verdict::Partial;
        }
        if self.changed.is_empty() {
            return Verdict::NothingToShow;
        }
        Verdict::Unverified
    }

    /// The panel as plain text, for headless output and logs.
    #[must_use]
    pub fn render(&self) -> String {
        if self.is_empty() {
            return String::new();
        }

        let mut out = String::from("proof\n");

        match self.changed.len() {
            0 => out.push_str("  changed   nothing\n"),
            1 => {
                let _ = writeln!(out, "  changed   {}", self.changed[0]);
            }
            n => {
                let _ = writeln!(out, "  changed   {n} files");
                for path in &self.changed {
                    let _ = writeln!(out, "              {path}");
                }
            }
        }

        if self.checks.is_empty() {
            out.push_str("  checks    none ran\n");
        } else {
            for check in &self.checks {
                let mark = if check.passed() { "ok  " } else { "FAIL" };
                let _ = writeln!(
                    out,
                    "  {mark}      {:<6} {}  (exit {})",
                    check.kind.label(),
                    check.command,
                    check.exit_code
                );
            }
        }

        for violation in &self.violations {
            let _ = writeln!(out, "  rule      broken: {}", violation.summary());
        }

        let verdict = self.verdict();
        let _ = writeln!(out, "  verdict   {}", verdict.label());
        if let Some(advice) = verdict.advice() {
            let _ = writeln!(out, "            {advice}");
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn changed_only() -> Proof {
        let mut proof = Proof::default();
        proof.record_change("src/lib.rs");
        proof
    }

    #[test]
    fn a_change_with_no_checks_is_unverified() {
        assert_eq!(changed_only().verdict(), Verdict::Unverified);
    }

    #[test]
    fn the_unverified_verdict_says_what_to_do_about_it() {
        let advice = Verdict::Unverified.advice().expect("there is advice");
        assert!(advice.contains("truecode verify"), "unexpected advice: {advice}");
        assert!(advice.contains("--permission-mode full"));
    }

    #[test]
    fn passing_tests_earn_a_verified_verdict() {
        let mut proof = changed_only();
        proof.record_check("cargo test --workspace", 0);

        assert_eq!(proof.verdict(), Verdict::Verified);
    }

    #[test]
    fn a_build_alone_is_only_partly_verified() {
        let mut proof = changed_only();
        proof.record_check("cargo build", 0);

        // It compiles. That says nothing about whether it works.
        assert_eq!(proof.verdict(), Verdict::Partial);
        assert!(Verdict::Partial.advice().expect("advice").contains("behaviour is unproven"));
    }

    #[test]
    fn a_failing_check_outranks_everything_else() {
        let mut proof = changed_only();
        proof.record_check("cargo build", 0);
        proof.record_check("cargo test", 1);

        assert_eq!(
            proof.verdict(),
            Verdict::Failing,
            "a run that broke the tests must not be summarised by what went well"
        );
    }

    #[test]
    fn a_failing_lint_is_still_a_failure() {
        let mut proof = changed_only();
        proof.record_check("cargo test", 0);
        proof.record_check("cargo clippy -- -D warnings", 101);

        assert_eq!(proof.verdict(), Verdict::Failing);
    }

    #[test]
    fn a_run_that_changed_nothing_and_checked_nothing_shows_nothing() {
        assert_eq!(Proof::default().verdict(), Verdict::NothingToShow);
        assert!(Proof::default().render().is_empty(), "an empty panel is noise");
    }

    #[test]
    fn checks_without_changes_still_count_as_verification() {
        let mut proof = Proof::default();
        proof.record_check("cargo test", 0);

        assert_eq!(proof.verdict(), Verdict::Verified);
    }

    #[test]
    fn a_command_that_proves_nothing_is_not_listed() {
        let mut proof = changed_only();
        proof.record_check("git status", 0);

        assert!(proof.checks.is_empty(), "padding invites skimming past the line that matters");
        assert_eq!(proof.verdict(), Verdict::Unverified, "and it earns no credit");
    }

    #[test]
    fn repeated_edits_to_one_file_are_listed_once() {
        let mut proof = Proof::default();
        proof.record_change("src/lib.rs");
        proof.record_change("src/lib.rs");
        proof.record_change("src/main.rs");

        assert_eq!(proof.changed, vec!["src/lib.rs".to_owned(), "src/main.rs".to_owned()]);
    }

    #[test]
    fn the_panel_states_plainly_that_nothing_ran() {
        let rendered = changed_only().render();

        assert!(rendered.contains("checks    none ran"), "unexpected panel:\n{rendered}");
        assert!(rendered.contains("UNVERIFIED"));
    }

    #[test]
    fn the_panel_shows_each_check_with_its_exit_code() {
        let mut proof = changed_only();
        proof.record_check("cargo test --workspace", 0);
        proof.record_check("cargo clippy", 101);

        let rendered = proof.render();

        assert!(rendered.contains("cargo test --workspace  (exit 0)"), "panel:\n{rendered}");
        assert!(rendered.contains("FAIL"), "a failure must be visible at a glance");
        assert!(rendered.contains("exit 101"));
    }

    #[test]
    fn broken_rules_appear_in_the_panel() {
        let mut proof = changed_only();
        proof.record_violation(Violation {
            description: "No unwrap()".to_owned(),
            evidence: "src/lib.rs: a.unwrap();".to_owned(),
        });

        assert!(proof.render().contains("rule      broken: No unwrap()"));
    }

    #[test]
    fn only_a_failure_or_a_missing_check_is_treated_as_alarming() {
        assert!(Verdict::Failing.is_alarming());
        assert!(Verdict::Unverified.is_alarming());
        assert!(!Verdict::Verified.is_alarming());
        assert!(!Verdict::Partial.is_alarming());
    }
}
