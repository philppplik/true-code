//! One MCP tool, presented to the agent as an ordinary tool.

use std::sync::Arc;

use rmcp::model::{CallToolRequestParams, ContentBlock};
use tc_tools::{Effect, Risk, Tool, ToolContext, ToolError};

/// The id the model calls an MCP tool by.
///
/// Namespaced on the *user's* name for the server, so two servers offering a
/// `search` tool stay distinguishable, and so a server cannot name its tool
/// `read_file` and quietly shadow the built-in one.
#[must_use]
pub fn qualified_name(server: &str, tool: &str) -> String {
    format!("mcp__{server}__{tool}")
}

/// A tool provided by an MCP server.
pub struct McpTool {
    /// Namespaced id, e.g. `mcp__files__read_text_file`.
    name: String,
    /// Name the server itself knows it by, sent back on a call.
    remote_name: String,
    /// Prompt text shown to the model.
    description: String,
    /// The server's own schema, passed through unchanged.
    schema: serde_json::Value,
    /// The connection to call it on.
    connection: Arc<rmcp::service::RunningService<rmcp::RoleClient, ()>>,
}

impl std::fmt::Debug for McpTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The connection is a live process handle with no useful Debug output.
        f.debug_struct("McpTool").field("name", &self.name).finish_non_exhaustive()
    }
}

impl McpTool {
    /// Adapts one listed tool.
    #[must_use]
    pub fn new(
        server: &str,
        tool: &rmcp::model::Tool,
        connection: Arc<rmcp::service::RunningService<rmcp::RoleClient, ()>>,
    ) -> Self {
        let remote_name = tool.name.to_string();
        // Naming the server in the description is not decoration: the model is
        // choosing between tools it did not know existed a moment ago, and the
        // origin is the most useful thing we can tell it about them.
        let description = match &tool.description {
            Some(text) => format!("[{server}] {text}"),
            None => format!("[{server}] {remote_name}"),
        };

        Self {
            name: qualified_name(server, &remote_name),
            remote_name,
            description,
            schema: serde_json::Value::Object((*tool.input_schema).clone()),
            connection,
        }
    }
}

#[async_trait::async_trait]
impl Tool for McpTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn input_schema(&self) -> serde_json::Value {
        self.schema.clone()
    }

    /// Always false.
    ///
    /// MCP's `readOnlyHint` is documented by the specification as a hint that is
    /// not guaranteed to describe the tool's real behaviour. Trusting it would
    /// mean a third-party server could opt itself out of confirmation by
    /// asserting something about its own code, which is not a security boundary.
    fn is_read_only(&self) -> bool {
        false
    }

    async fn preview(
        &self,
        input: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<Effect, ToolError> {
        // There is no way to ask a server what a call *would* do, so the preview
        // is the call itself, shown in full. Compacted to one line only when it
        // already is one: truncating arguments would hide the part that matters.
        let arguments = serde_json::to_string(&input).unwrap_or_else(|_| input.to_string());

        Ok(Effect::Execute {
            command: format!("{} {arguments}", self.name),
            // High for everything from a server, because the risk here is not
            // that the call is known to be dangerous — it is that nothing in
            // this process knows what it does at all.
            risk: Risk::High { reason: "an MCP server decides what this does".to_owned() },
        })
    }

    async fn run(&self, input: serde_json::Value, _ctx: &ToolContext) -> Result<String, ToolError> {
        let arguments = match input {
            serde_json::Value::Object(map) => Some(map),
            serde_json::Value::Null => None,
            other => {
                return Err(ToolError::InvalidInput {
                    tool: self.name.clone(),
                    detail: format!("expected an object of arguments, got {other}"),
                });
            }
        };

        let mut params = CallToolRequestParams::new(self.remote_name.clone());
        params.arguments = arguments;

        let result =
            self.connection.call_tool(params).await.map_err(|err| ToolError::InvalidInput {
                tool: self.name.clone(),
                detail: format!("the MCP server failed the call: {err}"),
            })?;

        let text = render(&result.content);

        // A tool-level error is returned to the model as an error, not as
        // content: a model that reads a failure as output will build on it.
        if result.is_error.unwrap_or(false) {
            return Err(ToolError::InvalidInput {
                tool: self.name.clone(),
                detail: if text.is_empty() {
                    "the tool reported an error with no message".to_owned()
                } else {
                    text
                },
            });
        }

        Ok(text)
    }
}

/// Flattens MCP content blocks into the plain text the agent works in.
///
/// Non-text blocks are named rather than dropped: a model told "[image]" can
/// say it cannot see it, whereas silence reads as an empty result.
fn render(content: &[ContentBlock]) -> String {
    let mut parts = Vec::new();
    for block in content {
        match block {
            ContentBlock::Text(text) => parts.push(text.text.clone()),
            ContentBlock::Image(_) => parts.push("[image omitted]".to_owned()),
            ContentBlock::Audio(_) => parts.push("[audio omitted]".to_owned()),
            other => {
                // Unknown block kinds are serialised rather than discarded: a
                // shape we do not recognise may still be the answer.
                match serde_json::to_string(other) {
                    Ok(json) => parts.push(json),
                    Err(_) => parts.push("[unrepresentable content]".to_owned()),
                }
            }
        }
    }
    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(body: &str) -> ContentBlock {
        ContentBlock::text(body)
    }

    #[test]
    fn tool_names_are_namespaced_by_the_users_name_for_the_server() {
        // Two servers may both offer `search`; neither may shadow `read_file`.
        assert_eq!(qualified_name("files", "search"), "mcp__files__search");
        assert_ne!(qualified_name("a", "search"), qualified_name("b", "search"));
    }

    #[test]
    fn text_blocks_are_joined_in_order() {
        assert_eq!(render(&[text("first"), text("second")]), "first\nsecond");
    }

    #[test]
    fn an_empty_result_renders_as_empty_rather_than_panicking() {
        assert_eq!(render(&[]), "");
    }
}
