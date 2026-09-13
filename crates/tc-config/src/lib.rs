//! Layered configuration and secret resolution.
//!
//! Precedence, lowest to highest:
//!
//! 1. built-in defaults
//! 2. user config — `~/.config/true-code/config.toml` (platform-specific, see [`user_config_path`])
//! 3. project config — `<cwd>/.truecode/config.toml`
//! 4. environment variables (`TRUE_CODE_MODEL`, …)
//! 5. command-line flags (applied by the caller)
//!
//! **API keys are never read from config files.** They come from the environment
//! only, so that a config file can be committed to a repository without turning
//! into a credential leak. An OS keyring backend is planned for P1 (see
//! `docs/adr/0006-secret-storage.md`).

pub mod catalog;
pub mod secrets;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub use catalog::{ModelInfo, Vendor, catalog, lookup, vendor};
pub use secrets::{SecretError, Source};

/// Directory name used for project-local state, inside a repository.
pub const PROJECT_DIR: &str = ".truecode";

/// File name of a configuration file, in both the user and project directory.
pub const CONFIG_FILE: &str = "config.toml";

/// Environment variable that overrides the configured model.
pub const ENV_MODEL: &str = "TRUE_CODE_MODEL";

/// Errors raised while resolving configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// A configuration file exists but could not be read.
    #[error("cannot read config file {path}: {source}")]
    Read {
        /// The file that failed to load.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },

    /// A configuration file exists but is not valid TOML, or has unknown keys.
    #[error("config file {path} is invalid: {source}")]
    Parse {
        /// The file that failed to parse.
        path: PathBuf,
        /// The underlying parse error.
        ///
        /// Boxed so that the success path of every `Result` in this crate stays
        /// small — a fat error variant is paid for on every call, not just on failure.
        source: Box<toml::de::Error>,
    },

    /// No API key was available for the selected provider.
    #[error(transparent)]
    MissingApiKey(#[from] SecretError),

    /// The configured model is not one this provider offers.
    #[error("`{provider}` has no model `{model}` — run `truecode models` to see what it does have")]
    UnknownModel {
        /// What was asked for.
        model: String,
        /// Which provider was asked.
        provider: &'static str,
    },

    /// The configured provider is not one true-code knows.
    #[error("unknown provider `{0}` — use anthropic, openai or openrouter")]
    UnknownProvider(String),

    /// The config file could not be written.
    #[error("cannot write {path}: {source}")]
    Write {
        /// The file that could not be written.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
}

/// Spending limits for a session.
///
/// A budget is not a nice-to-have: unattended agent loops are the documented way
/// people wake up to four-figure bills. The default is deliberately small.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    /// Hard stop for a single session, in USD.
    pub session_limit_usd: f64,
    /// Percentage of the limit at which the UI starts warning.
    pub warn_at_percent: u8,
}

impl Default for Budget {
    fn default() -> Self {
        Self { session_limit_usd: 5.0, warn_at_percent: 70 }
    }
}

impl Budget {
    /// Returns `true` once spending has reached the warning threshold.
    #[must_use]
    pub fn should_warn(&self, spent_usd: f64) -> bool {
        self.session_limit_usd > 0.0
            && spent_usd >= self.session_limit_usd * f64::from(self.warn_at_percent) / 100.0
    }

    /// Returns `true` once spending has reached the hard limit.
    #[must_use]
    pub fn is_exhausted(&self, spent_usd: f64) -> bool {
        self.session_limit_usd > 0.0 && spent_usd >= self.session_limit_usd
    }
}

/// Resolved configuration for a true-code session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Which provider to talk to: `anthropic`, `openai` or `openrouter`.
    ///
    /// Decides how [`Self::model`] is read. A gateway's own ids look exactly like
    /// our prefixed form — `anthropic/claude-sonnet-4.5` is a real OpenRouter
    /// model — so the provider has to be stated rather than guessed.
    pub provider: String,
    /// The model, as the chosen provider names it.
    pub model: String,
    /// Spending limits.
    pub budget: Budget,
    /// Optional override for the system prompt.
    ///
    /// Kept prefix-stable at runtime: nothing dynamic (date, file list) may be
    /// prepended to it, or prompt caching breaks and input costs multiply.
    pub system_prompt: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            provider: catalog::DEFAULT_VENDOR.to_owned(),
            model: catalog::DEFAULT_MODEL.to_owned(),
            budget: Budget::default(),
            system_prompt: None,
        }
    }
}

impl Config {
    /// Loads configuration by applying every layer in precedence order.
    ///
    /// Missing files are not an error — they are the common case.
    pub fn load(cwd: &Path) -> Result<Self, ConfigError> {
        let mut config = Self::default();

        if let Some(path) = user_config_path() {
            config = merge(config, &path)?;
        }
        config = merge(config, &cwd.join(PROJECT_DIR).join(CONFIG_FILE))?;

        // An empty variable means "not set" here, not "select the empty model".
        if let Ok(model) = std::env::var(ENV_MODEL)
            && !model.trim().is_empty()
        {
            config.model = model;
        }
        Ok(config)
    }

