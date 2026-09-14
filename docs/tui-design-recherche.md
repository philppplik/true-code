# true-code — TUI-Design-Recherche: Intuitiv & gut designt (v0.1)

**Stand:** 14.09.2026 · **Status:** Recherche abgeschlossen
**Frage:** Wie macht man die TUI intuitiv/gut designt (Model-Selector, Mode- &
Permission-Selector)? Was nutzen andere Harnesses? Brauchen wir eine andere UI?

---

## 0. TL;DR (die 5 Sätze)

1. **"Brauchen wir eine andere UI?" → Neue Paradigmen: nein. Neue Flächen: ja.** Die
   Kategorie hat sich 2026 auf ein Muster konvergiert: **Transcript + Statusbar + kontext-
   sensitive Footer-Hints + Diff-Pane + Overlay-Dialoge** (Model, Modes, Help, Rewind).
   Nutzer bringen Muskelgedächtnis mit (Shift+Tab, Esc, Tab, `/`, `?`). Das Brechen dieser
   Konventionen wäre UX-Kosten, nicht -gewinn. Deine Differenzierung liegt in **drei
   Panels, die niemand hat**: Kosten-Ticker, "Warum?"-Erklärung, Lern-Status (Konzept-Badges)
   — dazu das **2-Achsen-Modell** aus *Task-Modus* (CODE/WORK/LEARN) × *Autonomie*
   (Read-only/Workspace/Full).
