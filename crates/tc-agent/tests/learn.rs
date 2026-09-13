//! End-to-end tests for the comprehension check.
//!
//! Two properties matter more than the rest, and both are about restraint: it
//! must not fire when nobody asked for it, and a skipped or unusable question
//! must never leave a mark on the profile. A learning record that quietly
//! misreports how someone did is worse than no record — it is the basis for what
//! gets explained to them next.

mod common;

use std::sync::Arc;

use common::{agent_with_learning, answer, call_tool, config, drive, workspace};
use tc_agent::approval::ApproveAll;
use tc_agent::{AgentEvent, PermissionMode, Profile, Question};
use tc_core::Delta;
use tc_providers::Provider;

use crate::common::ScriptedProvider;

/// A turn that edits `src/lib.rs`.
fn edits_lib() -> Vec<Delta> {
    call_tool(
        "t1",
        "patch",
        serde_json::json!({ "path": "src/lib.rs", "old_string": "42", "new_string": "43" }),
    )
}

/// The model's reply to the question-generation request.
fn question_turn(json: &str) -> Vec<Delta> {
    answer(json)
}

/// A usable question.
const QUESTION: &str = r#"{"concept":"magic numbers",
    "question":"Why does the constant matter here?",
    "options":["It does not","It is the documented contract","It is faster"],
    "correct":1,
    "explanation":"Callers depend on the returned value, so changing it is a breaking change."}"#;

/// Extracts the question from a run's events.
fn asked(events: &[AgentEvent]) -> Option<Question> {
    events.iter().find_map(|event| match event {
        AgentEvent::Asked { question } => Some((**question).clone()),
        _ => None,
    })
}

#[tokio::test]
async fn nothing_is_asked_unless_teaching_is_on() {
    let dir = workspace();
    let config = config(5.0);
    let provider: Arc<dyn Provider> = Arc::new(ScriptedProvider::new(vec![
        edits_lib(),
        answer("Changed it."),
        question_turn(QUESTION),
    ]));
    let mut agent = agent_with_learning(
        provider.clone(),
        PermissionMode::Write,
        dir.path(),
        &config,
        Arc::new(ApproveAll),
        false,
    );

    let events = drive(&mut agent, "change it").await;

    assert!(asked(&events).is_none(), "an unrequested quiz is how a feature gets disabled");
}

#[tokio::test]
async fn a_question_follows_a_change_when_teaching_is_on() {
    let dir = workspace();
    let config = config(5.0);
    let provider: Arc<dyn Provider> = Arc::new(ScriptedProvider::new(vec![
        edits_lib(),
        answer("Changed it."),
        question_turn(QUESTION),
    ]));
    let mut agent = agent_with_learning(
        provider,
        PermissionMode::Write,
        dir.path(),
        &config,
        Arc::new(ApproveAll),
        true,
    );

    let events = drive(&mut agent, "change it").await;

    let question = asked(&events).expect("a question is asked");
    assert_eq!(question.concept, "magic numbers");
    assert_eq!(question.options.len(), 3);
}

#[tokio::test]
async fn a_run_that_changed_nothing_is_not_quizzed() {
    let dir = workspace();
    let config = config(5.0);
    let provider: Arc<dyn Provider> =
        Arc::new(ScriptedProvider::new(vec![answer("It already returns 42.")]));
    let mut agent = agent_with_learning(
        provider,
        PermissionMode::Write,
        dir.path(),
        &config,
        Arc::new(ApproveAll),
        true,
    );

    let events = drive(&mut agent, "what does it return?").await;

    // Asking about a change that was never made would be asking about nothing.
    assert!(asked(&events).is_none());
}

#[tokio::test]
async fn the_question_arrives_after_the_proof_panel() {
    let dir = workspace();
    let config = config(5.0);
    let provider: Arc<dyn Provider> = Arc::new(ScriptedProvider::new(vec![
        edits_lib(),
        answer("Changed it."),
        question_turn(QUESTION),
    ]));
    let mut agent = agent_with_learning(
        provider,
        PermissionMode::Write,
        dir.path(),
        &config,
        Arc::new(ApproveAll),
        true,
    );

    let events = drive(&mut agent, "change it").await;

    let proven = events.iter().position(|e| matches!(e, AgentEvent::Proven { .. }));
    let question = events.iter().position(|e| matches!(e, AgentEvent::Asked { .. }));

    assert!(proven < question, "see what happened, then be asked about it");
}