    /// The configured provider.
    pub fn vendor(&self) -> Result<&'static Vendor, ConfigError> {
        catalog::vendor(&self.provider)
            .ok_or_else(|| ConfigError::UnknownProvider(self.provider.clone()))
    }

    /// Looks up the catalogue entry for the configured model.
    pub fn model_info(&self) -> Result<ModelInfo, ConfigError> {
        let vendor = self.vendor()?;
        catalog::lookup_for(vendor, &self.model).ok_or_else(|| ConfigError::UnknownModel {
            model: self.model.clone(),
            provider: vendor.id,
        })
    }

    /// Reads the API key for the configured model's provider.
    ///
    /// Environment first, then the OS keyring. Never a file true-code wrote.
    pub fn api_key(&self) -> Result<String, ConfigError> {
        let info = self.model_info()?;
        Ok(secrets::key_for(info.vendor)?.0)
    }
}

/// Records the chosen provider and model in the project's config file.
///
/// Written after the setup screen so the next run needs no flags. Only these two
/// keys are touched; anything else already in the file is kept, because a setup
/// step that silently discards someone's budget limit has done real damage.
pub fn remember_provider(root: &Path, provider: &str, model: &str) -> Result<(), ConfigError> {
    let path = root.join(PROJECT_DIR).join(CONFIG_FILE);
    let existing = std::fs::read_to_string(&path).unwrap_or_default();

    let kept: String = existing
        .lines()
        .filter(|line| {
            let key = line.split('=').next().unwrap_or("").trim();
            key != "provider" && key != "model"
        })
        .collect::<Vec<_>>()
        .join("\n");

    let mut out = format!("provider = \"{provider}\"\nmodel = \"{model}\"\n");
    if !kept.trim().is_empty() {
        out.push_str(kept.trim());
        out.push('\n');
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|source| ConfigError::Write { path: path.clone(), source })?;
    }
    std::fs::write(&path, out).map_err(|source| ConfigError::Write { path, source })
}

/// Path of the user-level configuration file, if a home directory exists.
#[must_use]
pub fn user_config_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "true-code")
        .map(|dirs| dirs.config_dir().join(CONFIG_FILE))
}

/// Applies one configuration file on top of `base`, if the file exists.
fn merge(base: Config, path: &Path) -> Result<Config, ConfigError> {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(base),
        Err(source) => return Err(ConfigError::Read { path: path.to_path_buf(), source }),
    };

    tracing::debug!(path = %path.display(), "applying config layer");

    let layer: ConfigLayer = toml::from_str(&raw).map_err(|source| ConfigError::Parse {
        path: path.to_path_buf(),
        source: Box::new(source),
    })?;

    Ok(Config {
        provider: layer.provider.unwrap_or(base.provider),
        model: layer.model.unwrap_or(base.model),
        budget: layer.budget.unwrap_or(base.budget),
        system_prompt: layer.system_prompt.or(base.system_prompt),
    })
}

/// A single configuration file. Every field is optional so that a layer can
/// override one setting without restating the rest.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigLayer {
    provider: Option<String>,
    model: Option<String>,
    budget: Option<Budget>,
    system_prompt: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_point_at_a_model_that_exists_in_the_catalogue() {
        let config = Config::default();
        assert!(config.model_info().is_ok(), "default model must be in the catalogue");
    }

    #[test]
    fn budget_warns_before_it_stops() {
        let budget = Budget { session_limit_usd: 10.0, warn_at_percent: 70 };
        assert!(!budget.should_warn(6.9));
        assert!(budget.should_warn(7.0));
        assert!(!budget.is_exhausted(9.99));
        assert!(budget.is_exhausted(10.0));
    }

    #[test]
    fn a_zero_budget_means_unlimited_rather_than_instantly_exhausted() {
        let budget = Budget { session_limit_usd: 0.0, warn_at_percent: 70 };
        assert!(!budget.is_exhausted(1_000.0));
        assert!(!budget.should_warn(1_000.0));
    }

    #[test]
    fn a_missing_project_config_is_not_an_error() {
        let empty = std::env::temp_dir().join("true-code-test-no-config");
        let config =
            merge(Config::default(), &empty.join(CONFIG_FILE)).expect("absent files are skipped");
        assert_eq!(config, Config::default());
    }

    #[test]
    fn a_layer_overrides_only_the_keys_it_sets() {
        let layer: ConfigLayer = toml::from_str("model = \"openai/gpt-5\"").expect("layer parses");
        assert_eq!(layer.model.as_deref(), Some("openai/gpt-5"));
        assert!(layer.budget.is_none());
        assert!(layer.system_prompt.is_none());
    }

    #[test]
    fn unknown_keys_are_rejected_instead_of_silently_ignored() {
        let result: Result<ConfigLayer, _> = toml::from_str("modle = \"typo\"");
        assert!(result.is_err(), "a typo must fail loudly, not be dropped");
    }
}
