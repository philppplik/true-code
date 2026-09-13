//! The `truecode` binary.
//!
//! Three entry points, one engine:
//!
//! * `truecode` — the interactive TUI
//! * `truecode -p "…"` — headless, for scripts and CI
//! * `truecode models` / `config` — introspection, so the setup is never a guess
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

/// Shown under `truecode --help`.
///
/// Examples earn their space here: the first question is always "what do I
/// actually type", and a flag list does not answer it.
const EXAMPLES: &str = "Examples:
  truecode                                 start a session (read-only)
  truecode --permission-mode write         let it edit files, with confirmation
  truecode --permission-mode full          let it run commands too
  truecode --learn                         ask me one question after each change
  truecode -p \"what does src/lib.rs do?\"   one question, answer on stdout

  truecode auth login openrouter           store a key (anthropic | openai | openrouter)
  truecode doctor                          check the setup before anything else
  truecode verify                          run this project's build, tests and lint
  truecode undo                            revert the last change it made
  truecode constraints                     show the project rules in force
  truecode learn                           what you have been asked, and how it went

First run? `truecode doctor` tells you what is missing.";

/// Command-line interface.
#[derive(Debug, Parser)]
#[command(
    name = "truecode",
    version,
    about = "An agentic coding harness that shows you what it did — and what it cost.",
    long_about = None,
    after_help = EXAMPLES,
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

    /// After a change, ask one question about it.
    ///
    /// Off by default. Heavy AI use measurably erodes understanding of your own
    /// codebase; this is the only lever that reliably works against that.
    #[arg(long)]
    learn: bool,

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
    /// Check that the setup is complete and report anything missing.
    Doctor,
    /// Run this project's build, test and lint commands and report the results.
    ///
    /// What `unverified` in the proof panel tells you to reach for.
    Verify,
    /// Show what you have been asked about, and how it went.
    Learn,
    /// Store, check or remove an API key.
    Auth {
        #[command(subcommand)]
        action: AuthAction,
    },
}

/// Key management.
#[derive(Debug, Subcommand)]
enum AuthAction {
    /// Store a key for a provider in the system keyring.
    Login {
        /// Which provider: anthropic, openai or openrouter.
        provider: String,
    },
    /// Show which providers have a key, and where it came from.
    Status,
    /// Remove a provider's key from the system keyring.
    Logout {
        /// Which provider.
        provider: String,
    },
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
        Some(Command::Auth { action }) => auth(&action)?,
        Some(Command::Learn) => print_learning(&cwd)?,
        Some(Command::Verify) => {
            if !verify(&cwd)? {
                std::process::exit(1);
            }
        }
        Some(Command::Doctor) => {
            if !doctor(&config, &cwd) {
                std::process::exit(1);
            }
        }
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
            if cli.learn && cli.prompt.is_some() {
                anyhow::bail!("--learn needs an interactive session; there is nobody to ask in -p");
            }

