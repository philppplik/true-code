//! Where API keys come from.
//!
//! Two sources, in this order:
//!
//! 1. **The environment** — `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`,
//!    `OPENROUTER_API_KEY`. Wins, always, so CI and scripts stay predictable and
//!    a temporary override needs no cleanup.
//! 2. **The OS keyring** — Credential Manager on Windows, Keychain on macOS,
//!    the kernel session keyring on Linux. What the setup screen writes to.
//!
//!    Linux is the weak one: keyutils holds the key for the login session and
//!    forgets it on reboot. Secret Service would persist, but it needs
//!    `libdbus-1-dev` present before `cargo install` will build at all, and an
//!    install guide whose first command fails on a clean machine is the worse
//!    trade. On Linux, the environment variable is the durable answer.
//!
//! **Never a file true-code wrote.** That commitment is in `SECURITY.md` and it
//! is the reason a config file can be committed to a repository without turning
//! into a credential leak. The keyring is not a file we manage: the OS owns it,
//! encrypts it at rest, and gates access on the user's login.
//!
//! # Keys are never logged
//!
//! Nothing here returns a key in an error message, a `Debug` output or a trace.
//! The only thing that leaves this module with a key in it is the `Ok` value.

use crate::catalog::Vendor;

/// Service name under which keys are filed in the OS keyring.
const SERVICE: &str = "true-code";

/// Errors raised while storing or retrieving a key.
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    /// The OS keyring is unavailable or refused the operation.
    ///
    /// Common on a headless Linux box, which is why the message points at the
    /// environment variable instead.
    #[error("the system keyring is unavailable ({detail}) — set {env_var} instead")]
    Keyring {
        /// Which variable would work.
        env_var: &'static str,
        /// What the keyring reported.
        detail: String,
    },

    /// No key was found in either source.
    #[error("no API key for {provider} — run `truecode auth login {provider}`, or set {env_var}")]
    Missing {
        /// Provider that needs a key.
        provider: &'static str,
        /// Variable that would supply it.
        env_var: &'static str,
    },
}

/// Where a key was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// An environment variable.
    Environment,
    /// The OS keyring.
    Keyring,
}

impl Source {
    /// How the source is described to a human.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Environment => "environment",
            Self::Keyring => "system keyring",
        }
    }
}

/// Reads the API key for a provider.
pub fn key_for(vendor: &Vendor) -> Result<(String, Source), SecretError> {
    let from_env = std::env::var(vendor.api_key_env).ok();
    let from_keyring = entry(vendor)?.get_password().ok();

    resolve(from_env, from_keyring)
        .ok_or(SecretError::Missing { provider: vendor.id, env_var: vendor.api_key_env })
}

/// Picks between the two sources.
///
/// Split out from [`key_for`] so the precedence rule is testable without
/// mutating the process environment — which is global state shared by every test
/// in the binary, and which this workspace could not touch anyway without
/// `unsafe`.
fn resolve(from_env: Option<String>, from_keyring: Option<String>) -> Option<(String, Source)> {
    // The environment wins, so an override needs no cleanup and CI stays
    // predictable regardless of what is in the keyring of whoever set up the runner.
    let usable = |value: Option<String>| value.filter(|key| !key.trim().is_empty());

    usable(from_env)
        .map(|key| (key, Source::Environment))
        .or_else(|| usable(from_keyring).map(|key| (key, Source::Keyring)))
}

/// Whether a key is available, and from where. Never returns the key itself.
#[must_use]
pub fn source_for(vendor: &Vendor) -> Option<Source> {
    key_for(vendor).ok().map(|(_, source)| source)
}

/// Stores a key in the OS keyring.
pub fn store(vendor: &Vendor, key: &str) -> Result<(), SecretError> {
    entry(vendor)?.set_password(key).map_err(|err| keyring_error(vendor, &err))
}

/// Removes a key from the OS keyring.
///
/// Succeeds when there was nothing to remove: the end state is what matters.
pub fn forget(vendor: &Vendor) -> Result<(), SecretError> {
    match entry(vendor)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(keyring_error(vendor, &err)),
    }
}

/// Opens the keyring entry for a provider.
fn entry(vendor: &Vendor) -> Result<keyring::Entry, SecretError> {
    keyring::Entry::new(SERVICE, vendor.id).map_err(|err| keyring_error(vendor, &err))
}

/// Builds a keyring error that names the way out.
fn keyring_error(vendor: &Vendor, err: &keyring::Error) -> SecretError {
    SecretError::Keyring { env_var: vendor.api_key_env, detail: err.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog;

    #[test]
    fn the_environment_is_preferred_over_the_keyring() {
        let found = resolve(Some("from-env".to_owned()), Some("from-keyring".to_owned()));

        assert_eq!(found, Some(("from-env".to_owned(), Source::Environment)));
    }

    #[test]
    fn the_keyring_is_used_when_the_environment_is_unset() {
        let found = resolve(None, Some("from-keyring".to_owned()));

        assert_eq!(found, Some(("from-keyring".to_owned(), Source::Keyring)));
    }

    #[test]
    fn a_blank_value_counts_as_unset_in_either_source() {
        // Handing a provider a blank key produces a baffling 401; falling through
        // produces a message that says what to do.
        assert_eq!(
            resolve(Some("   ".to_owned()), Some("real".to_owned())),
            Some(("real".to_owned(), Source::Keyring))
        );
        assert_eq!(resolve(Some(String::new()), None), None);
    }

    #[test]
    fn no_key_anywhere_yields_nothing() {
        assert_eq!(resolve(None, None), None);
    }

    #[test]
    fn a_missing_key_names_both_ways_to_supply_one() {
        let vendor = catalog::vendor("openrouter").expect("openrouter is a known vendor");
        let error = SecretError::Missing { provider: vendor.id, env_var: vendor.api_key_env };

        let message = error.to_string();
        assert!(message.contains("truecode auth login openrouter"), "unexpected: {message}");
        assert!(message.contains("OPENROUTER_API_KEY"), "unexpected: {message}");
    }

    #[test]
    fn a_keyring_failure_points_at_the_environment_instead() {
        // The common case is a headless Linux box with no Secret Service. The
        // message has to offer the way that will actually work there.
        let error =
            SecretError::Keyring { env_var: "ANTHROPIC_API_KEY", detail: "no backend".to_owned() };

        assert!(error.to_string().contains("set ANTHROPIC_API_KEY instead"));
    }

    #[test]
    fn both_sources_describe_themselves() {
        assert_eq!(Source::Environment.label(), "environment");
        assert_eq!(Source::Keyring.label(), "system keyring");
    }
}
