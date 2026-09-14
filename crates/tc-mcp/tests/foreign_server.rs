//! The acceptance test for P3: a server true-code did not write, over the wire.
//!
//! The fixture is a plain Python script speaking JSON-RPC on stdio. Using an
//! rmcp-built server instead would only prove the crate agrees with itself.
//!
//! Skipped when no Python interpreter is on the PATH, with a printed reason —
//! a test that quietly passes when it did not run is worse than no test.

use std::collections::BTreeMap;
use std::path::PathBuf;

use tc_mcp::{McpConfig, ServerConfig, connect};
use tc_tools::ToolContext;

/// Locates a Python interpreter, or `None` if the test cannot run here.
fn python() -> Option<&'static str> {
    for candidate in ["python3", "python", "py"] {
        let found = std::process::Command::new(candidate)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success());
        if found {
            return Some(candidate);
        }
    }
    None
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/echo_server.py")
}

fn config(interpreter: &str) -> McpConfig {
    let mut servers = BTreeMap::new();
    servers.insert(
        "fixture".to_owned(),
        ServerConfig {
            command: interpreter.to_owned(),
            args: vec![fixture().display().to_string()],
            env: BTreeMap::new(),
            enabled: true,
        },
    );
    McpConfig { servers }
}

#[tokio::test]
async fn a_foreign_server_is_started_and_its_tools_become_usable() {
    let Some(interpreter) = python() else {
        eprintln!("skipped: no Python interpreter on PATH");
        return;
    };

    let result = connect(&config(interpreter)).await;

    assert!(result.failures.is_empty(), "server did not start: {:?}", result.failures);
    assert_eq!(result.server_count(), 1);

    let names: Vec<&str> = result.tools.iter().map(|tool| tool.name()).collect();
    assert!(names.contains(&"mcp__fixture__echo"), "got {names:?}");
    assert!(names.contains(&"mcp__fixture__explode"), "got {names:?}");

    let echo = result
        .tools
        .iter()
        .find(|tool| tool.name() == "mcp__fixture__echo")
        .expect("the echo tool is present");

    // The server's own schema is passed through, not reconstructed.
    assert_eq!(echo.input_schema()["properties"]["text"]["type"], "string");
    // Its description reaches the model, prefixed with where it came from.
    assert!(echo.description().contains("[fixture]"), "got {}", echo.description());

    let ctx = ToolContext::new(std::env::current_dir().expect("cwd"));
    let output = echo
        .run(serde_json::json!({ "text": "hello from the other side" }), &ctx)
        .await
        .expect("the call succeeds");

    assert_eq!(output, "hello from the other side");
}

#[tokio::test]
async fn a_foreign_tool_never_runs_without_approval() {
    // The security claim in ADR 0011, asserted rather than assumed: whatever the
    // server says about itself, the call is previewed and needs a decision.
    let Some(interpreter) = python() else {
        eprintln!("skipped: no Python interpreter on PATH");
        return;
    };

    let result = connect(&config(interpreter)).await;
    let echo = result.tools.first().expect("at least one tool");

    assert!(!echo.is_read_only(), "an MCP tool must never be treated as read-only");

    let ctx = ToolContext::new(std::env::current_dir().expect("cwd"));
    let effect = echo.preview(serde_json::json!({ "text": "hi" }), &ctx).await.expect("previews");

    assert!(effect.needs_approval(), "an MCP call must be confirmed before it runs");
    assert!(effect.summary().contains("mcp__fixture__"), "got {}", effect.summary());
}

#[tokio::test]
async fn a_tool_level_error_is_an_error_and_not_output() {
    // A model that reads a failure as content will build on it.
    let Some(interpreter) = python() else {
        eprintln!("skipped: no Python interpreter on PATH");
        return;
    };

    let result = connect(&config(interpreter)).await;
    let explode = result
        .tools
        .iter()
        .find(|tool| tool.name() == "mcp__fixture__explode")
        .expect("the failing tool is present");

    let ctx = ToolContext::new(std::env::current_dir().expect("cwd"));
    let err = explode
        .run(serde_json::json!({}), &ctx)
        .await
        .expect_err("a tool reporting isError must not look like success");

    assert!(err.to_string().contains("the tool refused"), "got {err}");
}
