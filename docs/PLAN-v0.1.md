# true-code — Projektplan v0.1

**Stand:** 13.09.2026 · **Status:** Planungsphase · **Sprache:** Deutsch (Code/Commits englisch)
**Idee:** Eine agentische CLI (Rust + TUI), die *vibe-coding*, *vibe-working* und *vibe-learning* in einem Terminal vereint.

---

## 0. Realitätscheck zuerst (bitte ehrlich lesen)

Claude Code und Codex CLI sind 2026 keine Nischentools mehr, sondern ausgereifte Produkte mit
Millionen-Nutzerbasis, eigenen Modellen und dutzenden Engineers. **Feature-Parität als Ziel ist der
sichere Tod des Projekts.** Du gewinnst nicht, indem du alles nachbaust, sondern indem du in
2–3 Dimensionen *messbar besser* bist als beide.

Was die Konkurrenz heute kann (Stand 2026) — das ist dein "Tisch", nicht dein Ziel:

| Dimension | Claude Code | Codex CLI |
|---|---|---|
| Implementierung | TypeScript | **Rust** (+ TS) |
| Sicherheit | App-Layer: Hooks (>20 Events) | **Kernel-Layer:** Seatbelt (macOS), Landlock+seccomp (Linux) |
| Projekt-Instruktionen | `CLAUDE.md` (proprietär) | `AGENTS.md` (Cross-Tool-Standard, 60k+ Projekte) |
| Erweiterbarkeit | MCP, Hooks, Skills, Slash-Commands | MCP, Skills, Subagents, Cloud-Exec |
| Modelle | Nur Anthropic | Nur OpenAI |
| Autonomie | Approval-Gates, Agent Teams | Full-Auto, Cloud-Tasks |

