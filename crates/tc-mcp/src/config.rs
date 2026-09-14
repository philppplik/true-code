//! Which servers to start.
//!
//! A separate `.truecode/mcp.toml` rather than a section of the main config: the
//! server list is the part people copy between projects and paste from a
//! README, and keeping it apart means a broken paste cannot take the model and
//! budget settings down with it.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

/// File name inside `.truecode/`.
pub const MCP_FILE: &str = "mcp.toml";

/// Everything read from `mcp.toml`.
#[derive(Debug, Default, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct McpConfig {
    /// Servers by name. The name becomes part of every tool id, so it is chosen
    /// by the user rather than reported by the server: a server cannot rename
    /// itself into another one's namespace.
    #[serde(default)]
    pub servers: BTreeMap<String, ServerConfig>,
}

/// One server.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    /// Executable to run, e.g. `npx`.
    pub command: String,

    /// Arguments passed to it.
    #[serde(default)]
    pub args: Vec<String>,

    /// Extra environment variables for the child process.
    ///
    /// Values are literal. There is deliberately no `${VAR}` expansion: a config
    /// file that can read the ambient environment is a config file that can
    /// forward a credential somewhere the user did not look.
    #[serde(default)]
    pub env: BTreeMap<String, String>,

    /// Set to false to keep a server configured but stopped.
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
}

const fn enabled_by_default() -> bool {
    true
}

/// Why a server list could not be read.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The file exists but could not be read.
    #[error("cannot read {path}: {source}")]
    Read {
        /// The file involved.
        path: String,
        /// The underlying error.
        source: std::io::Error,
    },

    /// The file is not valid TOML, or does not match the expected shape.
    #[error("{path} is not a valid MCP server list: {source}")]
    Parse {
        /// The file involved.
        path: String,
        /// What TOML said.
        source: toml::de::Error,
    },
}

impl McpConfig {
    /// Loads `.truecode/mcp.toml`, or returns an empty list if there is none.
    ///
    /// A missing file is not an error — most projects have no MCP servers — but
    /// a malformed one is. Silently ignoring a typo would leave the user
    /// wondering why a tool they configured never appeared.
    ///
    /// # Errors
    ///
    /// Returns an error if the file exists but cannot be read or parsed.
    pub fn load(root: &Path) -> Result<Self, ConfigError> {
        let path = root.join(tc_config::PROJECT_DIR).join(MCP_FILE);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => {
                return Err(ConfigError::Read { path: path.display().to_string(), source });
            }
        };
        toml::from_str(&text)
            .map_err(|source| ConfigError::Parse { path: path.display().to_string(), source })
    }

    /// The servers that should actually be started.
    pub fn enabled(&self) -> impl Iterator<Item = (&String, &ServerConfig)> {
        self.servers.iter().filter(|(_, server)| server.enabled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, body: &str) {
        let project = dir.join(tc_config::PROJECT_DIR);
        std::fs::create_dir_all(&project).expect("create project dir");
        std::fs::write(project.join(MCP_FILE), body).expect("write mcp.toml");
    }

    #[test]
    fn a_project_with_no_server_list_is_not_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");

        let config = McpConfig::load(dir.path()).expect("a missing file is fine");

        assert!(config.servers.is_empty());
    }

    #[test]
    fn a_malformed_list_is_reported_rather_than_ignored() {
        // Ignoring it would leave the user waiting for a tool that never arrives.
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "servers = oops");

        assert!(McpConfig::load(dir.path()).is_err());
    }

    #[test]
    fn an_unknown_key_is_refused_rather_than_silently_dropped() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "[servers.files]\ncommand = \"x\"\ncmd = \"typo\"\n");

        let err = McpConfig::load(dir.path()).expect_err("the typo is caught");

        assert!(err.to_string().contains("mcp.toml"), "unexpected: {err}");
    }

    #[test]
    fn servers_are_enabled_unless_turned_off() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(
            dir.path(),
            "[servers.files]\ncommand = \"a\"\n\n[servers.db]\ncommand = \"b\"\nenabled = false\n",
        );

        let config = McpConfig::load(dir.path()).expect("parses");
        let running: Vec<_> = config.enabled().map(|(name, _)| name.as_str()).collect();

        assert_eq!(running, ["files"]);
    }

    #[test]
    fn arguments_and_environment_are_read_as_given() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(
            dir.path(),
            "[servers.files]\ncommand = \"npx\"\nargs = [\"-y\", \"pkg\"]\n\
             [servers.files.env]\nROOT = \"/tmp\"\n",
        );

        let config = McpConfig::load(dir.path()).expect("parses");
        let server = &config.servers["files"];

        assert_eq!(server.args, ["-y", "pkg"]);
        assert_eq!(server.env["ROOT"], "/tmp");
    }
}
