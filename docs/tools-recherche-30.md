# true-code — Tool-Recherche: 30 Tools für P5+ (v0.1)

**Stand:** 14.09.2026 · **Status:** Recherche abgeschlossen · **Zielphase:** P5 → P7
**Frage:** Welche 30 Tools heben das true-code-Erlebnis ab P5 und stellen andere Harnesses in den Schatten?

---

## 0. TL;DR (die 5 Sätze, die du lesen musst)

1. **Dein Maßstab ist ab P5 nicht mehr Claude Code/Codex, sondern Omp (oh-my-pi).** Das
   Open-Source-Fork von Pi hat 32 Built-in-Tools, ~100k Zeilen Rust, hash-anchored Edits,
   LSP (14 Operationen, 53 Language-Server), einen DAP-Debugger, IRC-Agent-Kanäle, einen
   zweiten "Advisor"-Modell und Memory als Built-in [2][5]. Wer "andere Harnesses in den
   Schatten stellen" will, muss mindestens Omp-parity bei den Hot-Path-Tools erreichen.
2. **Drei echte White-Spaces bleiben offen — und alle drei liegen genau in deinen USPs:**
   (a) **LEARN als natives Tool-Surface** — Tutoren gibt es nur als Skill-Bolt-ons auf
   Claude Code, nie nativ in einem Harness [8][10][11]; (b) **Transparenz/Explain-Yourself**
   — kein CLI-Agent hat ein natives "Erkläre, was du getan hast und warum" [6];
   (c) **Kosten als Tool** — alle tracken Tokens, keiner verkauft Kosten als
   First-Class-Fähigkeit mit Routing-Entscheidungen und Audit-Trail.
3. **Zwei ehrliche Korrekturen an deinem Plan v0.1:**
   - **Die Windows-Lücke schließt sich gerade.** Codex läuft seit 03/2026 nativ auf Windows
     (Desktop-App, 05/13/2026 native CLI: PowerShell + Restricted Tokens + ACL-Sandbox,
     kein WSL nötig) [7][8]. Dein Windows-Story wird zur **"Open + Local-First + CLI-First"-Story**,
     nicht zur "Wir sind die Einzigen"-Story.
   - **/rewind + Checkpoints sind seit 25.06.2026 Standard** (Claude Code 2.1.191) [4].
     Time-Travel allein differenziert nicht mehr — **explainable Rewind + Branching** schon.
4. **Baue nicht 30 Tools als 30 sichtbare Tools.** Tool-Bloat = Token-Bloat (30 Schemas ≈
   5–9k Tokens System-Prompt). Lösung: **modus-spezifische Tool-Sets** (CODE/WORK/LEARN) +
   **lazy Loading** (Claude Codes `ToolSearch` lädt Tool-Definitionen auf Abruf) [1].
   Die 30 Tools sind das *Produkt-Surface*, nicht das *stets-sichtbare* Tool-Set.
5. **Edit-Format schlägt Modell.** Omps Benchmarks: dasselbe Modell (Grok Code Fast 1)
   steigt von **6,7 % → 68,3 % Pass-Rate**, nur weil der Edit-Format (hashline statt
   Zeilen-Diff) nicht mehr Retry-Loops erzeugt [5]. Das ist das stärkste Argument,
   `ast_edit`/hash-anchored Edits **früher** zu bauen als in dieser Recherche priorisiert.

---

## 1. Ist-Zustand: Was die Harnesses 2026 an Tools haben

### 1.1 Claude Code (der "Vokabular-Setzer")

~20 Built-in-Tools, Stand 04/2026 [1]:

| Bereich | Tools |
|---|---|
| Lesen | `Read`, `Glob`, `Grep` (ripgrep) |
| Editieren | `Edit`, `Write`, `NotebookEdit` |
| Ausführen | `Bash`, `PowerShell` (Windows!), `Monitor` (Background + Streaming), `BashOutput`, `KillShell` |
| Delegieren | `Agent` (Subagents: Explore/Plan/general-purpose), Agent Teams (experimentell, Messaging) |
| Code-Intelligenz | `LSP` (Type-Check, Goto-Definition, Find-References) — neu! |
| Aufgaben | `TaskCreate`/`TaskUpdate`/`TaskList` (interaktiv) / `TodoWrite` (Headless) |
| Planung | `EnterPlanMode`/`ExitPlanMode` |
| Scheduling | `CronCreate`/`CronDelete`/`CronList` — **recurring Runs in der Session** |
| Web | `WebFetch`, `WebSearch` |
| Meta | `Skill`, **`ToolSearch` (lädt *deferred* Tool-Definitionen — lazy Loading!)**, `AskUserQuestion` |

Dazu seit 06/2026: **`/rewind`** (Checkpoints vor jedem Edit + nach jedem Prompt, rollt Code
*und* Konversation zurück, Esc-Esc-Kürzel) [4], Auto-Checkpoints vor jedem File-Change [3].

### 1.2 Codex CLI (Sandbox- und Windows-Führer)

- **Subagents GA seit 03/14/2026:** Manager dekomponiert, bis zu 8 parallele Subagents in
  isolierten Containern (kein Netz by default), pro-Role-Config in `config.toml`
  (Model, `sandbox_mode`, `developer_instructions`), `spawn_agents_on_csv` für Batch-Workflows [9][10][12].
- **`monitor`-Role** für lange Befehle, `wait`-Tool mit Polling bis 1 h [11].
- **Browser Use als Built-in** (04/2026), Voice/Realtime-Speech, Plugin-Marketplace,
  Encrypted Remote Execution, `--oss` für lokale Modelle [2].
- **Windows nativ** (03/04/2026 App, 05/13/2026 CLI): PowerShell-nativ, **Restricted Tokens +
  Filesystem-ACLs** als Kernel-Sandbox, WSL-Option, Microsoft Store Distribution [7][8].
