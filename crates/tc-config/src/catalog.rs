//! The model catalogue.
//!
//! true-code is model-neutral by design: the harness must never assume a single
//! vendor. Each entry carries everything the rest of the system needs to make a
//! request *and* to price it — those two must not drift apart, or the cost display
//! quietly starts lying.
//!
//! # Known models and unknown ones
//!
//! The built-in table covers the models most people reach for, with their prices.
//! But OpenRouter alone proxies hundreds, and a fixed table would make most of
//! them unreachable. So any `openrouter/…` identifier resolves, and one whose
//! price we do not know says **so** rather than guessing — an invented price is
//! worse than an absent one, because it looks like information.
//!
//! # Prices
//!
//! The prices below are published list prices at the time of writing and are
//! **indicative defaults**, not a billing source of truth. Vendors change them.
//! `truecode models` prints what is currently assumed, so the number can be checked.

use tc_core::Price;

/// Model used when nothing else is configured.
pub const DEFAULT_MODEL: &str = "anthropic/claude-sonnet-4-5";

/// Which wire protocol a model speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    /// Anthropic Messages API (`/v1/messages`, `x-api-key`).
    Anthropic,
    /// OpenAI Chat Completions API (`/v1/chat/completions`, bearer token).
    ///
    /// Also used for every OpenAI-compatible gateway, OpenRouter included.
    OpenAi,
}

/// A provider true-code can talk to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Vendor {
    /// Identifier used as the prefix of a model id, e.g. `anthropic`.
    pub id: &'static str,
    /// Name shown to a human.
    pub name: &'static str,
    /// Wire protocol it speaks.
    pub kind: ProviderKind,
    /// Base URL of the API.
    pub base_url: &'static str,
    /// Environment variable holding the API key.
    pub api_key_env: &'static str,
    /// Where to get a key, for the setup screen.
    pub signup_url: &'static str,
    /// Whether any model identifier is accepted, not only catalogued ones.
    ///
    /// True for gateways that proxy a catalogue far larger than we could track.
    pub accepts_any_model: bool,
}

/// Every provider true-code knows how to reach.
pub static VENDORS: &[Vendor] = &[
    Vendor {
        id: "anthropic",
        name: "Anthropic",
        kind: ProviderKind::Anthropic,
        base_url: "https://api.anthropic.com",
        api_key_env: "ANTHROPIC_API_KEY",
        signup_url: "https://console.anthropic.com/settings/keys",
        accepts_any_model: false,
    },
    Vendor {
        id: "openai",
        name: "OpenAI",
        kind: ProviderKind::OpenAi,
        base_url: "https://api.openai.com",
        api_key_env: "OPENAI_API_KEY",
        signup_url: "https://platform.openai.com/api-keys",
        accepts_any_model: false,
    },
    Vendor {
        id: "openrouter",
        name: "OpenRouter",
        kind: ProviderKind::OpenAi,
        base_url: "https://openrouter.ai/api",
        api_key_env: "OPENROUTER_API_KEY",
        signup_url: "https://openrouter.ai/keys",
        // One key, hundreds of models from every vendor. Pinning that to a fixed
        // list would make most of them unreachable for no benefit.
        accepts_any_model: true,
    },
];

/// Looks up a provider by its identifier.
#[must_use]
pub fn vendor(id: &str) -> Option<&'static Vendor> {
    VENDORS.iter().find(|vendor| vendor.id == id)
}

/// Everything needed to call a model and to price the call.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelInfo {
    /// Qualified identifier used in config and on the command line.
    pub id: String,
    /// Identifier sent to the provider's API, which may differ from [`Self::id`].
    pub api_model: String,
    /// The provider this model is reached through.
    pub vendor: &'static Vendor,
    /// Size of the context window in tokens, used for the context-health meter.
    pub context_window: u32,
    /// Indicative list price, or `None` when it is genuinely unknown.
    ///
    /// `None` is honest rather than unhelpful: a made-up number would be
    /// indistinguishable from a real one in the cost display.
    pub price: Option<Price>,
}

