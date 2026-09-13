//! The `true-code` binary.
//!
//! Three entry points, one engine:
//!
//! * `true-code` — the interactive TUI
//! * `true-code -p "…"` — headless, for scripts and CI
//! * `true-code models` / `config` — introspection, so the setup is never a guess
//!
//! The headless path exists from day one on purpose: a harness that only works
//! inside its own UI cannot be tested in CI, scripted, or driven by an editor.

mod headless;

use std::path::PathBuf;

use anyhow::Context as _;
use clap::{Parser, Subcommand};
use tc_config::Config;

/// Command-line interface.
#[derive(Debug, Parser)]
#[command(
    name = "true-code",
    version,
    about = "An agentic coding harness that shows you what it did — and what it cost.",
    long_about = None,
)]
struct Cli {
    /// Run a single prompt without the TUI and print the answer to stdout.
    #[arg(short = 'p', long = "print", value_name = "PROMPT")]
    prompt: Option<String>,

    /// Override the configured model, e.g. `anthropic/claude-haiku-4-5`.
    #[arg(short, long, env = tc_config::ENV_MODEL)]
    model: Option<String>,

    /// Project directory. Defaults to the current working directory.
    #[arg(short = 'C', long, value_name = "DIR")]
    directory: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}

/// Introspection subcommands.
#[derive(Debug, Subcommand)]
enum Command {
    /// List the known models with their context window and assumed price.
    Models,
    /// Show the resolved configuration and where it came from.
    Config,
}

fn main() -> anyhow::Result<()> {
    init_tracing();

    let cli = Cli::parse();
    let cwd = match cli.directory {
        Some(dir) => dir,
        None => std::env::current_dir().context("cannot determine the current directory")?,
    };

    let mut config = Config::load(&cwd)?;
    if let Some(model) = cli.model {
        config.model = model;
    }

    match cli.command {
        Some(Command::Models) => {
            print_models();
            Ok(())
        }
        Some(Command::Config) => {
            print_config(&config);
            Ok(())
        }
        None => start_session(&config, cli.prompt),
    }
}

/// Starts either the headless turn or the interactive TUI.
fn start_session(config: &Config, prompt: Option<String>) -> anyhow::Result<()> {
    let info = config.model_info()?;
    let api_key = config.api_key()?;
    let provider = tc_providers::provider_for(info, api_key);

    match prompt {
        Some(prompt) => headless::run(provider, config, &prompt),
        None => tc_tui::run(provider, config),
    }
}

/// Prints the model catalogue.
///
/// Prices are shown rather than hidden so that a stale assumption is visible
/// instead of quietly producing a wrong cost estimate.
fn print_models() {
    println!("{:<32} {:>10}  {:>9}  {:>9}", "MODEL", "CONTEXT", "IN/MTOK", "OUT/MTOK");
    for info in tc_config::catalog() {
        println!(
            "{:<32} {:>10} {:>9.2} {:>10.2}",
            info.id, info.context_window, info.price.input_per_mtok, info.price.output_per_mtok,
        );
    }
    println!("\nPrices are indicative list prices, not a billing source of truth.");
}

/// Prints the resolved configuration.
fn print_config(config: &Config) {
    println!("model              {}", config.model);
    println!("budget (session)   ${:.2}", config.budget.session_limit_usd);
    println!("warn at            {} %", config.budget.warn_at_percent);
    println!(
        "system prompt      {}",
        if config.system_prompt.is_some() { "custom" } else { "built-in default" }
    );
    match tc_config::user_config_path() {
        Some(path) => println!("user config        {}", path.display()),
        None => println!("user config        (no home directory found)"),
    }
    println!("project config     ./{}/{}", tc_config::PROJECT_DIR, tc_config::CONFIG_FILE);

    match config.api_key() {
        Ok(_) => println!("api key            found"),
        Err(err) => println!("api key            MISSING — {err}"),
    }
}

/// Initialises logging.
///
/// Logs go to stderr so that `true-code -p "…" > answer.md` stays pipeable, and
/// are silent unless `RUST_LOG` asks for them.
fn init_tracing() {
    use tracing_subscriber::{EnvFilter, fmt};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    fmt().with_env_filter(filter).with_writer(std::io::stderr).init();
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory as _;

    #[test]
    fn the_cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn print_mode_is_available_as_dash_p() {
        let cli = Cli::try_parse_from(["true-code", "-p", "hello"]).expect("parses");
        assert_eq!(cli.prompt.as_deref(), Some("hello"));
        assert!(cli.command.is_none());
    }

    #[test]
    fn the_model_can_be_overridden_on_the_command_line() {
        let cli =
            Cli::try_parse_from(["true-code", "--model", "openai/gpt-4.1-mini"]).expect("parses");
        assert_eq!(cli.model.as_deref(), Some("openai/gpt-4.1-mini"));
    }

    #[test]
    fn introspection_subcommands_are_reachable() {
        let cli = Cli::try_parse_from(["true-code", "models"]).expect("parses");
        assert!(matches!(cli.command, Some(Command::Models)));
    }

    #[test]
    fn no_arguments_means_the_interactive_tui() {
        let cli = Cli::try_parse_from(["true-code"]).expect("parses");
        assert!(cli.prompt.is_none());
        assert!(cli.command.is_none());
    }
}