- Per-Thread-Token-Budgets, "Goals" mit Token-Budget, Cloud-Handoff [2].

### 1.3 Omp / oh-my-pi (DEIN echter Benchmark — Feature-König der Open-Source-Harnesses)

32 Built-in-Tools im flachen Namespace, Stand 09/2026 [5]:

| Tool | Was es kann |
|---|---|
| `read` | **Ein** Tool für: Dateien, Verzeichnisse, Archive, SQLite, PDFs, Notebooks, URLs, `ssh://`-Pfade **und interne Schemes: `pr://`, `issue://`, `agent://`, `skill://`, `vault://`, `mcp://`** |
| `edit` | **"hashline"-Patches:** Content-Hash-Anker statt Zeilennummern, stale-anchor recovery. Gekippte Benchmarks: Grok Code Fast 1 **6,7 % → 68,3 %**, Gemini 3 Flash +5 pp, Grok 4 Fast **−61 % Output-Tokens** [5] |
| `ast_edit` / `ast_grep` | Struktur-Rewrites mit **Preview vor Apply**, 50+ tree-sitter-Grammatiken (ast-grep) |
| `lsp` | **14 Operationen über 53 Language-Server**; Renames laufen über `workspace/willRenameFiles` (Barrel-Files/Re-Exports atomar) |
| `debug` | **DAP-Steuerung:** 28 Operationen, 14 Adapter (lldb-dap, dlv, debugpy, js-debug) — Breakpoints, Stepping, Variablen |
| `security_scan` | Native Security-Reviews planen/ausführen, treibt Codex-Security-Cloud-Scans (setting-gated, default aus) |
| `browser` | Puppeteer-Tabs über headless Chromium, CDP-Apps, **oder dein eigener Chrome via Relay-Extension** |
| `computer` | Persistente JS gegen den Host-Desktop: Fenster, Screenshots, Native-Input, AX-Tree, Clipboard |
| `web_search` | **23 Search-Backends** (keyless Fallbacks: DuckDuckGo, Startpage, Mojeek, Ecosia …) |
| `github` | PRs, Issues, Code-Search, **Actions-Run-Live-Watch** |
| `task` | Fan-out in isolierte Subagent-Worktrees, schema-validierte Typed Results, **IRC-Bus zwischen Siblings** |
| `checkpoint` / `rewind` | Setting-gated Time-Travel |
| `retain`/`recall`/`reflect`/`memory_edit` | **mnemopi**: lokales SQLite + Vektor-Embeddings + Graph-Tools, global oder pro Projekt |
| `generate_image`, `tts` | Medien per Provider-Modellen |
| `github`, `security_scan`, `checkpoint` … | Setting-gated, default aus → **Tool-Set pro Konfiguration, nicht starr** |

Dazu: **Advisor** (zweiter Model überwacht jeden Turn, injiziert *aside/concern/blocker* ohne
Main-Context zu teilen), **TTSR** (Time-Traveling Stream Rules: Regex auf dem Token-Stream
bricht live ab, korrigiert, resumpiert), **`/collab`** (Live-Session per Link, client-side
verschlüsselt), **`omp commit`** (splitet Unverwandtes in dependency-geordnete Commits,
rejectet Zyklen), **`conflict://N`-Dateien** mit `@ours`/`@theirs`/`@base` statt Text-Chirurgie,
**`/review`** (parallele Reviewer, P0–P3-Ranking, Ship-Verdict), Worktree-Isolation via
APFS-Clone/btrfs-Reflink/overlayfs, **`omp-stats`** (lokales Observability-Dashboard),
Prompt-Wörter (`ultrathink`, `orchestrate`, `workflowz`), 67 In-Process-CLI-Utilities [2][5].

> **Merksatz:** Omp beweist, dass ein 1-Personen-Community-Projekt mehr *Feature-Fläche* hat
> als beide Lab-Harnesses. Dein Gegner ist nicht "Claude Code kann es nicht" — es ist
> **"Omp kann es schon, ist aber ein Power-Tool mit kleinem Bus-Faktor"**. true-code gewinnt,
> wenn es Omps Substanz mit deiner Differenzierung (LEARN, Transparenz, Kosten) kombiniert
> und als *vertrauenswürdiges Werkzeug* statt Power-Tool positioniert.

### 1.4 Der Rest (kurz)

- **OpenCode** (182k Stars, 75+ Provider): LSP-Built-in, Undo/Redo, Git-Worktrees, kein
  Checkpoint/Rewind, kein Sandbox [2]. **Copilot CLI**: PR-nativ, per-Repo-Memory,
  `&`-Cloud-Delegation, Review-Agent [2]. **Grok Build**: bis 8 parallele Subagents in
  Worktrees ab Tag 1 [2]. **Junie**: tiefste statische Analyse via JetBrains-Indexer —
  aber geschlossen, IDE-gebunden [2]. **DeepSeek-Reasonix**: Cache-first-Design
  (99,82 % Cache-Hits berichtet) — Kosten-Engineering als Architektur, nicht als Tool [2].
- **MCP-Ökosystem:** ~8.000–12.000 Server (Q2 2026), De-facto-Standard [13]. Hot:
  GitHub, Slack, Postgres, Playwright, Context7 (Versions-doku), Terraform/K8s/Prometheus
  (Platform-Engineers) [13][14][15]. **agent-lsp** (MCP): 66 LSP-Tools, 30 Sprachen,
  Behauptung: grep-basierte Symbol-Suche kostet 5–34× mehr Tokens und macht 92–99 %
  False Positives [16]. **agentmemory** (MCP/Skill): 54 Tools, lokal, Observations →
  Memories → Lessons → Crystals + Graph [17].
- **Memory-Markt:** agentmemory, Cognee (Graph), Mem0, Letta (`/init`, `/doctor`,
  Memory-Subagents), Hindsight (retain/recall/reflect), Zep/Graphiti [18]. Alle extern.
  Nativ im Harness: nur Omp (mnemopi) und Claude (Memory-Files).
