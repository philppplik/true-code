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

## 3. Choose a provider and add a key

**The easy way** — just start it:

```bash
truecode
```

On a first run with no key, true-code asks which provider to use and lets you
paste a key. It is stored in your **operating system's keyring** — Credential
Manager on Windows, Keychain on macOS, the kernel session keyring on Linux. Never
in a file true-code wrote, so a config you commit can never leak a credential.

> **On Linux the key does not survive a reboot.** It lives in the kernel session
> keyring, which is the only option that does not require `libdbus-1-dev` to be
> installed before true-code will even build. Use the environment variable below
> if you want it to stick.

**Or from the command line:**

```bash
truecode auth login anthropic     # or: openai, openrouter
truecode auth status              # which providers have a key
truecode auth logout openai       # remove one
```

The key is read without echo, so it does not land in your shell history.

### Which provider?

| Provider | Key from | Why |
|---|---|---|
| **Anthropic** | [console.anthropic.com](https://console.anthropic.com/settings/keys) | The default model, `anthropic/claude-sonnet-4-5` |
| **OpenAI** | [platform.openai.com](https://platform.openai.com/api-keys) | `openai/gpt-4.1`, `openai/gpt-4.1-mini` |
| **OpenRouter** | [openrouter.ai/keys](https://openrouter.ai/keys) | **One key, every vendor.** Easiest if you want to try several |

OpenRouter is worth knowing about if you do not want an account per vendor. Any
model it proxies works, whether or not this build has heard of it:

```bash
truecode --model openrouter/anthropic/claude-sonnet-4.5
truecode --model openrouter/openai/gpt-4.1-mini
truecode --model openrouter/google/gemini-2.5-pro      # also fine
```

For a model true-code has no price for, the cost display reports **no cost rather
than a guessed one**. An invented price looks exactly like a real one, which is
the problem with inventing it.

### Environment variables still work

`ANTHROPIC_API_KEY`, `OPENAI_API_KEY` and `OPENROUTER_API_KEY` are read first and
**always win** over the keyring, so CI stays predictable and a temporary override
needs no cleanup.

```powershell
$env:ANTHROPIC_API_KEY = "sk-ant-..."          # this terminal only
[Environment]::SetEnvironmentVariable("ANTHROPIC_API_KEY", "sk-ant-...", "User")
```

```bash
export ANTHROPIC_API_KEY="sk-ant-..."          # add to ~/.zshrc to keep it
```

On Linux the keyring forgets the key when the session ends, and on a headless box
it may be unavailable entirely — the environment variable is the durable answer
there. true-code says so rather than failing obscurely.

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
truecode --learn                          # ask me one question after each change
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
| `truecode auth status` | Which providers have a key |
| `truecode learn` | What you have been asked about, and how it went |
| `truecode models` | Known models, context windows, assumed prices |
| `truecode constraints` | Show the project rules in force |
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

**`no API key for …`** — run `truecode auth login <provider>`, or set the
environment variable in the shell you are actually running `truecode` from. On
Windows a variable set with `$env:` only lasts for that terminal window.

**A new key seems to have no effect** — an environment variable beats the keyring.
`truecode auth status` shows which one is being used.

**`the system keyring is unavailable`** — usual on a headless Linux box with no
Secret Service running. Use the environment variable instead.

**`error: package requires rustc 1.88`** — `rustup update`.

**The terminal looks broken after a crash** — it should not; the alternate screen
is restored from a panic hook. If it ever does, `reset` on Unix or closing the
window on Windows fixes it, and please
[open an issue](https://github.com/philppplik/true-code/issues) with what you did.

**A build error after `git pull`** — `cargo clean` and build again. If it persists
it is a real bug, not your machine; an issue with the output is useful.
