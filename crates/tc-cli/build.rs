//! Records which commit this binary was built from.
//!
//! `truecode update` needs to answer "am I behind?", and a version number cannot
//! answer it while everything is 0.1.0 and installed from source. The commit can.
//!
//! A build outside a git checkout — a crates.io tarball, a vendored copy — simply
//! has no commit, and the update check says it cannot tell rather than inventing
//! an answer.

use std::process::Command;

fn main() {
    let commit = Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|sha| sha.trim().to_owned())
        .unwrap_or_default();

    println!("cargo:rustc-env=TRUECODE_COMMIT={commit}");

    // Without this the recorded commit would be whatever it was the first time
    // this crate compiled, which is worse than no commit at all.
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs/heads/main");
}
