//! The comprehension check.
//!
//! Heavy AI use measurably erodes understanding of one's own codebase: developers
//! who mostly generate score worse on comprehension tests, and most maintenance of
//! agent-written code is done by humans afterwards. An agent that leaves you less
//! able to work on your project has taken something from you, whatever it gave.
//!
//! So when this is on, a run that changed code ends with one question about the
//! change it just made.
//!
//! # What the evidence says, and what this does about it
//!
//! Built from `docs/RESEARCH-Lernen-2026-09.md`. Five of its principles are load
//! bearing here, and each one rules out something that would otherwise be obvious:
//!
//! - **Retrieval, not re-reading** (g = 0.74). So it asks a question rather than
//!   printing a summary of the diff.
//! - **Interrupt only at commit points.** So it fires once, after the run, never
//!   mid-loop — and then gives feedback immediately, because the window in which
//!   an answer is still live is under a minute.
//! - **Explanations must persist.** So the explanation lands in the transcript and
//!   the session log rather than in a dialog that scrolls away.
//! - **Differentiate on prior knowledge, not learning style.** Learning styles are
//!   a neuromyth (d = 0.04). The profile tracks *concepts you have met and how you
//!   did*, which is the predictor that actually works.
//! - **No points, no badges.** Gamification lowered both intrinsic motivation and
//!   exam performance. There is no score here, and there will not be one.
//!
//! # What it is not
//!
//! It does not block anything. The change is already applied by the time it asks,
//! so refusing to proceed would protect nothing and only train people to disable
//! it. It is off unless you ask for it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// File, relative to the project root, holding what you have met before.
pub const PROFILE_FILE: &str = ".truecode/learning.toml";

/// Instructions for generating one question.
///
/// Asked for as JSON because a free-text answer cannot be graded without a second
/// model call, and a check that cannot grade is not a check.
pub const QUESTION_PROMPT: &str = "\
You have just made a change to the user's project. Write ONE multiple-choice \
question that tests whether they understood it.

Rules:
- Ask about THIS change, in THIS codebase. Not about the language in general.
- Ask about consequence or reasoning — what breaks without it, why this approach \
over the obvious alternative. Never about syntax or naming.
- Exactly one option is correct. The wrong options must be plausible to someone \
who skimmed the diff, not obviously silly.
- The explanation is what they keep. Make it worth reading even if they answered \
correctly.
- Name the concept in two or three words, lowercase, e.g. \"error propagation\", \
\"ownership\", \"cache invalidation\".

Answer with JSON only, no prose around it:
{\"concept\":\"…\",\"question\":\"…\",\"options\":[\"…\",\"…\",\"…\"],\"correct\":0,\"explanation\":\"…\"}";

/// One question about a change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Question {
    /// What this is about, in two or three words. The unit the profile tracks.
    pub concept: String,
    /// The question itself.
    pub question: String,
    /// The answers to choose between.
    pub options: Vec<String>,
    /// Index into [`Self::options`] of the correct one.
    pub correct: usize,
    /// Why. Shown either way, and kept.
    pub explanation: String,
}

impl Question {
    /// Parses a model's answer.
    ///
    /// Tolerant of a fenced code block and of prose around the JSON, because
    /// models add both despite being asked not to — and failing the whole check
    /// over a stray ```json would be a silly way to lose it.
    pub fn parse(raw: &str) -> Result<Self, LearnError> {
        let json = extract_json(raw).ok_or_else(|| LearnError::Unusable {
            detail: "no JSON object in the answer".to_owned(),
        })?;

        let question: Self = serde_json::from_str(json)
            .map_err(|err| LearnError::Unusable { detail: err.to_string() })?;

        question.validate()?;
        Ok(question)
    }

    /// Rejects a question that cannot be asked honestly.
    fn validate(&self) -> Result<(), LearnError> {
        let unusable = |detail: &str| LearnError::Unusable { detail: detail.to_owned() };

        if self.options.len() < 2 {
            return Err(unusable("fewer than two options"));
        }
        // A question whose "correct" index is out of range would mark every answer
        // wrong, which is worse than not asking at all.
        if self.correct >= self.options.len() {
            return Err(unusable("the correct answer is not one of the options"));
        }
        if self.question.trim().is_empty() || self.explanation.trim().is_empty() {
            return Err(unusable("the question or explanation is empty"));
        }
        Ok(())
    }

    /// Whether the given choice is the right one.
    #[must_use]
    pub fn is_correct(&self, chosen: usize) -> bool {
        chosen == self.correct
    }
}

/// Errors raised while asking or recording.
#[derive(Debug, thiserror::Error)]
pub enum LearnError {
    /// The model's answer could not be used as a question.
    #[error("could not build a question: {detail}")]
    Unusable {
        /// What was wrong with it.
        detail: String,
    },

