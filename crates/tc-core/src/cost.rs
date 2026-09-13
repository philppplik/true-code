//! Token usage and money.
//!
//! Research finding that drives this module: cost is the single most-complained-about
//! blind spot of 2026-era coding agents, and no vendor has an incentive to fix it.
//! true-code therefore treats cost as a domain concept from turn one — not as an
//! export you can generate later.
//!
//! See `docs/RESEARCH-Gaps-2026-09.md`, gap 3.

use serde::{Deserialize, Serialize};

/// Tokens consumed by a single turn.
///
/// `cache_read_tokens` is tracked separately because cache hits are billed at a
/// fraction of the normal input price, and the hit rate is the best available
/// indicator of wasted money.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    /// Fresh input tokens, billed at the full input rate.
    #[serde(default)]
    pub input_tokens: u32,
    /// Generated output tokens.
    #[serde(default)]
    pub output_tokens: u32,
    /// Input tokens served from the provider's prompt cache.
    #[serde(default)]
    pub cache_read_tokens: u32,
    /// Input tokens written into the prompt cache.
    #[serde(default)]
    pub cache_write_tokens: u32,
}

impl Usage {
    /// Total number of input tokens, cached or not.
    #[must_use]
    pub const fn total_input(&self) -> u32 {
        self.input_tokens + self.cache_read_tokens + self.cache_write_tokens
    }

    /// Share of input tokens served from cache, in `0.0..=1.0`.
    ///
    /// Returns `None` when the turn had no input tokens at all, because a rate
    /// over zero tokens is meaningless and must not be rendered as "0 %".
    #[must_use]
    pub fn cache_hit_rate(&self) -> Option<f64> {
        let total = self.total_input();
        if total == 0 {
            return None;
        }
        Some(f64::from(self.cache_read_tokens) / f64::from(total))
    }

    /// Adds another turn's usage to this one.
    #[must_use]
    pub const fn saturating_add(self, other: Self) -> Self {
        Self {
            input_tokens: self.input_tokens.saturating_add(other.input_tokens),
            output_tokens: self.output_tokens.saturating_add(other.output_tokens),
            cache_read_tokens: self.cache_read_tokens.saturating_add(other.cache_read_tokens),
            cache_write_tokens: self.cache_write_tokens.saturating_add(other.cache_write_tokens),
        }
    }
}

/// Price of a model, in USD per one million tokens.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Price {
    /// Price per million fresh input tokens.
    pub input_per_mtok: f64,
    /// Price per million output tokens.
    pub output_per_mtok: f64,
    /// Price per million cached input tokens (read).
    pub cached_input_per_mtok: f64,
    /// Price per million tokens written to the cache.
    pub cache_write_per_mtok: f64,
}

/// One million — the unit prices are quoted in.
const TOKENS_PER_MTOK: f64 = 1_000_000.0;

impl Price {
    /// A free model (local inference via Ollama / llama.cpp).
    pub const FREE: Self = Self {
        input_per_mtok: 0.0,
        output_per_mtok: 0.0,
        cached_input_per_mtok: 0.0,
        cache_write_per_mtok: 0.0,
    };

    /// Computes the cost of the given [`Usage`] under this price.
    #[must_use]
    pub fn cost_of(&self, usage: Usage) -> Cost {
        let usd = f64::from(usage.input_tokens).mul_add(
            self.input_per_mtok,
            f64::from(usage.output_tokens).mul_add(
                self.output_per_mtok,
                f64::from(usage.cache_read_tokens).mul_add(
                    self.cached_input_per_mtok,
                    f64::from(usage.cache_write_tokens) * self.cache_write_per_mtok,
                ),
            ),
        ) / TOKENS_PER_MTOK;
        Cost { usd }
    }
}

/// Money spent, in USD.
#[derive(Debug, Clone, Copy, Default, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Cost {
    /// Amount in US dollars.
    pub usd: f64,
}

impl Cost {
    /// Adds another amount.
    #[must_use]
    pub const fn add(self, other: Self) -> Self {
        Self { usd: self.usd + other.usd }
    }

    /// Renders the amount for the status bar.
    ///
    /// Sub-cent amounts keep four decimals so that a cheap turn does not
    /// misleadingly display as `$0.00`.
    #[must_use]
    pub fn display(&self) -> String {
        if self.usd > 0.0 && self.usd < 0.01 {
            format!("${:.4}", self.usd)
        } else {
            format!("${:.2}", self.usd)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rounding tolerance for floating-point money comparisons.
    const EPSILON: f64 = 1e-9;

    fn sonnet_like_price() -> Price {
        Price {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
            cached_input_per_mtok: 0.3,
            cache_write_per_mtok: 3.75,
        }
    }

    #[test]
    fn cost_of_prices_each_token_class_separately() {
        let usage = Usage {
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            cache_read_tokens: 1_000_000,
            cache_write_tokens: 1_000_000,
        };
        let cost = sonnet_like_price().cost_of(usage);
        assert!((cost.usd - (3.0 + 15.0 + 0.3 + 3.75)).abs() < EPSILON);
    }

    #[test]
    fn free_price_never_charges() {
        let usage = Usage { input_tokens: 5_000, output_tokens: 5_000, ..Usage::default() };
        assert!((Price::FREE.cost_of(usage).usd - 0.0).abs() < EPSILON);
    }

    #[test]
    fn cache_hit_rate_is_none_without_input_tokens() {
        assert_eq!(Usage::default().cache_hit_rate(), None);
    }

    #[test]
    fn cache_hit_rate_reports_the_cached_share() {
        let usage = Usage { input_tokens: 250, cache_read_tokens: 750, ..Usage::default() };
        let rate = usage.cache_hit_rate().expect("input tokens are present");
        assert!((rate - 0.75).abs() < EPSILON);
    }

    #[test]
    fn sub_cent_amounts_keep_four_decimals() {
        assert_eq!(Cost { usd: 0.0031 }.display(), "$0.0031");
        assert_eq!(Cost { usd: 1.5 }.display(), "$1.50");
        assert_eq!(Cost { usd: 0.0 }.display(), "$0.00");
    }

    #[test]
    fn usage_accumulates_across_turns() {
        let first = Usage { input_tokens: 10, output_tokens: 5, ..Usage::default() };
        let second = Usage { input_tokens: 1, output_tokens: 2, ..Usage::default() };
        let total = first.saturating_add(second);
        assert_eq!(total.input_tokens, 11);
        assert_eq!(total.output_tokens, 7);
    }
}