2. **Der Model-Selector ist bei allen ein UX-Schmerzpunkt — deine Chance.** Gemini CLI hat
   bis 04/2026 nicht mal ein Hotkey dafür (Issue #25141: "multistep procedure") [G4].
   Omp hat Ctrl+L (Selector) + Ctrl+P (Role-Cycling) + Alt+P (temporär) [O1]. OpenCode:
   `ctrl+x m` [OC1]. Claude: Alt+P [CC2]. **Dein Spiel: 1-Tasten-Selector mit
   Preis/Kontext/Fit-für-diese-Task — und "Auto (Task-Router)" als Default.** Das ist
   Model-Neutralität (dein USP) *als UI*.
3. **Mode- & Permission-Selector: konvergiert auf 3–5 Stufen + Shift+Tab-Cycling.**
   Claude: 5 Modi (Ask/AcceptEdits/Plan/Auto/Bypass, Shift+Tab durch die 3 Kern) [CC1][CC3].
   Codex: zwei *orthogonale* Drehregler (sandbox_mode × approval_policy) → 3 TUI-Presets
   (Read Only / Auto / Full Access "extremely risky") + `/approvals`-Menü mit
   Beschreibungen [CX1][CX2]. Gemini: 4 Modi, Ctrl+Y + Shift+Tab [G3]. **Keiner trennt
   sauber "WAS machst du" von "WIE VIEL darf der Agent"** — genau da setzt deine
   2-Achsen-Idee an (s. §3.3).
4. **Die 10 Design-Regeln mit dem größten Hebel** (aus [T1][T2][T3][O1]): Progressive
   Disclosure (Footer 3–5 Kontext-Tasten → `?`-Overlay → Doku) · Kontextuelle Intelligenz
   (Keymap/Footer/Help ändern sich mit Modus+Fokus) · Semantische Farben (mit 16 Farben
   *nutzbar*, `NO_COLOR` respektieren, Farbblindmodus — Omp hat ihn als Setting!) ·
   Diff-First mit **Word-Level-Highlighting** · Nie blockieren (Feedback <100 ms, Spinner
   erst nach 200 ms, 15–30 FPS differential Rendering) · **Nie** Ctrl+C/Z/\ binden ·
   80×24-Minimum mit "Priority Collapse" · Cursor-Form pro Modus (DECSCA, Omp-Standard) ·
   Box-Drawing + Block-Elemente nur (Windows/Tmux-sicher) · "Content IS the interface"
   (kein überdesigntes Chrome).
5. **Architektur-Gesetz für die TUI:** Einseitiger Eventfluss
   `AgentEvents → purer Reducer → State → Render`. "Pure reducers make AI UIs predictable"
   [T3] — Agent-Antworten laufen durch dasselbe Event-System wie Tastendrücke. Damit wird
   die UI testbar, debugbar, **auditierbar** — und es verknüpft sich direkt mit deinem
   Event-Log aus dem Plan (§2.7): *dasselbe* Event-Log speist Session-Persistenz *und* UI.
   (Dein Plan §2.8/Regel 1 hat das mpsc-Kanal-Prinzip schon — die Recherche macht es zur
   Pflicht: Reducer ohne I/O.)

---

## 1. Was die anderen nutzen (Inventory)

### 1.1 Layout & Interaktions-Muster je Harness

| Harness | Layout | Model-Wechsel | Mode/Permission | Sonstiges Auffälliges |
|---|---|---|---|---|
| **Claude Code** [CC1–CC3] | Transcript + Inline-Diffs + Statusbar; Desktop-App zusätzlich | Alt+P | **Shift+Tab** zykliert Normal → Auto-Accept → Plan; 5 Modi insgesamt (Ask, AcceptEdits, Plan, Auto, Bypass); Desktop: `1`–`5` oder Ctrl+Shift+M; Mode-Indikator in UI ("⏸ plan mode on", "⏵⏵ accept edits on") | Ctrl+B Task→Background, Ctrl+T Task-List, Ctrl+O verbose toggle, Esc+Esc Rewind, `/permissions`, `/plan`, defaultMode in settings |
| **Codex CLI** [CX1–CX3] | Full-Screen; `/diff` als eigenener Review-Punkt | `-m` Flag, Config-**Profiles** (`fast`/`deep`/`zdr` = Model+Reasoning+Approval+Sandbox) | **Zwei Drehregler:** `sandbox_mode` (read-only / workspace-write / danger-full-access) × `approval_policy` (untrusted / on-request / never) → 3 TUI-Presets: **Read Only / Auto / Full Access** (letzteres mit Warnung "extremely risky"); `/approvals`-Menü mit 1-Zeilen-Beschreibung je Stufe | Workflow-Kanon: *Explore (Read Only) → Plan → Edit (Default) → /diff → Review → Test → Commit*; Enterprise: `requirements.toml` (erlaubte Policies, Prefix-Rules mit `justification`) |
| **OpenCode** [OC1] | Transcript + Panels; TUI-Config `tui.json` | `ctrl+x m` (Leader-Key + Buchstabe!) | **Tab** zykliert Primary-Agents (plan/build); Esc = cancel | **Leader-Key-System** `ctrl+x` (n/l/c/e/m/t/u/r/d/h), **Command Palette** `ctrl+p`, **Mouse: true**, `diff_style: auto`, Scroll-Beschleunigung, **Attention: Benachrichtigungen + Sound-Packs** (eigene mp3 pro Ereignis!), Cursor-Style/blink config |
| **Crush (Charm)** [CH1] | "Most polished TUI": soft borders, distinct accent colors | **1-Tasten-Wechsel mid-session ohne Kontextverlust** | — (Model-first, Provider-neutral) | Bubble Tea-Qualität (lazygit/gh-klassig); farbige Diffs **vor** Apply; Sessions persistieren; Statusbar zeigt *immer* welches Model; Cross-Plattform inkl. Android/BSD |
| **Omp** [O1][O2][O3] | Transcript + **Live-Subagent-Cards** + Status-Line + HUD | **Ctrl+L** = Model-Selector öffnen, **Ctrl+P** = Role-Cycling (smol/default/slow, Shift=temporär), **Alt+P** = temporär | **Alt+Shift+P** = Plan-Mode toggle; Role-Wechsel `ctrl+p` | **Status-Line-Presets** (default/minimal/compact/full/nerd/ascii/custom) mit **Powerline-Separators**; `colorBlindMode: true` = blau statt grün für Diff-Additions; **Hardware-Cursor pro Modus** (Block/Bar/Underline via DECSCA, Completion = dim); Mouse: Klick fokussiert Subagent-Cards/HUD (Shift+drag = Selektion); gepinnte Agent-Jump-List; Inline-Images; OSC-8-Hyperlinks; **Transcript-Suche** + Sprung zu Vorherigen/Nächsten-Prompt; Resize-Strategie für Scrollback (rebuild/append/preserve); **Differential Renderer** (AgentEvent-Stream → Render) |
| **Gemini CLI** [G1–G3] | Transcript + Dialoge | **`/model`-Dialog** (Auto-Gemini-3 / Auto-2.5 / Manual); **kein Hotkey** (Issue #25141, 04/2026 — offen!) | 4 Modi: default / auto_edit / yolo / plan; **Ctrl+Y** + Shift+Tab | `/settings`-Dialog (vimMode, Notifications, Plan-Model-Routing: Pro für Planung, Flash für Implementierung!); **"Topic & Update Narration"** = strukturierte Fortschritts-Reports statt Chatter |
| **Aider** [A1] | **Keine TUI** — Chat im plain Terminal + Git | `/model`-Chat-Befehl | `--yes-always` etc. | Der Minimal-Pol: beweist, dass "Terminal-Chat ohne TUI" funktioniert — und warum die Kategorie trotzdem zu TUIs migriert ist (Feedback, Diffs, State) |

**Referenz-Tools außerhalb der Kategorie** (Layout-Paradigmen, aus [T1]): lazygit
(Persistent Multi-Panel — Panels an festen Positionen, Nutzer baut räumliches Gedächtnis),
fzf/atuin (Overlay/Popup — auf Abruf, bricht Scrollback nie), btop (Widget Dashboard —
Braille-Dichte), k9s (Drill-Down — Enter runter, Esc hoch, Breadcrumb), yazi (Miller
Columns), htop/tig (Header + Scrollable List).

### 1.2 Der Model-Selector im Detail (wer was kann)

| | Öffnen | Inhalt | Auto/Routing | Pins/Fav | Kosten-Anzeige |
|---|---|---|---|---|---|
| Omp [O1][O3] | Ctrl+L | Provider+Modelle, Role-Modelle (smol/default/slow) | Role-Routing + Fallback-Chains | Role-Wechsel per Cycling | Live Token-Counting |
| OpenCode [OC1] | `ctrl+x m` | Liste 75+ Provider/Modelle | — | — | pro Session |
| Claude [CC2] | Alt+P | Claude-Modelle | — | — | Credit-Meter |
| Gemini [G2] | `/model` Dialog | Auto (Generationen) / Manual | **Auto als Default-Empfehlung** | — | Plan-Meter |
| Codex [CX1] | nur Config/Flag | Profiles (fast/deep/zdr) | — | Profiles | Budget-Tracking |
| Crush [CH1] | 1 Taste | Provider-Liste | — | Mid-Session, kein Kontextverlust | — |

**Muster (was "gut" heißt):** Fuzzy-Search in der Liste · Spalten: Name, Provider,
$ /1M tok (in/out), Kontextfenster, Features (Tools/Vision/Caching/Reasoning) ·
**"Auto" als erste Option** (Gemini macht's) · zuletzt benutzt + Favoriten · "nur für
diesen Turn" als explizite Option (Omp: Alt+P/Ctrl+P+Shift) · **Routing-Entscheidung
sichtbar** (niemand zeigt "warum dieses Model für diesen Task" — dein Router aus Plan §2.6
+ `budget_guard`-Tool liefert das Material, die TUI macht es sichtbar).

### 1.3 Mode- & Permission-Selector im Detail

**Der konvergierende 2026-Standard:**

- **Sicherheitsebene als Zyklus:** Shift+Tab (Claude, Gemini) oder Menü (Codex). Immer
  3–5 Stufen, immer mit Status-Indikator im UI, immer persistierbar (defaultMode/Profile).
- **Plan als Modus, nicht als Befehl:** read-only durchgesetzt *auf Tool-Ebene* (Claude:
  "can literally not modify"; Gemini: plan.directory + Model-Routing).
- **Beschreibung je Stufe** im Menü (Codex [CX2]: "Codex can read files… Approval is
  required to edit…") — das ist der UX-Standard, den Nutzer erwarten.
- **Warn-Stufung:** "Full Access" wird explizit als riskant gelabelt (Codex:
  "extremely risky"; Claude: Bypass "sandbox only").
- **Zwei Achsen werden von niemandem in der UI zusammengeführt:** Codex hat *zwei
  Drehregler* (sandbox × approval), die TUI in 3 Presets presst. Claude mischt Modus
  (Plan) und Permission (Ask/Accept/Bypass) in einer Achse. **Beides ist der Punkt, an dem
  Nutzer prompten-konfigurativer werden** (s. Codex-Anti-Patterns [CX1]: "approval=never
  bei read-only = kann nichts tun").

### 1.4 Weitere TUI-Patterns, die sich bewährt haben (Streu-Fund)

- **Cursor-Form pro Modus** (Omp, DECSCA): Block (Normal), Bar (Insert), Underline
  (Visual), dim bei Completion — Modi sind *sichtbar* ohne Statusbar lesen [O1].
- **Sound + Terminal-Notifications** (OpenCode: sound_packs, volume, pro-Ereignis mp3;
  Gemini: notificationMethod) — für "Agent wartet auf dich" [OC1][G3].
- **Live-Subagent-Cards** + gepinnte Agent-Liste (Omp) — Multi-Agent wird *sichtbar*,
  Klick fokussiert, Approval-Overlay zeigt Source-Thread (Codex [CX1]-Analogon: "o" öffnet
  den Thread) [O1].
- **Narration statt Chatter** (Gemini "Topic & Update Narration") — strukturierte
  Fortschritts-Events statt Fließtext [G3].
- **Word-Level-Diff-Highlighting** ("elevates quality", [T1]) — bei `similar`/`diffy`
  (dein Plan §4) als Pflicht.
- **Braille/Block-Sparklines** für Zeitreihen im Status ([T1]) — perfekt für
  Kosten-/Token-Ticker.
- **Overlay-Prinzip** (fzf): Dialoge poppen auf gedimmtem Hintergrund, brichten den
  Transcript-Flow nie, Esc schließt [T1].
- **Transcript-Suche + Prompt-Sprünge** (Omp/Pi: `ctrl+shift+f`, `ctrl+shift+up/down`)
  [O2].

---

## 2. Design-Prinzipien (was du befolgst, was du ablehnst)

### 2.1 Die 10 Gebote für true-code (aus [T1][T2][T3][O1][CH1])

1. **Keyboard-first, Mouse optional.** Jede Funktion ohne Maus erreichbar; Mouse ist
   Komfort (Fokus, Scroll), nie Pflicht. (OpenCode/Omp: Mouse-Setting.)
2. **Progressive Disclosure in 3 Stufen:** Footer zeigt **3–5 Kontext-Tasten** (immer
   sichtbar) → `?` zeigt **alle Tasten des aktuellen Kontexts** (Overlay) → `/help`+Doku
   zeigt alles. "Beginners see the floor, experts find the ceiling." [T1][T2]
3. **Kontextuelle Intelligenz:** Keybindings, Footer, Help und sogar verfügbare Tools
   ändern sich mit *Modus + Fokus*. "Die UI verdient Vertrauen, indem sie immer korrekt
   darüber ist, was gerade möglich ist." [T2] → Bei dir: CODE/WORK/LEARN zeigen je andere
   Footer-Tasten *und* andere Tool-Sets (Tool-Recherche §4.1!).
4. **Räumliche Konsistenz:** Panels stehen **nie** an wechselnden Orten (lazygit-Regel
   [T1]). Dein LEARN-Panel ist immer an derselben Stelle; Diff-Pane immer unten.
5. **Semantische Farben, nie hartcodiert:** Slots (`accent.primary`, `status.error`,
   `git.staged` …) statt Hex im Widget-Code [T1]. **Golden Rule: mit 16 ANSI-Farben
   nutzbar** (True Color *veredelt*, erzeugt nie die Hierarchie). `$NO_COLOR`
   respektieren, 3+ Emulatoren testen, Light-Dark via OSC-Query. **Farbblindmodus ab
   Tag 1** (Omp-Standard: blau statt grün für +Zeilen; sichere Paare: blau/orange,
   nie nur rot/ grün) [O1][T1].
6. **Einer Fokus, klarer Indikator:** Tab/Shift+Tab wandert, gefokustes Panel leuchtet
   (Border/Akzent), Modals sind Focus-Traps. **Cursor-Form pro Modus** (DECSCA) [O1].
7. **Diff-First mit Word-Level-Highlighting** [T1]: Preview *vor* Apply (Crush/Omp),
   `[a]ccept [e]dit [r]eject [?]erklären` (dein Plan) — plus Neu: **`[?]` öffnet
   `explain_turn`** inline (s. §3.4).
8. **Nie blockieren, nie flackern:** Feedback <100 ms, Spinner erst nach 200 ms (kein
   Flash), 15–30 FPS **differential** Rendering (nur geänderte Zellen), Synchronized
   Output, Overwrite statt Clear, Text-Streaming "im lesbaren Takt, nicht im
   Netz-Burst" [T1][T3]. (Dein Plan §2.8/1 "30–60 fps" → Recherche-Praxis: **15–30**
   reichen und sparen CPU.)
9. **Die Konsole nicht stehlen:** **Nie** `Ctrl+C` / `Ctrl+Z` / `Ctrl+\` rebinden
   (gehören dem Terminal) [T1]. Restore-Hook bei Panic (Plan §2.8/4–5). Interaktive
   Befehle per PTY abfangen (Plan §2.8/6).
10. **Windows & Multiplexer ab Tag 1:** Box-Drawing + Block-Elemente, **kein** fancy
    Unicode/Emoji über Unicode 9.0 [T1]; Windows Terminal/ConPTY + tmux/zellij in CI
    (Plan §7: Windows-CI von Anfang an).

### 2.2 Responsive & Minimal-Size

- Minimum **80×24**; darunter: "Terminal zu klein"-Hinweis, nie Crash [T1].
- Resize-Strategie: **Priority Collapse** (weniger wichtige Panels verstecken sich erst)
  → unter 100 Spalten: Side-Panel wird Tab/Overlay [T1].
- `SIGWINCH` sauber; Omp kennt drei Scrollback-Strategien (rebuild/append/preserve)
  [O1] — für dich: rebuild (einfach, korrekt).

### 2.3 Dialoge nach Schwere (Standard aus [T1])

| Schwere | Pattern |
|---|---|
| Reversibel | Machen, Statusbar-Toast (3–5 s) |
| Mittel (Datei überschreiben) | Inline: "Press `y` to confirm" |
| Schwer (destruktiver Befehl) | **Modal** mit Diff/Details + `[a]/[e]/[r]/[?]` |
| Irreversibel im Batch | `--dry-run` + explizite Bestätigung |

→ Dein Permission-Prompt fällt in Stufe 3: **immer mit Kontext** (was genau, woher die
Anfrage, was passiert bei Yes), nie bare Yes/No.

### 2.4 Anti-Pattern-Liste (Prüfliste vor jedem Release)

| # | Anti-Pattern | Fix |
|---|---|---|
| 1 | Farben brechen je Terminal | 16-ANSI-Basis, 3+ Emulatoren, Light/Dark [T1] |
| 2 | Flicker/Full-Redraws | Double-Buffer, synchronized output, Overwrite [T1] |
| 3 | Undiscoverable Keybindings | Kontext-Footer + `?`-Overlay + Which-Key-Hints [T1] |
| 4 | Windows/WSL-Breakage | Windows-Terminal-Tests, Unicode-Enthaltsamkeit [T1] |
| 5 | Unicode-Inkonsistenz | Box-Drawing + Blocks only, Emoji ≤ 9.0 [T1] |
| 6 | tmux/zellij-Breakage | Mouse-Capture darf Selektion nicht killen [T1] |
| 7 | Keine Accessibility | `NO_COLOR`, Monochrom, nie Farbe-allein, Farbblind [T1] |
| 8 | UI blockiert während Arbeit | Feedback <100 ms, async, Spinner, Progress [T1] |
| 9 | Modal-Verwirrung | **Immer aktuellen Modus in der Statusbar zeigen**, Cursor-Form pro Modus [T1] |
| 10 | Over-decorated Chrome | "Borders and colors serve content, not ego. **The content IS the interface.**" [T1] |

---

## 3. true-code TUI-Konzept (Vorschlag auf Basis der Recherche)

### 3.1 Layout: "Cockpit" — 1 Hauptfläche + 1 kontextabhängiges Panel

Paradigma: **Persistent Multi-Panel** (lazygit-Klasse [T1]) mit der Einschränkung, dass
das Side-Panel **modusspezifisch** ist (Progressive Disclosure: erscheint nur, wenn es
Inhalt hat):

```
┌─ true-code ─ CODE ──  workspace-write ── claude-sonnet-4.6 ── 12.4k/200k ── €0.31 ── 98% cache ─┐
│ ▸ Plan (2/4)                                                                                     │
│   1. [✓] Repo-Struktur scannen          (12 Dateien, 0.9s)                                       │
│   2. [▶] Auth-Modul refaktorieren       patch src/auth.rs   [?]                                  │
│   3. [ ] Tests + cargo test                                                                 │
│ ┌─ diff: src/auth.rs (word-level) ────────────────────────────────┐  ┌─ why? ──────────────────┐ │
│ │  42 │  let user = db.find(&id).context("user lookup")?;         │  │ Plan-Schritt 2: Auth-     │ │
│ │  43 │ -    .unwrap();   +    .context("…")?;   [−3+2 tok]       │  │ refactor → Error-Handling │ │
│ └─────────────────────────────────────────────────────────────────┘  │ Kosten: €0.04 · 1.2s     │ │
│ └───────────────────────────────────────────────────────────────────┘  │ Modell: sonnet (Plan)   │ │
│ [a]nnehmen [e]dit [r]evert [?]erklären [Esc]abbrechen                    └────────────────────────┘ │
│ › ▏                                                                                        ⏎ senden │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

- **Statusbar (oben, Powerline-Segmente à la Omp [O1]):** Modus (farbig) · Autonomie-Stufe
  (Symbol: ⏸/✎/) · Model · Tokens/Kontext · **Kosten + Cache-Hit** · Session-Akzentfarbe.
  → Auf einen Blick: *was, wie frei, mit wem, wie viel* — das ist die "andere UI" in
  24 Spalten.
- **Transcript (Mitte):** Narration-Events statt Chatter (Gemini-Muster [G3]), Tool-Calls
  als kompakte Zeilen mit `[?]`-Handle (→ explain_turn).
- **Diff-Pane (unten, bei Schreibzugriffen):** word-level, vor Apply, `[a]/[e]/[r]/[?]`.
- **Side-Panel (rechts, kontextabhängig):**
  - CODE: "why?"-Panel + offene Dateien
  - WORK: Artefakt-Liste (Dateien/PRs/Checklisten — dein Plan §1B)
  - LEARN: Konzept-Badges (🟥🟨), offene Frage, "Review fällig: 3 Konzepte"
- **Footer:** 3–5 Kontext-Tasten, z. B. CODE: `[Shift+Tab]stufe [Alt+P]model [?]help [Esc]abort`.
- **Overlays (alle:** gedimmter Hintergrund, Esc schließt, nie Scrollback-Killer [T1]):
  Model-Selector · Mode-Menü · Rewind-Timeline · Kosten-Report · Audit-Log · ?-Help.
- **< 100 Spalten:** Side-Panel wird Overlay, Statusbar "compact"-Preset (Omp hat 6
  Presets [O1] — bau dir 3: default/compact/minimal).

### 3.2 Model-Selector (konkret)

**Öffnen:** `Alt+P` (Konvention aus Claude/Omp [CC2][O3]) + klickbar auf Statusbar-Segment.

```
┌─ model (↑↓ wählen · / suchen · Enter setze · Esc ab) ─────────────────────────────┐
│  ⦿ Auto (Task-Router)   ← "Model pro Aufgabe wählen, Kosten live"                  │
│  ── Favoriten ──────────────────────────────────────────────────────────────────── │
│  ○ claude-sonnet-4.6      anthropic   $3/$15 per 1M   200k ctx   tools✓ cache✓     │
│  ○ gpt-5.5                openai      $1.25/$10       400k ctx   tools✓            │
│  ○ qwen3-coder (local)    ollama      €0              128k ctx   tools✓ offline    │
│  ── Für diesen Task passt: ─────────────────────────────────────────────────────── │
│    ✓ qwen3-mini — Triage/Recherche (€0.02, 40ms)     [1]                          │
│    ✓ claude-sonnet-4.6 — Planung/Refactor (aktuell)  [2]                           │
│  zuletzt: gemini-3-flash [Alt+P] · nur-diesen-Turn: [Enter]+[t]                   │
└───────────────────────────────────────────────────────────────────────────────────┘
```

- **Spalten:** Model · Provider · $/1M (in/out) · Kontext · Feature-Flags (Tools/
  Vision/Cache/Reasoning) — alle Daten kommen aus deinem Provider-Trait (`price()`,
  `context_window()`, `supports()`) → **kostenlos aus Plan §2.6**.
- **"Auto (Task-Router)" als erste Zeile** (Gemini-Empfehlung [G2] + dein Router aus
  Plan §2.6): zeigt die aktuelle Routing-Entscheidung + Begründung (Budget-Guard-Material).
- **"Für diesen Task passt:"** — der Moment, in dem Model-Neutralität *erlebbar* wird.
  Niemand sonst zeigt das (Recherche-Ergebnis: alle zeigen Listen, keiner Fit).
- **Temporär vs. persistent:** Enter = Session, `t` = nur dieser Turn (Omp-Konvention
  [O3]), `g` = Global-Default (Settings).
- **Profiles à la Codex** (`fast`/`deep`/`local`) als gespeicherte Bundles
  (Model+Reasoning+Approval+Sandbox) [CX1] — aber im TUI wählbar statt nur in TOML.
- Statusbar zeigt Routing-Wechsel **immer** (deine Regel "nie stillschweigend wechseln"
  wird UI) — z. B. blinkendes Segment `→ qwen3-mini (Triage)`.

### 3.3 Mode- & Permission-Selector (konkret) — das 2-Achsen-Modell

**Kernidee (Unterscheidung zu ALLEN Wettbewerbern):** Zwei Achsen, zwei Gesten:

| Achse | Werte | Geste | Bedeutung |
|---|---|---|---|
| **Task-Modus** (WAS) | `CODE` / `WORK` / `LEARN` | `1` `2` `3` oder Tab-Zyklus | Tool-Set, Side-Panel, Footer, Prompt-Vorlagen |
| **Autonomie** (WIE FREI) | ⏸ Read-only / ✎ Workspace / ⚡ Full | **Shift+Tab** (Kategorie-Muskelgedächtnis) | Permission-Stufe 1/2/3 aus deinem Plan §2.4 |

Warum so: (a) Shift+Tab = etabliertes Safety-Cycling (Claude/Gemini [CC1][G3]), (b)
die drei Autonomie-Stufen = exakt deine Plan-Stufen (Read-only/Workspace-Write/Full),
(c) Task-Modus ist deine Erfindung — `1/2/3` als Direktwahl ist self-explanatory und
kollidiert nicht mit Edit-Keys (Ziffern im Input bleiben nutzbar, weil Modus-Wechsel
nur mit Modus-Prefix? Nein: reine `1`-`3` im *leeren* Input feld, sonst `Tab`-Zyklus —
ADR-Entscheidung).

**Statusbar-Anzeige (Pflicht):** `CODE · ✎ workspace · claude-sonnet-4.6 · 12.4k/200k · €0.31`
— Modus farbig (z. B. CODE=blau, WORK=cyan, LEARN=amber), Autonomie als Symbol + Wort
beim Wechsel. **Leeres Statusbar-Segment = nie.** (Anti-Pattern #9 [T1].)

**Detail-Menü `/modes` (Overlay):** beide Achsen als Matrix mit 1-Zeilen-Beschreibungen
(Codex-Standard [CX2]) + "was sich ändert" je Wechsel (Tools: 14→10, Netz: aus→an,
Panels: diff→quiz):

```
┌─ modes ───────────────────────────────────────────────────────────────┐
│ Task-Modus        [1] CODE   [2] WORK   [3] LEARN                     │
│   CODE  — Agent plant, editiert, testet (14 Tools)                    │
│   WORK  — Research/Release/Triage, Artefakt-Output (13 Tools)         │
│   LEARN — erklärt, fragt, übt — du lernst mit (10 Tools, read-only)   │
│                                                                       │
│ Autonomie (Shift+Tab)                                                 │
│   ⏸ Read-only     lesen/suchen/planen — kein Schreiben, kein Netz     │
│   ✎ Workspace     schreiben im Projektordner — Standard [aktuell]     │
│   ⚡ Full          alles, jede Aktion fragt — "extremely risky"       │
│                                                                       │
│ Sandbox: landlock+seccomp aktiv · Netz: allowlist (2 Domains)          │
└───────────────────────────────────────────────────────────────────────┘
```

**LEARN-Modus-Spezial:** Footer wechselt zu `[Space]hint [e]rklären [q]frage [n]ächste`,
Side-Panel zeigt Konzept-Badges + nächste Wiederholung (FSRS aus Tool-Recherche §3G).
Autonomie in LEARN default = ⏸ Read-only (der Agent übt mit dir, nicht *für* dich).

**Permission-Prompt (wenn Autonomie fragen muss) — der Trust-Moment:**

```
┌─ ✎ write_file: src/auth.rs ───────────────────────────────────────────┐
│ Überschreibt 58 Zeilen (Diff rechts/vorher).                          │
│ [?] Warum: Plan-Schritt 2 "Auth-Refactor" — Error-Handling ergänzen.  │
│ [a]ccept  [e]dit  [r]eject  [A]ways für src/auth.rs  [t]Turn  [?]erkläre│
└──────────────────────────────────────────────────────────────────────┘
```

Die `[?]`-Erklärung im Permission-Prompt hat **niemand** (Recherche-Ergebnis) — und sie
ist billig: `explain_turn` auf dem Event-Log. Dazu: Scope-Optionen (`Always für diesen
Pfad` / `für diesen Turn`) statt nur global (Claude "add rule on the spot" [CC1],
Codex "on-request" [CX1]) — plus: destruktive Befehle (`rm`, `git push --f`, `DROP`)
fragen **immer**, egal welche Stufe (dein Plan §5/13).

### 3.4 Transparenz-Flächen (deine USP-Panel, §0-Punkt 1)

- **`[?]`-Handle an JEDEM Tool-Call** im Transcript → `explain_turn`-Inline-Box
  (Was/Warum/Kosten/Nächster-Schritt). In 100 ms, aus dem Event-Log.
- **/cost** (Overlay): Sparkline Kosten/Turn (Braille [T1]), Top-5 teuerste Aktionen,
  Cache-Hit-Rate, "Session-Ø pro Datei: €0.08".
- **/audit** (Overlay): alle Lese-/Schreib-/Netz-Aktionen mit Begründung, filterbar,
  exportierbar (Markdown). → Das Audit-Log-Tool (Tool-Recherche #21) *ist* die Datenquelle.
- **Kosten-Ticker in der Statusbar** ist nicht extra Feature, sondern Segment (Sparkline
  optional) — "…und dir sagt, was es kostet" im Dauerbetrieb.

### 3.5 LEARN-UX (kurz, Details in Tool-Recherche G)

- **Konzept-Badges** im Side-Panel: `Ownership 🟩 · async 🟨 · Result<T,E> 🟥 (fällig: heute)`
  (Badge-System aus agent-tutor-skill [L1]).
- **Quiz-Overlay:** 4 Optionen, zero-hint, danach Explain-Back-Feld ("Erkläre es mir
  zurück") — Tastatur-first, Zeitdruck optional aus.
- **Practice-Split:** linke Half = deine Datei (PTY/Editor), rechte Half = Agent-Hint
  (nie die Lösung). Enter = "schauen!" → Agent-reviewt.
- **Wochen-Report:** `learning_report` als renderbares Overlay + Export.

### 3.6 Visueller Charakter (Anti-Cookie-Cutter, Plan-Falle #19)

- **Eigenes "Cockpit"-Gefühl:** Powerline-Statusbar mit Session-Akzentfarbe (Omp-Muster
  [O1]) + Modus-Farbe + Kosten-Segment. "Auf einen Blick erkennbar: true-code."
- **Semantische Slots** (keine Hex im Code [T1]): `mode.code/accent`, `mode.learn/accent`,
  `autonomy.read/write/full`, `status.cost-ok/warn/over`, `diff.add/del/word`.
- **Symbol-Presets ab Tag 1:** `unicode` / `nerd` (Nerd-Font) / `ascii` (Windows-Legacy,
  kein Unicode) + **`colorBlindMode`** (Omp-Standard [O1]).
- **Dein Design-Ton** (entscheiden in ADR): minimal-präzise (Pi/Vim-Ästhetik) oder
  dichter (btop-Ästhetik)? Empfehlung: **minimal-präzise** — passt zu "Werkzeug, nicht
  Power-Tool", unterscheidet sich von Crushs "Glamour" [CH1] und Omps Dichte.
- **Sound/Notifications:** P7 (OpenCode macht's [OC1], nicht Pflicht).

---

## 4. "Brauchen wir eine andere UI?" — die direkte Antwort

**Nein, anders als:**
- Kein neues Layout-Paradigma (Transcript + Status + Footer + Overlays ist der
  konvergierte Standard; Abweichung = Lernkosten ohne Nutzen).
- Kein Desktop-/Web-GUI (Plan §8 bleibt; OMP Studio zeigt: Wrapper entstehen community-seitig
  [O4], du musst das nicht bauen).
- Keine exotischen Keymaps (Vim/Emacs-Modus *als Option*, nicht als Default — Gemini/
  OpenCode haben vimMode als Setting [G3][OC1]).

**Ja, anders als (diese fünf Flächen sind sichtbar "true-code"):**
1. **Statusbar-Cockpit** mit Kosten+Cache+Modus+Autonomie (Powerline, Akzentfarbe).
2. **2-Achsen-Modell** Task×Autonomie (Codex hat 2 Drehregler in der Config, du 2
   Gesten in der UI; Claude mischt die Achsen).
3. **`[?]` überall** — Permission-Prompt, Tool-Call, Diff, Plan-Schritt (Explain-
   Yourself als UX-Prinzip, nicht als Feature).
4. **LEARN-Flächen** — Konzept-Badges, Quiz-Overlay, Practice-Split (gibt es so in
   keinem Coding-Agenten, s. Tool-Recherche).
5. **Model-Selector mit "Auto (Task-Router)" + "Für diesen Task passt"** —
   Model-Neutralität als sichtbare, wählbare UI.

**Muskelgedächtnis-Verträge (übernehmen, weil Nutzer's sie erwarten):** Shift+Tab =
Safety-Stufe · Esc = abbrechen/schließen · Esc+Esc = Rewind · Tab = Autocomplete/Fokus ·
`/` = Befehle · `?` = Help · `!` = Shell · `@` = Datei · Enter = senden · Alt+P = Model.

---

## 5. ADR-Kandidaten (aus dieser Recherche)

| ADR | Entscheidung |
|---|---|
| ADR-0003 | TUI-Layout "Cockpit": 1 Haupt-Transcript + 1 modus-spezifisches Side-Panel + Overlays; ratatui/crossterm; Minimum 80×24; Priority-Collapse unter 100 Spalten |
| ADR-0004 | 2-Achsen-Modell: Task-Modus (1/2/3 oder Tab) × Autonomie (Shift+Tab, 3 Stufen); Autonomie = Permission-Stufe aus Plan §2.4; LEARN default Read-only |
| ADR-0005 | UI-State als **purer Reducer** über dem Event-Log (Events → State → Render, kein I/O im Reducer); dasselbe Event-Log für Persistenz + Replay + Audit |
| ADR-0006 | Semantisches Farb-System (Slots, 16-ANSI-Basis, NO_COLOR, 3+ Emulatoren), Symbol-Presets (unicode/nerd/ascii), colorBlindMode, Modus-Farben, Cursor-Form pro Modus (DECSCA) |
| ADR-0007 | Model-Selector: "Auto (Task-Router)" als Default, Fit-Empfehlungen, temporäre vs. persistente Auswahl, Routing-Wechsel immer in Statusbar sichtbar |

---

## 6. TUI-Prioritäten in der Roadmap (P0–P5, ergänzend zum Plan)

| Phase | TUI-Leistungsumfang |
|---|---|
| **P0** | Transcript + Statusbar (Modus/Model/Tokens) + Input (Bracketed-Paste, Multi-Line) + Esc-Abort + Footer (3 Tasten) + Restore-Hook |
| **P1** | Diff-Pane (word-level, `[a]/[e]/[r]/[?]`) + Permission-Prompt (Modal, Scoped-Optionen) + **Model-Selector v1** (Liste+Fuzzy+Kosten) + `/modes` (2 Achsen) + `?`-Overlay |
| **P2** | Rewind-Timeline-Overlay + Undo + **`[?]`-Inline (explain_turn)** + **/cost-Overlay (Sparkline)** + Narration-Events statt Chatter |
| **P3** | Subagent-Cards (live) + Transcript-Suche + Prompt-Sprünge + Themes (3 Presets) + Mouse (Fokus/Scroll, optional) |
| **P4** | LEARN-Panel: Konzept-Badges, Quiz-Overlay, Practice-Split, Wochen-Report-Overlay |
| **P5** | Symbol-Presets + colorBlindMode-Härtung + Windows-Terminal/ConPTY-Finaltests + tmux/zellij + (optional) Sound/Notifications |

**Aufwand-Hinweis:** P0+P1 = der größte TUI-Block des MVP (~3–4 Wochen bei
Rust-lernendem Entwickler, ratatui-Beispiele als Startpunkt). P2-P4 wachsen *mit* den
Features der Tool-Recherche (gleiche Datenquellen: Event-Log, Lernprofil).

---

## 7. Quellen

| # | Quelle | Was geliefert |
|---|---|---|
| CC1 | [claudefa.st — Claude Code Permissions: Safe vs Fast Modes](https://claudefa.st/blog/guide/development/permission-management) (12.09.2026) | 5 Permission-Modi, Shift+Tab-Cycling, defaultMode, Hooks-Permission |
| CC2 | [wil.dev — Using the Claude Code TUI](https://wil.dev/guides/using-the-claude-code-tui/) (06.03.2026) | Komplette Shortcut-Liste (Alt+P Model, Ctrl+B/T/O, Esc+Esc), Modi-Verhalten |
| CC3 | [bitsminds.com — Claude Code's Five Permission Modes](https://www.bitsminds.com/news/claude-code-permission-modes-explained-2026) (06.08.2026) | Ask/Accept/Plan/Auto/Bypass, Desktop-Keys 1–5, "Auto = background safety check" |
| CX1 | [thedeepfeed.ai — Stop fighting Codex CLI approval prompts](https://www.thedeepfeed.ai/posts/2026-06-10-stop-fighting-codex-cli-approval-prompts/) (10.06.2026) | Zwei Drehregler (sandbox×approval), 3 TUI-Presets, Profiles (fast/deep/zdr), Anti-Patterns |
| CX2 | [daehnhardt.com — Codex CLI Part 2: Security Controls](https://daehnhardt.com/blog/2026/02/06/codex-cli-part-2-security-controls-and-safe-edits/) | /approvals-Menü-Texte (3 Stufen mit Beschreibungen), Safe-Edit-Workflow /diff |
| CX3 | [blakecrosley.com — Codex CLI Guide 2026](https://blakecrosley.com/guides/codex) (07.09.2026) | Sandbox-Tabelle, Enterprise requirements.toml (Policies, Prefix-Rules) |
| OC1 | [opencode.ai/docs/tui](https://opencode.ai/docs/tui/) (11.09.2026) + [opencode.asia/tui](https://www.opencode.asia/tui/) | tui.json (Leader-Key, Palette, Mouse, Sound-Packs, diff_style, Cursor), /models /themes /undo |
| CH1 | [rywalker.com — Crush](https://rywalker.com/research/crush) (11.06.2026) + [aicoolies.com — Crush Review](https://aicoolies.com/reviews/crush-review) (24.06.2026) | "Most polished TUI", 1-Tasten-Model-Switch, Diffs vor Apply, Statusbar-Model, Bubble Tea |
| O1 | [can1357/oh-my-pi — settings.md](https://github.com/can1357/oh-my-pi/blob/main/docs/settings.md) (04.08.2026) + [oh-omp — TUI Overhaul & Shortcuts](https://github.com/open-horizon-labs/oh-omp) | Status-Line-Presets/Powerline, colorBlindMode, Mouse-Fokus, symbolPresets, Ctrl+L/Ctrl+P/Alt+P, Shift+Tab Thinking, Resize-Strategien |
| O2 | [pi.dev — Keybindings](https://pi.dev/docs/latest/keybindings) | Vollständige Keybinding-Aktionsliste, Fullscreen-Transcript-Scrolling, Prompt-Sprünge, Transcript-Search, OSC-8 |
| O3 | [yeluo45.github.io/oh-my-pi-design — 13·pi-tui](https://yeluo45.github.io/oh-my-pi-design/en/docs/13-pi-tui) | Differential-Renderer-Architektur (AgentEvent→TUI→PTY), DECSCA-Cursor pro Modus, 5 Border-Styles |
| O4 | [Bodyes26/OMP-Studio](https://github.com/Bodyes26/OMP-Studio) | Community-Desktop-Wrapper (GUI+Terminal-Wechsel) — Beleg für GUI-Nachfrage ohne Produkt-Pflicht |
| G1 | [gemini-cli Issue #25141 — Hotkey for model switching](https://github.com/google-gemini/gemini-cli/issues/25141) (10.04.2026) | Offener UX-Schmerz: Model-Wechsel = multistep, kein Hotkey |
| G2 | [gemini-cli docs — /model](https://github.com/AI-Xens-Consulting/gemini-cli-2026-05/blob/main/docs/cli/model.md) (01.05.2026) | Auto/Manual-Dialog-Design, "Auto empfohlen" |
| G3 | [geminicli.com — /settings](https://geminicli.com/docs/cli/settings/) (14.05.2026) + [toolsbase.dev — Gemini CLI Cheat Sheet](https://toolsbase.dev/en/reference/gemini-cli-commands) (05.07.2026) | vimMode, Notifications, Plan-Model-Routing (Pro/Flash), 4 Approval-Modi, Ctrl+Y/Shift+Tab, Topic-&-Update-Narration |
| T1 | [hyperb1iss/hyperskills — tui-design (TUI Design System)](https://www.explainx.ai/skills/hyperb1iss/hyperskills/tui-design) (11.09.2026) | Layout-Paradigmen, Keybinding-Layer L0–L3, 3-Stufen-Help, Dialog-Schwere, semantische Farben/Base16, Accessibility, 10 Anti-Patterns |
| T2 | [hyperbliss.tech — The Terminal Renaissance](https://hyperbliss.tech/blog/2026.04.04_terminal-renaissance/) (04.04.2026) | 7 Prinzipien (Progressive Disclosure, Kontextuelle Intelligenz, "16 Farben nutzbar"), reines-Reducer-Argument für AI-UIs |
| T3 | [pi-tui-design (lobehub)](https://lobehub.com/skills/joelhooks-pi-tools-pi-tui-design) (06.03.2026) | Terminal-Aesthetic-Vocabulary (Box-Drawing/Braille/Powerline), keyHint()-Muster, Segmente-Rendering |
| L1 | [agent-tutor-skill](https://github.com/Bhala-Srinivash/agent-tutor-skill) (aus Tool-Recherche) | Konzept-Mastery-Badges (🟥🟨🟦⬜) als UX-Vorlage |
| A1 | [arcbjorn.com — State of CLI Coding Agents](https://blog.arcbjorn.com/state-of-cli-coding-agents-2026) (aus Tool-Recherche) | Aider als no-TUI-Pol, 35-Agenten-Landschaft |

---

*Version 0.1 — TUI-Fundament für Plan v0.2. Nächste Schritte: ADR-0003…0007
konkretisieren, dann P0-TUI-Spike (Transcript+Statusbar+Input) mit ratatui-Example
aus Plan §9 Tag 1 verknüpfen.*