impl ModelInfo {
    /// The price to bill against, treating an unknown price as free.
    ///
    /// The cost display separately reports that the figure is not trustworthy, so
    /// this never silently invents a number the user might act on.
    #[must_use]
    pub fn price_or_free(&self) -> Price {
        self.price.unwrap_or(Price::FREE)
    }

    /// Environment variable holding this model's API key.
    #[must_use]
    pub const fn api_key_env(&self) -> &'static str {
        self.vendor.api_key_env
    }
}

/// One entry of the built-in table.
struct Known {
    id: &'static str,
    api_model: &'static str,
    vendor: &'static str,
    context_window: u32,
    price: Price,
}

/// Models true-code ships with a price for.
static KNOWN: &[Known] = &[
    Known {
        id: "anthropic/claude-sonnet-4-5",
        api_model: "claude-sonnet-4-5",
        vendor: "anthropic",
        context_window: 200_000,
        price: Price {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
            cached_input_per_mtok: 0.30,
            cache_write_per_mtok: 3.75,
        },
    },
    Known {
        id: "anthropic/claude-haiku-4-5",
        api_model: "claude-haiku-4-5",
        vendor: "anthropic",
        context_window: 200_000,
        price: Price {
            input_per_mtok: 1.0,
            output_per_mtok: 5.0,
            cached_input_per_mtok: 0.10,
            cache_write_per_mtok: 1.25,
        },
    },
    Known {
        id: "openai/gpt-4.1",
        api_model: "gpt-4.1",
        vendor: "openai",
        context_window: 1_000_000,
        price: Price {
            input_per_mtok: 2.00,
            output_per_mtok: 8.00,
            cached_input_per_mtok: 0.50,
            cache_write_per_mtok: 0.0,
        },
    },
    Known {
        id: "openai/gpt-4.1-mini",
        api_model: "gpt-4.1-mini",
        vendor: "openai",
        context_window: 1_000_000,
        price: Price {
            input_per_mtok: 0.40,
            output_per_mtok: 1.60,
            cached_input_per_mtok: 0.10,
            cache_write_per_mtok: 0.0,
        },
    },
    Known {
        id: "openrouter/anthropic/claude-sonnet-4.5",
        api_model: "anthropic/claude-sonnet-4.5",
        vendor: "openrouter",
        context_window: 200_000,
        price: Price {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
            cached_input_per_mtok: 0.30,
            cache_write_per_mtok: 3.75,
        },
    },
    Known {
        id: "openrouter/openai/gpt-4.1-mini",
        api_model: "openai/gpt-4.1-mini",
        vendor: "openrouter",
        context_window: 1_000_000,
        price: Price {
            input_per_mtok: 0.40,
            output_per_mtok: 1.60,
            cached_input_per_mtok: 0.10,
            cache_write_per_mtok: 0.0,
        },
    },
];

/// Context window assumed for a model we have never heard of.
///
/// Deliberately conservative: the figure only drives the context-health meter, and
/// a gauge that fills too early is a nuisance, while one that fills too late hides
/// the problem it exists to show.
const UNKNOWN_CONTEXT_WINDOW: u32 = 128_000;

/// Every catalogued model, with its price.
#[must_use]
pub fn catalog() -> Vec<ModelInfo> {
    KNOWN.iter().map(build).collect()
}