- **Checkpoint/Rewind:** Claude Code `/rewind` (25.06.2026) [4], Omp (setting-gated) [5],
  LangGraph Time-Travel (Framework) [19], **AgentRewind** (Forschung: Context- *und*
  Environment-Restore + "Rewind Memory" für den Agenten) [20]. Session-Branching
  (Fork statt Linear-Rollback) taucht in Tools auf, ist aber noch Nische [21].

### 1.5 LEARN-Markt — ehrlich geprüft (dein ⭐-USP)

Es gibt **bereits** Tutoren als Skills/Bolt-ons:

| Projekt | Was es kann | Status |
|---|---|---|
| agent-tutor-skill [22] | FSRS-Spacing, zero-hint quizzes, Konzept-Mastery-Badges, Explain→Example→Check→Evaluate→Practice, Missconception-Tracking | **Skill auf Claude Code**, nicht nativ |
| learn-faster-kit (FASTER) [23] | Syllabi, 4 Lernmodi, /learn /review /progress, Teach-Back | Skill + Wrapper, Claude-optimiert, Codex nur via AGENTS.md |
| AI Coding Tutor [24] | Fibonacci-Spacing-Quizzes, Learner-Profile, Codebase-ankerierte Tutorials | Skill |
| DeepTutor [25] | Multi-Agent-Tutoring, Kurs-Materialien, **konsultiert lokale Coding-Agenten mid-turn** | Getrennte Web-Workspace, kein Harness |
| anki-mcp-server [26] | Anki-Flashcards via MCP (449 Stars) | MCP |

