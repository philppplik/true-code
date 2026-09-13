//! The two things worth asking the network about.
//!
//! `truecode update` — am I behind? — and `truecode models`, which for a gateway
//! has to be a live question: OpenRouter proxies hundreds of models and the list
//! changes weekly, so a compiled-in table would be wrong the day it shipped.
//!
//! Both degrade rather than fail. Being offline is not an error here; it just
//! means the answer is "cannot tell", which is said plainly.

use std::time::Duration;

/// Commit this binary was built from, recorded by `build.rs`.
///
/// Empty when it was not built from a git checkout.
pub const BUILT_FROM: &str = env!("TRUECODE_COMMIT");

/// Where to look for newer commits.
const REPO_API: &str = "https://api.github.com/repos/philppplik/true-code/commits/main";

/// OpenRouter's public model list. No key needed.
const OPENROUTER_MODELS: &str = "https://openrouter.ai/api/v1/models";

/// Long enough for a slow connection, short enough not to feel hung.
const TIMEOUT: Duration = Duration::from_secs(15);

/// GitHub rejects requests without one.
const USER_AGENT: &str = concat!("truecode/", env!("CARGO_PKG_VERSION"));

/// Reports whether a newer version exists.
pub fn check_for_update() {
    println!("truecode {}", env!("CARGO_PKG_VERSION"));

    if BUILT_FROM.is_empty() {
        println!("\nThis build does not record a commit, so it cannot tell whether it is behind.");
        println!("Rebuild from a git checkout to get update checks:");
        println!("  git clone https://github.com/philppplik/true-code");
        return;
    }
    println!("built from {BUILT_FROM}");

    let latest = match newest_commit() {
        Ok(latest) => latest,
        Err(err) => {
            // Offline is not a failure of the tool. Exiting non-zero here would
            // break anyone who runs this in a script on a flaky connection.
            println!("\nCould not reach GitHub ({err}). No idea whether you are up to date.");
            return;
        }
    };

    if latest.starts_with(BUILT_FROM)
        || BUILT_FROM.starts_with(&latest[..BUILT_FROM.len().min(latest.len())])
    {
        println!("\nUp to date.");
        return;
    }

    println!("\nA newer version is available ({}).", &latest[..12.min(latest.len())]);
    println!("Update with:");
    println!("  cd <your true-code checkout>");
    println!("  git pull && cargo install --path crates/tc-cli --force");
    println!("\nNot run for you: it would need to know where you cloned it, and pulling into");
    println!("a checkout you have edited is not a decision this command gets to make.");
}

/// Fetches the newest commit on `main`.
fn newest_commit() -> anyhow::Result<String> {
    let response = client()?
        .get(REPO_API)
        .header("Accept", "application/vnd.github+json")
        .send()?
        .error_for_status()?;

    let body: serde_json::Value = response.json()?;
    body.get("sha")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| anyhow::anyhow!("GitHub returned no commit"))
}

/// Prints models a gateway offers, optionally filtered.
///
/// Live rather than compiled in, because OpenRouter's catalogue changes weekly
/// and the whole point of the gateway is reaching models this build never heard of.
pub fn print_gateway_models(filter: Option<&str>) -> anyhow::Result<()> {
    let response = client()?.get(OPENROUTER_MODELS).send()?.error_for_status()?;
    let body: serde_json::Value = response.json()?;

    let models = body
        .get("data")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("OpenRouter returned an unexpected shape"))?;

    let needle = filter.map(str::to_lowercase);
    let mut shown = 0usize;

    println!("{:<52} {:>10}  {:>10}  {:>10}", "MODEL", "CONTEXT", "IN/MTOK", "OUT/MTOK");

    for model in models {
        let Some(id) = model.get("id").and_then(serde_json::Value::as_str) else {
            continue;
        };
        if let Some(needle) = &needle
            && !id.to_lowercase().contains(needle)
        {
            continue;
        }

        let context = model.get("context_length").and_then(serde_json::Value::as_u64).unwrap_or(0);
        println!(
            "{id:<52} {context:>10}  {:>10}  {:>10}",
            per_mtok(model, "prompt"),
            per_mtok(model, "completion"),
        );
        shown += 1;
    }

    if shown == 0 {
        match filter {
            Some(filter) => println!("\nNothing matches `{filter}`."),
            None => println!("\nOpenRouter returned no models."),
        }
    } else {
        println!("\n{shown} of {} models. Use one with:", models.len());
        println!("  truecode --model <id>");
    }
    Ok(())
}

/// Formats an OpenRouter price, which is quoted per token, as price per million.
fn per_mtok(model: &serde_json::Value, field: &str) -> String {
    let raw = model
        .get("pricing")
        .and_then(|pricing| pricing.get(field))
        .and_then(serde_json::Value::as_str);

    match raw.and_then(|value| value.parse::<f64>().ok()) {
        // Zero is a real price on OpenRouter — the `:free` variants — and saying
        // "free" is clearer than a column of 0.00.
        Some(0.0) => "free".to_owned(),
        Some(price) => format!("{:.2}", price * 1_000_000.0),
        None => "?".to_owned(),
    }
}

/// Builds the HTTP client.
fn client() -> anyhow::Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder().timeout(TIMEOUT).user_agent(USER_AGENT).build()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model_with(prompt: &str) -> serde_json::Value {
        serde_json::json!({ "pricing": { "prompt": prompt, "completion": "0.000015" } })
    }

    #[test]
    fn a_per_token_price_becomes_a_per_million_one() {
        // OpenRouter quotes per token; every other price in true-code is per
        // million, and mixing the two would be off by six orders of magnitude.
        assert_eq!(per_mtok(&model_with("0.000003"), "prompt"), "3.00");
        assert_eq!(per_mtok(&model_with("0.0000004"), "prompt"), "0.40");
    }

    #[test]
    fn a_zero_price_is_shown_as_free() {
        // The `:free` variants really are zero; a column of 0.00 reads like a
        // rounding artefact instead of the point.
        assert_eq!(per_mtok(&model_with("0"), "prompt"), "free");
    }

    #[test]
    fn a_missing_or_unparsable_price_is_marked_unknown() {
        assert_eq!(per_mtok(&serde_json::json!({}), "prompt"), "?");
        assert_eq!(per_mtok(&model_with("not-a-number"), "prompt"), "?");
    }

    #[test]
    fn the_user_agent_names_the_tool_and_version() {
        // GitHub rejects requests without one, and an anonymous agent string
        // makes an outage impossible to attribute.
        assert!(USER_AGENT.starts_with("truecode/"));
    }
}