/// Resolves a model identifier.
///
/// Catalogued models come back with their price. An identifier belonging to a
/// gateway that proxies anything resolves with **no** price rather than being
/// rejected, so a model newer than this build is still reachable.
#[must_use]
pub fn lookup(id: &str) -> Option<ModelInfo> {
    if let Some(known) = KNOWN.iter().find(|known| known.id == id) {
        return Some(build(known));
    }

    // `openrouter/anthropic/claude-opus-4.5` → vendor `openrouter`, model
    // `anthropic/claude-opus-4.5`.
    let (prefix, rest) = id.split_once('/')?;
    let vendor = vendor(prefix)?;

    (vendor.accepts_any_model && !rest.is_empty()).then(|| ModelInfo {
        id: id.to_owned(),
        api_model: rest.to_owned(),
        vendor,
        context_window: UNKNOWN_CONTEXT_WINDOW,
        price: None,
    })
}

/// Turns a table entry into a resolved model.
fn build(known: &Known) -> ModelInfo {
    ModelInfo {
        id: known.id.to_owned(),
        api_model: known.api_model.to_owned(),
        vendor: vendor(known.vendor).expect("every catalogue entry names a known vendor"),
        context_window: known.context_window,
        price: Some(known.price),
    }
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
        let mut ids: Vec<&str> = KNOWN.iter().map(|known| known.id).collect();
        let before = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), before, "duplicate model id in the catalogue");
    }

    #[test]
    fn every_entry_names_a_known_vendor_and_is_priced() {
        for info in catalog() {
            assert!(info.context_window > 0, "{} has no context window", info.id);
            assert!(info.price.is_some(), "{} is catalogued without a price", info.id);
            assert!(!info.api_key_env().is_empty());
        }
    }

    #[test]
    fn cached_input_is_never_more_expensive_than_fresh_input() {
        for info in catalog() {
            let price = info.price.expect("catalogued models are priced");
            assert!(
                price.cached_input_per_mtok <= price.input_per_mtok,
                "{} prices cache reads above fresh input — likely a typo",
                info.id
            );
        }
    }

    #[test]
    fn an_unknown_model_from_a_fixed_vendor_is_rejected() {
        // Anthropic's catalogue is small and we track it; a typo here is a typo,
        // not a new model, and guessing would produce a confusing API error.
        assert!(lookup("anthropic/claude-does-not-exist").is_none());
    }

    #[test]
    fn any_model_resolves_through_a_gateway_that_proxies_everything() {
        let info = lookup("openrouter/some-vendor/brand-new-model")
            .expect("a gateway must not be limited to what this build knows");

        assert_eq!(info.api_model, "some-vendor/brand-new-model");
        assert_eq!(info.vendor.id, "openrouter");
    }

    #[test]
    fn a_model_we_cannot_price_says_so_rather_than_guessing() {
        let info = lookup("openrouter/some-vendor/brand-new-model").expect("it resolves");

        assert_eq!(info.price, None, "an invented price looks exactly like a real one");
        assert_eq!(info.price_or_free(), Price::FREE, "and bills as zero rather than as fiction");
    }

    #[test]
    fn a_catalogued_gateway_model_keeps_its_price() {
        let info = lookup("openrouter/anthropic/claude-sonnet-4.5").expect("it is catalogued");

        assert!(info.price.is_some(), "the table wins over the passthrough");
        assert_eq!(info.api_model, "anthropic/claude-sonnet-4.5");
    }

    #[test]
    fn an_identifier_without_a_vendor_prefix_is_rejected() {
        assert!(lookup("gpt-4.1").is_none(), "a bare model name is ambiguous");
        assert!(lookup("openrouter/").is_none(), "a vendor with no model is not a model");
    }

    #[test]
    fn every_vendor_is_reachable_and_documented() {
        for vendor in VENDORS {
            assert!(vendor.base_url.starts_with("https://"), "{} is not on https", vendor.id);
            assert!(!vendor.api_key_env.is_empty());
            assert!(
                vendor.signup_url.starts_with("https://"),
                "{} has nowhere to get a key",
                vendor.id
            );
        }
    }

    #[test]
    fn vendor_identifiers_are_unique() {
        let mut ids: Vec<&str> = VENDORS.iter().map(|vendor| vendor.id).collect();
        let before = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), before);
    }
}
