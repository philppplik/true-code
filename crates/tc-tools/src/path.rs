//! Workspace-scoped path resolution.
//!
//! This is the security boundary of the tool layer. Every path a model supplies
//! passes through [`resolve_in_workspace`] before it reaches the filesystem.
//!
//! # Why string checks are not enough
//!
//! Rejecting paths that contain `..` is the obvious approach and it is wrong in
//! both directions: it rejects the legitimate `src/../README.md`, and it misses
//! a symlink inside the workspace that points at `/etc/shadow`. The only check
//! that holds is to resolve the path for real — following symlinks — and then
//! ask whether the result is still inside the workspace.
//!
//! Because a path being written to may not exist yet, resolution walks up to the
//! deepest ancestor that does exist, canonicalises *that*, and re-appends the
//! remainder. A non-existent file inside a symlinked-out directory is therefore
//! still caught.

use std::path::{Component, Path, PathBuf};

/// Reasons a path is refused.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PathError {
    /// The resolved path lies outside the workspace root.
    #[error("path `{0}` is outside the workspace — true-code only touches the project directory")]
    OutsideWorkspace(String),

    /// The workspace root itself could not be resolved.
    #[error("workspace root `{0}` does not exist or is not readable")]
    UnusableRoot(String),

    /// The path was empty.
    #[error("path must not be empty")]
    Empty,
}

/// Resolves a model-supplied path against the workspace root.
///
/// Returns the absolute, symlink-resolved path, guaranteed to be inside `root`.
pub fn resolve_in_workspace(root: &Path, candidate: &str) -> Result<PathBuf, PathError> {
    if candidate.trim().is_empty() {
        return Err(PathError::Empty);
    }

    let root =
        root.canonicalize().map_err(|_| PathError::UnusableRoot(root.display().to_string()))?;

    let requested = Path::new(candidate);
    let joined =
        if requested.is_absolute() { requested.to_path_buf() } else { root.join(requested) };

    let resolved = canonicalize_deepest_existing(&joined);

    if resolved.starts_with(&root) {
        Ok(resolved)
    } else {
        Err(PathError::OutsideWorkspace(candidate.to_owned()))
    }
}

/// Canonicalises as much of `path` as exists, then re-appends the rest.
///
/// `Path::canonicalize` fails outright on a missing path, which would make it
/// useless for checking a file about to be created. Walking up to the deepest
/// existing ancestor keeps the symlink resolution that makes the check sound.
fn canonicalize_deepest_existing(path: &Path) -> PathBuf {
    if let Ok(resolved) = path.canonicalize() {
        return resolved;
    }

    let mut remainder = Vec::new();
    let mut current = path;

    while let Some(parent) = current.parent() {
        if let Some(name) = current.file_name() {
            remainder.push(name.to_owned());
        }
        if let Ok(resolved) = parent.canonicalize() {
            let mut result = resolved;
            for part in remainder.iter().rev() {
                result.push(part);
            }
            return result;
        }
        current = parent;
    }

    // Nothing along the path exists. Fall back to a lexically normalised form,
    // which still cannot escape the root once the prefix check runs.
    normalise_lexically(path)
}

/// Resolves `.` and `..` textually, without touching the filesystem.
fn normalise_lexically(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                result.pop();
            }
            Component::CurDir => {}
            other => result.push(other.as_os_str()),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a workspace with `src/lib.rs` and a `secret.txt` sibling *outside* it.
    fn workspace() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("temp dir is creatable");
        let root = dir.path().join("project");
        std::fs::create_dir_all(root.join("src")).expect("workspace is creatable");
        std::fs::write(root.join("src/lib.rs"), "fn main() {}").expect("file is writable");
        std::fs::write(dir.path().join("secret.txt"), "sensitive").expect("file is writable");
        (dir, root)
    }

    #[test]
    fn a_relative_path_inside_the_workspace_resolves() {
        let (_guard, root) = workspace();
        let resolved = resolve_in_workspace(&root, "src/lib.rs").expect("path is inside");
        assert!(resolved.ends_with("lib.rs"));
    }

    #[test]
    fn parent_traversal_that_stays_inside_is_allowed() {
        let (_guard, root) = workspace();
        // Legitimate: a string-based `..` check would wrongly reject this.
        let resolved = resolve_in_workspace(&root, "src/../src/lib.rs").expect("path is inside");
        assert!(resolved.ends_with("lib.rs"));
    }

    #[test]
    fn parent_traversal_that_escapes_is_refused() {
        let (_guard, root) = workspace();
        let error = resolve_in_workspace(&root, "../secret.txt").expect_err("path escapes");
        assert!(matches!(error, PathError::OutsideWorkspace(_)));
    }

    #[test]
    fn deep_traversal_to_a_home_directory_is_refused() {
        let (_guard, root) = workspace();
        let error =
            resolve_in_workspace(&root, "../../../../../../.ssh/id_rsa").expect_err("path escapes");
        assert!(matches!(error, PathError::OutsideWorkspace(_)));
    }

    #[test]
    fn an_absolute_path_outside_the_workspace_is_refused() {
        let (guard, root) = workspace();
        let outside = guard.path().join("secret.txt");
        let error =
            resolve_in_workspace(&root, &outside.display().to_string()).expect_err("path escapes");
        assert!(matches!(error, PathError::OutsideWorkspace(_)));
    }

    #[test]
    fn an_absolute_path_inside_the_workspace_is_allowed() {
        let (_guard, root) = workspace();
        let inside = root.join("src/lib.rs");
        assert!(resolve_in_workspace(&root, &inside.display().to_string()).is_ok());
    }

    #[test]
    fn a_file_that_does_not_exist_yet_still_resolves_inside() {
        let (_guard, root) = workspace();
        let resolved = resolve_in_workspace(&root, "src/new_module.rs").expect("path is inside");
        assert!(resolved.ends_with("new_module.rs"));
    }

    #[test]
    fn a_missing_file_in_an_escaping_directory_is_still_refused() {
        let (_guard, root) = workspace();
        let error = resolve_in_workspace(&root, "../elsewhere/new.rs").expect_err("path escapes");
        assert!(matches!(error, PathError::OutsideWorkspace(_)));
    }

    #[test]
    fn an_empty_path_is_refused() {
        let (_guard, root) = workspace();
        assert_eq!(resolve_in_workspace(&root, "   "), Err(PathError::Empty));
    }

    #[test]
    fn the_workspace_root_itself_resolves() {
        let (_guard, root) = workspace();
        assert!(resolve_in_workspace(&root, ".").is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_pointing_out_of_the_workspace_is_refused() {
        let (guard, root) = workspace();
        // The case a textual `..` check cannot see: the path contains no `..`
        // at all, yet it leaves the workspace.
        std::os::unix::fs::symlink(guard.path().join("secret.txt"), root.join("innocent.txt"))
            .expect("symlink is creatable");

        let error = resolve_in_workspace(&root, "innocent.txt").expect_err("symlink escapes");
        assert!(matches!(error, PathError::OutsideWorkspace(_)));
    }

    #[test]
    fn lexical_normalisation_collapses_parent_components() {
        assert_eq!(normalise_lexically(Path::new("a/b/../c")), PathBuf::from("a/c"));
        assert_eq!(normalise_lexically(Path::new("a/./b")), PathBuf::from("a/b"));
    }
}
