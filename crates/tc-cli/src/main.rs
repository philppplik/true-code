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

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context as _;
use clap::{Parser, Subcommand};
use tc_agent::approval::ApproveAll;
use tc_agent::{Agent, AgentSetup, Approver, DenyAll, PermissionMode, SessionLog};
use tc_config::Config;
use tc_core::SessionId;
use tc_tools::Ledger;
use tc_tools::{ToolContext, ToolSet};

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

    /// What the agent may do: read-only, write, or full.
    ///
    /// `write` adds file creation and editing, `full` also allows shell commands.
    /// Every change is shown as a diff and confirmed before it is applied.
    #[arg(long, value_name = "MODE", default_value = "read-only")]
    permission_mode: String,

    /// Approve every change without asking. Headless mode only.
    ///
    /// Without it, `-p` refuses changes, because there is nobody to ask.
    #[arg(long)]
    yes: bool,

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
    /// Revert the most recent change true-code made in this project.
    ///
    /// Reads the newest session log, so it works long after the session ended.
    Undo,
    /// Show the project rules and whether each one is checked automatically.
    Constraints,
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
        Some(Command::Models) => print_models(),
        Some(Command::Config) => print_config(&config),
        Some(Command::Undo) => println!("{}", undo_last(&cwd)?),
        Some(Command::Constraints) => print_constraints(&cwd)?,
        None => {
            let mode = PermissionMode::parse(&cli.permission_mode).ok_or_else(|| {
                // Never fall back to a default here: guessing a permission level
                // from a typo is how an agent ends up with more access than the
                // user asked for.
                anyhow::anyhow!(
                    "unknown permission mode `{}` — use read-only, write or full",
                    cli.permission_mode
                )
            })?;

            if cli.yes && cli.prompt.is_none() {
                anyhow::bail!(
                    "--yes only applies to headless runs (-p); in the TUI you confirm each change"
                );
            }

            // A guard rail that stopped the run is not a crash, but it is also not
            // a success — scripts need to be able to tell the difference.
            let code = start_session(&config, &cwd, mode, cli.prompt, cli.yes)?;
            if code != 0 {
                std::process::exit(code);
            }
        }
    }
    Ok(())
}

/// Starts either the headless run or the interactive TUI.
fn start_session(
    config: &Config,
    cwd: &Path,
    mode: PermissionMode,
    prompt: Option<String>,
    auto_approve: bool,
) -> anyhow::Result<i32> {
    let info = config.model_info()?;
    let api_key = config.api_key()?;
    let provider: Arc<dyn tc_providers::Provider> =
        Arc::from(tc_providers::provider_for(info, api_key));

    let tools = ToolSet::for_mode(mode);
    let log = SessionLog::create(cwd, SessionId::new());

    // Loaded before anything runs: a rule the user wrote down but that does not
    // compile must stop the session, not be silently skipped.
    let ledger = Ledger::load(cwd)?;

    if let Some(prompt) = prompt {
        // Nobody is watching a headless run, so the choice is between an explicit
        // opt-in and refusing. It is never "apply and hope".
        let approver: Arc<dyn Approver> =
            if auto_approve { Arc::new(ApproveAll) } else { Arc::new(DenyAll) };

        let setup = AgentSetup {
            provider,
            tools,
            tool_ctx: ToolContext::new(cwd),
            ledger,
            log,
            approver,
            mode,
        };
        return headless::run(Agent::new(setup, config), &prompt);
    }

    let (approver, approvals) = tc_tui::approver();
    let setup = AgentSetup {
        provider,
        tools,
        tool_ctx: ToolContext::new(cwd),
        ledger,
        log,
        approver,
        mode,
    };
    tc_tui::run(Agent::new(setup, config), config, mode, approvals).map(|()| 0)
}

/// Reverts the most recent change recorded in this project.
///
/// Deliberately available without starting a session: the moment you want undo is
/// usually after you have closed the terminal and noticed something.
fn undo_last(cwd: &Path) -> anyhow::Result<String> {
    let session_dir = tc_agent::checkpoint::newest_session(cwd)?;
    let events = session_dir.join("events.jsonl");

    let Some(entry) = tc_agent::checkpoint::last_undoable(&events)? else {
        return Err(tc_agent::UndoError::NothingToUndo.into());
    };

    tc_agent::checkpoint::restore(&session_dir, cwd, &entry)?;

    // Recorded in the same log, so a second `undo` walks one step further back
    // rather than repeating itself.
    let mut log = SessionLog::reopen(&session_dir);
    log.append(tc_core::EventKind::Reverted {
        path: entry.path.clone(),
        checkpoint_seq: entry.seq,
    });

    Ok(match entry.backup {
        Some(_) => format!("Reverted {}.", entry.path),
        None => format!("Removed {}, which true-code had created.", entry.path),
    })
}

/// Prints the project's rules.
///
/// Checked and unchecked rules are listed separately, because a user who cannot
/// tell them apart will trust a reminder as if it were enforced.
fn print_constraints(cwd: &Path) -> anyhow::Result<()> {
    let ledger = Ledger::load(cwd)?;

    if ledger.is_empty() {
        println!("No rules. Create {} to add some.", tc_tools::constraints::CONSTRAINTS_FILE);
        return Ok(());
    }

    if !ledger.constraints().is_empty() {
        println!("Checked automatically before any change is applied:");
        for rule in ledger.constraints() {
            println!("  [x] {}", rule.description);
        }
    }
    if !ledger.reminders().is_empty() {
        println!(
            "
Sent to the model but NOT checked:"
        );
        for note in ledger.reminders() {
            println!("  [ ] {note}");
        }
    }
    Ok(())
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