#[tokio::test]
async fn a_correct_answer_is_recorded_and_still_explains() {
    let dir = workspace();
    let config = config(5.0);
    let provider: Arc<dyn Provider> = Arc::new(ScriptedProvider::new(vec![
        edits_lib(),
        answer("Changed it."),
        question_turn(QUESTION),
    ]));
    let mut agent = agent_with_learning(
        provider,
        PermissionMode::Write,
        dir.path(),
        &config,
        Arc::new(ApproveAll),
        true,
    );

    let events = drive(&mut agent, "change it").await;
    let question = asked(&events).expect("a question is asked");

    let feedback = agent.answer(&question, 1);

    assert!(feedback.contains("Right."));
    // Someone who guessed correctly has learned nothing yet.
    assert!(feedback.contains("breaking change"), "the explanation comes either way");

    let profile = Profile::load(dir.path()).expect("the profile is written");
    let record = profile.concepts.get("magic numbers").expect("it is recorded");
    assert_eq!((record.seen, record.correct), (1, 1));
}

#[tokio::test]
async fn a_wrong_answer_names_the_right_one() {
    let dir = workspace();
    let config = config(5.0);
    let provider: Arc<dyn Provider> = Arc::new(ScriptedProvider::new(vec![
        edits_lib(),
        answer("Changed it."),
        question_turn(QUESTION),
    ]));
    let mut agent = agent_with_learning(
        provider,
        PermissionMode::Write,
        dir.path(),
        &config,
        Arc::new(ApproveAll),
        true,
    );

    let events = drive(&mut agent, "change it").await;
    let question = asked(&events).expect("a question is asked");

    let feedback = agent.answer(&question, 0);

    assert!(feedback.contains("Not quite."));
    assert!(feedback.contains("documented contract"), "unexpected feedback: {feedback}");

    let profile = Profile::load(dir.path()).expect("the profile is written");
    let record = profile.concepts.get("magic numbers").expect("it is recorded");
    assert_eq!((record.seen, record.correct), (1, 0));
}

#[tokio::test]
async fn an_unusable_question_is_dropped_without_failing_the_run() {
    let dir = workspace();
    let config = config(5.0);
    let provider: Arc<dyn Provider> = Arc::new(ScriptedProvider::new(vec![
        edits_lib(),
        answer("Changed it."),
        question_turn("I could not think of a good question."),
    ]));
    let mut agent = agent_with_learning(
        provider,
        PermissionMode::Write,
        dir.path(),
        &config,
        Arc::new(ApproveAll),
        true,
    );

    let events = drive(&mut agent, "change it").await;

    assert!(asked(&events).is_none());
    assert!(
        events.iter().any(|event| matches!(event, AgentEvent::Finished { .. })),
        "the work succeeded; the optional part failing must not look like failure"
    );
    assert!(!Profile::path(dir.path()).exists(), "nothing was asked, so nothing is recorded");
}

#[tokio::test]
async fn the_question_is_billed_like_any_other_call() {
    let dir = workspace();
    let config = config(5.0);
    let provider: Arc<dyn Provider> = Arc::new(ScriptedProvider::new(vec![
        edits_lib(),
        answer("Changed it."),
        question_turn(QUESTION),
    ]));
    let mut agent = agent_with_learning(
        provider,
        PermissionMode::Write,
        dir.path(),
        &config,
        Arc::new(ApproveAll),
        true,
    );

    let events = drive(&mut agent, "change it").await;

    // Three turns: the edit, the answer, the question. A feature that spends the
    // user's money quietly is the opposite of what this project is for.
    let turns = events.iter().filter(|e| matches!(e, AgentEvent::TurnCompleted { .. })).count();
    assert_eq!(turns, 3, "the question's cost must be reported, not hidden");
    assert!(agent.spent().usd > 0.0);
}

#[tokio::test]
async fn past_difficulty_reaches_the_question_prompt() {
    let mut profile = Profile::default();
    profile.record("ownership", false);
    profile.record("ownership", false);

    let section = profile.prompt_section();

    // Differentiation on prior knowledge, which is the predictor that works —
    // not on a self-declared learning style, which is a neuromyth.
    assert!(section.contains("ownership"));
    assert!(section.contains("explain it more fully"));
}
