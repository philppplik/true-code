## What and why

<!-- What changed, and what problem it solves. The diff already shows the "what";
     the "why" is what a reviewer cannot reconstruct. -->

## How it was tested

<!-- What you actually ran and observed. Be specific. -->

- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] Tried it by hand (say how)

## What was not covered

<!-- Honest gaps: untested paths, platforms you could not check, known
     limitations. A reviewer who knows where to look is worth more than a
     confident summary. -->

## Checklist

- [ ] Commits follow [Conventional Commits](https://www.conventionalcommits.org/)
- [ ] No API key, token or session log in the diff
- [ ] Engine crates (`tc-core`, `tc-config`, `tc-providers`) gained no UI dependency
- [ ] Structural changes are recorded in an ADR under `docs/adr/`
- [ ] `CHANGELOG.md` updated for user-visible changes
