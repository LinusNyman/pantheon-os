## 19. Studium — the studies lens

Studium is a **lens** (§12): a Porticus app that leads with a mosaic of Tessera tiles, composes *across* cores, owns no primitive, holds no data, and never originates a write (I1, I2). §12 governs the lens contract and every rule there binds here unchanged. This chapter records only what is *Studium's*: the studies domain a study life derives, and the figure that names the lens — the **GPA**.

Its governing move is one reduction: **the record is the unit of study, the folder is only where it sits.** A grade, a credit total, an hour given, a deadline, a professor — each is already some core's record; Studium mints none of it, and its whole substance is the fold that reads them together. What it inherits from the tool it replaces (a decade of course directories, a GPA calculator, a study-time log) it inherits as *readings*, never as a schema to re-impose. Three cores carry the weight — Fasti the enrolment's period, Annales its result, and (through the curriculum file, §19.3) a scale to weigh it — and none of them learns what a "course" is.

### 19.1 The enrolment is a span

An **enrolment** is a Fasti `span` (§8.4): a Partitioned entity, one `.json`, with a `from` and an optionally-open `to`. It is the course's *period* and nothing more — **open while you are taking it, closed when it ends** — and it carries no credits and no grade, because Fasti owns *placement*, not grading (I5). A span references what it concerns (I9): the institution (`refs: ["album:kth"]`, §8.1) and the **programme span** it belongs to (`refs: ["fasti:teknisk_fysik"]`). A programme is itself a span — the multi-year window of a degree, open until it completes — so "declaring a programme" is minting its span, by any hand (I8); the tree is the only enumeration there is, because a programme *is* a period on the timeline and Fasti is where periods live. Courses group under a programme by naming it in `refs`, read from either end, never by a directory nesting them (I3, §5.4).

This is the correction the seed forced on the first draft of this chapter: a span cannot *carry* the grade. Fasti's `Span` is a closed, `deny_unknown_fields` record of `{from, to, note}` (§8.4), and a lens reads only a core's emitted JSON (I4, I5) — so a `credits`/`grade` field stuffed into a span's `data` neither validates nor survives the read. The grade lives elsewhere, and elsewhere is the truer home for it anyway.

### 19.2 The grade is a fact

A grade is not a property of the enrolment entity; it is a **fact you record** when the course ends — the purest expression of I1 — so it is an Annales `log` (§8.6). The fact homes at the course node and is **paired to its span by sharing its slug**: the enrolment `fasti:mekanik` and the grade log `annales:mekanik` sit at the same node, and Studium reads one from the other (a `fasti:<slug>` span, then the `annales:<slug>` log beside it). The two coexist because a reference is always `core:slug` and the cores differ (§5.4) — `fasti:mekanik` and `annales:mekanik` resolve to two files and never collide. A grade log named `grade` at every course node would *not* coexist — an Annales series name is a tree-wide-unique ref target, so nine `annales:grade` logs raise `duplicate_slug` (§18); naming the log for its course is what keeps each one singular.

The log's one reading carries `values: ["A", "7.5"]` — grade then credits — and references its span (`refs: ["fasti:mekanik"]`). Credits ride here rather than on the span because the grade fact is the only record that must exist for a *completed* course; an in-progress course has a span and no fact, and needs no credits figure until it earns one.

**A retake is a second reading, not a second record.** Because the grade is a *series* (§8.6), re-doing a course appends a new keyed line to the same `annales:<course>` log — an Fx in the P2 exam, then an E from the August re-exam, are two readings dated to their two sittings, the log holding the whole attempt history one line per date. This is the distinction §6.1 draws and Studium relies on: a **new attempt** is a new key (a new date, a new reading), while a **correction** — a grade mistyped — is a rewrite of the *existing* keyed line, keeping no second copy. The enrolment span's `to` moves to the sitting that finally passed; the log remembers every attempt that got there. Which reading the GPA honours is §19.4's to settle, not the log's — the log keeps the facts, whole.

### 19.3 The curriculum file

A grade is a symbol; the GPA needs a number, and `A = 5.0` is not a reading of your life but a fact about the school that issued it — and a school you might publish this tree without. It is declared in a per-programme **`[code]_curriculum.toml`**, a file at the programme node that **governs that node and everything under it** (§6.3), exactly as a rule's scope is where its file sits (§9.1). It carries the university, the default scale, the grading scales (each grade's value, which grades pass, whether the scale counts toward the GPA), and the academic calendar (§19.5):

```toml
university    = "kth"
default_scale = "af"
periods_per_year = 5

[scale.af]                       # grade -> GPA value
counts_in_gpa = true
grades  = { A = 5, B = 4, C = 3, D = 2, E = 1, Fx = 0, F = 0 }
passing = ["A", "B", "C", "D", "E"]

[scale.pf]
counts_in_gpa = false
grades  = { P = 0, F = 0 }
passing = ["P"]
```

