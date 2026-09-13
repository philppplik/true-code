# Deep Research: Wie man Lernen in einer TUI *wirklich* wirksam macht

**Stand:** 13.09.2026 · **Für:** `true-code` — P4 / `tc-learn` · **Autor:** Recherchen-Synthese
**Kernfrage:** *„Wie genau kann man das lerneffektiv umsetzen — in einer TUI, für Menschen, die
verschieden sind?"*

---

## TEIL 0 — Die zehn Gebote (lies nur das, wenn du nichts anderes liest)

| # | Prinzip | Warum |
|---|---|---|
| 1 | **Differenziere nach Vorwissen, nicht nach Lerntyp.** | Lernstile sind ein Neuromyth: Matching-Hypothese über vier Meta-Analysen **d = 0,04** [2](https://carlhendrick.substack.com/p/the-learning-styles-illusion-debunking). Der mit Abstand stärkste Prädiktor ist **Vorwissen** [1](https://gc-bs.org/articles/from-styles-to-science-debunking-the-learning-styles-myth-and-embracing-an-evidence-based-framework-for-learning/) |
| 2 | **Lernziele kommen aus *deinem* Repo, nicht aus einem Lehrplan.** | Bottom-up-Relevanz schlägt Top-down-Curriculum. Das ist der einzige echte Daten-Vorsprung, den ein Coding-Agent hat |
| 3 | **Erklären ist Diagnose *und* Therapie.** | Die *Illusion of Explanatory Depth*: Menschen überschätzen kausales Verständnis massiv — und der Versuch zu erklären kalibriert sie nach unten [1](https://grokipedia.com/page/Illusion_of_explanatory_depth) |
| 4 | **Abfragen, nicht nochmal lesen.** | Spaced Retrieval vs. massiert: **g = 0,74** [1](https://eric.ed.gov/?id=EJ1310148) |
| 5 | **Führe erst, dann fade.** | Worked-Example-Effekt für Novizen, **Expertise-Reversal** danach: dieselbe Hilfe wird für Fortgeschrittene schädlich [7](https://www.uky.edu/~gmswan3/EDC608/Kalyuga2007_Article_ExpertiseReversalEffectAndItsI.pdf) |
| 6 | **Nur Nahttransfer erwarten.** | Ferntransfer ist selten und schwer; Expertise ist domänenspezifisch [3](https://www.aei.org/research-products/report/the-problem-with-transferable-skills/) |
| 7 | **Unterbrich nur an Commit-Punkten.** | Niemals mitten in einem Tool-Loop. Aber dann: sofortiges Feedback — das „kognitive Fenster" ist < 1 Minute [5](https://pmc.ncbi.nlm.nih.gov/articles/PMC9995700/) |
| 8 | **Erklärungen müssen dauerhaft sein.** | *Transient Information Effect*: wegscrollende Erklärung = vertane Lernmühe [10](https://leadinglearner.me/wp-content/uploads/2019/02/sweller2019_article_cognitivearchitectureandinstru.pdf) |
| 9 | **Der Agent ist Tutor, nicht Antwortmaschine.** | LLM-Effekte sind am stärksten bei **Tutor-Rolle in dauerhafter Nutzung** [1](https://arxiv.org/html/2509.22725v1). Ungesteuerte Nutzung wird zur Krücke [3](https://www.sciencedirect.com/science/article/pii/S2666920X25001699) |
| 10 | **Keine Punkte, keine Badges.** | Gamification senkte in Studien intrinsische Motivation *und* Prüfungsleistung [1](https://journals.librarypublishing.arizona.edu/itlt/article/id/4872/print/) |

---

## TEIL 1 — Wo fängt Lernen an? (Der Kaltstart)

### 1.1 Die falsche erste Frage
„Wo fange ich an?" ist falsch, weil sie einen Lehrplan voraussetzt. Die richtige Frage lautet:
**„Was davon begegnet dir in *diesem* Repo, und was davon kannst du schon?"** — zwei Fragen, die
ein Coding-Agent beantworten kann und ein Online-Kurs nicht.

### 1.2 Was ist wichtig? — Relevanz bottom-up aus dem Code

Keine generische Rust-/Python-Roadmap. Stattdessen ein **Relevanz-Score aus vier Signalen**:

```
Relevanz(Konzept) =
     0,40 × Vorkommen_im_Repo        (tree-sitter: wie oft taucht das Konstrukt auf?)
   + 0,30 × Berührung_durch_User     (welche Dateien/Funktionen editiert der Nutzer selbst?)
   + 0,20 × Hebelwirkung             (wie viele andere Konzepte setzen es voraus? Abhängigkeitsgraph)
   + 0,10 × Nähe_zum_Ziel            (vom Nutzer genanntes Ziel: „Ich will X bauen können")
```

Das ist der **eigentliche Datenschatz**: true-code sieht, welche Konstrukte in *deinem* Code
wirklich vorkommen und welche du selbst anfasst. Ein Lehrbuch kann das nicht.

### 1.3 Was ist erreicht? — Vier Evidenzquellen, strikt gewichtet

Die entscheidende Einsicht: **Die beste Evidenz ist kein Quiz.** Sie ist unbeobachtetes Verhalten.

| Rang | Evidenzquelle | Gewicht | Beispiel |
|:-:|---|---|---|
| **1** | **Assistanzfreie Anwendung** (hat der Nutzer es selbst gemacht, ohne den Agenten zu fragen?) | 0,45 | Nutzer schreibt selbst `?` statt `unwrap()` — in einer Datei, in der der Agent nichts tat |
| **2** | **Produktiver Abruf** (Gate/Parsons/Completion bestanden) | 0,30 | Nutzer ordnet die Blöcke korrekt |
| **3** | **Erklären-Können** (Explain-back) | 0,20 | Nutzer erklärt den Fehlerpfad richtig |
| **4** | **Selbstauskunft** („kenne ich") | 0,05 | fast wertlos — siehe IOED |

**Metrik-Idee, die es so noch nicht gibt: die *Assistanzfreie Anwendungsrate* (AFA).**
true-code weiß, wann ein Konzept im Spiel war (Symbol-Index + Diff-Analyse) und ob der Nutzer
dabei Hilfe geholt hat. AFA ist **passiv gemessene, echte Beherrschung** — kein Quiz, keine
Selbsttäuschung, keine zusätzliche Zeit. Das ist dein stärkster Vertrauens- und Marketingwert:
*„Wir messen nicht, was du behauptest zu können. Wir messen, was du tust."*

### 1.4 Der Kaltstart (5 Minuten, kein Quiz-Frust)

Niemand füllt gern einen Einstufungstest aus. Also: **drei Aufgaben, die sich wie Arbeit anfühlen.**

```
┌ true-code · Erste Begegnung ────────────────────────────────────────────────┐
│ Ich habe dein Repo gescannt: 47 Dateien, Rust, 3,2k Zeilen.                 │
│ Damit ich dich richtig unterstütze, drei kurze Sachen (~4 Min).             │
│                                                                             │
│ 1/3  Was gibt diese Funktion zurück, wenn `id` nicht existiert?             │
│                                                                             │
│      17  fn load(id: &str) -> Result<User> {                                │
│      18      let row = DB.query("SELECT …", id)?;                           │
│      19      Ok(User::from(row))                                            │
│      20  }                                                                  │
│                                                                             │
│      › Err(...) aus Zeile 18 – das ? propagiert den Datenbankfehler         │
│                                                                             │
│ 2/3  [Parsons]  Bringe die Blöcke in Reihenfolge   (j/k ↓↑ · J/K verschieben)│
│ 3/3  Erkläre in 1–2 Sätzen: was macht `Cow<'a, str>`?                       │
└─────────────────────────────────────────────────────────────────────────────┘
```

Warum genau diese drei?
* **Tracing** ist die höchsthebelsamste Fähigkeit in der Programmierdidaktik — und der beste
  Prädiktor für Schreibfähigkeit [3](https://www.researchgate.net/publication/332693327_Teaching_computer_programming_with_PRIMM_a_sociocultural_perspective).
* **Parsons-Probleme** sind *niedrig in kognitiver Last* und **sensitiver für Lernfortschritt** als
  klassische Schreibaufgaben [5](https://dl.acm.org/doi/pdf/10.1145/2839509.2844617).
* **Explain-back** kalibriert gleichzeitig die Selbstüberschätzung (IOED).

Danach: **ein** Frage nach dem Ziel (Autonomie — siehe Teil 7):
`Was willst du in 3 Monaten können, was du heute nicht kannst?`

### 1.5 Der ZPD-Filter (was *nicht* gelehrt wird)
Nur Konzepte, die **eine Stufe über** dem geschätzten Niveau liegen. Alles andere ist
verschwendete Zeit oder Frust. Konkret:

* unterhalb → ignorieren (kein „wusstest du schon?")
* weit oberhalb → *erwähnen*, nicht lehren („das brauchst du erst, wenn …")
* **genau eine Stufe drüber → Lernziel.** Das ist die *Zone der nächsten Entwicklung*.

---

## TEIL 2 — Was sind wann warum Lernziele?

### 2.1 Vier Lernziel-Typen (nicht alle sind gleich zu behandeln)

| Typ | Was es ist | Beste Methode | Falsche Methode |
|---|---|---|---|
| **Konzept** | `Result`, Ownership, Trait-Objekte | Erklären + Kontrast + Erklären-lassen | Auswendiglernen |
| **Prozedur** | „Fehlerpropagierung hinzufügen" | **Worked Example → Fading → eigenes Lösen** | Erst selbst versuchen (PF hilft hier *nicht*) |
| **Mentales Modell** | Was macht der Rechner bei `?`? | **Tracing**, Vorhersage, Notional Machine | Lesen |
| **Strategie/Metakognition** | Wie lokalisiert man einen Bug? | Debug-Jagd mit Lautem Denken | Zuschauen |

**Zentrale Unterscheidung aus der Forschung:** *Productive Failure* (erst selbst versuchen, dann
Instruktion) verbessert **konzeptionelles Verständnis und Transfer, aber nicht prozedurale
Flüssigkeit** [5](https://www.cse.iitk.ac.in/users/se367/14/Readings/papers/kapur-14_productive-failure-in-learning-math.pdf).
Kapur rät ausdrücklich: **PF sparsam einsetzen — 3–5 Schlüsselkonzepte pro Semester**, nicht in
jeder Lektion [3](https://boldscience.org/wp-content/uploads/2025/04/Productive-Failure.pdf). Und:
PF funktioniert nur bei hoher Design-Treue — bei 95 experimentellen Vergleichen scheitert es
regelmäßig, wenn die Design-Kriterien nicht eingehalten werden [2](https://www.researchgate.net/publication/333005127_When_Productive_Failure_Fails).

> **Regel für true-code:** Prozeduren → führen (Worked Example, Fading).
> Konzepte & mentale Modelle → **kurz** selbst versuchen lassen (PF), dann erklären.
> Strategie → Debug-Jagd.

### 2.2 Wann? — Die Timing-Regel

| Zeitpunkt | Anteil | Mechanik |
|---|---|---|
| **Just-in-Time** (Konzept steckt im Diff, den du gleich annimmst) | 70 % | Explain-before-Apply + Mini-Gate |
| **Just-in-Case** (Voraussetzung für die nächsten 2–3 geplanten Schritte) | 20 % | Vorab-Worked-Example, aus dem Plan abgeleitet |
| **Spaced Re-Check** (1 / 3 / 7 / 21 / 60 Tage) | 10 % | 60-Sekunden-Abruf beim Session-Start |
| **Just-for-Fun / „könnte dich interessieren"** | **0 %** | 🔥 Verboten. Tötet die Glaubwürdigkeit |

**Wichtiges Detail zum Spacing:** Meta-Analyse findet **keinen** Unterschied zwischen
expandierenden und gleichmäßigen Abständen (g = 0,034) [1](https://eric.ed.gov/?id=EJ1310148).
→ **Nimm feste Abstände** (1/3/7/21/60 Tage). Einfach, und die Evidenz stützt das „expandierende"
Dogma von Anki & Co. bei Retrieval-Praxis *nicht*.

### 2.3 Warum? — Jedes Lernziel braucht ein sichtbares „Warum jetzt"
Kein Lernziel ohne eine Zeile, die beantwortet: *„Was kann ich nachher, was ich jetzt nicht kann?"*
Das dient doppelt: Es stiftet **Relevanz** (Motivation) und es erlaubt dem Nutzer,
**abzulehnen** (Autonomie).

### 2.4 Die Lernziel-Pipeline (technisch)

```
Diff → tree-sitter-Knoten → Konzept-IDs (Konzept-Registry, versioniert)
     → Relevanz-Score (Repo + Nutzerverhalten)
     → ZPD-Filter (BKT-Schätzung)
     → Deckung mit schon gezeigten Zielen (max. 2 neue pro Session!)
     → Lernziel {typ, zeitpunkt, warum, mechanic}
```

---

## TEIL 3 — Wie erfasst man Kompetenz? (Wissensdiagnostik)

### 3.1 Bayesian Knowledge Tracing — aber angepasst

Klassisches BKT modelliert pro Skill ein verstecktes Markov-Modell mit vier Parametern [1](https://www.emergentmind.com/topics/bayesian-knowledge-tracing):

* **P(L₀)** — Anfangswahrscheinlichkeit, es schon zu können
* **P(T)** — Lernrate pro Gelegenheit
* **P(G)** — *Guess*: richtig geraten, ohne zu können
* **P(S)** — *Slip*: gekonnt, aber Flüchtigkeitsfehler

Update nach jeder Beobachtung per Bayes. **Warum BKT und nicht Deep Knowledge Tracing?** DKT ist
präziser (bis +25 % AUC), aber **nicht erklärbar** — du kannst nicht sagen, *warum* das System
glaubt, der Nutzer könne etwas [3](https://www.emergentmind.com/topics/bayesian-knowledge-tracing-bkt).
Für ein Tool, das dem Nutzer gegenüber transparent sein will, ist Erklärbarkeit Pflicht.

**Anpassungen für true-code:**
1. **Mehrere Evidenzgewichte** statt binär „richtig/falsch" — siehe 1.3 (AFA wiegt 0,45).
2. **Vergessen-Komponente** (P(F)): Kompetenz sinkt ohne Nutzung. Ohne sie überschätzt das Modell
   Dauerbeherrschung.
3. **Konzept-Abhängigkeiten**: BKT nimmt unabhängige Skills an [4](https://theneuralbase.com/ai-for-education/learn/intermediate/bayesian-knowledge-tracing/) —
   bei Code ist das falsch (`Iterator` setzt `Closure` voraus). Lösung: ein **Prärequisiten-Graph**;
   Mastery-Bestätigung eines Kindes erhöht P(L) des Eltern-Konzepts leicht.
4. **Unsicherheit anzeigen**: „Ich schätze `?`-Propagierung bei **72 %** — 4 Beobachtungen."
   Ehrlichkeit ist hier Features, nicht Schwäche.

**Realistische Erwartung:** BKT trifft die „nächste Fertigkeit" zu ~70–85 %, *wenn* die Skills
sauber geschnitten sind [4](https://theneuralbase.com/ai-for-education/learn/intermediate/bayesian-knowledge-tracing/).
**Die Konzept-Registry ist die eigentliche Arbeit** — nicht der Algorithmus.

### 3.2 Was wir nicht tun: Lerntypen diagnostizieren
Matching-Hypothese über vier Meta-Analysen: **d = 0,04** — praktisch null [2](https://carlhendrick.substack.com/p/the-learning-styles-illusion-debunking).
Noch schlimmer: Etikettierung senkt Erwartungen („kinästhetisch" wird zum Code für
„weniger fähig") [2](https://carlhendrick.substack.com/p/the-learning-styles-illusion-debunking).
→ **Kein „Welcher Lerntyp bist du?"-Dialog.**

### 3.3 Die Kalibrierungs-Falle (und warum das Gate eine Therapie ist)
Die **Illusion of Explanatory Depth**: Menschen überschätzen ihr Verständnis kausaler Systeme
deutlich, und erst der Versuch, es Schritt für Schritt zu erklären, kalibriert sie nach
unten [2](https://www.sciencedirect.com/science/article/abs/pii/S0022096503001590).
Bemerkenswert: schon das Erklären eines *anderen* Phänomens reduziert die Selbstüberschätzung
bereichsübergreifend [4](https://www.cambridge.org/core/journals/judgment-and-decision-making/article/broad-effects-of-shallow-understanding-explaining-an-unrelated-phenomenon-exposes-the-illusion-of-explanatory-depth/9B9B8927C3E530EBCF0453504730E3F3).

> **Das heißt für true-code:** Das Comprehension Gate ist nicht in erster Linie Messung.
> **Es ist die wirksamste einzelne Lernintervention, die wir haben** — weil es die
> Selbstüberschätzung zerstört, bevor sie sich als Code-Fehler materialisiert.
> Deshalb: „Weiß ich nicht" ist ein **erfolgreiches** Ergebnis, kein Fehler.

---

## TEIL 4 — Die acht Wirksamkeits-Mechaniken

| Mechanik | Effekt (Evidenz) | In der TUI | Aufwand |
|---|---|---|---|
| **1. Retrieval Practice** | Spaced vs. massiert **g = 0,74** [1](https://eric.ed.gov/?id=EJ1310148) | 60-Sek-Check beim Session-Start | **klein** |
| **2. Guidance Fading** | Worked Example → Completion → Problem; Expertise-Reversal ist gut belegt [7](https://www.uky.edu/~gmswan3/EDC608/Kalyuga2007_Article_ExpertiseReversalEffectAndItsI.pdf) | Diff mit **Lücke** statt ganzer Lösung | mittel |
| **3. Tracing / Vorhersage** | Trace-basierte Lehre: bessere Noten, weniger Abbrecher; Tracing → Schreibfähigkeit (Semantik) [3](https://www.researchgate.net/publication/332693327_Teaching_computer_programming_with_PRIMM_a_sociocultural_perspective) | „Was gibt Zeile 23 aus?" | **klein** |
| **4. Parsons-Probleme** | Niedrige kognitive Last, sensitiver als Schreibaufgaben; **Subgoal-Labels: gegeben > selbst generiert > keine** [5](https://dl.acm.org/doi/pdf/10.1145/2839509.2844617) | Liste mit `J/K` verschieben | mittel |
| **5. Elaboration / Self-Explanation** | Erklären fördert Schemaaufbau und kalibriert (IOED) | Explain-back, 1–2 Sätze | **klein** |
| **6. Subgoal Labels** | Reduziert Durchfall- und Abbruchquote in CS1, wirkt sprach- und formatübergreifend [2](https://computinged.wordpress.com/2020/06/29/subgoal-labelling-influences-student-success-and-retention-in-cs/) | Kommentarzeilen im Diff | **klein** |
| **7. Kontrast / Was-wäre-wenn** | Stört oberflächliches Pattern-Matching, erzwingt semantisches Denken [1](https://lacuna.tiptreesystems.com/direction/decoupling-code-generation-from-program-comprehension/txn_8cdd04c4065e4236a67f22d6515eb6a3) | „Was passiert mit `unwrap()`?" | klein |
| **8. Debug-Jagd** | Debugging hat neural unterscheidbare Phasen: Aufgabenverständnis → Fehlerlokalisierung → Editieren → Kompilieren → Output-Verständnis [2](https://web.eecs.umich.edu/~weimerw/p/weimer-tse2024-debugging.pdf) | Agent injiziert Bug, Nutzer lokalisiert | mittel |

**Nicht übernehmen (obwohl populär):**
* **Ferntransfer-Training.** Selten, schwer, domänengebunden [3](https://www.aei.org/research-products/report/the-problem-with-transferable-skills/).
  → Baue auf **Nahttransfer**: dasselbe Konzept in *anderem* Code, nicht in anderer Domäne.
* **Gamification.** Siehe Gebot 10.

---

## TEIL 5 — Was das Terminal erzwingt — und was es schenkt

### 5.1 Vier Effekte der kognitiven Lastentheorie, die im Terminal anders wirken

| CLT-Effekt | Konsequenz für die TUI |
|---|---|
| **Split-Attention** (getrennte Quellen müssen mental integriert werden) | 🚫 **Keine Erklärung in einem Seitenpane!** Erklärung **inline** in den Code schreiben — als annotierte Zeilen direkt über/unter der betroffenen Zeile. Das ist im Terminal sogar leichter als in einer GUI |
| **Transient Information** (wegscrollende Info erhöht Last) | Erklärungen müssen **persistent** sein: Scrollback, `/note`, automatisch in `.truecode/notes/` geschrieben. Streaming-Erklärung = Lernmühe zum Fenster raus |
| **Redundanz-Effekt** (doppelte Info kostet Last) | Nicht den Diff in der Erklärung wiederholen. Erklärung = *Warum* + *Konzept* + *Alternative*. Nicht: *Was* |
| **Modality-Effekt** (Audio + Visual > nur Visual) | In der TUI nicht nutzbar — **aber**: der *Expertise-Reversal* gilt auch hier. Für Fortgeschrittene ist erklärender Text **redundant und schädlich**. Der Verstehens-Regler muss deshalb textarm werden können |

### 5.2 Was das Terminal *schenkt*
* **Code ist Text.** Kein Medium passt besser zum Lernen von Code als ein Text-Terminal.
  Kein Diagramm-Overhead, kein Kontextwechsel.
* **Tastaturgetriebene Mikro-Interaktion.** Ein Parsons-Problem mit `J/K` zu lösen kostet
  Sekunden — die *Hürde zum Abruf* ist minimal, was die Compliance massiv erhöht.
* **Niedrige Kosten der Unterbrechung.** Ein 20-Sekunden-Check in der TUI fühlt sich nicht nach
  „Lern-App" an, sondern nach einem kurzen Innehalten.

### 5.3 Sechs Interaktionsmuster (konkret skizziert)

**① Completion — der Königsmodus** (Fading, direkt am echten Diff)
```
┌ src/auth.rs:42 ────────────────────────────────────────────────────────────┐
│  SUBGOAL: Fehler nach oben durchreichen, statt zu paniken                   │
│                                                                             │
│  41  pub fn load(id: &str) -> Result<User> {                                │
│  42      let row = db.find(id)░░░░░░░░░░░░░░░░░░░░░░;                        │
│  43      Ok(User::from(row))                                                │
│  44  }                                                                      │
│                                                                             │
│  Fülle die Lücke:  › ?                                                      │
│  [Tab] Hinweis (Stufe 1)   [Enter] prüfen   [w]eiß nicht                    │
└─────────────────────────────────────────────────────────────────────────────┘
```
Warum Königsmodus: Es ist **Completion-Problem + Fading + Subgoal Label** in einer Interaktion —
drei der am besten belegten Effekte, und es kostet den Nutzer **fünf Sekunden**.

**② Trace-Vorhersage**
```
  17  let names: Vec<_> = users.iter().map(|u| &u.name).collect();
  18  println!("{}", names.len());
  › Wenn `users` leer ist, Ausgabe: ___
```

**③ Parsons (Ordnen)** — mit Subgoal-Labels als Kommentare, `J/K` zum Verschieben.

**④ Explain-Back (das Gate)** — 1–2 Sätze, Bewertung gegen 2–3 Erwartungspunkte.

**⑤ Kontrast** — „Was ändert sich, wenn wir `?` durch `.unwrap_or_default()` ersetzen?"
(Genau die Manipulation, die in der Forschung oberflächliches Pattern-Matching aufdeckt.)

**⑥ Debug-Jagd** — Agent injiziert/reaktiviert einen Bug; Nutzer nennt **erst die Datei+Zeile**,
dann den Fix. Trainiert Fehlerlokalisierung getrennt vom Reparieren [2](https://web.eecs.umich.edu/~weimerw/p/weimer-tse2024-debugging.pdf).

### 5.4 Wann unterbrechen? (Die Interrupt-Ökonomie)
```
❌ Mitten im Tool-Loop           → nie
❌ Bei jedem Datei-Edit          → nie
❌ Beim ersten Anzeichen von Unsicherheit des Nutzers → nie
✅ Nach dem Diff, vor dem Annehmen       → Explain + Completion
✅ Nach grünen Tests                     → Gate („warum funktioniert es jetzt?")
✅ Beim Session-Ende                     → 20-Sek-Rückblick (optional)
✅ Beim Session-Start (1/3/7/21/60 Tage) → Spaced Retrieval, max. 60 Sekunden
✅ Wenn der Nutzer `?` eintippt           → sofort, auf Abruf
```
**Budget:** max. **2 Gates pro Session**, max. **3 Minuten** Zusatzzeit. Danach ist Schluss —
die Uhr ist sichtbar. Das ist der Unterschied zwischen „hilfreich" und „nervig".

---

## TEIL 6 — „Jeder lernt anders" — was *wirklich* differenziert

Die ehrliche Antwort: **Lerntypen sind Quatsch, aber Lern-*Voraussetzungen* sind sehr real.**
Was du personalisieren kannst und solltest:

| Dimension | Wie messen | Wie anpassen |
|---|---|---|
| **Vorwissen** ⭐ | Kaltstart + BKT + Repo-Analyse | **Der wichtigste Hebel.** Steuert Tiefe, Tempo, Fading-Geschwindigkeit |
| **Kognitive Last (Element-Interaktivität)** | Messbar über Antwortzeit + Hinweis-Nutzung + Fehlerrate | Bei Überlast: Konzept **isolieren** (erklären ohne Code), dann wieder zusammenfügen |
| **Ziel / Motivation** | Eine Frage beim Start, editierbar | Bestimmt die **Auswahl** der Lernziele (Relevanz-Faktor) |
| **Zeitbudget** | Verstehens-Regler + Nutzungsmuster | Bestimmt die **Dosis** (0–2 Gates) |
| **Selbstregulation** | Nutzt der Nutzer Hinweise, bevor er's versucht? Nutzt er `/note`? | Bei schwacher Selbstregulation: mehr Struktur (Subgoals, Checklisten); bei starker: nur Ziele, kein Drill |
| **Format-Präferenz** | direkt abfragbar, **ohne** Lerntyp-Behauptung | „Ich tippe ungern lang" → Parsons/Choice statt Freitext. **Präferenz ehren, ohne Wirkung zu behaupten** |
| ~~Lerntyp (visuell/auditiv)~~ | — | 🚫 **Nicht erheben.** d = 0,04; Etikettierung schadet [2](https://carlhendrick.substack.com/p/the-learning-styles-illusion-debunking) |

**Und die TUI-spezifische Ausnahme:** Ja, manche Menschen lesen gern Diagramme. Im Terminal geht
das nicht. Aber das ist kein Lerntyp-Problem, sondern eine **Medien-Entscheidung** — und die
Antwort heißt: **ASCII-Diagramme und Tabellen**, nicht farbige Graphen. Box-Drawing-Zeichen,
Einrückung und Farbe tragen im Terminal erstaunlich weit.

**Der Regler als Personalisierung:** Stufe 0–3 aus PLAN-v0.2 ist *exakt* die
Expertise-Reversal-Mechanik — nur **nutzerkontrolliert statt systemkontrolliert**. Das vereint
zwei Dinge, die sich sonst widersprechen: wissenschaftlich korrektes Fading **und** Autonomie.

---

## TEIL 7 — Motivation ohne Mumpitz

**Selbstbestimmungstheorie:** intrinsische Motivation entsteht aus **Autonomie, Kompetenzerleben,
sozialer Eingebundenheit** [1](https://journals.librarypublishing.arizona.edu/itlt/article/id/4872/print/).

**Was daraus folgt:**
* **Autonomie:** Der Nutzer kann *jedes* Lernziel ablehnen — mit einem Tastendruck, ohne
  Rechtfertigung. Der Regler ist sichtbar und jederzeit verstellbar. Kein versteckter Zwang.
* **Kompetenzerleben:** Zeige **echte Evidenz**, kein geschöntes Fortschrittsbalken-Gefühl.
  „Du hast `Result`-Propagierung diese Woche **3× selbstständig** angewendet" ist stärker als
  jeder Fortschrittsbalken, weil es wahr ist.
* **Gamification: nein.** Hanus & Fox fanden *gesunkene* intrinsische Motivation, Zufriedenheit
  **und** Prüfungsleistung unter Punkten/Badges/Bestenlisten [1](https://journals.librarypublishing.arizona.edu/itlt/article/id/4872/print/).
  Mekler et al.: Punkte/Level/Boards wirken als **extrinsische Anreize für Leistungsmenge**, nicht
  für intrinsische Motivation [2](https://www.ntnu.edu/documents/139799/1279149990/04+Article+Final_camildah_fors%C3%B8k_2017-12-06-13-53-55_TPD4505.Camilla.Dahlstr%C3%B8m.pdf).
* **Stattdessen: Verstehens-Bilanz.** Eine ehrliche, nüchterne Übersicht — inklusive der Dinge,
  die du *nicht* verstanden hast. Nichts motiviert einen Entwickler mehr als eine sichtbare,
  schließbare Lücke.

---

## TEIL 8 — Die Anti-Pattern (wie KI das Lernen gerade kaputt macht)

| Anti-Pattern | Mechanismus | Gegenmaßnahme in true-code |
|---|---|---|
| **Helpfulness Bias** | Der Agent antwortet sofort und vollständig → kein Abruf, keine Elaboration | **Versuch-zuerst-Regel** (Stufe 2/3): der Agent gibt keinen Lösungsweg, bevor der Nutzer einen Versuch formuliert hat |
| **Metacognitive Laziness** | Nutzer verlagern das *Denken*, nicht nur das Tippen [3](https://www.sciencedirect.com/science/article/pii/S2666920X25001699) | Gates + AFA-Messung: wer nie selbst anwendet, sieht es in der Bilanz |
| **Illusion of Competence** | Flüssig lesbare Erklärung fühlt sich an wie Verstehen [6](https://doi.org/10.3390/educsci15111502) | Tracing/Parsons/Completion statt noch mehr Erklärtext |
| **Über-Scaffolding** | Zu viel Hilfe wird für Fortgeschrittene **schädlich** (Expertise-Reversal) | Der Regler; automatischer Vorschlag „runterstufen" bei hoher Trefferquote |
| **Transiente Erklärung** | Erklärung streamt vorbei, ist weg, wirkt nicht nach | Persistenz: `/note`, Scrollback, `.truecode/notes/` |
| **Antwortmaschine statt Tutor** | Meta-Analyse: Wirkung am stärksten bei **Tutor-Rolle + dauerhafter Nutzung** [1](https://arxiv.org/html/2509.22725v1) | Dauerhaftes Konzept-Gedächtnis über Sessions, nicht punktuelle Hilfe |
| **Delegiertes Verstehen** | „Mach, dass es funktioniert" ≠ „Ich weiß, was passieren muss; schreib es" [3](https://var0.xyz/posts/beyond-recall-and-the-illusion-of-competence.html) | Explain-before-Apply **vor** dem Anwenden, nicht danach |

**Die eine Design-Regel, die alles zusammenfasst:**
> **Der Agent darf nie etwas erklären, was der Nutzer nicht gerade selbst versucht hat —
> und nie etwas tun, was der Nutzer gerade lernen soll.**

---

## TEIL 9 — Wie wir beweisen, dass es wirkt

Ohne Messung ist alles Behauptung. Drei Ebenen:

**Ebene 1 — Prozessmetrik (automatisch, immer)**
* AFA-Rate pro Konzept, BKT-Schätzung mit Unsicherheit, Gate-Trefferquote, Antwortzeit.

**Ebene 2 — Retention-Test (nach 7 / 21 / 60 Tagen)**
* 3 Fragen beim Session-Start, opt-in, < 60 Sekunden.
* Erwarteter Effekt: Spacing sollte die Retention deutlich über die Massiert-Lernen-Baseline
  heben (g ≈ 0,74 in der Literatur — bei uns realistisch kleiner, aber messbar).

**Ebene 3 — Transfer-Test (der ehrliche Härtetest)**
* Nahtransfer: dasselbe Konzept in **anderem** Code (anderes Modul, andere Datei).
* **Ferntransfer nicht versprechen** — die Evidenz sagt, dass er selten ist [3](https://www.aei.org/research-products/report/the-problem-with-transferable-skills/).

**Realistische Erwartungskalibrierung:** Blooms „2 Sigma" ist überzeichnet; moderne Meta-Analysen
finden für Tutoring typischerweise **0,33–0,37 SD**, für Mastery Learning ~0,48 (0,76 für
Blooms spezifisches LFM-Design), und bei standardisierten (statt experimentell erstellten) Tests
schrumpft der Effekt dramatisch [4](https://www.educationnext.org/two-sigma-tutoring-separating-science-fiction-from-science-fact/) [1](https://nintil.com/bloom-sigma/).
→ **Versprich 0,3–0,5 SD. Nicht mehr.** Und: „Experimenter-made tests" zeigen größere Effekte als
unabhängige — rechne deine eigenen Zahlen konservativ.

---

## TEIL 10 — Bauplan für `tc-learn`

### Phasen
| Phase | Inhalt | DoD |
|---|---|---|
| **L0** | Konzept-Registry (versioniert, ~150 Rust/Python-Konzepte mit Prärequisiten-Graph) + tree-sitter-Extraktion | Ich sehe pro Diff, welche Konzepte berührt werden |
| **L1** | Relevanz-Score + ZPD-Filter + **Subgoal Labels** + Explain-before-Apply | Der Diff kommt mit WAS/WARUM/KONZEPT/ALTERNATIVE |
| **L2** | **Completion-Modus** (Lücke im Diff) + Trace-Vorhersage + Kaltstart (3 Aufgaben) | Ich kann eine Lücke in einem echten Diff füllen |
| **L3** | BKT-Engine (mit Guess/Slip/Vergessen/Prärequisiten) + AFA-Tracking + Spaced Re-Checks | `true-code concepts` zeigt Schätzungen mit Unsicherheit |
| **L4** | Parsons + Kontrast + Debug-Jagd + Verstehens-Bilanz + `/note` | Alle sechs Interaktionsmuster verfügbar |

### Datenmodell (Skizze)
```rust
pub struct Concept {                       // Registry, versioniert
    pub id: ConceptId,                     // "rust/result-propagation"
    pub prereqs: Vec<ConceptId>,
    pub kind: ConceptKind,                 // Concept | Procedure | MentalModel | Strategy
    pub detectors: Vec<Detector>,          // tree-sitter-Query + Heuristik
    pub subgoal_label: String,             // "Fehler nach oben durchreichen"
}

pub struct Mastery {
    pub p_known: f32,                      // BKT-Posterior
    pub observations: u32,
    pub guess: f32, pub slip: f32,        // pro Konzept kalibriert
    pub last_seen: DateTime<Utc>,
    pub afa: f32,                          // assistanzfreie Anwendungsrate
    pub level: Level,                      // Seen|Explained|Applied|Verified|Mastered
}

pub enum Evidence {
    UnaidedUse    { concept, event },      // Gewicht 0,45  ← stärkstes Signal
    RetrievalOk   { concept, latency_ms }, // 0,30
    ExplainedWell { concept, rubric },     // 0,20
    SelfReport    { concept },             // 0,05
    Forgot        { concept },             // gesonderter Vergessens-Pfad
}
```

### Die eine Sache, die du *nicht* outsourcen kannst
Die **Konzept-Registry**. BKT-Algorithmen sind 200 Zeilen. Eine gute, gepflegte Registry aus
150–300 Konzepten mit Prärequisiten-Graphen und tree-sitter-Detektoren ist **Monate an Arbeit —
und genau dein Burggraben.** Sie ist der einzige Teil von `tc-learn`, den ein Konkurrent nicht an
einem Wochenende nachbauen kann.

---

## Quellen

**Kognitive Lastentheorie / Instruktionsdesign**
1. Cognitive Load Theory Teaching Strategies: [teachermagazine.com](https://www.teachermagazine.com/au_en/articles/cognitive-load-theory-teaching-strategies)
7. Expertise Reversal Effect (Kalyuga et al.): [uky.edu PDF](https://www.uky.edu/~gmswan3/EDC608/Kalyuga2007_Article_ExpertiseReversalEffectAndItsI.pdf)
10. Cognitive Architecture and Instructional Design (Sweller 2019): [leADinglearner PDF](https://leadinglearner.me/wp-content/uploads/2019/02/sweller2019_article_cognitivearchitectureandinstru.pdf)
4. Expertise Reversal — Adaptive Fading: [Wikipedia](https://en.wikipedia.org/wiki/Expertise_reversal_effect)
9. Expertise Reversal Special Issue: [Springer](https://link.springer.com/article/10.1007/s11251-009-9102-0)

**Informatikdidaktik**
5. Subgoals Help Students Solve Parsons Problems (SIGCSE'16): [ACM PDF](https://dl.acm.org/doi/pdf/10.1145/2839509.2844617)
1. Subgoals + Parsons (ResearchGate): [researchgate.net](https://www.researchgate.net/publication/311489054_Subgoals_Help_Students_Solve_Parsons_Problems)
2. Subgoal Labelling reduziert Abbruchquoten: [computinged.wordpress.com](https://computinged.wordpress.com/2020/06/29/subgoal-labelling-influences-student-success-and-retention-in-cs/)
3. PRIMM / Code Tracing → Schreibfähigkeit: [researchgate.net](https://www.researchgate.net/publication/332693327_Teaching_computer_programming_with_PRIMM_a_sociocultural_perspective)
5. Notional Machine (Berry & Kölling, ITICSE'14): [kent.ac.uk PDF](https://kar.kent.ac.uk/43795/1/ITICSE-14.pdf)
2. Cognitive Model of Dynamic Debugging (fNIRS-Studie): [umich PDF](https://web.eecs.umich.edu/~weimerw/p/weimer-tse2024-debugging.pdf)

**Gedächtnis / Abruf / Transfer**
1. Spaced Retrieval Meta-Analyse (g = 0,74; expanding ≈ uniform): [ERIC](https://eric.ed.gov/?id=EJ1310148) · [Springer](https://link.springer.com/article/10.1007/s10648-020-09572-8)
5. Spacing & Retrieval in Mathematik (g = 0,26 / 0,22): [researchgate.net](https://www.researchgate.net/publication/394099339_A_Meta-analytic_Review_of_the_Effectiveness_of_Spacing_and_Retrieval_Practice_for_Mathematics_Learning)
1. The trouble with transfer (Willingham, Barnett & Ceci): [learningspy.co.uk](https://learningspy.co.uk/learning/trouble-transfer-can-make-learning-flexible/)
3. The Problem with Transferable Skills (AEI): [aei.org](https://www.aei.org/research-products/report/the-problem-with-transferable-skills/)

**Metakognition / Kalibrierung**
1./2. Illusion of Explanatory Depth (Rozenblit & Keil): [grokipedia](https://grokipedia.com/page/Illusion_of_explanatory_depth) · [ScienceDirect](https://www.sciencedirect.com/science/article/abs/pii/S0022096503001590)
4. Explaining an unrelated phenomenon exposes IOED: [Cambridge JDM](https://www.cambridge.org/core/journals/judgment-and-decision-making/article/broad-effects-of-shallow-understanding-explaining-an-unrelated-phenomenon-exposes-the-illusion-of-explanatory-depth/9B9B8927C3E530EBCF0453504730E3F3)
3. Beyond recall and the illusion of competence (Debugging & mentale Modelle): [var0.xyz](https://var0.xyz/posts/beyond-recall-and-the-illusion-of-competence.html)
1. Decoupling Code Generation from Program Comprehension: [lacuna](https://lacuna.tiptreesystems.com/direction/decoupling-code-generation-from-program-comprehension/txn_8cdd04c4065e4236a67f22d6515eb6a3)

**Wissensdiagnostik**
1./3. Bayesian Knowledge Tracing: [emergentmind](https://www.emergentmind.com/topics/bayesian-knowledge-tracing) · [emergentmind BKT](https://www.emergentmind.com/topics/bayesian-knowledge-tracing-bkt)
4. BKT in Produktion (Grenzen, 70–85 %, unabhängige KCs): [theneuralbase](https://theneuralbase.com/ai-for-education/learn/intermediate/bayesian-knowledge-tracing/)
2. Knowledge Tracing als Bayes-Inferenz: [theneuralbase](https://theneuralbase.com/ai-for-education/learn/intermediate/knowledge-tracing/)

**Lernstile / Individualisierung**
2. Learning Styles Illusion (Hattie & O'Leary, d = 0,04): [carlhendrick.substack.com](https://carlhendrick.substack.com/p/the-learning-styles-illusion-debunking)
1. From Styles to Science (Vorwissen als Kernvariable): [gc-bs.org](https://gc-bs.org/articles/from-styles-to-science-debunking-the-learning-styles-myth-and-embracing-an-evidence-based-framework-for-learning/)
4. Learning Styles Myth — was stattdessen wirkt: [structural-learning.com](https://www.structural-learning.com/post/learning-styles-myth-debunked)

**Motivation**
1. Gamification & SDT (Hanus & Fox): [arizona journals](https://journals.librarypublishing.arizona.edu/itlt/article/id/4872/print/)
2. Impacts of Gamification on Intrinsic Motivation (Mekler et al.): [NTNU PDF](https://www.ntnu.edu/documents/139799/1279149990/04+Article+Final_camildah_fors%C3%B8k_2017-12-06-13-53-55_TPD4505.Camilla.Dahlstr%C3%B8m.pdf)

**KI & Lernen**
1. LLM-Meta-Analyse (Tutor-Rolle, Dauer): [arxiv 2509.22725](https://arxiv.org/html/2509.22725v1)
3. Systematic Review LLM in Education (88 Studien, Over-reliance): [ScienceDirect](https://www.sciencedirect.com/science/article/pii/S2666920X25001699)
7. LLM-Feedback wirkt nur bei Engagement (0,10 SD): [arxiv 2506.17006](https://arxiv.org/html/2506.17006v1)
9. AI tutoring outperforms in-class active learning (RCT): [Nature](https://www.nature.com/articles/s41598-025-97652-6)
6. Perceived vs. Actual Effectiveness of LLM Tutors: [MDPI EducSci](https://doi.org/10.3390/educsci15111502)

**Wirksamkeits-Erwartung & Feedback-Timing**
4. Two-Sigma Tutoring — separating fact from fiction: [educationnext.org](https://www.educationnext.org/two-sigma-tutoring-separating-science-fiction-from-science-fact/)
1. Bloom's Two Sigma — systematic review: [nintil.com](https://nintil.com/bloom-sigma/)
3. Bloom's 2 Sigma (Kulik 0,48 / LFM 0,76 / ITS 0,76): [grokipedia](https://grokipedia.com/page/Bloom's_2_sigma_problem)
1./5. Feedback-Timing (sofort bei Prozedur, verzögert bei Konzept; Fenster < 1 min): [tea4teacher](https://www.tea4teacher.com/post/feedback-timing-when-immediate-feedback-helps-and-when-it-doesn-t) · [PMC9995700](https://pmc.ncbi.nlm.nih.gov/articles/PMC9995700/)
2./5. Productive Failure (Kapur — Budget 3–5 Konzepte; When PF fails): [boldscience PDF](https://boldscience.org/wp-content/uploads/2025/04/Productive-Failure.pdf) · [researchgate](https://www.researchgate.net/publication/333005127_When_Productive_Failure_Fails)
5. Productive Failure in Learning Math (konzeptionell > , prozedural =): [IITK PDF](https://www.cse.iitk.ac.in/users/se367/14/Readings/papers/kapur-14_productive-failure-in-learning-math.pdf)

---

*Nächster Schritt: Konzept-Registry für Rust + Python skizzieren (L0) — das Fundament, an dem
alles andere hängt.*
