//! The built-in model catalogue.
//!
//! true-code is model-neutral by design: the harness must never assume a single
//! vendor. Each entry carries everything the rest of the system needs to make a
//! request *and* to price it — those two must not drift apart, or the cost display
//! quietly starts lying.
//!
//! # Prices
//!
//! The prices below are the published list prices at the time of writing and are
//! **indicative defaults**, not a billing source of truth. Vendors change them.
//! A wrong price produces a wrong estimate, never a wrong charge; `true-code models`
//! prints what is currently assumed so the number can be checked.

use tc_core::Price;

/// Model used when nothing else is configured.
pub const DEFAULT_MODEL: &str = "anthropic/claude-sonnet-4-5";

/// Which wire protocol a model speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    /// Anthropic Messages API (`/v1/messages`, `x-api-key`).
    Anthropic,
    /// OpenAI Chat Completions API (`/v1/chat/completions`, bearer token).
    OpenAi,
}

/// A catalogue entry.
#[derive(Debug, Clone, Copy)]
pub struct ModelInfo {
    /// Qualified identifier used in config and on the command line.
    pub id: &'static str,
    /// Identifier sent to the provider's API, which may differ from [`Self::id`].
    pub api_model: &'static str,
    /// Wire protocol to use.
    pub provider: ProviderKind,
    /// Environment variable holding the API key.
    pub api_key_env: &'static str,
    /// Base URL of the API.
    pub base_url: &'static str,
    /// Size of the context window in tokens, used for the context-health meter.
    pub context_window: u32,
    /// Indicative list price.
    pub price: Price,
}

/// All models true-code knows about.
static CATALOG: &[ModelInfo] = &[
    ModelInfo {
        id: "anthropic/claude-sonnet-4-5",
        api_model: "claude-sonnet-4-5",
        provider: ProviderKind::Anthropic,
        api_key_env: "ANTHROPIC_API_KEY",
        base_url: "https://api.anthropic.com",
        context_window: 200_000,
        price: Price {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
            cached_input_per_mtok: 0.30,
            cache_write_per_mtok: 3.75,
        },
    },
    ModelInfo {
        id: "anthropic/claude-haiku-4-5",
        api_model: "claude-haiku-4-5",
        provider: ProviderKind::Anthropic,
        api_key_env: "ANTHROPIC_API_KEY",
        base_url: "https://api.anthropic.com",
        context_window: 200_000,
        price: Price {
            input_per_mtok: 1.0,
            output_per_mtok: 5.0,
            cached_input_per_mtok: 0.10,
            cache_write_per_mtok: 1.25,
        },
    },
    ModelInfo {
        id: "openai/gpt-4.1-mini",
        api_model: "gpt-4.1-mini",
        provider: ProviderKind::OpenAi,
        api_key_env: "OPENAI_API_KEY",
        base_url: "https://api.openai.com",
        context_window: 1_000_000,
        price: Price {
            input_per_mtok: 0.40,
            output_per_mtok: 1.60,
            cached_input_per_mtok: 0.10,
            cache_write_per_mtok: 0.0,
        },
    },
];

/// Returns every known model.
#[must_use]
pub fn catalog() -> &'static [ModelInfo] {
    CATALOG
}

/// Looks up a model by its qualified identifier.
#[must_use]
pub fn lookup(id: &str) -> Option<&'static ModelInfo> {
    CATALOG.iter().find(|info| info.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_model_is_in_the_catalogue() {
        assert!(lookup(DEFAULT_MODEL).is_some());
    }

    #[test]
    fn identifiers_are_unique() {
        let mut ids: Vec<&str> = CATALOG.iter().map(|info| info.id).collect();
        let before = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), before, "duplicate model id in the catalogue");
    }

    #[test]
    fn unknown_models_are_reported_rather_than_guessed() {
        assert!(lookup("anthropic/does-not-exist").is_none());
    }

    #[test]
    fn every_entry_is_priced_and_sized() {
        for info in CATALOG {
            assert!(info.context_window > 0, "{} has no context window", info.id);
            assert!(info.price.output_per_mtok >= 0.0, "{} has a negative price", info.id);
            assert!(!info.api_key_env.is_empty(), "{} has no key variable", info.id);
        }
    }

    #[test]
    fn cached_input_is_never_more_expensive_than_fresh_input() {
        for info in CATALOG {
            assert!(
                info.price.cached_input_per_mtok <= info.price.input_per_mtok,
                "{} prices cache reads above fresh input — likely a typo",
                info.id
            );
        }
    }
}