    /// The profile could not be read or written.
    #[error("{operation} failed for {PROFILE_FILE}: {source}")]
    Io {
        /// What was being attempted.
        operation: &'static str,
        /// The underlying error.
        source: std::io::Error,
    },
}

/// Finds the JSON object in a model's answer.
fn extract_json(raw: &str) -> Option<&str> {
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    (end > start).then(|| &raw[start..=end])
}

/// How someone has done on one concept.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    /// How many times it has come up.
    pub seen: u32,
    /// How many of those were answered correctly.
    pub correct: u32,
}

impl Record {
    /// Whether this looks like something already understood.
    ///
    /// Used to decide when to stop explaining from first principles: the same
    /// scaffolding that helps a novice measurably hinders someone past that point.
    #[must_use]
    pub const fn looks_solid(&self) -> bool {
        self.seen >= 2 && self.correct == self.seen
    }

    /// Whether this is worth coming back to.
    #[must_use]
    pub const fn needs_revisiting(&self) -> bool {
        self.seen > 0 && self.correct * 2 <= self.seen
    }
}

/// What the user has met before, and how it went.
///
/// Deliberately just counts. There is no score, no streak and no badge: both
/// intrinsic motivation and exam performance went *down* under gamification.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    /// Concept name to record. Ordered so the file has a stable diff.
    #[serde(default)]
    pub concepts: BTreeMap<String, Record>,
}

impl Profile {
    /// Loads the profile for a project. A missing file means a fresh start.
    pub fn load(root: &Path) -> Result<Self, LearnError> {
        match std::fs::read_to_string(Self::path(root)) {
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(LearnError::Io { operation: "read", source }),
            // A corrupt profile is not worth failing a session over: it holds
            // counts, not work. Starting fresh loses less than refusing to run.
            Ok(raw) => Ok(toml::from_str(&raw).unwrap_or_default()),
        }
    }