            let code = start_session(&config, &cwd, mode, cli.prompt, cli.yes, cli.learn)?;
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
    teaching: bool,
) -> anyhow::Result<i32> {
    let info = config.model_info()?;

    // The first run should not dead-end on "set an environment variable". In an
    // interactive session we can just ask; headless has nobody to ask, so it
    // keeps the error.
    let api_key = match config.api_key() {
        Ok(key) => key,
        Err(err) if prompt.is_none() => first_run_setup(info.vendor).ok_or(err)?,
        Err(err) => return Err(err.into()),
    };

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
            teaching: false,
            profile: tc_agent::Profile::default(),
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
        teaching,
        // What this person has met before, so the question can prefer something
        // they have struggled with rather than starting from nothing each time.
        profile: tc_agent::Profile::load(cwd)?,
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

/// Offers the setup screen when no key is configured.
///
/// Returns the key to use, or `None` if the user backed out — in which case the
/// original "no key" error is the right thing to report.
fn first_run_setup(vendor: &'static tc_config::Vendor) -> Option<String> {
    // A failure to *draw the setup screen* must not be reported as the problem;
    // the missing key is.
    let Ok(Some(chosen)) = tc_tui::setup::run(tc_config::catalog::VENDORS) else {
        return None;
    };

    if let Err(err) = tc_config::secrets::store(chosen.vendor, &chosen.key) {
        // Storing failed — a headless Linux box with no keyring, usually. The key
        // still works for this session, so use it and say what happened.
        eprintln!("Could not save the key ({err}). Using it for this session only.");
    } else {
        println!("Saved {} to the system keyring.", chosen.vendor.name);
    }

    if chosen.vendor.id != vendor.id {
        // They picked a different provider than the configured model belongs to.
        // Saying nothing would produce a baffling auth error on the first request.
        eprintln!(
            "Note: the configured model is a {} model. Run `truecode --model {}/…` or set              `model` in .truecode/config.toml to use {}.",
            vendor.name, chosen.vendor.id, chosen.vendor.name
        );
        return None;
    }
    Some(chosen.key)
}

/// Prints the learning profile.
///
/// No score and no streak, on purpose: gamification measurably lowered both
/// intrinsic motivation and exam performance. Counts and a nudge, nothing more.
fn print_learning(cwd: &Path) -> anyhow::Result<()> {
    let profile = tc_agent::Profile::load(cwd)?;

    if profile.concepts.is_empty() {
        println!(
            "Nothing recorded yet. Run `truecode --learn` and it will ask one question \
             after each change it makes."
        );
        return Ok(());
    }

    println!("{:<28} {:>6} {:>8}", "CONCEPT", "ASKED", "RIGHT");
    for (concept, record) in &profile.concepts {
        println!("{concept:<28} {:>6} {:>8}", record.seen, record.correct);
    }

    let weak = profile.weak_spots();
    if !weak.is_empty() {
        let names: Vec<&str> = weak.iter().map(|(name, _)| *name).collect();
        println!(
            "
Worth coming back to: {}",
            names.join(", ")
        );
    }
    Ok(())
}

/// Stores, reports on, or removes API keys.
fn auth(action: &AuthAction) -> anyhow::Result<()> {
    match action {
        AuthAction::Status => {
            println!("{:<14} {:<20} SET IT WITH", "PROVIDER", "KEY");
            for vendor in tc_config::catalog::VENDORS {
                let found = tc_config::secrets::source_for(vendor)
                    .map_or_else(|| "—".to_owned(), |source| source.label().to_owned());
                println!("{:<14} {:<20} truecode auth login {}", vendor.name, found, vendor.id);
            }
            println!(
                "
An environment variable always wins over the keyring."
            );
        }

        AuthAction::Login { provider } => {
            let vendor = resolve_vendor(provider)?;

            println!("Get a key at {}", vendor.signup_url);
            // Read without echo: a key pasted into a terminal otherwise lands in
            // the scrollback and, on most shells, in the history file.
            let key = rpassword::prompt_password(format!("{} API key: ", vendor.name))?;

            if key.trim().is_empty() {
                anyhow::bail!("nothing entered — no key was stored");
            }
            tc_config::secrets::store(vendor, key.trim())?;

            println!("Stored in the system keyring.");
            if std::env::var(vendor.api_key_env).is_ok() {
                // Saying nothing here would leave someone debugging why their new
                // key had no effect.
                println!(
                    "Note: {} is also set in your environment and takes precedence.",
                    vendor.api_key_env
                );
            }
        }

        AuthAction::Logout { provider } => {
            let vendor = resolve_vendor(provider)?;
            tc_config::secrets::forget(vendor)?;
            println!("Removed {} from the system keyring.", vendor.name);
        }
    }
    Ok(())
}

/// Resolves a provider name, listing the alternatives when it is wrong.
fn resolve_vendor(provider: &str) -> anyhow::Result<&'static tc_config::Vendor> {
    tc_config::vendor(provider).ok_or_else(|| {
        let known: Vec<&str> = tc_config::catalog::VENDORS.iter().map(|v| v.id).collect();
        anyhow::anyhow!("unknown provider `{provider}` — try one of: {}", known.join(", "))
    })
}

/// Runs the project's own verification commands.
///
/// The commands come from the project's shape, not from configuration, so this
/// works the first time in a repository true-code has never seen.
///
/// Returns whether everything passed, so it is usable as a CI step.
fn verify(cwd: &Path) -> anyhow::Result<bool> {
    let Some(profile) = tc_tools::verify::detect(cwd) else {
        anyhow::bail!(
            "no recognised project in {} — expected one of Cargo.toml, go.mod,              pyproject.toml or package.json",
            cwd.display()
        );
    };

    println!(
        "{} project — running {} checks
",
        profile.name,
        profile.checks.len()
    );
    let mut all_passed = true;

    for check in profile.checks {
        println!("$ {}", check.command);

        // Inherited stdio: a test suite's own output is what the user wants to
        // read, and capturing it to re-print would only delay and mangle it.
        let status = if cfg!(windows) {
            std::process::Command::new("cmd").arg("/C").arg(check.command).current_dir(cwd).status()
        } else {
            std::process::Command::new("sh").arg("-c").arg(check.command).current_dir(cwd).status()
        };

        match status {
            Ok(status) if status.success() => println!(
                "  ok    {}
",
                check.kind.label()
            ),
            Ok(status) => {
                println!(
                    "  FAIL  {} (exit {})
",
                    check.kind.label(),
                    status.code().unwrap_or(-1)
                );
                all_passed = false;
            }
            Err(err) => {
                println!(
                    "  FAIL  could not run: {err}
"
                );
                all_passed = false;
            }
        }
    }

    println!("{}", if all_passed { "All checks passed." } else { "Some checks failed." });
    Ok(all_passed)
}