This is the one place Studium departs from §12's records-native default, and the departure is deliberate: the scale is neither a reading of a life (so no core owns it) nor a knob on any tool's behaviour (so §18's no-config rule does not reach it) — it is **external reference data**, a fact about a university that a grade needs to become a number, and it is homed in the tree at the node it concerns so that a published subtree carries its own scales. §18 records the carve-out and its boundary: a file that tuned behaviour would still be forbidden; this one only supplies a datum. It is not the Album institution's `data` (Album's `Agent` is a typed record that drops unknown fields, §8.1, so a scale set there is invisible across the contract) and it is not hardcoded (a hand studies at more than one university, and publishes to hands at others).

A `curriculum.toml` is not yet a shape the spine classifies — `pan validate` reports it `unclassifiable_file` (a warning: only `[code]__.toml` is a recognised TOML, every other `.toml` falls through where a `.pdf` would land in `Bulk`, §5.2). Teaching `classify` to route a node-prefixed non-annotation `.toml` to `Bulk` closes the warning and is the one spine change this lens asks for; until then the file works and the warning is cosmetic.

### 19.4 The GPA fold

The GPA is the credit-weighted mean of graded enrolments, derived on sight exactly as net worth is (§8.3) and stored nowhere (I1). Over the grade facts in scope:

```
GPA = Σ( value(grade) × credits ) / Σ( credits )
      for each completed course whose scale counts_in_gpa and whose grade is passing
```

Studium resolves each grade's scale by the curriculum file that governs the fact's node (§19.3), and within it **by which scale holds the symbol** — `A` is on `af`, `P` is on `pf` — so a course needs to name no scale of its own. The inclusion rules are the prior tool's, corrected to read the declared scale rather than a hardcoded switch: an **open** enrolment (a span with no grade fact) is in neither sum; a grade on a **non-GPA scale** (a pass/fail `P`) is excluded from the mean though its credits still count toward *completed credits*, a separate figure; a **failing** grade earns no credits and enters neither sum. An empty numerator — no graded course in scope yet — is not `0.0` but **no GPA**, a dash emitted as `null`: the honest answer that the fold ran and found nothing to weigh (§12, the count-versus-null discipline).

**On a retake, the fold takes the best passing grade** — the KTH convention that a re-sit only ever lifts a mark — reading every attempt in the course's log and weighing the highest that passed. That the log keeps all attempts (§19.2) is what makes this a fold rather than a stored decision: no `final_grade` is written, "best" is recomputed on sight, and a curriculum file may name `latest` instead where an institution counts the most recent sitting. This is the figure the prior tool's dead `retake_grade_selection` never actually computed; here it is a line of the fold.

**Scope is node-agnostic, like net worth.** Net worth sums every `balance`-bearing holding wherever it sits (§8.3); the GPA folds every course that has a grade fact, weighing what it finds, and needs no declaration of "which directory is studies." A hand who wants one programme's mean scopes the fold at invocation — `stu -H asd_f_teknisk_fysik`, or `-C` a subtree — the same lever every fold takes (§6.3, §7.3); nothing is stored to remember the choice. In the reference tree such records home under Disciplina (`asd`), but the fold is keyed to the grade fact, not the node.

### 19.5 Terms and periods

The academic calendar is a **term** — a semester the enrolment span sits inside — subdivided into **periods**, and a period is an *optional subspan of a term* (KTH: `ht` = P1,P2 · `vt` = P3,P4 · summer = P5). The curriculum file declares them with year-less anchors; the enrolment span supplies the actual year, and the two fold together into where a course sat:

```toml
terms = [ { slug = "ht", periods = ["P1","P2"] }, { slug = "vt", periods = ["P3","P4"] }, … ]
periods = [ { n = 1, slug = "P1", term = "ht", start = "0826", end = "1025" }, … ]
```

**Period labels run continuously across the programme.** With `periods_per_year` periods to a year, a course's label is `(study_year − 1) × periods_per_year + n`, so a year-2 P1 reads as **P6** — the absolute index a study life counts in. Study year comes from the span's `from` against the programme span's start; the period comes from the span's interval against the anchors.

**A course spans as many periods as its interval covers** — Mekanik's `250114 → 250602` overlaps both P3 and P4 and reads as **P3–P4** — and Studium *derives* that set from the interval rather than storing a period list (I1). A span cannot carry a `periods` field any more than it can carry a grade (§8.4); it does not need to, because the interval already points at every period it overlaps. This is the whole of "a course can point to several periods": the placement is a period, and a period is however much of the timeline the enrolment occupied.

### 19.6 What else a study life derives

The GPA names the lens, but a study life folds from six cores at once, each contributing the records it owns and each absent gracefully when its core is off `PATH` (§12):

- **Credits & progress** — the enrolment spans and their grade facts, summed: credits completed, credits in progress (open spans), a programme's total against its expected length. Mostly a fold over Fasti and Annales.
- **Deadlines & exams** — Fasti `event`s (§8.4). A deadline and an exam sitting are dated occurrences, each optionally referencing its enrolment span; the prior tool's examination/session machinery is this and nothing more — lines on the timeline, folded into a "next 28 days" or dropped onto a Calendar (§11).
- **Tasks** — Pensum (§8.5): the open/done doing at a course node, marked done in place. A task carries no due date (Pensum has none, §8.5); a *dated* obligation is the Fasti event above, which the task may reference.
- **Study time** — Annales (§8.6): time spent is a `log` of what you gave your hours to, one dated line per session. The live timer and weekly aggregates fold from these; the CSV of hours the prior tool kept becomes an Annales series.
- **People** — Album (§8.1): a TA, a professor, a coursemate is a `person`, referenced from a course's records, never copied under it. "Contacts" is a fold over the people a course's records point at.
- **Reflections** — Tabella (§8.7): an after-action review, a course-end retrospective is a Document whose `type` is `reflection`, homed at what it is about. Studium folds their frontmatter and shows the body, and originates none of it.

These are the prior tool's tabs — dashboard, courses, timeline, study, tasks, goals — rebuilt as Porticus views over folded records (§11); a **goal** (a target grade) is an intention, so a Pensum task, not a seventh record shape. What the prior tool answered from a SQLite index beside the tree, the lens answers from a live fold over the tree — no index to rebuild, nothing to reconcile (§18). Its notification daemon is not a lens's to run (I2, §18: no watcher): the Pantheon shape of "remind me the registration window closes" is an **Auspex rule** (§9) proposing a task, and its git and export are the hand's own over an ordinary directory.

### 19.7 The folder is not the schema

A decade of course directories do not agree. One programme files lectures under `_f_föreläsning`, another adds a lab, a project subtree, a `_k_kontrollskrivning`; a course is sometimes a git repo, and a master level is not courses at all but scholarship applications. The prior tool met this with a rigid template; the lens meets it by **not reading the shape at all**. Studium folds records — spans, events, tasks, logs, documents — uniform across every programme because the contract makes them so (I4). The section directories beneath a course are bulk beside the record (§6.5): browsable through `pan` and the raw-file view (§10, §11), never parsed, never required to match a form. Coherence lives in the records, and the file tree is free to stay as varied as a study life made it (I3). A subtree with no enrolment span — an applications level, an unstarted programme — contributes nothing to the GPA and is *correctly* absent, not broken: the lens shows what its readings hold and no more.

### 19.8 Relays

Every write Studium performs is a human's, relayed to the core verb a hand would type (I2, §12), with `-C <root>` and `-y` supplied by Porticus and its confirm owned by the modal (§7.3, §12). The relays the studies domain wants:

- **Mark a task done** — `pen edit <slug> --done` (§7.2), the Atrium relay unchanged.
- **Close an enrolment** — `fas edit <span> --to <date>`: the day the course ended.
- **Record or re-mark a grade** — `ann <course> <grade> <credits> --at <date>`: a fresh reading for a first mark or a retake, the same reading rewritten for a correction (§19.2).
- **Log study time** — `ann <log> --at <date> <hours>` (§8.6).
- **Place an exam or deadline** — `fas add` an event line (§8.4), referencing the enrolment span.

Each relay is available only when its core answers on `PATH` (§12): no Fasti, no enrolment and no GPA; no Annales, no grade and no time log. The action is absent, not broken (P§7). A remove or bulk change opens the Confirm overlay over a `--dry-run` and relays its plan token (§12, §11.1).

### 19.9 The CLI surface

At a terminal the bare `stu` opens the mosaic; down a pipe it emits the figures behind that mosaic as JSON (§7.3, §12), so an LLM reads what a human sees (I8) and a headless build (§14) is the fold without the chrome:

```json
{ "gpa": 4.09, "credits_completed": 60.0, "credits_in_progress": 30.0,
  "open_courses": 4, "study_hours": 128.5, "next_exam": { "date": "260315", "course": "sf1624" } }
```

Each field is a fold, and each obeys the count-versus-null discipline (§12): a core off `PATH` yields `null`, never `0` — an absent Fasti is not a GPA of zero, and no graded course yet is a `gpa` of `null`. Picking one figure out of that object is the caller's, as with any tool declaring no read flags of its own (§8.7). Studium reports its crate and format versions like any app, so `pan doctor` sees it (§5.5, §15.5), and nothing consumes it in turn — no arrow points at a lens (§4). It is a lens like the other two, distinguished only by the domain it folds and the one figure that gives it its name.
