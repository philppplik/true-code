//! Finding an executable the way the user's shell would.
//!
//! # The bug this exists for
//!
//! Almost every MCP server in the wild is launched with `npx` or `uvx`. On
//! Windows those are installed as `npx.cmd` and `uvx.exe`, and the bare name
//! `npx` also exists as an extensionless shell script for Git Bash. Rust's
//! `Command::new("npx")` does not apply `PATHEXT`, so it looks only for a file
//! literally called `npx`, finds the shell script, and fails to execute it.
//!
//! The result was `cannot run \`npx\`: program not found` on a machine where
//! `npx --version` works in the same terminal — the least believable error
//! message a tool can produce. For a project that calls itself Windows-first,
//! that is not a papercut.
//!
//! On Unix this is a no-op: the name is returned unchanged and the OS does the
//! lookup, which is what `PATH` is for.

use std::path::PathBuf;

/// Resolves `command` to something the OS will actually execute.
///
/// Returns the input unchanged on Unix, when the command already has an
/// extension, when it contains a path separator, or when nothing matches —
/// in the last case the OS produces the error, which is the right place for it.
#[must_use]
pub fn resolve(command: &str) -> String {
    if !cfg!(windows) {
        return command.to_owned();
    }
    // A path is the user being specific; second-guessing it would be worse than
    // any error it produces.
    if command.contains('/') || command.contains('\\') {
        return command.to_owned();
    }
    if PathBuf::from(command).extension().is_some() {
        return command.to_owned();
    }

    let Some(path) = std::env::var_os("PATH") else {
        return command.to_owned();
    };
    // The order of PATHEXT is the order the shell would try, and it matters:
    // `.CMD` before `.PS1` is why `npx` resolves to the batch wrapper rather
    // than a PowerShell script that CreateProcess cannot run.
    let extensions = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_owned());

    for dir in std::env::split_paths(&path) {
        for extension in extensions.split(';').filter(|ext| !ext.is_empty()) {
            let candidate = dir.join(format!("{command}{extension}"));
            if candidate.is_file() {
                return candidate.display().to_string();
            }
        }
    }

    command.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_with_an_extension_is_left_alone() {
        assert_eq!(resolve("npx.cmd"), "npx.cmd");
    }

    #[test]
    fn a_path_is_left_alone() {
        // Being specific is the user's prerogative.
        assert_eq!(resolve("./tools/server"), "./tools/server");
        assert_eq!(resolve("C:\\tools\\server"), "C:\\tools\\server");
    }

    #[test]
    fn an_unknown_command_is_returned_unchanged() {
        // So the OS reports it, rather than this function inventing an error.
        assert_eq!(resolve("definitely-not-installed-8c1a"), "definitely-not-installed-8c1a");
    }

    #[test]
    #[cfg(unix)]
    fn unix_resolution_is_left_to_the_operating_system() {
        assert_eq!(resolve("sh"), "sh");
    }

    #[test]
    #[cfg(windows)]
    fn a_windows_command_resolves_to_something_that_exists() {
        // `cmd` is on PATH as cmd.exe on every Windows installation.
        let resolved = resolve("cmd");

        assert!(
            resolved.to_lowercase().ends_with(".exe"),
            "expected an executable with an extension, got {resolved}"
        );
        assert!(std::path::Path::new(&resolved).is_file(), "{resolved} should exist");
    }
}