/// Checks the setup and reports what is missing.
///
/// Returns whether everything needed to start a session is in place. A first run
/// fails for one of about four reasons, and guessing which one from a stack trace
/// is a bad first impression.
fn doctor(config: &Config, cwd: &Path) -> bool {
    let mut ready = true;

    println!(
        "truecode {}
",
        env!("CARGO_PKG_VERSION")
    );

    let mut check = |label: &str, outcome: Result<String, String>| match outcome {
        Ok(detail) => println!("  ok    {label:<18} {detail}"),
        Err(problem) => {
            println!("  FAIL  {label:<18} {problem}");
            ready = false;
        }
    };

    check(
        "model",
        config
            .model_info()
            .map(|info| format!("{} ({} token context)", info.id, info.context_window))
            .map_err(|err| err.to_string()),
    );

    check(
        "api key",
        config.api_key().map(|_| "found in the environment".to_owned()).map_err(|err| {
            format!(
                "{err}
        PowerShell: $env:ANTHROPIC_API_KEY = \"sk-...\""
            )
        }),
    );

    check(
        "project rules",
        Ledger::load(cwd)
            .map(|ledger| match ledger.constraints().len() {
                0 => "none set".to_owned(),
                n => format!("{n} checked, {} reminders", ledger.reminders().len()),
            })
            .map_err(|err| err.to_string()),
    );

    // Written to on every session; a read-only checkout degrades to no log, which
    // silently costs undo and replay.
    let sessions = cwd.join(".truecode").join("sessions");
    check(
        "session dir",
        std::fs::create_dir_all(&sessions)
            .map(|()| format!("writable — {}", sessions.display()))
            .map_err(|err| format!("not writable: {err} (undo and replay will be unavailable)")),
    );

    println!();
    if ready {
        println!(
            "Ready. Start with `truecode`, or `truecode --permission-mode write` to let it edit."
        );
    } else {
        println!("Fix the FAIL lines above, then run `truecode doctor` again.");
    }
    ready
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
    println!("{:<40} {:>10}  {:>9}  {:>9}", "MODEL", "CONTEXT", "IN/MTOK", "OUT/MTOK");
    for info in tc_config::catalog() {
        match info.price {
            Some(price) => println!(
                "{:<40} {:>10} {:>9.2} {:>10.2}",
                info.id, info.context_window, price.input_per_mtok, price.output_per_mtok
            ),
            // Shown as unknown rather than as zero: a confident 0.00 would be
            // read as "free", which is a costly thing to believe.
            None => {
                println!("{:<40} {:>10} {:>9} {:>10}", info.id, info.context_window, "?", "?");
            }
        }
    }
    println!("\nPrices are indicative list prices, not a billing source of truth.");
    println!(
        "Any `openrouter/<vendor>/<model>` also works. A model true-code cannot price\n\
         reports no cost rather than an invented one."
    );
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
/// Logs go to stderr so that `truecode -p "…" > answer.md` stays pipeable, and
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
        let cli = Cli::try_parse_from(["truecode", "-p", "hello"]).expect("parses");
        assert_eq!(cli.prompt.as_deref(), Some("hello"));
        assert!(cli.command.is_none());
    }

    #[test]
    fn the_model_can_be_overridden_on_the_command_line() {
        let cli =
            Cli::try_parse_from(["truecode", "--model", "openai/gpt-4.1-mini"]).expect("parses");
        assert_eq!(cli.model.as_deref(), Some("openai/gpt-4.1-mini"));
    }

    #[test]
    fn introspection_subcommands_are_reachable() {
        let cli = Cli::try_parse_from(["truecode", "models"]).expect("parses");
        assert!(matches!(cli.command, Some(Command::Models)));
    }

    #[test]
    fn no_arguments_means_the_interactive_tui() {
        let cli = Cli::try_parse_from(["truecode"]).expect("parses");
        assert!(cli.prompt.is_none());
        assert!(cli.command.is_none());
    }
}