**Fazit:** Das White-Space ist real, aber schmal. *Niemand* hat Learning als **natives
Tool-Surface im Coding-Harness**, *niemand* macht es **model-neutral**, *niemand* verknüpft
Konzept-Tracking mit **realen Coding-Sessions** ("bei `Result<T,E>` vor 2 Wochen in deinem
Projekt"). Die Skills beweisen den Bedarf (437 Repos unter `ai-tutor` auf GitHub) und geben
dir die Pädagogik (FSRS, Teach-Back, zero-hint) quasi als Open-Source-Vorlage.
**Deine Antwort auf den Markt: nicht "besserer Tutor-Skill", sondern "Lernen ist der
dritte native Modus der Engine" — mit dem Event-Log und Lernprofil aus deinem Plan als Fundament.**

---

## 2. Auswahl-Kriterien für die 30 Tools

1. **Hot Path vs. Long Tail:** Wird es in >10 % der Tasks gebraucht? → Built-in.
   Sonst → MCP (deine Regel aus dem Plan bleibt für den Long Tail stehen; Codex/Omp zeigen
   den Präzedenzfall, dass Browser/Search/RichRead auf dem Hot Path sind).
2. **Differenzierungspotenzial:** White-Space (niemand hat es) > Omp-Parity >
   Baseline-Parity (alle haben es).
3. **Riding the Fundament:** Was auf deinem Event-Log/Lernprofil/Sprachprofil aus dem Plan
   quasi gratis liegt, hat das beste Aufwand-Nutzen-Verhältnis (→ F- und G-Gruppe).
4. **Aufwand:** S (< 1 Woche), M (1–3 Wochen), L (> 3 Wochen) — für einen
   Rust-lernt-parallel-Entwickler (dein Plan sagt: ×2–3 einrechnen).

⚪ = echtes White-Space (niemand hat es nativ) · 🔶 = Parity-Pflicht (Baseline 2026) · 🟡 = Omp-Parity

---

## 3. Die 30 Tools

### A. Code-Intelligenz — beyond grep (4 Tools)

| # | Tool | Zweck | Wer hat's | Aufwand | Phase | Wert |
|---|---|---|---|---|---|---|
| 1 | `lsp_diagnostics` | Nach jedem Write: Fehler/Warnungen als strukturiertes Tool-Result → **Error→Fix-Loop ohne kompletten Build** | Omp (14 LSP-Op/53 LS) [5], Claude Code (LSP, teils) [1], OpenCode [2] | M (LSP-Client) + L (Multi-LS-Lebenszyklus) | P5.5 | 🔶 Baseline |
| 2 | `lsp_navigate` | Goto-Definition, References, Hover, Symbols, **Rename**, Code-Actions — in EINEM Tool (Namespace `lsp_*`) | Omp [5], agent-lsp-MCP (66 Tools, "92–99 % False Positives mit grep") [16] | M | P5.5–P6 | 🔶 Baseline |
| 3 | `ast_edit` | Struktursuche/-Rewrites (ast-grep) mit **Preview-then-Accept**; Ersatz für fragile String-Diffs bei Refactors | Omp (50+ Grammatiken) [5] | M (Wrap) + M (Preview-UX) | P6 | 🟡 + Edit-Verlässlichkeit = Vertrauen. **Vorziehen wegen Omp-Benchmarks (6,7 % → 68,3 %)** |
| 4 | `impact_analysis` | Callgraph + Dependency-Abfrage: "Wer ruft `handle_event`? Was bricht, wenn ich X ändere?" (tree-sitter + LSP-Refs) | **⚪ White-Space** (Junie via JetBrains-Indexer, aber closed/IDE [2]) | L | P7 | **USP-High** — und LEARN-Perfekt: Anfänger sehen den "Blast Radius", bevor sie editieren |

### B. Verifikation & Sicherheit (3 Tools)

| # | Tool | Zweck | Wer hat's | Aufwand | Phase | Wert |
|---|---|---|---|---|---|---|
| 5 | `run_tests` | Strukturierter Runner pro Sprachprofil: **per-Test-Result, Coverage, Flaky-Retry, nur geänderte Tests**. Kein rohes `cargo test` in die Konsole | Alle via rohem `shell` (das ist der Standard — Struktur ist der Unterschied) | M | P5.5 | 🔶 + Trust |
| 6 | `check` | Lint + Typecheck + Fmt in einem Call, **bevor** der Diff akzeptiert wird (clippy/ruff/tsc … pro Sprachprofil aus dem Plan) | Claude via Hooks [1], Omp via LSP | S–M | P5.5 | 🔶 Baseline |
| 7 | `security_scan` | semgrep / cargo-audit / govulncheck / trivy als Tool mit strukturierten Findings + **Fix-Loop** + (LEARN) "warum ist das ein Bug?" | Omp (setting-gated, treibt Codex-Security-Cloud) [5] — sonst nur roher Shell | S–M (Wrapper) + M (Fix-Loop) | P6 | 🟡 **Offline/Lokal-Scans + Erklär-Loop = deine Antwort** auf Omps Cloud-Variante |

### C. Delivery — Git bis Release (4 Tools)

| # | Tool | Zweck | Wer hat's | Aufwand | Phase | Wert |
|---|---|---|---|---|---|---|
| 8 | `commit_smart` | Splitet verwandte Änderungen in **dependency-geordnete Commits**, rejectet Zyklen, Conventional-Commits-Format (dein Plan §7) | Omp (`omp commit`) [5]; Aider (atomare Commits, aber pair-programming-Ära) [2] | M | P6 | 🟡 |
| 9 | `conflict_resolve` | Merge-Konflikte als **Datei**: `conflict://N` öffnen, `@ours`/`@theirs`/`@base` schreiben statt Text-Chirurgie | **⚪ Omp-only** [5] | M | P6 | **USP-Medium-High** — macht Refactor/Merge-Tasks von Agenten robust; niemand außer Omp hat es |
| 10 | `review` | Parallele Reviewer (Korrektheit/Security/Tests/Doku), **P0–P3-Ranking + Ship-Verdict** | Claude `/code-review` [2], Codex `/review` [2], Omp (parallel, P0–P3) [5] | M | P5.5 | 🔶 **Baseline 2026** (4 von 5 Vergleichs-Harnesses) — fehlt's, wirks man |
| 11 | `ci_watch` | Actions-Runs **live** watchen, Fehler auto-triagieren, "CI rot → Fix-Loop" | Omp (`github`-Tool) [5], Copilot (plattform-nativ) [2] | M | P6–P7 | 🟡 |

### D. Außenwelt (3 Tools)

| # | Tool | Zweck | Wer hat's | Aufwand | Phase | Wert |
|---|---|---|---|---|---|---|
| 12 | `browser` | Headless Chromium: navigate/click/fill/wait/screenshot; E2E-Verifikation nach UI-Änderungen; optional CDP zu bestehendem App | Codex Built-in (04/2026) [2], Omp (Puppeteer/CDP/Chrome-Relay) [5], sonst MCP (Playwright) | L (Embedding) oder M (Playwright-Wrapper) | P6 | 🔶 Parity-Pflicht für UI-Work; **Playwright-MCP als Fallback-Option halten** |
| 13 | `web_search` | Provider-Chain mit Keyless-Fallbacks, site-awaree Extraktion, Zitate im Result | Omp (23 Backends) [5], Claude Built-in [1], Codex (Server-approved mode) [2] | M | P5.5 | 🔶 Baseline (dein Plan sagte "Web via MCP" — korrigieren: Hot Path) |
| 14 | `rich_read` | Ein Tool für: PDF, Jupyter-Notebooks, Archive, SQLite, URLs → Markdown | Omp (`read`) [5] | M | P6 | 🟡 — beendet das Spec-Seiten-Ins-Prompt-Kopieren; ideal für RESEARCH-Tasks (Modus B) |

### E. Orchestrierung & Session (5 Tools)

| # | Tool | Zweck | Wer hat's | Aufwand | Phase | Wert |
|---|---|---|---|---|---|---|
| 15 | `spawn_agent` | Subagent mit Role + Scope + **Model-Pin + eigenes Budget** + optional Worktree | Alle (Codex GA 03/2026, bis 8 parallel, Pro-Role-Config [9][10][12]) | M–L | P6 | 🔶 Baseline |
| 16 | `agent_message` | Kanal zwischen live Agenten (IRC-Stil), Task-Koordination | Omp [2][5], Claude Teams (experimentell) [2] | M | P7 | 🟡 |
| 17 | `advisor` | Zweiter Model prüft **jeden Turn** (aside/concern/blocker), ohne Main-Context zu teilen | Omp [5], Amp "Oracle" [2] | S–M | P7 | 🟡 — **Dein Model-Neutralitäts-Play:** Claude schreibt, Gemini/Kimi prüft. Das kann kein Lab-Harness (Lock-in!) |
| 18 | `rewind` | Checkpoints + **Branching** (Fork statt nur Rollback) + **explainable Rewind** ("warum hier zurück?") | Claude `/rewind` (06/2026) [4], Omp [5], AgentRewind (Forschung [20]), Branching (Nische [21]) | M (Event-Log aus deinem Plan ist die Grundlage) | P5.5 | 🔶 Rewind = Baseline; **Branch + Erklärung = Differenzierung** |
| 19 | `worktree_task` | Parallele Tasks, jede mit eigenem Git-Worktree/Branch (dein Plan §2.7) | Grok Build (8 Worktrees) [2], Codex, Claude Teams [2] | M | P6 | 🔶 |

### F. Transparenz & Kosten — dein USP #1 und #4 (4 Tools) ⚪ WHITE-SPACE

> Recherche-Ergebnis: **Kein** CLI-Agent hat "Erkläre, was du getan hast / warum / was es
> gekostet hat" als natives Tool. CodeRabbit macht Explainability für *Reviews* [27],
> OpenAI/Anthropic betreiben Sandbox-Transparenz für *Sicherheit* — aber niemand zeigt dem
> Endnutzer **seiner eigenen Session** verständlich auf, was der Agent gemacht hat.
> Das ist dein größtes, billigstes White-Space, weil das Event-Log (Plan §2.7) schon da ist.

| # | Tool | Zweck | Wer hat's | Aufwand | Phase | Wert |
|---|---|---|---|---|---|---|
| 20 | `explain_turn` | **Explain-Yourself-Report** nach jedem/je beliebigem Turn: Was wurde gelesen/geschrieben/ausgeführt, **warum** (Plan-Schritt-Verweis), Kosten, Was kommt als Nächstes. Günstiger Modell-Call auf dem Event-Log. | **⚪ White-Space** | M | **P5.5** | **USP-HIGH** — direkt = USP #4 aus deinem Plan; ist auch das "Erkläre"-Feature (`[?]` im Diff-UI) mit Engine-Begründung |
| 21 | `audit_log` | Mensch-lesbarer Audit-Trail der ganzen Session: jede Lese-/Schreib-/Netzwerk-Aktion mit **Begründung**, exportierbar (Markdown). Für Nutzer, nicht nur `tracing`-Logs. | **⚪ White-Space** (Codex-Sandbox ist *auditierbar* [8], aber kein Nutzer-Audit-Tool) | S–M | **P5.5** | **USP-HIGH** — "Vertrauen ist das Produkt" (Fall #20 deines Plans) als Feature; EU-AI-Act-Narrativ gratis mitgeliefert [28] |
| 22 | `cost_report` | Kosten **pro Turn / pro Datei / pro Tag**, Cache-Hit-Rate, "teuerste Entscheidungen". Nicht nur Statuszeile (die hast du), sondern **analysierbares Artefakt**. | ⚪ Als Tool: niemand (Reasonix: Cache-first als *Design* [2]; Omp: Live-Counting [2]) | S (Tracker aus Plan) | **P5.5** | **USP-HIGH** — "…und dir sagt, was es kostet" aus deinem Merksatz, endlich messbar |
| 23 | `budget_guard` | Soft/Hard-Limits **plus sichtbare Routing-Entscheidungen:** "Weil Budget 70 %: Triage-Subtask geht jetzt auf GPT-4.1-mini statt Sonnet — [Details]". Auto-Pause mit Report. | Codex: Per-Thread-Budgets [2]; Omp: Role-Routing [2] — **aber keiner macht die Entscheidung als Tool-Result für den Nutzer sichtbar** | M | **P5.5** | **USP-HIGH** — "Nie stillschweigend das Modell wechseln" (dein Plan §2.6) wird zu Feature |

### G. LEARN — dein USP ⭐ (7 Tools) ⚪ WHITE-SPACE

> Pädagogik-Vorlagen aus dem Open-Source-Markt [22][23][25]: **FSRS** (adaptive
> Wiederholung, open-spaced-repetition), **Teach-Back** (Erkläre-mir's-umgekehrt),
> **zero-hint quizzes**, **Missconception-Tracking**, Explain→Example→Check→Evaluate→Practice.
> Nimm die Algorithmen, baue die Tools in die Engine.

| # | Tool | Zweck | Wer hat's | Aufwand | Phase | Wert |
|---|---|---|---|---|---|---|
| 24 | `learn_profile` | Lernprofil lesen/aktualisieren (`~/.true-code/profile.toml` aus deinem Plan): Level pro Sprache, Stil (Analogien/abstrakt), Ziele, bekannte Missconceptions | Nur Skills [22][23] | S | **P6** | **USP-HIGH** — Fundament von allem LEARN |
| 25 | `explain` | Erklärung auf Level 1–5 (Stufe aus `learn_profile`), **ankert in echtem Repo-Code** statt abstrakten Snippets, verknüpft mit `concept` | Nur Skills (Codebase-Ankerung: AI-Coding-Tutor [24]) | S–M | **P6** | **USP-HIGH** — "Kein 'wie du weißt…', aber auch kein 'Compiler = Übersetzer'" (dein Plan §1C) |
| 26 | `concept` | **FSRS-Mastery-Tracker** pro Konzept: gesehen/begreifen/verinnerlicht, Missconceptions, Review-fällig-Daten. Sätze wie "Du hattest bei `Result<T,E>` vor 2 Wochen gefragt — hier der Transfer." | Nur Skills (FSRS in agent-tutor-skill [22]) | M (FSRS-Rust-Port) | **P6** | **USP-HIGH** — der Mechanismus hinter deinem Konzept-Tracker-Versprechen |
| 27 | `quiz` | Sokratische Fragen, **zero-hint**, **Explain-Back** (Nutzer erklärt umgekehrt, Agent findet Lücken), Missconception landet in `learn_profile` | Nur Skills [22][23] | S–M | **P6** | **USP-HIGH** — "Kein 'Ich hab's verstanden' ohne Beweis" [22] |
| 28 | `practice` | **Sandbox-Übung:** Agent stellt Aufgabe, wartet (PTY/Sandbox), **reviewt deinen Code, gibt Hinweise statt Lösungen**. "Versuch es selbst, ich schaue zu." (dein Plan §1C) | algo-sensei (LeetCode-only, Skill) [29] | M (Sandbox + Review-Prompt-Engine) | **P6** | **USP-HIGH** — in *keinem* Coding-Agenten vorhanden; das ist der emotional stärkste LEARN-Moment |
| 29 | `notes` | `/note`: wachsende lokale Wiki (Markdown), auto-verlinkt mit `concept`-Einträgen, später durchsuchbar | ⚪ (agentmemory & Co. sind *Agenten*-Memory, nicht *Lerner*-Notizen [17][18]) | S | **P6** | USP-Medium-High |
| 30 | `learning_report` | **Wochen-Report:** Konzepte gemeistert, Reviews fällig, Wissensgraph (Mermaid), "Was du *beiläufig beim Coden* gelernt hast". Teilbar (Markdown) → Retention + Marketing (dein Plan §6: "Lernfortschritt wird zum Marketing-Asset") | ⚪ (DeepTutor hat Progress in eigener Workspace [25]) | M | **P6** | **USP-HIGH** — macht den Lernwert *sichtbar*; Share-Artefakt = Viralität |

### Auf der Bank (6 Kandidaten — via MCP oder später, NICHT Built-in)

| Tool | Warum Bank |
|---|---|
| `ui_diff` (Screenshot + Pixel-Diff) | Playwright-MCP deckt's ab; erst wenn `browser` (12) läuft, neu bewerten |
| `db_query` (Read-only SQL) | Postgres-MCP ist reif [13]; Long Tail |
| `bench` (Benchmark + Regression-Gate) | Selten im MVP-Pfad; `run_tests` reicht bis P7 |
| `flashcards` (Anki-Import/-Export) | anki-mcp-server [26] + CSV-Export aus `notes` reicht für P6 |
| `schedule` (Cron/Recurring-Runs) | Nice-to-have; Claude hat CronCreate [1], Codex Reminders [2] — Parity für P7 |
| `dependency_upgrade` (getestete Updates) | Interessant, aber Risiko-lastig; erst nach `security_scan` + `run_tests` |

**Anti-Liste (bleiben MCP, werden nie Tools):** Jira, Slack, Figma, Docker, Kubernetes,
Terraform, Sentry, Linear — 8.000–12.000 Server existieren bereits [13], dein `rmcp`-Client
aus P3 ist der Zugang.

---

## 4. Architektur-Folgen (das, was die Recherche am meisten kostet, wenn du's ignorierst)

### 4.1 Tool-Bloat = Token-Bloat (Pflicht-Architektur ab P5.5)

- 30 Tools × 150–300 Tokens Schema ≈ **5–9k Tokens** im stets-sichtbaren Prompt. Das
  frisst Budget und senkt Tool-Auswahl-Genauigkeit.
- **Lösung 1 — Modus-spezifische Tool-Sets:**
  - CODE: Core-7 + `lsp_*` + `run_tests` + `check` + `review` + `rewind` + `commit_smart` ≈ 14
  - WORK: Core-7 + `web_search` + `rich_read` + `browser` + `spawn_agent` + `ci_watch` ≈ 13
  - LEARN: Core-7 (read-only-bias) + `explain` + `concept` + `quiz` + `practice` + `notes` + `learn_profile` ≈ 13
  - Meta-Tools (`budget_guard`, `cost_report`, `audit_log`, `explain_turn`) in allen Modi —
    sie sind der Kern deiner Differenzierung.
- **Lösung 2 — Lazy Loading:** Claude Codes `ToolSearch` zeigt den Weg [1]: Tool-*Definitionen*
  werden erst geladen, wenn der Modus/die Task es braucht. Bei dir: `tc-tools` registriert
  alle Tools, der Provider-Layer (Plan §2.6) filtert pro Request. **ADR schreiben: "Tool-Registrierung
  ist modus- und budget-abhängig."**
- **Lösung 3 — Setting-gated Tools** (Omp-Vorbild [5]): `browser`, `security_scan`,
  `advisor` default aus → weniger Angriffsfläche, weniger Prompt-Fläche, User-Vertrauen.

### 4.2 Hot Path wird Built-in — die MCP-Regel aus deinem Plan braucht eine Präzisierung

Dein Plan §2.3 sagt "Alles Weitere ausschließlich über MCP". Die 2026-Realität: Codex hat
Browser gebuilt, Omp hat 32 Tools gebuilt. **Korrektur-Regel:** Was in >10 % der Tasks
vorkommt (`web_search`, `lsp_diagnostics`, `run_tests`, `review`, `rewind`) wird Built-in —
dort ist der Integrationswert (strukturierte Results, Permissions, Kosten-Tracking) höher als
der Wartungsaufwand. Long Tail (DB, Jira, Figma …) bleibt MCP.

### 4.3 Edit-Format ist ein Performance-Feature, kein Detail

Omps Benchmarks [5] (dasselbe Modell, nur anderes Edit-Format):

| Modell | Vorher | Nachher (hashline) |
|---|---|---|
| Grok Code Fast 1 | 6,7 % | **68,3 %** |
| Grok 4 Fast | — | **−61 % Output-Tokens** |
| Gemini 3 Flash | — | +5 pp vs. str_replace |

→ `ast_edit` (Tool 3) und hash-anchored Edits in `patch` sind **Qualitäts- und
Kosten-Multiplikatoren**, keine nice-to-have. Wenn du nur EIN Tool aus Gruppe A vorziehst:
dieses. (Alternative: Omp ist MIT-lizenziert — `pi-edit`/`pi-ast` sind Code-Studium wert,
nicht Blind-Kopie.)

### 4.4 Windows-Korrektur für den Plan (USP #3 wird enger)

- Codex: nativ Windows seit 03/2026 (App) + 05/13/2026 (CLI: PowerShell, **Restricted Tokens
  + ACLs**, kein WSL) [7][8]. Claude Code: `PowerShell`-Tool [1]. Microsoft: Build 2026
  positioniert Windows explizit als Agent-Plattform (MXC-Policy-Layer, noch "kein
  Security-Boundary" [30]).
- **Neue Windows-Positionierung für true-code:** (1) **open & audierbar** (Codex-Sandbox ist
  offen, aber das gesamte true-code ist es — inkl. Event-Log/Audit-Trail), (2) **local-first**
  (Ollama/llama.cpp nativ auf Windows — Codex-Local ist `--oss`-Nachrüst), (3) **CLI-first ohne
  Desktop-App** (dein Plan ist CLI; Codex geht den App-Weg), (4) Job-Objects + ACLs als
  Sandbox mit **WSL-Hybrid** (Linux-Toolchains via WSL2, Windows-Toolchains via PowerShell).
  → In ADR-0002 festhalten, dass Windows-First = *open + local + CLI*, nicht *the only one*.

---

## 5. Priorisierung: 4 Wellen statt 30 gleichzeitig

> Kopfzahl-Ehrlichkeit (dein Plan §6): Das sind ~12–18 weitere Entwickler-Monate obendrauf.
> Die Wellen sind so geschnitten, dass **jede Welle für sich ein marktfähiges Upgrade** ist.

### Welle 0 — "Nicht schlechter aussehen" (P5.5, ~3 Wochen)
`review` (10) · `run_tests` (5) · `check` (6) · `lsp_diagnostics` (1) · `rewind` (18) ·
`web_search` (13) · `cost_report` (22) · `budget_guard` (23)
→ Danach: Baseline-Parity bei den Dingen, die 4 von 5 Vergleichs-Harnesses haben [2],
**plus** deine zwei einzigen Kosten-Tools am Markt.

### Welle 1 — "Vertrauen & Zuverlässigkeit" (P6, ~4 Wochen)
`audit_log` (21) · `explain_turn` (20) · `security_scan` (7) · `ast_edit` (3) ·
`lsp_navigate` (2) · `conflict_resolve` (9)
→ Danach: Omp-Parity bei Edit-Verlässlichkeit + ein Trust-Narrativ, das kein Lab-Tool
erzählen kann (Audit-Trail + Explain-Yourself).

### Welle 2 — "LEARN" (P6.5, ~4 Wochen) ⭐
`learn_profile` (24) · `explain` (25) · `concept` (26) · `quiz` (27) · `notes` (29) ·
`practice` (28) · `learning_report` (30)
→ Danach: **Der Markt, den es sonst nicht gibt.** Dein Plan-P4-DoD ("Ein Anfänger versteht
nach 20 Min. ein Konzept") wird testbar über `quiz` + `learning_report`.

### Welle 3 — "Skalierung" (P7, flexibel)
`spawn_agent` (15) · `worktree_task` (19) · `advisor` (17) · `agent_message` (16) ·
`ci_watch` (11) · `commit_smart` (8) · `browser` (12) · `rich_read` (14) · `impact_analysis` (4)

**Top 5 "In-den-Schatten"-Tools** (White-Space × eigene USPs × billig auf dem Event-Log):
**20 `explain_turn` · 21 `audit_log` · 22/23 `cost_report`+`budget_guard` · 26 `concept` · 28 `practice`**

---

## 6. Offene Fragen für v0.2

1. **`ast_edit`-Strategie:** Eigene Implementierung oder MIT-lizenzierte `pi-edit`/`pi-ast`
   (Omp) als Referenz implementieren? (Lizenz passt: MIT/Apache-kompatibel, dein Plan §7.)
2. **FSRS:** Rust-Port des offenen FSRS-Algorithmus [22] oder `fsrs-rs`-Crate?
3. **Browser:** Eigenes headless-Chromium-Embedding (L-Aufwand, aber Sandbox-Integration)
   oder Playwright-Wrapper (M-Aufwand, Playwright-MCP als Upgrade-Pfad)?
4. **LSP:** Alle 53 Server (Omp-Breite) oder erst 4 (rust-analyzer, gopls, pyright, tsserver)?
   Empfehlung: 4 + Sprachprofile-TOML-Erweiterung (`lsp = "rust-analyzer"`).
5. **LEARN-Tiefe P6:** `practice` mit echter PTY-Sandbox (M) oder erst "Agent wartet und
   reviewt via `read_file`" (S)? Empfehlung: S-Version in Welle 2, PTY-Version in Welle 3.
6. **Achtung Scope:** 30 Tools sind das *Produkt-Surface bis P7* — nicht die MVP-Erweiterung.
   Dein Plan §8 ("harte Kürzungsliste") gilt weiter: Welle 0 erst, wenn P1–P5 stabil laufen.

---

## 7. Quellen

| # | Quelle | Was geliefert |
|---|---|---|
| [1] | [israynotarray.com — Claude Code Built-in Tools Explained](https://israynotarray.com/en/ai/2026/04/29/claude-code-built-in-tools-explained/) (29.04.2026) | Claude-Code-Tool-Liste 2026, `ToolSearch` (lazy Loading), Subagents, PowerShell, Cron |
| [2] | [arcbjorn.com — State of CLI Coding Agents, Mid-2026](https://blog.arcbjorn.com/state-of-cli-coding-agents-2026) (04.07.2026) | Feature-Matrizen (5 Harnesses), 35 Agenten, Omp-Feature-Surface, Open-Weights-Entwicklung |
| [3] | [jsmanifest.com — Claude Code Checkpoints](https://jsmanifest.com/claude-code-checkpoints-restore-agent-state) (22.08.2026) | Auto-Checkpoints, /rewind-Workflows |
| [4] | [aiforanything.io — Claude Code /rewind Guide](https://www.aiforanything.io/blog/claude-code-rewind-checkpoint-rollback-guide-2026) (15.07.2026) | /rewind Release 25.06.2026 (v2.1.191), Semantik |
| [5] | [github.com/can1357/oh-my-pi](https://github.com/can1357/oh-my-pi) + [everydev.ai/tools/omp-oh-my-pi](https://www.everydev.ai/tools/omp-oh-my-pi) (07–09/2026) | Omp: 32 Built-in-Tools, hashline-Benchmarks, LSP/DAP-Zahlen, Schemes, 23 Search-Provider, Crates |
| [6] | [coderabbit.ai — AI Agent Explainability (2026 SDLC Primer)](https://www.coderabbit.ai/guides/ai-agent-explainability) (04.06.2026) | Explainability-Standard (Verification/Debugging/Auditability) — belegt White-Space für native Session-Transparenz |
| [7] | [digitalapplied.com — Codex Windows Native Agent Sandbox Guide](https://www.digitalapplied.com/blog/codex-windows-native-desktop-agent-sandbox-app-guide) (08.03.2026) | Codex Windows-App 04.03.2026, PowerShell, offene Sandbox |
| [8] | [totalum.app — Codex on Windows: Honest Setup Guide](https://www.totalum.app/blog/codex-on-windows-totalum) (04.08.2026) | Native CLI 13.05.2026, Restricted Tokens + ACLs, Parity-Table |
| [9] | [aimagicx.com — OpenAI Codex Subagents GA](https://www.aimagicx.com/blog/openai-codex-subagents-autonomous-coding-team-2026) (23.03.2026) | Subagent-Architektur, Pro-Role-Config, Container-Isolation |
| [10] | [learn.chatgpt.com — Subagents (Docs)](https://learn.chatgpt.com/docs/agent-configuration/subagents) (03.08.2026) | Offizielle Codex-Subagent-Doku, Sandbox-Erbschaft |
| [11] | [morphllm.com — Codex Multi-Agent Guide](https://www.morphllm.com/codex-multi-agent) (05.03.2026) | `monitor`-Role, `wait`-Tool, CSV-Batch |
| [12] | [explainx.ai — OpenAI Agents API Public Beta](https://www.explainx.ai/blog/openai-agents-api-public-beta-sandbox-2026) (12.09.2026) | Codex-Harness als API, 9 Sandbox-Partner, Multi-Agent |
| [13] | [presenc.ai — MCP Server Ecosystem Statistics 2026](https://presenc.ai/research/mcp-server-ecosystem-statistics-2026) (08.05.2026) | 8–12k Server (Q2 2026), meist-installierte Server |
| [14] | [stackgen.com — 10 Best MCP Servers for Platform Engineers](https://stackgen.com/blog/the-10-best-mcp-servers-for-platform-engineers-in-2026) (01.09.2026) | Terraform/K8s/Prometheus/Datadog-MCP-Landschaft |
| [15] | [workos.com — Everything your team needs to know about MCP in 2026](https://workos.com/blog/everything-your-team-needs-to-know-about-mcp-in-2026) (26.03.2026) | Client-Adoption, Context7 |
| [16] | [pypi.org/project/agent-lsp](https://pypi.org/project/agent-lsp/) (18.05.2026) | 66 LSP-Tools, 30 Sprachen, 5–34× Token-Claim, 92–99 % False Positives |
| [17] | [agent-memory.dev](https://www.agent-memory.dev/docs) (15.08.2026) | agentmemory: 54 Tools, Memory-Taxonomie, lokal |
| [18] | [omidsaffari.com — Best Persistent Memory Systems 2026](https://omidsaffari.com/blog/best-persistent-memory-systems-for-ai-agents-2026) (30.08.2026) | Mem0/Letta/Hindsight/Zep/Cognee-Vergleich |
| [19] | [programmingcentral.hashnode.dev — Time Travel Debugging in LangGraph](https://programmingcentral.hashnode.dev/master-time-travel-debugging-in-langgraphjs-rewind-edit-and-replay-agent-states) (19.03.2026) | Time-Travel-Muster (Rewind/Edit/Replay) |
| [20] | [arxiv.org — AgentRewind](https://arxiv.org/html/2608.14380v1) (14.08.2026) | Recoverable Execution: Context+Environment-Checkpoints, "Rewind Memory" |
| [21] | [openclawai.io — AI Agent Session Branching](https://openclawai.io/blog/ai-agent-session-branching-rewind) (29.07.2026) | Branching vs. Linear-Rollback |
| [22] | [github.com/Bhala-Srinivash/agent-tutor-skill](https://github.com/Bhala-Srinivash/agent-tutor-skill) | FSRS-Spacing, zero-hint quizzes, Missconception-Tracking, Teaching Loop |
| [23] | [github.com/hluaguo/learn-faster-kit](https://github.com/hluaguo/learn-faster-kit) | FASTER-Framework, Syllabi, Teach-Back, /learn /review /progress |
| [24] | [mcpmarket.com — AI Coding Tutor Skill](https://mcpmarket.com/tools/skills/ai-coding-tutor) | Fibonacci-Spacing, Codebase-Ankerung, Learner-Profile |
| [25] | [scriptbyai.com — DeepTutor](https://www.scriptbyai.com/deeptutor-ai-learning-assistant/) (31.08.2026) | Multi-Agent-Tutoring, konsultiert lokale Coding-Agenten |
| [26] | [github.com/ankimcp/anki-mcp-server](https://github.com/ankimcp/anki-mcp-server) (449 Stars) | Anki via MCP |
| [27] | vgl. [6] | Explainability-Standard für Agent-Reviews |
| [28] | [agility-at-scale.com — AI Transparency & Explainability](https://agility-at-scale.com/ai/governance/transparency-and-explainability/) (13.07.2026) | EU-AI-Act Art. 13, Audit-Trail als Governance-Pflicht |
| [29] | [github.com/karanb192/algo-sensei](https://github.com/karanb192/algo-sensei) (252 Stars) | LeetCode-Mentor mit progressiven Hinweisen (Practice-Vorbild) |
| [30] | [windowsforum.com — Build 2026: Windows als Agent-Plattform](https://windowsforum.com/threads/build-2026-microsoft-makes-windows-an-agent-platform-for-ai-developers.420496/) (28.05.2026) | MXC-Policy-Layer (Early Preview, "noch kein Security-Boundary") |

---

*Version 0.1 — Recherche-Fundament für Plan v0.2. Nächster Schritt: ADRs zu 4.1 (modus-spezifische
Tool-Registrierung) und 4.4 (Windows-Positionierung) + Welle-0-Backlog mit Story-Points.*
