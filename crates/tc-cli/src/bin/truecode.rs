//! The `truecode` command.
//!
//! Both names install the same CLI: the project is called true-code and the
//! command is `truecode`, and mistyping one for the other should not be an
//! error the user has to diagnose.

fn main() -> anyhow::Result<()> {
    tc_cli::run()
}