    /// Writes the profile back.
    pub fn save(&self, root: &Path) -> Result<(), LearnError> {
        let path = Self::path(root);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|source| LearnError::Io { operation: "create directory", source })?;
        }
        let encoded = toml::to_string_pretty(self).map_err(|err| LearnError::Io {
            operation: "encode",
            source: std::io::Error::other(err.to_string()),
        })?;

        std::fs::write(&path, encoded)
            .map_err(|source| LearnError::Io { operation: "write", source })
    }

    /// Where the profile lives.
    #[must_use]
    pub fn path(root: &Path) -> PathBuf {
        root.join(PROFILE_FILE)
    }

    /// Records how an answer went.
    pub fn record(&mut self, concept: &str, correct: bool) {
        let entry = self.concepts.entry(concept.trim().to_lowercase()).or_default();
        entry.seen += 1;
        if correct {
            entry.correct += 1;
        }
    }

    /// Concepts worth revisiting, most-missed first.
    #[must_use]
    pub fn weak_spots(&self) -> Vec<(&str, Record)> {
        let mut weak: Vec<(&str, Record)> = self
            .concepts
            .iter()
            .filter(|(_, record)| record.needs_revisiting())
            .map(|(name, record)| (name.as_str(), *record))
            .collect();

        weak.sort_by_key(|(_, record)| record.correct);
        weak
    }

    /// A line reminding the model what this person has already struggled with.
    ///
    /// This is the differentiation that works: what you have met and how it went,
    /// not a self-declared learning style.
    #[must_use]
    pub fn prompt_section(&self) -> String {
        let weak = self.weak_spots();
        if weak.is_empty() {
            return String::new();
        }

        let names: Vec<&str> = weak.iter().take(5).map(|(name, _)| *name).collect();
        format!(
            "\n\nThis user has previously answered questions about {} incorrectly. If the \
             change touches one of those, prefer it as the subject and explain it more fully.",
            names.join(", ")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"{"concept":"error propagation","question":"What breaks without the ? operator here?",
        "options":["Nothing","The error is swallowed","It will not compile"],"correct":1,
        "explanation":"unwrap would panic instead of returning the error to the caller."}"#;

    #[test]
    fn a_well_formed_question_parses() {
        let question = Question::parse(VALID).expect("it is valid");

        assert_eq!(question.concept, "error propagation");
        assert_eq!(question.options.len(), 3);
        assert!(question.is_correct(1));
        assert!(!question.is_correct(0));
    }

    #[test]
    fn a_fenced_code_block_is_tolerated() {
        // Models add fences despite being told not to. Losing the check over a
        // stray ```json would be a silly way to lose it.
        let raw = format!("Here you go:\n```json\n{VALID}\n```\nHope that helps!");

        assert!(Question::parse(&raw).is_ok());
    }

    #[test]
    fn a_correct_index_outside_the_options_is_rejected() {
        let raw = r#"{"concept":"x","question":"q","options":["a","b"],"correct":5,
            "explanation":"e"}"#;

        // Accepting it would mark every possible answer wrong, which is worse
        // than not asking.
        assert!(Question::parse(raw).is_err());
    }

    #[test]
    fn a_question_with_one_option_is_rejected() {
        let raw = r#"{"concept":"x","question":"q","options":["a"],"correct":0,
            "explanation":"e"}"#;

        assert!(Question::parse(raw).is_err());
    }

    #[test]
    fn an_empty_explanation_is_rejected() {
        // The explanation is the part the user keeps; a question without one is
        // a quiz, which is not the point.
        let raw = r#"{"concept":"x","question":"q","options":["a","b"],"correct":0,
            "explanation":"  "}"#;

        assert!(Question::parse(raw).is_err());
    }

    #[test]
    fn an_answer_with_no_json_is_reported_rather_than_guessed_at() {
        assert!(Question::parse("I could not think of a good question.").is_err());
    }

    #[test]
    fn the_profile_counts_attempts_per_concept() {
        let mut profile = Profile::default();
        profile.record("Error Propagation", true);
        profile.record("error propagation", false);

        // Case and spacing must not split one concept into two.
        let record = profile.concepts.get("error propagation").expect("it is recorded");
        assert_eq!(*record, Record { seen: 2, correct: 1 });
    }

    #[test]
    fn a_concept_answered_correctly_twice_looks_solid() {
        assert!(Record { seen: 2, correct: 2 }.looks_solid());
        assert!(!Record { seen: 1, correct: 1 }.looks_solid(), "once is not evidence");
        assert!(!Record { seen: 3, correct: 2 }.looks_solid());
    }

    #[test]
    fn a_concept_missed_at_least_half_the_time_needs_revisiting() {
        assert!(Record { seen: 2, correct: 1 }.needs_revisiting());
        assert!(Record { seen: 1, correct: 0 }.needs_revisiting());
        assert!(!Record { seen: 3, correct: 2 }.needs_revisiting());
        assert!(!Record::default().needs_revisiting(), "never seen is not weak");
    }

    #[test]
    fn weak_spots_come_back_worst_first() {
        let mut profile = Profile::default();
        profile.record("ownership", false);
        profile.record("ownership", false);
        profile.record("lifetimes", true);
        profile.record("lifetimes", false);

        let weak = profile.weak_spots();

        assert_eq!(weak[0].0, "ownership", "the one they never got comes first");
        assert_eq!(weak.len(), 2);
    }

    #[test]
    fn a_fresh_profile_adds_nothing_to_the_prompt() {
        assert!(Profile::default().prompt_section().is_empty(), "it must cost no tokens");
    }

    #[test]
    fn past_difficulty_is_what_the_prompt_carries() {
        let mut profile = Profile::default();
        profile.record("ownership", false);

        let prompt = profile.prompt_section();

        assert!(prompt.contains("ownership"));
        assert!(prompt.contains("explain it more fully"));
    }

    #[test]
    fn the_profile_round_trips_through_a_file() {
        let dir = tempfile::tempdir().expect("temp dir is creatable");
        let mut profile = Profile::default();
        profile.record("async", true);

        profile.save(dir.path()).expect("it saves");
        let loaded = Profile::load(dir.path()).expect("it loads");

        assert_eq!(loaded, profile);
    }

    #[test]
    fn a_missing_profile_is_a_fresh_start_not_an_error() {
        let dir = tempfile::tempdir().expect("temp dir is creatable");

        assert_eq!(Profile::load(dir.path()).expect("it loads"), Profile::default());
    }

    #[test]
    fn a_corrupt_profile_starts_fresh_rather_than_failing_the_session() {
        let dir = tempfile::tempdir().expect("temp dir is creatable");
        std::fs::create_dir_all(dir.path().join(".truecode")).expect("dir is creatable");
        std::fs::write(Profile::path(dir.path()), "this is not toml {{{").expect("it writes");

        // It holds counts, not work. Starting over loses less than refusing to run.
        assert_eq!(Profile::load(dir.path()).expect("it loads"), Profile::default());
    }

    #[test]
    fn the_generation_prompt_forbids_the_easy_useless_question() {
        // "What is this variable called" tests nothing. The prompt has to say so.
        assert!(QUESTION_PROMPT.contains("Never about syntax or naming"));
        assert!(QUESTION_PROMPT.contains("THIS codebase"));
    }
}
