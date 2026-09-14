//! Reusable prompts, stored as files.
//!
//! A command is a Markdown file in `.truecode/commands/`. Its file name is the
//! command name; its body is the prompt.
//!
//! ```markdown
//! ---
//! description: Review the staged diff
//! argument-hint: [focus area]
//! ---
//! Run `git diff --staged`, then review it for $ARGUMENTS.
//! ```
//!
//! # Why this exact format
//!
//! YAML frontmatter, `$ARGUMENTS`, `$1`…`$9`, one file per command. This is the
//! shape Claude Code and several other harnesses already use, and matching it
//! means someone's existing command files work here without being rewritten.
//! Inventing a better format would cost them a migration and gain them nothing.
//!
//! # What a command is not
//!
//! Text substitution, and nothing more. There is no shell execution, no file
//! inclusion, no conditionals. A prompt file that can run commands is a prompt
//! file that a pull request can turn into a backdoor, and the whole point of
//! these is that they are cheap to read before you trust one.

use std::path::Path;

/// Directory inside `.truecode/` holding command files.
pub const COMMANDS_DIR: &str = "commands";

/// One reusable prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserCommand {
    /// Name typed after the slash, from the file name.
    pub name: String,
    /// One line for the help list. Empty when the file has no frontmatter.
    pub description: String,
    /// What the arguments are, e.g. `[file] [focus]`.
    pub argument_hint: String,
    /// The prompt, with placeholders still in it.
    pub body: String,
}

impl UserCommand {
    /// Fills in the placeholders.
    ///
    /// `$ARGUMENTS` is everything typed after the name; `$1`…`$9` are the
    /// whitespace-separated words. A placeholder with no matching argument
    /// becomes empty rather than being left as literal `$2`, which would reach
    /// the model as nonsense it has to guess at.
    ///
    /// When the body uses no placeholder at all and arguments were given, they
    /// are appended on their own line. Silently discarding what someone typed is
    /// worse than putting it somewhere sensible.
    #[must_use]
    pub fn expand(&self, arguments: &str) -> String {
        let arguments = arguments.trim();
        let words: Vec<&str> = arguments.split_whitespace().collect();

        let uses_placeholder = self.body.contains("$ARGUMENTS")
            || (1..=9).any(|index| self.body.contains(&format!("${index}")));

        let mut out = self.body.replace("$ARGUMENTS", arguments);
        for index in 1..=9 {
            let value = words.get(index - 1).copied().unwrap_or("");
            out = out.replace(&format!("${index}"), value);
        }

        if !uses_placeholder && !arguments.is_empty() {
            out = format!("{}\n\n{arguments}", out.trim_end());
        }
        out
    }

    /// The line shown in `/help` and `truecode commands`.
    #[must_use]
    pub fn summary(&self) -> String {
        let invocation = if self.argument_hint.is_empty() {
            format!("/{}", self.name)
        } else {
            format!("/{} {}", self.name, self.argument_hint)
        };
        if self.description.is_empty() {
            invocation
        } else {
            format!("{invocation:<24} {}", self.description)
        }
    }
}

/// Why a command directory could not be read.
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    /// The directory exists but could not be listed or read.
    #[error("cannot read {path}: {source}")]
    Read {
        /// The path involved.
        path: String,
        /// The underlying error.
        source: std::io::Error,
    },
}

/// Loads every command in `.truecode/commands/`.
///
/// A missing directory is not an error — most projects have none. Files are
/// returned sorted by name so the help list is stable between runs.
///
/// # Errors
///
/// Returns an error if the directory exists but cannot be read.
pub fn load(root: &Path) -> Result<Vec<UserCommand>, CommandError> {
    let dir = root.join(tc_config::PROJECT_DIR).join(COMMANDS_DIR);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(CommandError::Read { path: dir.display().to_string(), source });
        }
    };

    let mut commands = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|source| CommandError::Read { path: dir.display().to_string(), source })?
            .path();

        if path.extension().is_none_or(|ext| ext != "md") {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        // A name with a space could never be typed, and one with a slash would
        // read as a path. Skipping beats offering a command that cannot be used.
        if name.is_empty() || name.contains(char::is_whitespace) || name.contains('/') {
            tracing::warn!(?path, "skipping command file: the name cannot be typed");
            continue;
        }

        let text = std::fs::read_to_string(&path)
            .map_err(|source| CommandError::Read { path: path.display().to_string(), source })?;

        commands.push(parse(name, &text));
    }

    commands.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(commands)
}

