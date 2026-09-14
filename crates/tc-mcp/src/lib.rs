//! MCP client: tools from other people's servers.
//!
//! true-code is a client and never a server. It starts the servers listed in
//! `.truecode/mcp.toml`, asks each one what tools it has, and presents them to
//! the agent alongside the built-in ones.
//!
//! ```text
//! mcp.toml ─► spawn (stdio) ─► initialize ─► list_tools ─► Box<dyn Tool>
//!                                                              │
//!                                            agent ◄───────────┘
//! ```
//!
//! # Why every MCP tool needs approval
//!
//! The protocol has a `readOnlyHint` annotation, and the specification is
//! explicit that annotations are **hints** — untrusted, and not a guarantee of
//! behaviour. A server that wants its tool to run unattended has only to claim
//! it is read-only. true-code therefore treats every MCP tool as mutating, and
//! shows the call for approval before it runs. See `docs/adr/0011-mcp-client.md`.
//!
//! # Failure is per server
//!
//! One server that will not start must not stop the session. Failures are
//! collected and reported, and the tools from the servers that *did* start are
//! still offered. A missing `npx` is a reason to lose one server's tools, not
//! the whole run.

pub mod config;
mod tool;

pub use config::{ConfigError, McpConfig, ServerConfig};
pub use tool::{McpTool, qualified_name};

use std::sync::Arc;

use rmcp::ServiceExt as _;
use rmcp::transport::TokioChildProcess;
use tc_tools::Tool;

/// A server that would not start, and why.
///
/// Carried rather than logged and dropped: a tool the user configured and cannot
/// see is exactly the situation that needs an explanation on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupFailure {
    /// The name from `mcp.toml`.
    pub server: String,
    /// What went wrong, already phrased for a human.
    pub detail: String,
}

impl std::fmt::Display for StartupFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MCP server `{}` did not start: {}", self.server, self.detail)
    }
}

/// Everything that came back from starting the configured servers.
#[derive(Debug, Default)]
pub struct McpTools {
    /// Tools ready to hand to the agent.
    pub tools: Vec<Box<dyn Tool>>,
    /// Servers that failed, in configuration order.
    pub failures: Vec<StartupFailure>,
    /// Live connections, kept alive for as long as this value lives.
    ///
    /// Dropping it closes the child processes. The field exists for that reason
    /// alone, which is why it is not public: nothing should reach into it.
    connections: Vec<Arc<Connection>>,
}

impl McpTools {
    /// How many servers are connected.
    #[must_use]
    pub fn server_count(&self) -> usize {
        self.connections.len()
    }
}

/// One running server.
type Connection = rmcp::service::RunningService<rmcp::RoleClient, ()>;

/// Starts every enabled server and collects their tools.
///
/// Never fails as a whole: a server that will not start becomes a
/// [`StartupFailure`], and the rest of the session continues without it.
pub async fn connect(config: &McpConfig) -> McpTools {
    let mut result = McpTools::default();

    for (name, server) in config.enabled() {
        match start(name, server).await {
            Ok((connection, tools)) => {
                result.connections.push(connection);
                result.tools.extend(tools);
            }
            Err(detail) => {
                tracing::warn!(server = name, %detail, "MCP server did not start");
                result.failures.push(StartupFailure { server: name.clone(), detail });
            }
        }
    }

    result
}

/// Starts one server and asks it for its tools.
async fn start(
    name: &str,
    server: &ServerConfig,
) -> Result<(Arc<Connection>, Vec<Box<dyn Tool>>), String> {
    let mut command = tokio::process::Command::new(&server.command);
    command.args(&server.args);
    for (key, value) in &server.env {
        command.env(key, value);
    }

    let transport = TokioChildProcess::new(command).map_err(|err| {
        // The overwhelmingly common cause is the command not being installed,
        // and the raw OS error does not say so.
        format!("cannot run `{}`: {err} — is it installed and on your PATH?", server.command)
    })?;

    let connection =
        ().serve(transport).await.map_err(|err| format!("the MCP handshake failed: {err}"))?;

    let listed = connection
        .list_all_tools()
        .await
        .map_err(|err| format!("connected, but listing its tools failed: {err}"))?;

    let connection = Arc::new(connection);
    let tools = listed
        .into_iter()
        .map(|tool| Box::new(McpTool::new(name, &tool, Arc::clone(&connection))) as Box<dyn Tool>)
        .collect();

    Ok((connection, tools))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[tokio::test]
    async fn no_configured_servers_means_no_tools_and_no_failures() {
        let result = connect(&McpConfig::default()).await;

        assert!(result.tools.is_empty());
        assert!(result.failures.is_empty());
        assert_eq!(result.server_count(), 0);
    }

    #[tokio::test]
    async fn a_server_that_cannot_start_is_reported_rather_than_fatal() {
        // The point of the test: `connect` returns, it does not propagate.
        let mut config = McpConfig::default();
        config.servers.insert(
            "ghost".to_owned(),
            ServerConfig {
                command: "definitely-not-an-installed-binary-9f2c".to_owned(),
                args: Vec::new(),
                env: BTreeMap::default(),
                enabled: true,
            },
        );

        let result = connect(&config).await;

        assert!(result.tools.is_empty());
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.failures[0].server, "ghost");
    }

    #[tokio::test]
    async fn a_disabled_server_is_not_started() {
        let mut config = McpConfig::default();
        config.servers.insert(
            "off".to_owned(),
            ServerConfig {
                command: "definitely-not-an-installed-binary-9f2c".to_owned(),
                args: Vec::new(),
                env: BTreeMap::default(),
                enabled: false,
            },
        );

        let result = connect(&config).await;

        assert!(result.failures.is_empty(), "a disabled server must not even be attempted");
    }

    #[test]
    fn a_startup_failure_names_the_server_and_the_cause() {
        let failure =
            StartupFailure { server: "files".to_owned(), detail: "no such command".to_owned() };

        let shown = failure.to_string();

        assert!(shown.contains("files"), "unexpected: {shown}");
        assert!(shown.contains("no such command"), "unexpected: {shown}");
    }
}