Quellen: [1](https://www.nxcode.io/resources/news/claude-code-vs-codex-cli-terminal-coding-comparison-2026), [3](https://blakecrosley.com/blog/claude-code-vs-codex)

**Die Lücken, die beide offen lassen — da greifst du an:**

1. **Keiner ist model-neutral.** Claude Code = nur Anthropic, Codex = nur OpenAI. Ein Harness, der
   pro *Aufgabe* das beste Modell wählt und Kosten live anzeigt, ist ein echter USP.
2. **Keiner lehrt.** Beide *machen* Code. Niemand erklärt dem Anfänger, *warum* — mit Wissensstand,
   Lernpfad und Wiederholung. → dein „vibe-learning".
3. **Windows ist bei beiden zweitklassig** (Kernel-Sandbox = Unix-only). Du kommst von Windows →
   baue Windows als *First-Class*-Plattform. Riesiger, underservter Markt.
4. **Transparenz.** Keiner zeigt verständlich: *Was hat der Agent gerade gelesen? Warum? Was kostet
   der Turns? Was würde er als Nächstes tun?* → „Explain yourself"-Modus.
5. **Offline/Lokal.** Beide brauchen Cloud-Abos. Ein Modus mit lokalen Modellen (Ollama/llama.cpp)
   + eigenem API-Key ist für viele der Kaufgrund.

> **Merksatz:** Nicht „besseres Claude Code" bauen. Sondern: *„Der agentische Harness, der dir
> beibringt, was er gerade gebaut hat — und dir sagt, was es kostet."*

---

## 1. Produktkern: Die drei Modi (scharf definieren!)

Das ist der wichtigste Schritt. „Vibe-X" ist ein Gefühl, kein Feature. Mach es konkret:

### Modus A — **CODE** (vibe-coding)
Agentischer Default: Plan → Tools → Diff → Bestätigung → Test → Commit.
* Nicht-chatlastig: **Diff-first-UI**, nicht Textwall.
* Jeder Schreibzugriff = sichtbarer Diff + `[a]ccept [e]dit [r]eject [?]erklären`.
* Auto-Verify: erkennt Sprache, führt `cargo test` / `pytest` / `npm test` aus, zeigt Fehler.

### Modus B — **WORK** (vibe-working)
Nicht-Code-Arbeit im selben Agenten: Repo-Research, Refactor-Plan, Release-Notes, Issue-Triage,
Doku, Migrationen, Shell-Workflows.
* **Artefakt-orientiert:** Ergebnis ist eine Datei/PR/Checkliste, kein Chatverlauf.
* Multi-Agent: `researcher` (liest nur) → `planner` → `implementer` → `reviewer`.
* Session ist **wiederaufnehmbar** (`true-code resume`) und als Markdown exportierbar.

### Modus C — **LEARN** ⭐ (dein Differenzierungsmerkmal)
Gleiche Engine, anderes Ziel: **Der Nutzer soll am Ende mehr können.**
* Der Agent schreibt nicht still den Code, sondern **erklärt jeden Schritt** und stellt
  Verständnisfragen (Sokratischer Modus), bevor er etwas ändert.
* **Lernstands-Profil** (`~/.true-code/profile.toml`): Rust-Anfänger? Python-Profi? Erklärt
  accordingly (kein „wie du weißt…", aber auch kein „Compiler = Übersetzer").
* **Konzept-Tracker:** erkannte Themen (Ownership, async, Traits, pytest-Fixtures …) mit
  Wiedererkennung: „Du hattest bei `Result<T,E>` letzte Woche gefragt — hier ist der Transfer."
* **Sandbox-Übungen:** „Versuch es selbst, ich schaue zu" — Agent wartet, reviewt deinen Code,
  gibt Hinweise statt Lösungen.
* **Glossar/Notizen:** `/note` speichert Erklärungen in eine wachsende Wissensdatenbank
  (lokal, Markdown, später durchsuchbar).

---

## 2. Architektur: Die 10 Schichten

Der Kern einer agentischen CLI ist **nicht** die UI und **nicht** das Modell. Es ist der
**Harness** — die Software, die dem Modell Werkzeuge, Kontext und Grenzen gibt.

```
┌─────────────────────────────────────────────────────────────┐
│ 10  TUI (ratatui + crossterm)  ·  Diff-Viewer  ·  Streaming │
├─────────────────────────────────────────────────────────────┤
│  9  Session / Persistenz: Event-Log, Resume, Undo, Checkpoints│
├─────────────────────────────────────────────────────────────┤
│  8  Integrationen: MCP · ACP · LSP · Git · Hooks · Skills    │
├─────────────────────────────────────────────────────────────┤
│  7  Permissions & Sandbox (Trust-Modell + OS-Limits)         │
├─────────────────────────────────────────────────────────────┤
│  6  Tool-Layer: read · write · patch · grep · shell · web    │
├─────────────────────────────────────────────────────────────┤
│  5  Context-Engine: Index, Retrieval, Kompression, Budget    │
├─────────────────────────────────────────────────────────────┤
│  4  Agent-Loop (ReAct/Tool-Call-Schleife) + Plan-Modus       │
├─────────────────────────────────────────────────────────────┤
│  3  Provider-Layer: Anthropic · OpenAI · Google · Ollama · … │
├─────────────────────────────────────────────────────────────┤
│  2  Telemetrie / Kosten / Evals / Tracing                    │
├─────────────────────────────────────────────────────────────┤
│  1  Fundament: tokio · Config · Secrets · Logging · Errors   │
└─────────────────────────────────────────────────────────────┘
```

### 2.1 Crate-Workspace (Repo-Layout)

```
true-code/
├── Cargo.toml                 # Workspace
├── crates/
│   ├── tc-core/               # Domänenmodell: Message, ToolCall, Session, Event
│   ├── tc-agent/              # Der Loop, Plan-Modus, Subagent-Orchestrierung
│   ├── tc-tools/              # Tool-Trait + Built-ins (fs, shell, grep, git, web)
│   ├── tc-context/            # Index, Chunking, Retrieval, Kompression, Token-Budget
│   ├── tc-providers/          # Provider-Trait + Anthropic/OpenAI/Gemini/Ollama-Adapter
│   ├── tc-sandbox/            # Permission-Engine + OS-Sandbox (Unix: Landlock, Win: Job Objects)
│   ├── tc-mcp/                # MCP-Client (rmcp), Server-Lifecycle, Tool-Namespace
│   ├── tc-acp/                # ACP-Agent-Seite (Editor-Integration)
│   ├── tc-lsp/                # Optional: Diagnostics nach dem Editieren
│   ├── tc-tui/                # ratatui: Widgets, Keymap, Themes, Diff-Viewer
│   ├── tc-config/             # true-code.toml, AGENTS.md/TRUECODE.md, Profiles, Secrets
│   └── tc-cli/                # clap-basiertes Binary, Subcommands
├── docs/                      # dieser Plan + ADRs
├── evals/                     # Eval-Suite (eigene Tasks + Skripte)
└── xtask/                     # Build-/Release-Helfer
```

**Wichtig:** `tc-agent` darf **keine** ratatui-Abhängigkeit haben. Der Headless-Modus
(`true-code -p "…"` für CI) muss ohne TUI funktionieren — sonst kannst du später nicht testen,
nicht in CI laufen und nicht ACP bedienen. **Trennung von Engine und UI ist Architektur-Gesetz #1.**

### 2.2 Der Agent-Loop (Kern, ~200 Zeilen, aber 80 % des Verhaltens)

```rust
loop {
    if turn > MAX_TURNS { bail("Zu viele Schritte – bitte Ziel präzisieren") }
    if ctx.tokens > BUDGET_SOFT { ctx = ctx.compress(model).await?; }   // Auto-Compact
    if cost > COST_LIMIT && !confirmed { abort_with_report(); }

    let stream = provider.stream(messages.clone(), &tools).await?;      // SSE
    let (text, calls) = render_and_collect(stream).await?;               // TUI lebt weiter

    if calls.is_empty() { break; }                                       // fertig

    for call in calls {
        let decision = permissions.evaluate(&call).await?;   // AllowOnce/Always/Deny/Ask
        match decision {
            Deny  => messages.push(tool_error(&call, "vom Nutzer abgelehnt")),
            Ask   => match ui.ask_permission(&call).await? {            // UI-Callback, nicht TUI!
                        once  => { run(call).await?; }
                        always=> { permissions.remember(&call); run(call).await?; }
                        deny  => messages.push(tool_error(...)),
                        abort => return Ok(Aborted),
                     },
            Allow => { run(call).await?; }
        }
    }
    events.append(&messages);      // nach JEDEM Turn auf Disk → Resume/Undo
}
```

Details, die Anfänger gerne vergessen:
* **Abort jederzeit:** `Esc` bricht den Stream ab, der Loop muss `CancellationToken` (tokio_util)
  respektieren — inkl. laufender Shell-Befehle (Prozessgruppe killen!).
* **Tool-Fehler sind Kontext, kein Crash.** Ein fehlgeschlagener `cargo build` muss als
  Tool-Result ans Modell zurück — das ist der Hauptlernkanal des Agenten.
* **Tool-Result-Bloat:** Ein `cat` auf eine 5-MB-Datei ruiniert Kontext und Budget. Harte Limits:
  Ausgabe kappen (z. B. 30 kB), in Temp-Datei umleiten, Pfad + Vorschau zurückgeben.
* **Idempotenz/Determinismus:** gleiche Eingabe → nachvollziehbarer Ablauf. Mach Zufälliges
  (Reihenfolge paralleler Tool-Calls) stabil, sonst sind Evals wertlos.

### 2.3 Tool-Layer

Starte mit **7 Tools**, nicht 30:

| Tool | Warum | Gefahr |
|---|---|---|
| `read_file` (Zeilenbereiche) | Basis | große Dateien → limitieren |
| `write_file` | Basis | Überschreiben → Backup/Undo |
| `patch` (Suche/Ersetze-Blöcke) | das wichtigste Editier-Tool | Fuzzy-Match nötig |
| `list_dir` / `glob` | Orientierung | `.git`, `node_modules`, `target` ignorieren |
| `grep` (ripgrep) | Code-Suche | Regex-Kosten, Binary-Files |
| `shell` (ein Befehl, Timeout) | Build/Test/Git | **Sicherheitsrisiko #1** |
| `ask_user` | Rückfragen | Endlosschleifen |

Alles Weitere (Web, DB, Jira, Browser) kommt **ausschließlich über MCP** — nie hart einbauen.

### 2.4 Permissions & Sandbox — dein wichtigstes Vertrauens-Feature

Drei Stufen, sichtbar in der UI (wie Codex: read-only / workspace-write / full-access):

* **Stufe 1 – Read-Only (Default):** Lesen, Suchen, Planen. Kein Schreiben, kein Netz.
* **Stufe 2 – Workspace-Write:** Schreiben nur innerhalb des Projektordners + `TMPDIR`.
* **Stufe 3 – Full:** alles, mit Bestätigung pro Befehl.

**Technisch:**
* App-Layer: Allowlist/Denylist für Befehle (`rm -rf`, `curl | sh`, `git push --force` → immer fragen),
  Pfadprüfung gegen Projektwurzel, Symlink-Auflösung (Path-Traversal!).
* OS-Layer (draufsetzen, wo möglich):
  * Linux: **Landlock + seccomp** (wie Codex) — verhindert, dass ein prompt-injizierter Agent
    `~/.ssh` liest, selbst wenn dein Code einen Bug hat.
  * macOS: `sandbox_init` / Seatbelt.
  * Windows: **Job Objects** + eigenständiger, eingeschränkter Token (Integrity Level Low) als
    tragfähige Variante; alternativ Dev-Sandbox/Container. **Das ist deine Windows-Chance.**
* **Netzwerk:** Default aus. Wenn an: nur Allowlist-Domains (die Provider-Endpunkte + MCP-Server).

> **Prompt-Injection ist dein Hauptgegner.** Repository-Inhalte, Webseiten, Issue-Texte sind
> *untrusted input*. Regel: Tool-Resultate werden klar als Daten markiert; Datei-Inhalte dürfen
> keine System-Instruktionen überschreiben; Destruktives (`rm`, `git push`, `DROP TABLE`) braucht
> immer menschliche Bestätigung — auch im Full-Auto-Modus.

### 2.5 Context-Engine — hier gewinnst oder verlierst du

Das Modell ist nur so gut wie das, was du reinschiebst. Baue von Tag 1 an:

1. **Projekt-Index:** Dateibaum + Symbole via **tree-sitter** (Funktionen, Klassen, Imports).
   Nicht Vektordatenbank-first — **Struktur first**. „Wo wird `handle_event` definiert?" ist eine
   Symbolfrage, keine Semantikfrage.
2. **Chunking:** nicht nach Zeilen, sondern nach **Syntax-Knoten** (Funktion/Impl-Block). Das ist
   der Grund, tree-sitter statt syntect zu nehmen (syntect kann nur *färben*).
3. **Token-Budget-Verwaltung:** jeden Schnipsel mit Token-Kosten versehen, priorisieren
   (aktuell geöffnete Datei > Importe > Suchtreffer > Rest), Rest kürzen.
4. **Auto-Compact:** bei ~70 % des Kontextfensters ältere Tool-Results zu Zusammenfassungen
   verdichten (eigener, günstiger Modell-Call).
5. **Prompt-Caching nutzen:** System-Prompt + Projekt-Instruktionen müssen **präfixstabil** sein
   (nie Datum/Uhrzeit/Dateiliste vorn einbauen!). Das spart bei Anthropic/DeepSeek bis ~90 % der
   Input-Kosten. Gammlige dynamische Inhalte gehören ans **Ende**.
6. **Respektiere `.gitignore`** und ein `.truecodeignore`. Sonst indizierst du `target/` mit
   40k Dateien.
7. **AGENTS.md / TRUECODE.md:** Projekt-Instruktionen automatisch laden (aufsteigend vom CWD).
   Lies zusätzlich `CLAUDE.md`, falls vorhanden — das senkt die Wechselhürde massiv.

### 2.6 Provider-Layer (Model-Neutralität = USP)

```rust
#[async_trait]
pub trait Provider {
    fn id(&self) -> &str;                       // "anthropic/claude-…", "ollama/qwen3"
    fn context_window(&self) -> u32;
    fn price(&self) -> Price { input, output, cached_input };
    async fn stream(&self, req: Request) -> Result<Pin<Box<dyn Stream<Item = Delta>>>>;
    fn supports(&self, f: Feature) -> bool;     // Tools, Vision, Reasoning, Caching, Structured
}
```

* **Erst eigene Typen, dann fremde Crates.** Ein eigener schmaler Trait schützt dich vor
  API-Churn der SDKs. Implementiere ihn anfangs mit `reqwest` + SSE von Hand (~150 Zeilen pro
  Provider) — das klingt nach Mehrarbeit, erspart dir aber den Kampf mit inkompatiblen
  Abstraktionen. Sobald 3+ Provider stehen, evaluierst du `genai` / `rig` / `llmrust`
  ([Vergleich](https://github.com/llmrust/llmrust)) als Austausch der *Implementierung*, nicht der
  Schnittstelle.
* **Zwingend normalisieren:** Streaming-Deltas, Tool-Call-Formate (`tool_use` vs. `function`),
  Usage/Cost, Stop-Reasons, Reasoning-Blöcke. Jede API macht das anders.
* **Router:** Aufgabentyp → Modellklasse.
  * Planen/Architektur → Frontier-Modell
  * Datei-Suche/Triage → schnelles, billiges Modell
  * Auto-Compact → günstigstes Modell
  * Offline/Privacy → lokales Modell (Ollama/llama.cpp)
* **Fallback:** 429/5xx → Retry mit Jitter → nächster Provider gleicher Klasse → Nutzermeldung.
  **Nie stillschweigend das Modell wechseln** — der Nutzer muss es sehen.
* **Auth:** API-Key (Env/Keyring) **und** OAuth-Gerätecode für Abo-Logins. Keys niemals in Config-
  Dateien, sondern OS-Keyring (`keyring`-Crate) + `.env`-Support mit `.gitignore`-Check.

### 2.7 Session, Persistenz, Undo

* **Append-Only Event-Log** pro Session (JSONL in `~/.true-code/sessions/<id>/events.jsonl`).
  Daraus lassen sich Resume, Replay, Kostenbilanz und Debugging ableiten.
* **Undo/Checkpoints:** Vor dem ersten Schreibzugriff `git stash`-artigen Snapshot bzw. — besser —
  Arbeit in einem **Git-Worktree** (`true-code task "…"` → eigener Branch). Dann ist „Rückgängig"
  einfach und du kannst parallel mehrere Agenten auf einem Repo laufen lassen, ohne Chaos.
* **Respektiere fremde Arbeitsstände:** nie in ein dreckiges Repo schreiben ohne Warnung.

### 2.8 TUI (ratatui) — UX-Prinzipien

```
┌─ true-code · CODE mode · claude-sonnet · 12.4k/200k tok · €0.31 ─────────────┐
│ ▸ Plan                                                                        │
│   1. [✓] Repo-Struktur scannen            (12 Dateien)                        │
│   2. [▶] Auth-Modul refaktorieren         tool: patch src/auth.rs             │
│   3. [ ] Tests anpassen + cargo test                                          │
│ ──────────────────────────────────────────────────────────────────────────── │
│ ┌ diff: src/auth.rs ───────────────────────────────────────────────────────┐ │
│ │  42 │ -    let user = db.find(&id).unwrap();                             │ │
│ │  42 │ +    let user = db.find(&id).context("user lookup")?;              │ │
│ └──────────────────────────────────────────────────────────────────────────┘ │
│ [a]nnehmen  [e]dit  [r]evert  [?] erklären  [Esc] abbrechen                  │
│ › ▏                                                            ⏎ senden      │
└──────────────────────────────────────────────────────────────────────────────┘
```

Regeln, die du sonst schmerzhaft lernst:

1. **Der Event-Loop blockiert nie.** LLM-Streaming und Tool-Ausführung laufen in tokio-Tasks und
   schicken Events über einen Kanal (`tokio::sync::mpsc`) an die UI. Die UI rendert maximal
   ~30–60 fps, nicht pro Token (sonst flackert alles und die CPU glüht).
2. **Scrollback ist Pflicht.** Pager-Verhalten (wie `less`): eigener Alternate-Screen, Suche,
   Mausrad. Ratatui selbst hat **keine** Scroll-History — du musst sie bauen.
3. **Bracketed Paste** + Mehrzeilen-Editor nötig, sonst ist Copy/Paste von Code kaputt.
4. **Windows:** `crossterm` nutzen, auf Windows-Terminal/ConPTY testen, UTF-8 + Farben prüfen,
   `enable_raw_mode` und Restore-Panic-Hook (Terminal nicht im Raw-Modus zurücklassen!).
5. **Terminal auf Alt-Screen + Restore-Hook:** Bei Panic/Absturz muss der Bildschirm sauber
   zurückgesetzt werden, sonst ist das Terminal danach unbenutzbar.
6. **Nutzer-Eingaben im Terminal:** Der Agent darf nicht „die Konsole stehlen". Ein interaktiver
   Befehl (`git rebase -i`) muss sauber abgefangen werden (PTY oder mit Warnung verbieten).
7. **Theme/Keymap konfigurierbar** + Accessibility (Farbenblind-Modus, kein Blinken).
8. **Syntax-Highlighting im Diff/Code-Viewer:** tree-sitter; im Editor-Input syntect reicht.

### 2.9 Integrationen

| Protokoll | Wofür | Stand 2026 | Empfehlung |
|---|---|---|---|
| **MCP** | Tools & Daten von außen | De-facto-Standard, 10k+ Server | `rmcp` 2.0 (offizielles Rust-SDK) [2](https://github.com/modelcontextprotocol/rust-sdk/blob/80a74795e9d9d061197efc27d288a1ae4ffa27de/crates/rmcp/CHANGELOG.md) |
| **ACP** | Agent in Editoren (Zed, JetBrains, NeoVim) | Zed-Standard, Apache-2.0, Registry mit JetBrains, 60+ Agents [2](https://rywalker.com/research/zed-agent-client-protocol) | **Ja, ab v0.3.** Einmal implementieren, in allen Editoren laufen |
| **LSP** | Diagnostics nach dem Editieren | reif | Optional, spät — MCP/ACP zuerst |
| **Hooks** | `PreToolUse`, `PostToolUse`, `Stop` | bei beiden etabliert | Früh als JSON-Config, später Shell-Hooks |
| **Skills/Slash-Commands** | Wiederverwendbare Prompts | SKILL.md-Format | Kompatibles Format = Wechselhürde senken |

### 2.10 Telemetrie, Kosten, Evals

* **Kosten-Tracker ab Tag 1**: pro Turn, pro Session, pro Tag. Live in der Statuszeile. Verhindert
  die „€200 über Nacht"-Überraschung und ist gleichzeitig dein Verkaufsargument.
* **Tracing** mit `tracing` + JSON-Export nach `~/.true-code/logs/` (opt-in, keine Code-Inhalte).
* **Evals:** baue dir *vor* dem großen Feature-Bau 20–30 eigene Mini-Tasks
  („Finde und fixe diesen Bug in Repo X", „Refaktoriere Y ohne Tests zu brechen") und messe
  Erfolgsrate + Kosten. **Ohne Evals optimierst du Bauchgefühl.** Das ist der Hebel, der gute von
  mittelmäßigen Agenten trennt.

---

## 3. Sprach-Support (dein Punkt: „ggf. noch .py oder weitere Sprachen")

Nicht hardcoden! **Sprachprofile als TOML** in `crates/tc-tools/languages/`:

```toml
# rust.toml
extensions = ["rs"]
comment    = ["//", "///"]
tree_sitter = "rust"
symbols    = ["function_item", "impl_item", "struct_item", "enum_item"]
test       = "cargo test"
lint       = "cargo clippy -- -D warnings"
fmt        = "cargo fmt"
build      = "cargo build"
entrypoints = ["src/main.rs", "src/lib.rs"]
```

Damit bekommst du fast geschenkt: Auto-Verify (Test/Lint pro Sprache), Symbol-Index,
Test-Datei-Erkennung, „wo fange ich an?" für Anfänger (`entrypoints`), Language-Aware Chunking.

**Reihenfolge:** Rust (deine Hauptsprache) → Python (pytest, ruff/mypy) → TypeScript/JS
→ Go → danach Community-Profile per PR.

---

## 4. Technische Entscheidungen (Empfehlung, Stand Sept. 2026)

| Baustein | Empfehlung | Alternative | Warum |
|---|---|---|---|
| UI-Framework | **ratatui 0.30.x** [2](https://lib.rs/crates/ratatui) | cursive, reratui | De-facto-Standard, aktiv, große Community |
| Terminal-Backend | **crossterm** | termion | Windows-Support (dein Punkt!), Async-Events |
| Async-Runtime | **tokio** | smol | Ökosystem-Default, alle SDKs setzen es voraus |
| CLI-Parsing | **clap** (derive) | – | Subcommands, Shell-Completion gratis |
| Syntax/Analyse | **tree-sitter** (Struktur) **+ syntect** (nur Färben im Input) | nur syntect | tree-sitter = Symbol-/Chunk-Wissen; syntect = Sublime-Grammatiken, schnell, kein WASM-Build-Ärger |
| Code-Diff | **similar** / `diffy` | eigener | Unified Diff + Inline-Highlight |
| Suche | **ripgrep** einbetten (`grep` crate) | eigener | respektiert .gitignore, schnell |
| MCP-Client | **rmcp 2.0** [4](https://github.com/modelcontextprotocol/rust-sdk/blob/main/crates/rmcp/README.md) | rust-mcp-sdk | offiziell, treibt Spec |
| LLM-Provider | eigener Trait + `reqwest` + SSE | `genai`, `rig`, `llmrust` | erst Schnittstelle stabil, dann fremde Impl. prüfen |
| Streaming-Parser | `eventsource-stream` / `futures` | – | SSE robust parsen (Partial Chunks!) |
| Persistenz | **JSONL** (Sessions) + `redb`/SQLite (Index) | reine Dateien | JSONL = debugbar, später Index nötig |
| Fehler | **thiserror** (Libs) + **anyhow/miette** (Binary) | – | miette = hübsche Fehler in der CLI |
| Config | **figment**/TOML + `keyring` | – | Layering: Defaults → Global → Projekt → CLI-Flags |
| Sandbox (Unix) | Landlock/seccomp (`landlock` crate) | Container | leichtgewichtig, kein Docker nötig |
| Sandbox (Win) | Job Objects + Low-Integrity-Token | – | 🔥 Chance: hier ist niemand gut |
| PTY | `portable-pty` (wezterm) | – | interaktive Befehle, Windows-ConPTY |
| Tokenizer | `tiktoken-rs` + provider-eigene Counter | – | Kosten & Kontext-Budget müssen stimmen |
| Logging | `tracing` + `tracing-subscriber` | – | Strukturiert, abschaltbar |
| Installer | `cargo-binstall` + `dist` (cargo-dist) | Handarbeit | Release-Binaries für 4 Plattformen |

**Nicht am Anfang:** Vektordatenbank, Embeddings, LSP-Client, GUI/Web-UI, Plugins in WASM,
Cloud-Backend, Multi-User. Alles scope-Fallen.

---

## 5. Die 20 Fallen (Dinge, die dich sonst Wochen kosten)

**Technisch**
1. Prompt-Caching kaputt durch dynamischen System-Prompt (Datum, Uhrzeit, Dateiliste).
2. Tool-Results unbegrenzt → Kontext voll → Qualität sinkt, Kosten steigen.
3. Kein Abort → Nutzer sitzt 3 Minuten in einer Schleife fest.
4. Terminal nach Panic im Raw-Modus → sieht aus wie „Programm hat mein Terminal zerstört".
5. Streaming ohne Backpressure → UI ruckelt, Tokens kommen als Brei.
6. Windows: Pfade (`\` vs `/`), Zeilenenden (CRLF!), Symlinks, ConPTY, Farben.
7. `.gitignore` ignorieren → Index explodiert, Kosten explodieren.
8. Modellwechsel mitten in der Session → Tool-Call-Format-Passung prüfen (`tool_call_id`!).
9. Rate-Limits/429 ohne Retry → Session stirbt bei großem Task.
10. Kosten unkontrolliert im Autonom-Modus → Obergrenze + Bestätigung.
11. Secrets: API-Keys in `true-code.toml`, im Repo, in Logs, im Event-Log.
12. Prompt-Injection aus Repo/Web → destruktive Befehle nie ohne Mensch.
13. `rm -rf`, `git push --force`, `DROP TABLE` nie auto-approven.
14. Symlink/Path-Traversal: `../../.ssh/id_rsa` muss blockiert werden.
15. Binärdateien/Großdateien einlesen → Binary-Garbage im Kontext.

**Prozess / Produkt**
16. **Scope-Falle:** du willst alles gleichzeitig. MVP heißt: ein Modell, ein Modus, 7 Tools.
17. Ohne Evals kein Fortschritt — nur Meinung.
18. Zu viel Architektur vor dem ersten laufenden Loop („Framework-Bauen" als Prokrastination).
19. Cookie-Cutter-UI: Wenn deine TUI aussieht wie Claude Code, fragt jeder „warum nicht das Original?".
20. **Vertrauen ist das Produkt.** Ein einziger „Agent hat meine Datei gelöscht"-Moment killt die
    Adoption. Undo, Diffs und Sandbox sind keine Nice-to-haves, sie sind der Kern.

---

## 6. Roadmap (realistisch, mit Abnahmekriterien)

| Phase | Ziel | Inhalt | DoD (Definition of Done) |
|---|---|---|---|
| **P0** (1–2 Wo) | **Spike** | Repo, Workspace, ratatui-Hallo-Welt, Provider-Trait, ein Streaming-Call, Chat im Terminal | Ich kann im Terminal mit einem Modell chatten, Streaming sichtbar, `Esc` bricht ab |
| **P1** (3–4 Wo) | **MVP** | Agent-Loop, 7 Tools, Diff-Bestätigung, Permissions (3 Stufen), Kostenanzeige, Session-Log | Ich lasse den Agenten eine echte Datei in meinem Projekt ändern und kann den Diff annehmen/ablehnen |
| **P2** (4 Wo) | **Vertrauen** | Undo/Checkpoints, Git-Worktree-Tasks, Auto-Compact, `.gitignore`-Index, `AGENTS.md`, Sprachprofile (Rust/Python) | 10 eigene Eval-Tasks laufen ohne Datenverlust; Undo funktioniert |
| **P3** (4–6 Wo) | **Erweiterbarkeit** | MCP (rmcp), Slash-Commands/Skills, Hooks, `true-code -p` Headless, CI-Nutzung | Ein fremder MCP-Server (z. B. Filesystem/Postgres) funktioniert |
| **P4** (4 Wo) | **LEARN-Modus** ⭐ | Lernprofil, Erklär-Modus, `/note`, Konzept-Tracker, Übungs-Modus | Ein Anfänger versteht nach 20 Min. ein Konzept, das er vorher nicht konnte (selbst testen!) |
| **P5** (4 Wo) | **Reichweite** | ACP-Support (Zed/JetBrains/Neovim), Sandbox auf Windows+Linux (Kernel), Release-Pipeline, Installer, Doku | `scoop install true-code` / `brew install true-code` / `cargo install true-code` |
| **P6+** | **Differenzierung** | Multi-Agent-Orchestrierung, LSP-Diagnostics, lokale Modelle, Team-Features | – |

**Kopfzahl-Check:** P0–P2 sind ca. 150–250 Stunden *wenn du Rust schon kannst*. Du lernst Rust
parallel → rechne ehrlich das 2–3-fache. Das ist ein **6–12-Monats-Projekt**, kein Wochenende.
Deshalb: Der **LEARN-Modus ist dein kürzester Weg** — du baust ihn *für dich selbst* und dokumentierst
den Weg. Dein Lernfortschritt wird zum Marketing-Asset (Blog/Devlog/YouTube) und zieht genau die
Nutzer an, die Claude Code zu kalt finden.

---

## 7. Repo- & Arbeits-Konventionen (jetzt festlegen, später nicht diskutieren)

* **Commits:** Conventional Commits (`feat:`, `fix:`, `refactor:`), kleine Commits.
* **Rust:** Edition 2024, Rust ≥ 1.85; `cargo fmt --check` + `clippy -D warnings` in CI.
* **Tests:** Unit-Tests in den Crates, Integration-Tests in `tests/`, **Evals in `evals/` separat**.
* **ADRs:** Jede Architekturentscheidung als `docs/adr/NNNN-titel.md` (1 Seite: Kontext,
  Entscheidung, Konsequenzen). Das ist dein Gedächtnis in 6 Monaten.
* **CI (GitHub Actions)** von P0 an: `fmt`, `clippy`, `test` auf **Linux + Windows + macOS**.
  Windows-CI von Anfang an — sonst wird's nie was mit Windows-First.
* **Lizenz:** Apache-2.0 oder MIT (kompatibel mit MCP/ACP-Ökosystem). **Nicht GPL** — das schließt
  dich aus Teilen des Ökosystems aus. Überlege dir *vorher*: Open Source mit kommerziellem Kern
  (z. B. Team-/Cloud-Features) oder komplett offen?
* **Keine Code-Geheimnisse im Event-Log:** Session-Logs können Ausschnitte deines Codes enthalten —
  Opt-in fürs Teilen, Standard: lokal.

---

## 8. Was NICHT ins MVP gehört (harte Kürzungsliste)

❌ Multi-Agent-System · ❌ Vektorsuche/Embeddings · ❌ LSP · ❌ ACP · ❌ Web-UI · ❌ Cloud-Sync
❌ Plugin-System/WASM · ❌ Autocomplete im Editor · ❌ Vision/Bild-Input · ❌ Voice
❌ Support für 10 Provider · ❌ Perfektes Theme-System · ❌ Telemetrie-Backend

Ein Agent, der eine Datei sicher ändert, erklärt was er tat, und das rückgängig machen kann,
ist 100× wertvoller als ein Agent mit 30 halbfertigen Features.

---

## 9. Deine nächsten 7 Tage (konkret)

1. **Tag 1:** Rust-Toolchain installieren (`rustup`), `cargo` verstehen, `ratatui`-Beispiel
   (`cargo run --example demo`) zum Laufen bringen. Ziel: *irgendwas* rendert im Terminal.
2. **Tag 2:** Repo anlegen, Workspace-Struktur aus 2.1 (leere Crates), README mit Vision,
   **ADR-0001: "Warum Rust + ratatui"**.
3. **Tag 3:** `tc-providers`: Trait + eine Implementierung (Anthropic *oder* OpenAI), rohes
   Streaming per `reqwest` + SSE. Test: Chat im Terminal, einseitig.
4. **Tag 4:** `Esc`-Abbruch, Token-Zähler, Kostenanzeige. (Klingt trivial, ist der Unterschied
   zwischen Spielzeug und Werkzeug.)
5. **Tag 5:** Erstes Tool: `read_file` + `list_dir` mit Pfad-Sicherheit (Projektwurzel-Check!).
6. **Tag 6:** Agent-Loop in `tc-agent` (headless, ohne TUI!) — im Terminal per `println!` beobachten.
7. **Tag 7:** **Retro:** Was war schwer? Was hat Spaß gemacht? Dann entscheiden wir gemeinsam
   über P1-Scope und ich baue dir den Scaffold.

---

## 10. Offene Fragen (beantworte ich dir im nächsten Schritt)

1. **USP-Fokus:** LEARN-Modus als Hauptfeature oder als Extra zum Coding-Agent?
2. **Modell-Zugang:** eigene API-Keys / Abo-OAuth / lokale Modelle — was ist dir wichtig?
3. **Zeitbudget:** wie viele Stunden pro Woche realistisch?
4. **Ziel:** reines Lernprojekt → Open-Source-Projekt mit Community → kommerzielles Produkt?
5. **Windows-First** oder Linux/macOS zuerst?

---

*Version 0.1 — wird nach deinen Antworten zu v0.2 konkretisiert (mit Repo-Scaffold, ersten
Code-Skizzen und priorisiertem Backlog).*
