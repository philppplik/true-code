# Installing true-code

The command is **`truecode`** (no hyphen). The project is called true-code; the
thing you type is `truecode`.

There is no installer yet — no `scoop`, no `brew`, no prebuilt binaries. That is
on the roadmap (phase P5) and deliberately not yet done: shipping signed binaries
for four platforms is its own project, and the tool is still changing shape every
week. Until then you build it, which takes one command and about two minutes.

---

## 1. Install Rust

true-code needs Rust **1.88 or newer**.

**Windows**

```powershell
winget install Rustlang.Rustup
```

**macOS / Linux**

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Open a **new** terminal afterwards — the installer adds `~/.cargo/bin` to your
`PATH`, and an already-open shell will not have it yet. Check:

```bash
cargo --version
```

If that prints nothing, your `PATH` has not picked up `~/.cargo/bin` (on Windows:
`%USERPROFILE%\.cargo\bin`). Restarting the terminal fixes it in almost every case.

## 2. Build and install

```bash
git clone https://github.com/philppplik/true-code
cd true-code
cargo install --path crates/tc-cli
```

`cargo install` puts `truecode` in `~/.cargo/bin`, which is already on your `PATH`
from step 1. Confirm:

```bash
truecode --version
```

<details>
<summary>Prefer not to install it globally?</summary>

```bash
cargo build --release
./target/release/truecode --help     # .\target\release\truecode.exe on Windows
```

</details>

## 3. Set an API key

Keys are read from the **environment only** — never from a config file, so a
config you commit can never leak a credential.

**Windows (PowerShell), for this session**

```powershell
$env:ANTHROPIC_API_KEY = "sk-ant-..."
```

**Windows, permanently**

```powershell
[Environment]::SetEnvironmentVariable("ANTHROPIC_API_KEY", "sk-ant-...", "User")
```

Open a new terminal afterwards for it to take effect.

**macOS / Linux**

```bash
export ANTHROPIC_API_KEY="sk-ant-..."     # add to ~/.zshrc or ~/.bashrc to keep it
```

For OpenAI models the variable is `OPENAI_API_KEY`. `truecode models` lists which
models exist and what each is assumed to cost.

## 4. Check the setup

```bash
truecode doctor
```

It checks the model, the API key, the project rules and whether the session
directory is writable, and names whatever is missing. Run it first whenever
something behaves oddly — it answers the four questions a first run usually
fails on.

## 5. Use it

```bash
truecode                                  # a session, read-only
truecode --permission-mode write          # can edit files, asks before each change
truecode --permission-mode full           # can also run commands
truecode -p "what does src/lib.rs do?"    # one question, answer on stdout
```

Run it **from inside the project you want to work on**. true-code only ever
touches the directory it was started in, and `truecode -C <dir>` points it
somewhere else.

Commands: `truecode --help` lists everything. The ones worth knowing early:

| Command | What it does |
|---|---|
| `truecode doctor` | Check the setup |
| `truecode undo` | Revert the last change true-code made |
| `truecode verify` | Run this project's build, tests and lint |
| `truecode constraints` | Show the project rules in force |
| `truecode models` | Known models, context windows, assumed prices |
| `truecode config` | Resolved configuration and where each part came from |

Inside a session: `Enter` sends · `Esc` aborts the running turn · `/undo` reverts
the last change · `/help` lists commands · `Ctrl+C` quits.

## Updating

```bash
cd true-code
git pull
cargo install --path crates/tc-cli --force
```

## Uninstalling

```bash
cargo uninstall tc-cli
```

Project state lives in `.truecode/` inside each project you used it in — session
logs, undo backups and your rules. Deleting that directory removes the history and
makes `truecode undo` unable to revert anything; the rules file is worth keeping.

---

## When something goes wrong

**`truecode: command not found`** — `~/.cargo/bin` is not on your `PATH`, or the
terminal predates the install. Open a new one; if it persists, add the directory
to `PATH` by hand.

**`no API key found`** — set the variable in the shell you are actually running
`truecode` from. On Windows a variable set with `$env:` only lasts for that
terminal window.

**`error: package requires rustc 1.88`** — `rustup update`.

**The terminal looks broken after a crash** — it should not; the alternate screen
is restored from a panic hook. If it ever does, `reset` on Unix or closing the
window on Windows fixes it, and please
[open an issue](https://github.com/philppplik/true-code/issues) with what you did.

**A build error after `git pull`** — `cargo clean` and build again. If it persists
it is a real bug, not your machine; an issue with the output is useful.