/// Splits optional YAML frontmatter from the body.
///
/// Only `description` and `argument-hint` are read, and only as plain strings.
/// A full YAML parser here would be a dependency and an attack surface for two
/// fields; anything it does not understand is ignored rather than rejected, so
/// a file written for another harness still works.
fn parse(name: &str, text: &str) -> UserCommand {
    let mut description = String::new();
    let mut argument_hint = String::new();

    let body = match text.strip_prefix("---") {
        Some(rest) => match rest.find("\n---") {
            Some(end) => {
                for line in rest[..end].lines() {
                    let Some((key, value)) = line.split_once(':') else { continue };
                    let value = value.trim().trim_matches('"').trim_matches('\'').to_owned();
                    match key.trim() {
                        "description" => description = value,
                        "argument-hint" | "argument_hint" => argument_hint = value,
                        _ => {}
                    }
                }
                rest[end + "\n---".len()..].trim_start_matches('-').trim_start()
            }
            // An unterminated frontmatter block is far more likely to be a
            // document that happens to start with a rule than a broken header.
            None => text,
        },
        None => text,
    };

    UserCommand { name: name.to_owned(), description, argument_hint, body: body.trim().to_owned() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, body: &str) {
        let commands = dir.join(tc_config::PROJECT_DIR).join(COMMANDS_DIR);
        std::fs::create_dir_all(&commands).expect("create commands dir");
        std::fs::write(commands.join(name), body).expect("write command");
    }

    fn command(body: &str) -> UserCommand {
        UserCommand {
            name: "t".to_owned(),
            description: String::new(),
            argument_hint: String::new(),
            body: body.to_owned(),
        }
    }

    #[test]
    fn a_project_with_no_commands_is_not_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");

        assert!(load(dir.path()).expect("a missing directory is fine").is_empty());
    }

    #[test]
    fn frontmatter_is_read_and_kept_out_of_the_prompt() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(
            dir.path(),
            "review.md",
            "---\ndescription: Review the diff\nargument-hint: [focus]\n---\nReview it.",
        );

        let commands = load(dir.path()).expect("loads");

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].name, "review");
        assert_eq!(commands[0].description, "Review the diff");
        assert_eq!(commands[0].argument_hint, "[focus]");
        assert_eq!(commands[0].body, "Review it.");
    }

    #[test]
    fn a_file_without_frontmatter_is_all_prompt() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "plain.md", "Just the prompt.");

        let commands = load(dir.path()).expect("loads");

        assert_eq!(commands[0].body, "Just the prompt.");
        assert!(commands[0].description.is_empty());
    }

    #[test]
    fn an_unknown_frontmatter_key_is_ignored_rather_than_refused() {
        // Files written for another harness must keep working.
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "x.md", "---\nmodel: something-else\ndescription: Hi\n---\nBody.");

        let commands = load(dir.path()).expect("loads");

        assert_eq!(commands[0].description, "Hi");
        assert_eq!(commands[0].body, "Body.");
    }

    #[test]
    fn non_markdown_files_are_left_alone() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "notes.txt", "not a command");

        assert!(load(dir.path()).expect("loads").is_empty());
    }

    #[test]
    fn commands_are_listed_in_a_stable_order() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "zebra.md", "z");
        write(dir.path(), "alpha.md", "a");

        let names: Vec<String> =
            load(dir.path()).expect("loads").into_iter().map(|c| c.name).collect();

        assert_eq!(names, ["alpha", "zebra"]);
    }

    #[test]
    fn dollar_arguments_takes_everything_typed() {
        assert_eq!(
            command("Look at $ARGUMENTS now").expand("the auth module"),
            "Look at the auth module now"
        );
    }

    #[test]
    fn numbered_placeholders_take_one_word_each() {
        assert_eq!(command("$1 then $2").expand("first second"), "first then second");
    }

    #[test]
    fn a_placeholder_with_no_argument_becomes_empty_rather_than_literal() {
        // Leaving `$2` in place would reach the model as nonsense to guess at.
        assert_eq!(command("$1|$2").expand("only"), "only|");
    }

    #[test]
    fn arguments_are_appended_when_the_body_has_nowhere_to_put_them() {
        // Silently discarding what someone typed is the worse failure.
        assert_eq!(
            command("Review the diff.").expand("focus on errors"),
            "Review the diff.\n\nfocus on errors"
        );
    }

    #[test]
    fn a_body_with_no_placeholders_is_unchanged_when_nothing_was_typed() {
        assert_eq!(command("Review the diff.").expand("   "), "Review the diff.");
    }

    #[test]
    fn the_summary_shows_the_argument_hint_when_there_is_one() {
        let shown = UserCommand {
            name: "review".to_owned(),
            description: "Review the diff".to_owned(),
            argument_hint: "[focus]".to_owned(),
            body: String::new(),
        }
        .summary();

        assert!(shown.contains("/review [focus]"), "unexpected: {shown}");
        assert!(shown.contains("Review the diff"), "unexpected: {shown}");
    }
}
