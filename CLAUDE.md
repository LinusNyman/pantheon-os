# CLAUDE.md

Guidance for working in this repo. Read this, then let the spec govern the details.

## What this is

PantheonOS — a suite of terminal tools over one idea: your life modeled as a directory
tree you can read, edit, and reason about by hand, and so can an LLM, and so can a script.
No database, no app. The ontology *is* the filesystem. Rust, shipped as standalone binaries.

## The spec is law

The full specification lives in `docs/src/spec/` (an mdBook, one chapter per file). It is not
background reading — it is the source of truth, and design choices are downstream of it. Before
building or changing any component, read its chapter. Key anchors:

- `01-overview.md`, `02-ontology.md` — what and why.
- `03-invariants.md` — **I1–I9. These are binding law**; every choice traces to one. Cite them.
- `04-architecture.md` — the four layers and the dependency rule (I5).
- `05-spine.md` — Pantheon: addressing, resolution, the record envelope, `pan` CLI.
- `06-storage.md` — the three storage shapes on disk, the record lock (§6.4).
- `07-core-contract.md` — **the `Core` trait, the twelve verbs, the shared grammar.** The most
  load-bearing chapter for building a core.
- `08-cores.md` — each core's primitive, tokens, and record shape.
- `14-workspace-layout.md`, `15-publishing.md` — crate layout and release mechanics.
- `16-build-order.md` — the dependency-ordered build sequence (see BUILD-PLAN.md for how to execute it).
- `10-pan-tui.md`, `11-ui-layer.md`, `12-lenses.md` — the screen layer: `pan`'s two tabs, the
  Porticus/Tessera split, and what makes a lens a lens.
- `18-non-goals.md` — **what must NOT be built.** Read early; it fences the design.

`docs/src/APPENDIX-A-NAMES.md` explains the Latin naming.

**`docs/src/PORTICUS-SPEC.md` is the chrome's own spec, cited `P§n`** — the app/view model, the
view catalog, the keymap tiers, the theme, the instrument registry. §11.1 defers to it and every
Porticus decision traces to it, so read it before touching `porticus`.

**It is deliberately untracked** (`.gitignore`: `/docs/src/*-SPEC.md`, "private design docs"), so
it is **not in a fresh clone** — it lives only on the author's machine. Two consequences worth
knowing rather than rediscovering: `docs/src/SUMMARY.md` links it, so `mdbook build` on a clean
checkout meets a missing chapter (nothing in CI builds the book today); and a session working
from a clone cannot read it, so cite `P§n` from what the tracked chapters say and ask rather than
guess at the rest.

## The invariants that bite most often

- **I4 — one contract.** A core's CLI JSON is the *only* thing that crosses a component boundary.
- **I5 — references, not dependencies.** Hub-and-spoke: everything links `pantheon`, nothing points
  sideways. No core imports or reads another core — reach across cores is a *lens*'s alone, at runtime
  over PATH-discovered JSON. If you find yourself adding `album` as a dep of another core, stop.
- **I3 — the path is the home.** A record's home/core/kind/slug are its file's location and name,
  never stored in the record. Don't add a `home` field.
- **I1 — samples in, present out.** No `current_*` fields; the present is always derived from readings.
- **I8 — three hands (human, LLM, code).** Same files, same JSON, same validation for all three.
  The tie-breaker when other choices are balanced.

## Architecture (see §4)

Four layers, all over the spine:

- **Packages (libs):** `pantheon` (spine), `porticus` (TUI chrome over ratatui), `tessera` (tiles over
  ratatui-core). Porticus and Tessera are peers — neither depends on the other.
- **Cores (CLI+TUI, JSON contract):** `album` people, `mappa` places, `rationes` holdings, `fasti`
  placement, `pensum` intention, `annales` fact, `tabella` documents.
- **System tools:** `pan` (structural: tree/validate/annotate), `auspex` (the one reactive writer, I2).
- **Lenses (TUIs, read + relay writes):** `speculum`, `atrium`, `studium`.

Every tool has a three-char short and is both a CLI emitting JSON and a Porticus TUI. **A bare
short opens the screen at a TTY and emits `help` as JSON down a pipe** (§7.3) — a screen has
nothing to draw down a pipe. A crate built `--no-default-features` has no screen to open, so its
bare short prints a banner pointing at `--help` instead; that is the only path the old
CLI-first behaviour survives on.

## Workspace layout

Single public Cargo workspace (monorepo forced by I5). Members: `crates/*` and `xtask`.

- `crates/pantheon` — the spine lib (~5.7k lines; nearly all the logic). `crates/pan` — the tool
  over it, its own crate, with `cli.rs` and `screen.rs` of its own.
- `crates/porticus` (~4.2k lines), `crates/tessera` — the peer libs. Porticus links `ratatui` whole;
  Tessera takes `ratatui-core` and links no Porticus, now or ever.
- **Every instrument is a lib with a five-line bin over it** — `main.rs` is §14's "~30-line clap
  shell" and holds nothing but `fn main() { <crate>::run_cli() }`. Four files:
  `lib.rs` holds the record struct(s), `impl Core`, and the module declarations;
  `cli.rs` (785–1742) holds the clap `Cli`, the twelve verbs, `Ctx`, the editor form, and the tail
  helpers; `screen.rs` (208–395, behind `tui`) holds `impl App` and the folds its views close over.
  Put verb logic in `cli.rs` — the spine already owns everything a core would otherwise share.
  **A two-shape core sits at the top of every range** (Fasti, then Rationes): a `Record` enum, a
  shape question in front of each verb, and two token vocabularies to refuse across.
- **Why the lib and not the bin: an integration test links the lib.** A screen in the bin is a
  screen no test can reach, and step 6 proved that gap expensive. **What it must not cost is I4** —
  a verb reachable as a Rust function would be a second door into a core — so each lib exposes
  exactly two things, `run_cli` and its `App`, while `Cli`, `run`, and every verb stay `pub(crate)`.
  Keep it that way: the JSON is the only contract, and this is the one place the type system now
  carries that rather than the crate layout.
- `xtask/` — workspace automation (run via `cargo xtask`).
- `docs/` — the mdBook spec. `deny.toml`, `dist-workspace.toml`, `release-plz.toml` — supply chain & release.

## Status — build order steps 1–10 done (§16); step 11 (releases) deferred indefinitely

**Built and green:** `pantheon` + `pan` (step 1), `annales` (2), `album` (3), `pensum` (4),
`tabella` (5), `porticus` + `tessera` + `atrium` (6), `mappa` + `rationes` + `fasti` (7),
`auspex` (8) — the reactive loop now closes: a write wakes it, a rule proposes, and Auspex
applies through the core CLIs. **Step 9 (cleanups)** paid the deferrals — pan's node cascade
(§10.1), the add form and the chrome debts (P§4–P§7), validate's candidate fixes (§10.2), the
nested-`data` render — all landed. **Step 10 (lenses)** built the last two, `speculum` (review
across horizons) and `studium` (the studies lens, §19), so **every instrument now exists**:
twelve binaries, all `pan doctor`-visible.
**All three storage shapes exist** — Partitioned, Series in *both* its hand-named and nameless
forms, and Document — plus the `core:slug` resolver, the record-level rename cascade, and the
record lock under contention.

**The vertical slice closed at step 6**, which is what it was for: a real screen renders
derived-out (I1) and relays a human write back through a core (I2, §12) — `d` on an Atrium row
runs `pen edit … --done -y` and `pen list` reads it back from another process. **Every instrument
now has a TUI** — the nine of steps 6–7 (`pan`, `atr`, `alb`, `ann`, `pen`, `tab`, `map`, `rat`,
`fas`), plus `aus`'s rules browser (8) and the two lens mosaics `spe`/`stu` (10); the table
renderer fills §7.3's "TTY → table"; `cargo xtask seed` mints a tree to look at.

**Step 7 built the three cores the slice did not need**, against a contract a screen had already
exercised — and they were built in parallel git worktrees off one `main`, each touching only its
own crate plus one line of `Cargo.lock`. That worked *because* of I5: three cores that cannot
import each other cannot conflict either. It is the cheapest confirmation of hub-and-spoke the
repo has produced, and worth repeating for any future fan-out.

Step 7 also added the **two-shape core** as a settled pattern (Rationes `holding`/`balance`,
Fasti `span`/`event`): a `#[serde(untagged)]` `Record` enum with `deny_unknown_fields` on both
variants — a *dispatch type, not a disk format*, since the filename already names the variant
(§5.2, §7.1) and §18 forbids writing a tag. **Two tokens alone do not earn an enum**: Mappa's
`location`/`region` are one storage shape, so it keeps one flat struct, and an enum there would
have turned `edit -k` into a record transformation when §7.2 says it is a file rename.

**Still stubbed** — one not-implemented line: `pan migrate` (`Cmd::Migrate`, §5.5). It was
*blocked, not deferred*: shape-directed and idempotent, it can only be built once a *prior*
format version exists to rewrite from. **Wave 10 unblocked it** — the date width moved and
`format_version` is now `2`, so there is a 1→2 rewrite for it to do, and `cargo xtask
migrate-dates` is the one-off standing in for it. Building it is now a choice, not a wait.
**Step 8 (`auspex`) is done**, landed in four parts against §16's own "`plan` before `run`"
sequencing: the hook (below), the **read half** (discovery, the header, `ls`/`version`/`help`,
the browser screen), the **propose protocol** (`plan`/`test`), and **apply** (`run` — the
capability check, the dedupe, and the writes, §9.5).

**Step 11 (releases) is deferred indefinitely — a deliberate hold, not a debt.** The OS is
feature-complete across all four layers; the current phase is **user-testing each app and
improving it** before anything is tagged and published. So: do **not** start `release-plz`,
per-crate tags, or a `dist` run until the improvement phase closes and a release is *explicitly*
asked for, and `pan migrate` stays blocked until then (it needs that first version boundary).
Treat incoming work as **per-app improvement requests**, scoped to one instrument at a time — not
build-order steps. The fan-out is done; the shape now is iterate-on-one-app.

**What steps 9–10 settled that a later change must not undo:**
- **The lenses follow Atrium's shape** (lib-with-a-five-line-bin, the I4 guard, the feature split
  that drops `porticus`/`ratatui` under `--no-default-features` while `tessera` stays). `speculum`
  is a horizon dashboard (day→week→month→year across every core); `studium` folds the **GPA**.
- **§19 is law and it *replaced* §12's one-line sketch.** The original §12 said a Studium enrolment
  is a Fasti span "whose fields carry credits and grade" — **unbuildable**, since Fasti's `Span` is
  `deny_unknown_fields {from,to,note}`. The settled design (`docs/src/spec/19-studium.md`): grade is
  an **Annales `log` fact** paired to its span by shared slug; the grading **scale lives in a
  per-programme `[code]_curriculum.toml`** — a deliberate **§18 carve-out** (reference data a lens
  reads, never a behaviour knob). GPA = credit-weighted fold, best-passing on retake, null-not-zero.
  **If you touch grading, read §19, not §12.**
- **`classify_toml` routes a non-annotation `.toml` to `Bulk`** (not `unclassifiable_file`), so a
  `curriculum.toml` — or a project's `Cargo.toml` (§6.5) — validates clean; only an
  annotation-shaped stem (`[code]__`) with a bad code stays flagged.
- **Every app's `version -f json` MUST spell the key `format_version`, never `format`.** `pan
  doctor` reads `format_version` to check the suite agrees (§15.5); an app spelling it `format`
  parses as absent and is *silently skipped* by the agreement fold — `agreed` stays true while the
  app never contributes. `atr` and `spe` both carried this bug (atrium from step 6, speculum copied
  it); fixed, and `doctor.rs` now asserts **every seen app also contributes a format_version**, so
  the next misspelling fails CI instead of vanishing. Copy `studium`'s `version_json`, not the older
  ones, for any new app.

**`aus` is `pan`-shaped and `pan doctor` sees it** — it emits `version -f json` with
`format_version: 2`, so it reads as installed. Three things about its shape a later change must not
undo:

- **`aus` is `pan`-shaped, not a core's shape**: its own structural verbs, **no `schema`** (it
  owns no records, and `pantheon::schema::<C>` is bounded on `Core` so it is not even callable),
  and **no `Ctx`** — a core's `Ctx` exists to hold a `Store`, and Auspex holds none. It must also
  stay out of `KNOWN_CORE_SHORTS`, which is the file→core token map.
- **No argv pre-pass.** §13: "`pan`, `aus`, and the lenses have no implicit verb and need no
  pre-pass". A bare short is `cmd: None`, and Atrium — not `pan`, which has a hidden `lookup`
  default — is the model.
- **The §9.3 refusal is Auspex's own, not `contract::refused_under_rule`.** A core refuses a
  write because a rule may not borrow a hand's authority; `aus` refuses `run`/`plan`/`test`
  because they would **re-enter the engine and recurse without bound**. Same exit `6`, different
  danger — and the spine's wording points at `get` and `where`, which `aus` does not have.

**The header grammar has one rule the spec does not state: `desc=` takes the rest of the line and
must come last.** §9.2 calls it "one-line", which a whitespace-separated field cannot hold; the
alternative was quoting, and a header a hand must escape to write is worse than one with an
ordering rule. `watch` and `writes` are single tokens by construction (comma- and
semicolon-separated), so nothing else wants the space.

**A rule whose header will not parse stays default-deny and is reported** — never dropped from
the listing. `writes` is the whole guard (§9.2, §9.5), so the unreadable case must fail closed.
The same holds for a filename whose code disagrees with the meta dir holding it: **the meta dir
wins** (§9.1 — where the file sits is the whole of its scope) and the disagreement is reported as
`misfiled_as`, never honoured as a scope.

**Rule discovery is the only walk in the workspace that looks for `FileClass::Rule`** — note the
variant is `Rule`, not `Function`; `"function"` is the reserved *token*. No `Store` walk could
ever yield one, since a rule belongs to no core's token set, so it is built from `build_tree` +
a per-node meta-dir `read_dir` + `classify`. **`pan new` does not mint meta dirs** — they appear
on first write — so a fixture placing a rule must `create_dir_all` it.

### Running a rule (§9.3) — what `plan` and `test` settled

- **Three streams move at once, on their own threads.** A rule gets its context on stdin and
  answers on stdout, and doing that in sequence deadlocks: a rule writing more than a pipe buffer
  before reading would block on stdout while Auspex blocked on stdin. `std::thread::scope` in
  `rule.rs` is what avoids it, and `a_rule_that_ignores_its_context_still_proposes` pins it with a
  200 KB context against a rule that never reads.
- **The child gets two variables and both are load-bearing.** `PANTHEON_RULE=1` is the enforcement
  every core already honours. **`PANTHEON_ROOT` is the one easy to forget**: §9.3 has a rule read
  the tree through the core CLIs, the context JSON carries no root, so without it a rule under
  `aus -C /some/tree` reads whatever the ambient environment named — the relay bug again, one
  layer down.
- **A 30-second deadline, hardcoded.** The spec bounds a rule's runtime nowhere, and both extremes
  are wrong: no limit lets a hung rule leave a detached process per write alive forever, a short
  one kills the API-calling rule §9.3 explicitly permits. Not a knob (§18) — `evaluate` takes the
  deadline as a parameter only so a test can reach the mechanism without waiting 30s.
- **Four failure modes, all per-rule**: could not spawn (usually a missing exec bit — a rule is run
  directly, so its shebang is the interpreter), exited non-zero, timed out, or emitted unparseable
  JSON. Each is reported against its own rule and the others still run (§9.5).
- **`plan` parses rather than echoing**, though §9.3 says "print stdout": `aus`'s own stdout is
  contract JSON (I4) and a rule emitting garbage would corrupt it rather than being reported.
- **The exit code folds worst-wins** — `0` all ran, `1` any errored — which is what `pan validate`
  and `pan resolve` both do. §9.5's "others are unaffected" is about not aborting the batch, not
  about claiming success.
- **`tracing` is live and stderr-only**, off unless `RUST_LOG` asks, so stdout stays pure contract.
  A rule's own stderr is captured (to quote on failure) and traced at debug, so a succeeding rule's
  diagnostics are not simply lost.
- **A test harness driving `aus` must set `.stdin(Stdio::null())`** — `aus test` reads a fixture
  from stdin whenever stdin is not a terminal, so a child inheriting the runner's stdin waits on a
  pipe that never closes. Both auspex test files do it and say why.

### Applying a proposal (§9.5) — what `run` settled

- **The grant is the whole guard, so it fails closed and rejects by the batch.** `grant.rs` parses
  `writes=` into capabilities; a grant it cannot parse rejects the rule rather than reading as an
  empty (deny-all) one that would hide the typo. A single unauthorized proposal — or a malformed
  one, or two colliding on one derived key — rejects the *entire* rule's batch (§9.5), so the
  granted proposal beside a forbidden one does not land either. Pinned by
  `one_ungranted_proposal_rejects_the_whole_batch`.
- **Auspex applies by spawning the core CLI a hand would type**, with `-y` (its authorization is
  the grant, not a prompt) and `PANTHEON_NO_HOOKS=1` (so the write does not wake `aus` again —
  `an_applied_write_does_not_wake_auspex_again` proves it with a fake `aus` on the child's PATH).
  It maps a proposal's **universal fields only** — name/key/series/refs, `--at now` for a date-keyed
  line — via `apply.rs`.
- **The `data` wall is real and refused loudly (I5).** §9.3 shows proposals carrying a `data`
  object, but **no core's CLI can ingest an arbitrary record** — every `add` builds from typed
  positionals/flags, and Auspex links no core. So a `data`-bearing proposal is *refused, never
  mis-stored*: `aus run` applies only what a hand could type. This is a genuine gap between §9.3's
  proposal format and the cores, and closing it means giving cores a `--data`/JSON-record path
  first (none exists). The `/series` grant slot's mint-licensing half waits on the same, since every
  minting proposal (a reading) carries data and is refused before it gets there.
- **`sign` is `manual` unless `--trigger` is present, and that is honest** — §9.4 has a TUI-open
  spawn a *bare* `aus run`, indistinguishable from a hand's, so only a triggered run can claim
  `hook`. The `watch=` filter applies only when a trigger names a core; a bare run evaluates every
  rule (§9.3).
- **Idempotence is the core's overwrite, not Auspex's bookkeeping.** A fresh add and an overwrite
  are the same `add` verb; `-y` lands the overwrite. So a rule run twice keeps one record, and
  Auspex needs no "upsert" path of its own (§9.5 step 5).
- **`aus run` maps `core`→`short` via `CoreRegistry::discover()`** (once per run, not per proposal),
  so it needs the cores on PATH — which is why `tests/apply.rs` sets PATH on each child, the
  `hook.rs` move, and stays parallel-safe without `unsafe set_var`.

**Step 8's hook half landed first, and it is the spine's, not each core's.** §16 step 8 says the
`aus`-not-on-`PATH` no-op "is exercised" through steps 1–7 — it was not: no core spawned anything,
and `PANTHEON_NO_HOOKS` was read nowhere. It is real now, in `pantheon::hook`, and the shape is
worth knowing before touching it. **The `Store` mutators *note* a write, `contract::dispatch`
*fires* once at process end** — so a verb writing three lines wakes Auspex once, not three times,
and no core crate carries a line of it (`Store` is generic over `Core`, so `C::NAME` is the trigger
the spine forwards without naming a core, I5). Two consequences a later step must not trip on:

- **Two notes that disagree collapse to a triggerless wake**, which is §9.3's own rule — a trigger
  names a write, and a `move` between homes or a rename cascade (`Cascade::apply`, which *notes*
  rather than fires, for exactly this reason) has no one write to name. Do not "fix" this into
  naming the last writer.
- **`pan` is not covered and does not need to be.** It hand-rolls its own tail instead of calling
  `dispatch`, and holds no `Store`. When §10.1's node-level cascade lands, `pan`'s tail owes
  `hook::wake_if_noted()` a call — nothing else will make it.

A screen opening wakes bare and triggerless from `porticus::run`, one site for all nine
instruments — and deliberately **not** from `porticus::drive`, so a driven screen in a test stays
quiet. `pensum/tests/hook.rs` pins all of it against a fake `aus` on the child's `PATH`; it needs
no `unsafe set_var` because the env is set on the child, not this process.

Two further things the spawn needs, both easy to leave out and neither caught by a test:

- **Nulled stdio and an un-awaited child are not detachment.** §13 names the mechanism —
  `process_group(0)` on Unix, the `DETACHED_PROCESS` creation flag on Windows — and without it the
  child stays in the caller's process group, so a `Ctrl-C` at the terminal can kill a rule
  mid-write.
- **`hook::suppress()` is how a process opts out of waking**, and Auspex calls it at startup.
  Without it `aus`'s own rules browser spawns `aus run` on open, since `porticus::run` wakes for
  every instrument. It is the twin of the `PANTHEON_NO_HOOKS` Auspex sets on the cores it spawns:
  one says *not this process*, the other *not that child*, and neither substitutes for the other.

### What step 6 deliberately left

Not oversights — decisions, each with the reason:

- **The figlet `roman` banner (P§8).** The Title overlay ships the tracked name-word instead.
  The face needs a vendored third-party `.flf` that `cargo deny` cannot see, so it is a
  licensing call rather than a coding one.
- **`Pick` is a line prompt, not P§4's tree-as-modal.** A Full view's `a` resolves a home by
  typed code; the tree-as-modal is the nicer form of the same question.
- **§10.2's auto-apply and candidate fixes.** A `Finding` carries a code, a severity, a path
  and a message — no candidates. The validate tab shows findings; offering or applying a fix
  needs the spine to produce candidates first.
- **`Calendar` and `Timeline`** waited for Fasti at step 7. Both now exist — see the catalog note
  below; step 6's deferral is closed.

### The chrome grew two views, and the shape grew a lib

Step 7's follow-ups, all landed:

- **`Calendar` (row · Full) and `Timeline` (draw · Full)** are in the catalog. A Calendar is a
  **row-view that also paints a grid** — P§3 is explicit that it is row + Full — so the grid is the
  locator, the rows beneath it are the focused day, and search/filter/scroll stay Porticus's (P§6).
  It declares its grid through `View::grid()`, exactly as `Insights` hands up `Panel`s; the app
  never paints. `[`/`]` page the month and `t` returns to today, as declared Tier-3 keys. **The cell
  dates the add** — `a` relays `--at 260718` — which is what `Target::Node.at` was built for at step
  6 with nothing to exercise it.
- **`Span_` carries a `home`**, as P§3 always specified. A Timeline is cross-node, so a bar resolves
  an action the way a row does (P§7); without an address a draw-view could not offer `Edit` at all.
- **A row-view's focused row wins over any address the view also names.** A dated Full view names
  its *cell* so `a` can date the add, and that cell must not stand in for the event under the cursor.
- **The body is drawn before the header**, though it appears second. A Full view's locator is
  *derived* — a Timeline's range is its bars' extent — so asking the header first reports the fold
  before last. Invisible to every earlier view, whose locators are constants or cursor state.
- **Every instrument is now a lib with a five-line bin**, so its screen can be driven. See the
  workspace layout above for the rule and its I4 guard.

### Things a later step must not be surprised by

- **`pan`'s node-level cascade (§10.1) is built** (step 9's node-cascade branch). All six
  mutators (`mv`, `rm`, `rename` incl. `--def`, `rename-prefix`, `rename-pattern`, `mv-file`)
  work over `pantheon::node_ops` — `plan_recode` walks a branch and emits a `Change::Rename`
  per descendant dir + `[code]` file prefix; `Change::RewriteRefs` folds a def-prefix /
  pattern re-slug's ref cascade into the same plan and token. `r`/`x` are live in `pan`'s
  TUI (`m` stays dark — no destination prompt yet), and the validate tab's `d` applies a
  finding's fix (step 9's 2b). The *record*-level cascade (§5.4, `cascade.rs`) is reused for
  the ref rewrites. **Still deferred:** the `writes=core@home` rule-header token cascade and the
  "dead code in a header" validate finding — Auspex's header parser now *exists* (step 8,
  `grant.rs`), but `pan`'s node cascade does not yet reach into rule headers to rewrite the
  `writes=` grant tokens a recode invalidates; rule *files* are renamed by prefix like any other.
- **`classify` is structural, and only the registry knows what a name *means*.** A determined
  series whose determinant is a *slug* (`crp__balance__checking.jsonl`) wears the same three
  segments as a hand-named one, so `classify` calls it `NamedSeries` — correctly. Only the
  registry's `named` bit separates them, which is what `SeriesRef`'s doc comment in `store.rs`
  says. `resolve.rs::register_record` once picked the ref-target identity off `FileClass` alone
  and so registered `rationes:checking` twice, making every holding ambiguous against its own
  balance file and raising a spurious `duplicate_slug`; it now asks the registry and routes a
  `named: false` token to `register_series_lines` like the nameless form. **Anything else reading
  `FileClass` to decide what a file *is* owes the registry the same question.** Pinned by
  `pantheon/tests/units.rs::a_determined_series_is_never_a_ref_target_even_when_it_carries_a_name`.
- **`Store::write_line` mints any `Shape::Series { named: false }` series on first write**
  (§7.3: a determined series is minted by its determinant). For Pensum the determinant is the
  node, so that is right. **Rationes' `balance` is determined by a holding *entity***, and the
  store links no core and cannot know (I5) — so `rat` checks in its own bin, via
  `holding_for_balance`, which every balance write goes through: **no such holding → exit `4`**
  (§7.3 already gives `4` to an `add` appending to a series that does not exist), **holding is a
  `claim` → exit `3`** (the write is well-formed; Rationes' own vocabulary refuses it). Not `6`,
  which §7.3 scopes to a write refused under `PANTHEON_RULE=1`. The lookup doubles as the home,
  which is why `rat checking 4200` needs neither `-H` nor `$PWD`. **Any future determined-series
  core must make this check itself** — `refusal_a_balance_without_its_determinant` guards it, and
  its load-bearing assertion is the second one: *the file was not minted*.
- **`plan_cascade` cannot refuse an occupied slug for a Document core**, and this is by design
  rather than a bug: it gates that check on the caller's own tokens, and Tabella declares none —
  and it walks meta dirs, where no document lives. So **Tabella makes the check itself**
  (`find_documents` tree-wide, then `pantheon::occupied_slug` for the shared wording). Any
  future Document core must do the same, or a rename will silently produce two records with
  one name. `tabella/tests/contract.rs::refusal_rename_onto_an_occupied_slug` guards it.
- **Every instrument's screen is now driven by its own `tests/screen.rs`** — nine of them, plus
  Atrium's `tests/relay.rs`. Each builds the *real* `App` (`PensumApp::new(&root)`) and drives it
  with `porticus::drive`, so a keystroke reaches a file and is read back **through the binary**.
  Add one whenever you add an instrument; a lineup is otherwise checked nowhere but a hand's
  terminal, since `check_lineup` runs at launch.
- **`atrium/tests/relay.rs` is the only test of §12's cross-process relay**, and it is alone in its
  file on purpose: it mutates `PATH` so Porticus can discover the cores, and Cargo gives each
  integration-test file its own process, so a lone test there races nothing. It locates the
  binaries *beside `atr`* rather than through a core's `CARGO_BIN_EXE_*`, because a lens depends on
  no core and could not name one (I5). Keep both properties if you touch it.
- **A `#[cfg]` above a `mod` you delete lands on whatever follows.** Removing Atrium's
  `mod mosaic;` orphaned its `#[cfg(feature = "tui")]` onto the next `use`, and `PathBuf` vanished
  from headless builds while `--all-features` stayed green. **`cargo build --no-default-features`
  is the only thing that catches this class** — the feature matrix is not optional here.
- **The table renderer now meets nested `data` for the first time.** Mappa is the first core whose
  `data` carries an object, so `map list -f table` renders `{"lat":59.3293,"lon":18.0686}` inside
  the cell. This is what §7.3 describes — `data`'s keys hoisted, and "the flatness test is
  deliberately *not* recursive" — so it is designed behaviour rather than a defect. It is merely
  *visible* now, and worth a deliberate call (a nested value could fall back to pretty JSON as
  `pan tree` does). That is a spine commit.
- **Two contract facts that only a screen test pinned**, both found by writing the first one for a
  core: a **partitioned entity is named by `slug`** where a **series line is named by `key`**; and
  **`ann … -c` mints an empty series**, so a fixture stopping there has a file with no records in
  it. Pensum's twin: a plain `pen list` is every *open* task, so `--all` is required to see a done
  one, and `done` carries the **date** rather than a flag.

## Commands (match CI exactly — see `.github/workflows/ci.yml`)

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -W clippy::pedantic -D warnings
cargo build --workspace --bins                                 # REQUIRED before tests: a core's contract
                                                               # test drives another tool's binary (`alb` writes,
                                                               # `pan resolve` reads back), `pan doctor`'s tests
                                                               # need the cores on PATH, and cargo builds no
                                                               # bin for a crate that is not under test
cargo nextest run --workspace --all-features --no-tests=pass   # falls back to `cargo test` if nextest absent
test -z "$(find . -name '*.snap.new' -print -quit)"            # CI fails on a PENDING snapshot: assert it or
                                                               # delete it. Every command above can pass while
                                                               # this one does not.
cargo build --workspace                                        # CI runs the `--target` matrix form of this,
                                                               # over 5 targets on their native runners
cargo build --workspace --no-default-features                  # the headless half (§14). Catches what nothing
                                                               # else does: a `#[cfg]` orphaned onto the wrong
                                                               # item is invisible under --all-features
cargo audit                                                    # advisories
cargo deny check bans licenses sources                         # licenses/bans/sources
```

Run fmt + clippy + tests before every commit — CI denies warnings *and* pedantic lints.

## Conventions & gotchas

- **Edition 2024, MSRV 1.88** (`rust-toolchain.toml` pins the toolchain; floor set by ratatui 0.30).
- **Shared deps live in `[workspace.dependencies]`** (Cargo.toml). A crate opts in with `<dep>.workspace = true`;
  don't pin versions per-crate.
- **crossterm is never a direct dep** — it arrives via the `ratatui::crossterm` re-export so backend and
  call sites can't drift. Apps take full `ratatui`; widget libs take `ratatui-core`.
- **`walkdir`, not `ignore`** — no ignore-file may govern the tree (§13, §18).
- **`panic = "unwind"` in release is required** — Porticus's terminal-teardown Drop guard rides on
  unwinding; `abort` would leave the terminal in raw mode on a panic. Do not change it.
- **The contract is frozen by `insta` snapshots**, taken from the real binary rather than the library
  behind it (I4). **Only the plan token is redacted** — a `key` never is, being the record's identity
  and its name at once (§5.4). Any change to a core's JSON is a visible snapshot diff in review —
  regenerate deliberately, never blindly `cargo insta accept`.
- **A snapshot cannot see the plan token move.** `RecordChange::body()`'s exact bytes *are* the token,
  and every snapshot redacts it — so editing that function is invisible workspace-wide while silently
  invalidating any token a hand holds from an earlier `--dry-run`. One test catches it:
  `pantheon/tests/units.rs::a_change_body_names_a_series_only_when_there_is_one`, which pins the byte
  string. If it fails, the token contract moved — decide that deliberately; do not update the pin.
- **Keep snapshots off the wall clock.** Pass every date explicitly (`ann -a 260718`, `pen --done 260719`);
  a core that reads `now` in a snapshotted path makes the suite fail tomorrow.
- **Name normalization is one rule** (§5.1): lowercase, NFC, alphanumeric+`_`, fold space/`-` to `_`,
  collapse and strip `_`. NFC is not optional (macOS/Linux byte disagreement). Apply on write, compare NFC on read.
- **Exit codes are contract** (§7.3): `0` ok · `1` runtime · `2` usage · `3` validation · `4` not found ·
  `5` confirm required · `6` write refused under a rule. Errors print `{"error":{"code":…,"msg":…}}` to stderr.
- **A failure follows *stderr's* hand, a result follows stdout's** (§7.3). Two questions, two streams:
  `contract::format_is_json` asks stdout, `contract::error_format_is_json` asks stderr, and
  `contract::dispatch` therefore takes the hand's `-f` (`Option<bool>`) rather than a resolved bool
  and asks each separately. Keyed off stdout — as it was — every `$(pan cd …)` that missed answered a
  human with the machine's envelope, since the shipped shim (`pan init`, §5.5) *always* pipes stdout
  while stderr stays the terminal. Only a pty reaches that case, so `pan/tests/grammar.rs` allocates
  one (`openpty`, a `cfg(unix)` dev-dep) and **drains the controller on a thread while the child
  runs** — after `wait`, Darwin discards what the closing device end left unread.
- **All TOML is `toml_edit`'s, and frontmatter is never re-serialized** (§6.6). `pantheon::document`
  owns the `+++` fence; `Document` carries `front_raw`, the fence's original TOML, and a rewrite edits
  *that* `DocumentMut` and re-emits. Rebuilding the fence from `Frontmatter`'s two fields instead would
  silently destroy a hand's comments, its key ordering, and every key Tabella does not read — the exact
  thing §6.6 keeps `toml_edit` for. Same rule for `[code]__.toml` (`meta.rs`).
- **A fold never reads bodies** (§6.1, §7.1, §7.2, §8.7 — the spec says it four times). `list` uses
  `document::read_frontmatter`, which stops at the closing fence. Reading the whole file and discarding
  the prose satisfies the letter and not the thing.
- **Format follows the hand:** TTY → table, piped → compact JSON, one code path
  (`contract::emit` → `pantheon::table`). The renderer lives in the **spine**, not Porticus: a
  bin built `--no-default-features` drops the chrome and is still a CLI that must table (§14).
  It knows no core (I5) — columns are whatever keys the value carries, with `data`'s hoisted —
  and it **declines what it cannot honestly flatten**: `pan tree` nests nodes, `schema` nests a
  schema, so those fall back to pretty JSON. The flatness test is deliberately *not* recursive.
  No contract snapshot covers any of this, because every contract test pipes.

### Improvement phase — Wave 1 (chrome UX, `porticus`; IMPROVEMENT-PLAN.md)

The first per-app improvement pass, all in `porticus`, so **one edit moved all twelve** (P-II).
What a later change must not undo:

- **Scrolloff is one stateless helper** — `runtime::scroll_first(cursor, len, height)`, shared by
  `draw_rows` and `Rail::draw` so list and tree scroll identically (I3, C3). It **centres the
  cursor** (half a pane from the top), top-anchored near the head and bottom-anchored at the end;
  derived from the cursor each frame, never a stored offset (I1, §18 — no knob). It *replaced* the
  bottom-anchored `first = cursor - (height-1)`, which is why the C1 clamp test no longer asserts
  "one Up lifts off the last row" (that was bottom-anchor behaviour) but instead that stepping back
  the list length returns to the head.
- **Search is the content surface, and ranked (C4).** `/` now takes **content focus** wherever a
  Rail view held the tree (`Chrome::Search` sets `Focus::Content`), so typing narrows the *rows*,
  not the tree cursor — **`Rail::seek` was removed**, not just unused. `filtered()` ranks matches
  **prefix > word-boundary > substring** with a stable sort (original index breaks ties), so the
  order is deterministic frame-to-frame — every caller (`draw_rows`, `current_target`,
  `row_targets`) must agree on the same cursor row. A per-frame **fold memo was deliberately not
  added**: while typing, `rows()` folds once per frame (live_search only sets the filter), so the
  reported slowness was the missing rank + tree-seek, not repeated folds.
- **The Title splash is a full-page banner painted on its own path (C7, C6).** `Overlay::Title`
  routes to `draw_title` (like `Overlay::Tree`→`draw_tree_modal`), **not** through `draw_overlay`'s
  line body — so `draw_overlay` no longer takes `ident`. The face is **`porticus::banner`**, an
  embedded **8-row serifed Roman-caps** alphabet (serif feet/heads, tall inscriptional/Trajan)
  **authored in-repo** (public-domain, no dependency, nothing loaded at runtime) — the plan's "render
  the caps without a third-party `.flf`" path, taken because no `cargo deny`-clean figlet font was on
  hand and none could be fetched. (A first 5-row solid-block cut read too modern; redrawn taller and
  serifed to land as classical.) It renders `ident.name`,
  falls back to the tracked word when too narrow, and **drops the tagline** (C6 — the `Ident.tagline`
  field stays, removing it is 12-crate churn for no gain; it is simply no longer rendered anywhere).
  The version line stays verbatim (`crate … · format 1`), which one frame test keys on.
- **The theme pass (C5) is values-only** — `theme.rs` palette + spheres, a warm/legibility lift over
  the same ink-on-vellum model; the accent's restraint (name + focus alone, P§8) is unchanged.
  Invisible to the frame snapshots (`as_text` strips style), so **no snapshot churned** — a later
  palette edit is free of snapshot review for the same reason, but must keep the P§8 table in sync.
- **The untracked `PORTICUS-SPEC.md` was updated in step** (P§4 Title row, P§6 search/scroll + the
  `count_at`-only count model, P§8 banner + palette table). Cite `P§n` from these; they now match
  the code.

### Improvement phase — Wave 4 (the lenses: N1, N2, N3 + G8; IMPROVEMENT-PLAN.md)

The lens wave, plus G8 folded in first because it is what makes a cross-core relay honest.
What a later change must not undo:

- **A target names its core (G8).** `RecordRef` and `Target::Node` each carry
  `core: Option<String>`. A **row** is stamped by the fold that built it
  (`RecordRef::in_core`); a **new** record takes the *view's* declaration (`View::core()`,
  set through `TreeFile`/`Agenda`/`Horizon`'s `.in_core(short)`), which Porticus stamps in
  `target_for` and in the pick-a-home modal. `None` stays right for a core's own TUI — it
  has one core and names it in `on_action` (I5). **Speculum's `core_of` re-read is gone**:
  routing is by provenance, never by matching home/key against every dated core.
- **`TreeFile::called(id)`** exists because a lineup's view ids must be unique (P§3) and a
  lens now stacks several record lists in one lineup.
- **Atrium relays across `pen`/`alb`/`tab`** with *one* `on_action`: the verb grammar is
  the shared one (§7.2), so only the core differs and nothing guesses it. `d` stays
  Pensum's alone. Its `count_at` is now node-local (`--here`) as P§6 requires, and stays
  one core's question — a badge summing three would spawn three children per visible node
  per frame.
- **Studium folds one programme at a time**, and a programme is **a node with a
  `[code]_curriculum.toml`** — discovered, never declared, so §18 gains no config and
  §19.3's file gains no key. The choice is **view state** (§19.4 says nothing stores it),
  held in one `scope::Studies` shared by every view; a fresh launch opens on all the
  studies. `]`/`[`/`p` are wrapped around **every** view by `scope::Switch` (a full `View`
  delegate) rather than declared on the mosaic alone — the inner view claims a key first.
  The scope is applied as `-H` and nothing else, so a screen figure reproduces as
  `stu -H <code>` (I8).
- **`FieldSpec::switch(label, flag)`** is a form field whose flag takes **no value** —
  appended only on a typed yes (`y`/`yes`/`true`/`1`). It exists so minting stays the
  hand's word: Speculum's `new log` and Studium's `new course log` both relay `-c`, and a
  lens must **never** infer a mint by reading whether the container exists (a typo would
  mint a series, and §18 keeps no undo). This closed G3's deferred first-grade mint.
- **The horizon dates the reading.** `Horizon::target()` names the anchor as `at`, and
  `target_for` now gives `Action::QuickAdd` the same `at` as `Add` — `A` differs from `a`
  only in how the *home* is chosen, so a dated view dates both.

### Improvement phase — Wave 6 (the audit: G1–G7; IMPROVEMENT-PLAN.md)

The audit wave. **Every Wave-6 item is now built** (G8 landed in Wave 4), so the plan's
"buildable now" list is empty and its three stale comments are corrected. What a later
change must not undo:

- **The rule header grammar lives in the spine** (`pantheon::rule`), not in `auspex`. It is
  a *structural* fact about the tree: `pan`'s node cascade rewrites the `writes=core@home`
  grants a recode invalidates (§10.1), and `pan validate` reports one naming no node
  (`dead_header_code`, §10.2). The spine cannot ask Auspex anything (I5), so the grammar is
  read from the hub and Auspex's copy is gone. **Enforcement stays in `auspex::grant`** —
  parsing a capability into what a proposal is checked against is §9.5's, not the spine's.
- **`Change::RewriteHeader` is `RewriteRefs` one layer down**: a ref names a record, a
  grant names a node, and a recode invalidates both. Emitted for **every rule in the tree**,
  not only the branch's — a rule may grant writes anywhere (§9.1 scopes where it *runs*).
  Rules the op moves are named by the path they land at, since the rewrite runs after the
  renames; the tree walk filters by those paths rather than by directory, or a root-scoped
  `rename-prefix` would rewrite each moving rule twice, the second time at a path that no
  longer exists. Only `writes=` is touched — `watch=` names cores, `desc=` is prose.
  `rename-prefix` cascades too, which §10.2 states outright.
- **A genuine choice is a row per candidate, never a chooser.** `Finding` carries
  `candidates: Vec<String>` beside `fix`; a cross-node duplicate slug lists one
  `pan rename-pattern` per holder. `pan`'s validate tab renders each on its own line and
  `d` relays **whatever command the focused row carries**, so the tab teaches `pan` no fix
  shapes and a new one in the spine works there the day it lands.
- **`Overlay::Tree` carries a `Picking`**: `Home` hands the node to the add form (`A`),
  `Destination` appends it to the invocation the app built (`m`). A move names a *node*, so
  it is picked off the tree, never typed as a code.
- **`f` follows a ref chip, and only within one core** (I5). `View::focused_ref` says
  *which* reference; Porticus resolves it through the spine, checks the core against
  `ident().short` via the registry, then `Rail::reveal`s the node and pins the record. A
  cross-core chip is answered with which binary owns it — never silently ignored, never
  rendered by an instrument that links no such core. `Rail::reveal` is an *address* seek,
  distinct from the search jump `Rail::seek` once was (removed in C4).
- **`porticus::drive` draws before each key**, as `run` does. A view that establishes
  something while painting — an `EntityCard`'s chip strip, the one cursor a card has — had
  established nothing by the time a key arrived, so a screen only reachable after a frame
  was a screen no test could reach.
- **`View::add_form` overrides the app's.** One instrument one form is right for a *core* —
  it has one primitive. A **lens** does not: Studium's `a` records a grade, logs hours, or
  places an occurrence depending on the tab, so the form belongs to what the tab is about.
- **`list` is the present, not the history** (I1). A core's `list` answers with the *latest*
  line of each series, so a fold that reads refs or occurrences straight off it hides every
  earlier reading — a professor named on one grade vanishes the day a retake lands.
  `fold::series_lines` names the series from `list`, then reads each. **Any cross-core fold
  over samples owes the same two-step**; only entities (a Fasti `span`) are whole in `list`.
- **§19.5 is built and its study year anchors on the programme's start**, which is what
  §19.5 says. Anchoring on the first period's date looks equivalent and is not: a programme
  beginning 1 August against a P1 opening on the 26th would read its own first spring as
  year two. A period may also *wrap* the new year (KTH's P2 runs into January), so each
  year-less anchor is materialised per calendar year from the one before the span opens.
- **Studium is §19.6's seven tabs and §19.8's five relays, on one `on_action`.** That works
  because the verb grammar is shared (§7.2) and only the core differs — read off the
  *address* (G8), never guessed from the action. **People and reflections stay read-only**:
  §19.8 lists no relay for either, and a person is Album's to edit in `alb`.
- **The occurrence form carries a `concerns` field** because §8.4 gives an event no kind
  beyond `event`. What makes a sitting an exam is the reference to its enrolment, and a hand
  writes it — a lens inferring one would be deciding a core's vocabulary (I5).

### Improvement phase — Wave 7 (`a` on a Full view · Help's Tier 2)

Not a plan item — a user-testing report ("why can't I add todos in the Pensum TUI?"), and both
halves turned out to be **`porticus` failing to implement P§4 rather than a design call**. One
edit each, so all twelve moved (P-II).

- **A `Full` view's `a` opens the node picker**, as P§4's Pick row always said ("a `Full`-view
  add … for a write with no ambient node"). It did not: `target_for` handed `Action::Add` the
  **rail cursor**, and a Full view draws no rail — so `a` filed at a node the hand could not
  see, silently the tree's first node on a fresh launch. Fasti's Calendar, Speculum's Horizon
  and Studium's two dated lists were all writing to an unseen home. Routed in `act()` for
  every Full view at once; **the date survives the detour** because `Picking::Home` re-reads
  `view_at` when the node is taken, which is what keeps a Calendar cell dating its own add.
  A screen test that types `a` on a Full view now needs an `<enter>` to take the node.
- **Help lists Tier 2**, which P§4 also always said ("chrome keys plus the current view's
  own"). `help_lines` folded `CHROME_HELP` and stopped, so nothing on screen ever said that
  `a` adds or `d` marks done — and `Action::label` ("the label Help shows") and
  `keymap::key_for` had **no caller at all**, which is the tell. Now two columns where they
  fit and stacked below `HELP_TWO_COLUMN`: the box neither scrolls nor truncates, so a
  wrapped second column interleaves with the first. Stacked, the chrome label is **unpadded**
  — the pad itself wraps to a blank line and reads as a gap between rows.
- **An unoffered action is greyed, never dropped** (P§5). The reservation is suite-wide, and
  one a hand cannot see is one they will try to rebind. `as_text` strips style, so the greying
  is pinned by a unit test in `runtime`, not a frame test.
- **Pensum's agenda offers what its records tab offers.** A row carries its own home (P§7), so
  `e`/`x`/`r`/`m` always worked from either tab and the gap was arbitrary; `a` was the one that
  mattered, since the agenda is the tab a hand sits on to see every open task and was the one
  place `a` did nothing at all. Atrium's agenda gains `Add` for the same reason — its "offer
  `A` alone" workaround is now handled centrally.

### Improvement phase — Wave 8 (`pan`'s seven defects; pan-bugs.md)

A user-testing report against `pan 0.1.0`, run on synthetic trees before the `~/aedes`
migration. **B1 destroyed data silently and B2 made it reachable everywhere**; the rest were
missing verbs the migration needs. What a later change must not undo:

- **`Plan::preflight` refuses any rename onto an occupied path, and it simulates.**
  `std::fs::rename` *replaces* its destination, and a recode plans one rename per file
  whose prefix changes — so a stray `aoi_notes.md` beside `aoa_notes.md` was destroyed at
  exit `0`. The load-bearing part is that **a naive `to.exists()` misses the flagship
  case**: a recode renames the branch dir *first*, so the doomed file's planned
  destination names a directory that does not exist yet while the file itself sits under
  the old one. Each virtual path is mapped back through the plan's own renames
  (`real_path`) before the tree is asked, and a path this plan already vacated is not a
  collision. Every collision is collected so one dry-run answers for the whole plan, and
  the message names **real** paths — a plan path is virtual and a hand cannot act on it.
  Called from `Plan::apply` *and* from `pan`'s `run_plan` before the dry-run emit.
- **`Plan::apply` asks again immediately before each rename.** Plan-time alone is a TOCTOU
  window and **the plan token does not close it** — the token is checked against a freshly
  *computed* plan, recomputed from the same tree, so a file created in between is invisible
  to both. `occupied()` is `symlink_metadata`, never `Path::exists`: a **dangling symlink**
  is a name a rename replaces just the same. The atomic form (`renameat2(RENAME_NOREPLACE)`
  / `renamex_np(RENAME_EXCL)`) was deliberately *not* taken — it costs `libc` + `unsafe` in
  a spine that has neither, plus a Windows arm.
- **No `--force`, deliberately.** §18 ships no undo, so a clobber must not be one flag
  away. The escape hatch is to move the blocker aside, which leaves it on disk.
- **This closed B5 too**: an occupied destination is refused whether it holds a file or a
  directory, so the old asymmetry (a **non-empty dir** caught by the OS mid-apply with
  `ENOTEMPTY`, a **file** silently clobbered) is one answer in both directions, and the
  half-applied plan never starts.
- **`rename-prefix` walks the node tree and nothing else** (`prefix_contents`, §6.3). It
  recursed through *every* directory under its scope, so a project homed at a node put
  `.git`, `node_modules` and `target` inside it — it renamed files in git object stores and
  build output. It is now bounded exactly as `rename`/`mv` are (`build_tree` + per-node
  meta dirs + loose files), which is why `crates/` always rode along untouched. **Do not
  "fix" this with a skip list or `.stignore`** — §13/§18 leave no ignore file any say over
  the tree, and a hardcoded `node_modules`/`target` list is the same thing spelled
  differently. The cost is that a loose file in a non-node dir (or at the root) is out of
  reach, which is B7's accepted constraint and validate's own rule ("stray files at the
  root are bulk, not the tree's concern"). A **drifted meta dir** (any `…__` name) is still
  entered — that is what the repair is for.
- **`mv-file` takes any file and many of them.** It refused everything without a `__`,
  which left nothing at all for bulk — the overwhelming majority of a migration. A
  `__`-named file lands in the meta dir, a document or bulk loose in the open node dir
  (§6.1, §6.5). **Where the code comes from differs by half**: a `__` file names its own
  code in its first segment (so a *misfiled* one is re-prefixed correctly, §10.2's case),
  while a document or bulk file takes its *node's*, read off where it sits. A name carrying
  no code moves verbatim. `tab move` remains the record-level verb — it wakes Auspex, as a
  core's write does; this is the structural half and knows no core (I5).
- **`in_root_spelling` is not optional.** One file has more than one absolute name
  (`/tmp/x` vs `/private/tmp/x`) and a shell glob hands over whichever the cwd wore. Two
  things break silently on the difference: the plan carries an absolute path where every
  other change is root-relative, and the node holding the file compares unequal, so its
  code goes unrecognized and **the prefix is never swapped**. Canonicalize both ends,
  rebuild on `root`. `pan`'s `beside_the_hand` resolves a *relative* path against the
  **cwd** first (what a glob produces), keeping ambient cwd out of the spine.
- **`pan merge <src> --into <dst>` is the verb `mv` cannot be.** `mv` refuses a code
  collision (§5.3) and is right to, but that left no verb for a tree assembled from two
  trees. **The union key is the code and it recurses**: a `src` child whose recoded code
  matches one already at `dst` is merged *into* it, never renamed onto it. Where labels
  differ, **`dst`'s survives** and the dropped one is reported — one code is one node, so
  one name has to go and only the existing one is not a guess. **Everything in the open dir
  moves**, bulk directories included (one rename, never descended into), because what the
  walk does not carry the final removal would destroy. Grants cascade (`header_cascade`, as
  for `mv`); refs do not (a def-prefix node keeps its slug on a move). File collisions —
  two annotation files being the usual pair — are refused and *all* listed by `preflight`;
  what a merged `[code]__.toml` should say is not the tool's to invent.
- **`Change::RemoveEmptyDir` is `Remove` that refuses a surprise.** `Remove` recurses
  (`remove_dir_all`), which is right for `rm` — the node is *proven* empty first — and
  wrong for `merge`, where the dir is empty only because this same plan just emptied it.
  Anything that arrived meanwhile must stop the removal, not be swept up by it.
- **The annotation key set is open, and an unknown key is a *field*** (`[fields]` in
  `[code]__.toml`, §5.2). It was closed at four, so placement rule 4 — "fields, not nodes"
  — had nowhere to land: the only home for a warrant or a role was `keywords`, documented
  as search hints for an LLM, which would have made the field indistinguishable from one.
  The four typed keys keep their shapes; everything else is namespaced under `[fields]` so
  it can never shadow one. Surfaced in `pan constitution` and the `pan` node card, because
  a rule about what colours a record is unreadable if it is not shown. **A field is
  annotation and never behaviour** (§18): every value is an unvalidated string and **no
  tool may branch on one** — a field that tuned a tool would be the config file §18 forbids
  whatever it were called.
- **B7 is a constraint, not a debt.** `sort/` and `vol_o/` are not in `[char]_[label]`
  form, so every verb stops at them and `pan validate` cannot be the completeness check
  there. The naming rule is doing what it says; nothing was changed for it.
- **B8 — `occupied` asks identity, not existence, and it must stay that way.** The
  pre-flight above shipped asking whether the destination *existed*, which on **APFS and
  HFS+ is the wrong question**: those compare names case- and normalization-insensitively,
  so `träning` NFD and `träning` NFC are byte-different paths naming **one file**, as are
  `Ars` and `ars`. A normalizing rename therefore collided with *itself* — the message
  printed one path twice and named nothing a hand could move. What made it severe is which
  rename it refused: normal form is lowercase and NFC (§5.1), so `pan validate` emitted
  `pan rename <code> --label <normalized>` as the fix and the tool then refused its own
  prescription, with 59 of them waiting in the live tree. `occupied(dest, src)` now compares
  **`dev` + `ino`** — the question itself, not a proxy, answering across hard links and
  every spelling an insensitive filesystem accepts. Both call sites pass the source:
  `preflight` has `real_from` in hand, `apply` has `from`.
  - **Keep `symlink_metadata` at both ends**, never `metadata`: a dangling symlink is a
    name a rename replaces just the same, and a link must be compared as itself rather
    than as its target.
  - **Off unix `same_file` is `false`, so an unknown case refuses.** `canonicalize` would
    follow symlinks and a `same-file` dep buys one predicate (§13). This fix *loosens* a
    guard against data loss, so the untested platform keeps the strict answer.
  - `a_normalizing_rename_is_not_a_collision_with_itself` and `a_case_only_rename_applies`
    pin it, and **both were confirmed to fail without the identity check** — on a
    case-sensitive filesystem they would otherwise pass for the ordinary reason and prove
    nothing. The second also re-asserts that a *genuinely different* file at the
    destination still refuses: identity is not a licence to clobber.
- **A stale install is invisible and the whole suite goes stale together.** Everything
  above is verified against a fresh `cargo build`, which is **not** what a shell runs.
  `porticus` compiles into all twelve binaries, so a chrome fix reaches none of them until
  each is reinstalled — reinstalling `pan` alone leaves eleven apps carrying the bug.
  `cargo install` uses a private target dir per crate, so set `CARGO_TARGET_DIR` to share
  the dependency build across the twelve or it recompiles the world twelve times under
  `lto = true`. `pan` is `0.1.1` from this wave so `pan -V` and `pan doctor` can tell the
  builds apart; a crate version drifts freely beneath `format_version` (§15.5).

### Improvement phase — Wave 9 (the char slot is one character; §5.1)

A naming-rule change, so **§5.1 moved and the code is downstream of it**. The padded
two-digit numeric level (`01`…`99`) is gone: **a defining char is exactly one character,
a letter or a digit, and an enumerated level counts `0`…`9` then continues `a`…`z`.**
What a later change must not undo:

- **Every compact token is one character, so `tokenize_compact` is a `chars()` walk.**
  The old scanner looked ahead — "a letter is a one-char token, a digit begins a
  two-digit token" — which is why the width had to be fixed in the first place. With one
  width there is nothing to disambiguate, so `asdl01` is six levels, not five. **The
  ordering is free**: ASCII puts every digit before every letter, so the counting order
  `0`…`9`,`a`…`z` *is* the text sort order, which the padded form only achieved by
  padding. A level never needs a 37th sibling (I7).
- **`CharToken` was deleted, not narrowed.** The enum existed solely to hold "one letter
  or two digits"; once both are one character it is `char`, and `NodeName.ch` /
  `Node.ch` are `Option<char>`. Nothing ever branched on Alpha-vs-Numeric — do not
  reintroduce a type to carry a distinction the rule no longer makes.
- **`Code::parse` still refuses an opening digit**, and that rule is now independent of
  tokenizing rather than a consequence of it: it is what keeps a sphere alphabetic and a
  code visually distinct from a date. A counted level therefore always takes a defining
  char above it.
- **There is no compat mode, deliberately.** Reading both widths would bring back exactly
  the ambiguity the fixed width existed to prevent (`aop01` = `a,o,p,0,1` or
  `a,o,p,01`?), so an un-migrated padded dir is simply `malformed_dir`. The consequence
  is that **the old and new binaries cannot both read a half-migrated tree**, which is
  what shapes the migration below.
- **The live `~/aedes` was migrated by bouncing through letters** (43 nodes): old `pan`
  0.1.1 renamed every padded char to a spare *letter* (legal under both rules, so the
  intermediate tree is valid to each binary), then the new `pan` renamed that letter to
  its digit. Both passes ran `pan rename --char` deepest-first — so a node's ancestors
  are untouched when its turn comes and its already-renamed descendants ride the cascade
  — and every step dry-ran first. `00`…`09` de-padded (`07`→`7`); the two levels
  numbered past nine (`aotke_m_mou`, one Avicii album) were recounted in order. **A whole
  phase must finish before the next starts**: renaming one branch at a time leaves a tree
  that neither binary can walk.
- **`aop00_07_esa_26_` is a definition-prefix node, not a padded char.** Its trailing `_`
  is the marker (§5.1), so `pan` never read `07` as a char and it needed no rename — it
  rode its parent's cascade. A hand-rolled scan of the directory names got this wrong;
  **source a structural question from `pan tree -f json`, not from splitting names.**

### Improvement phase — Wave 10 (a date is `YYYYMMDD`; §5.4, format 1 → 2)

The second naming-rule change, and the first **on-disk format** change: every date in the
suite is eight digits. §5.4 moved and the code is downstream of it. What a later change
must not undo:

- **The reason is the century, and it was silently wrong rather than merely imprecise.**
  A two-digit year forced every reader to assume `20xx` — fine for a reading taken today,
  wrong for any date a hand records *about the past*. It surfaced as "why can't Album hold
  a birthday?": `940317` read as 2094. The assumption lived in four separate readers
  (`studium::fold`'s `2000 + year`, `porticus`'s Calendar and Timeline, `speculum`'s
  horizon), each independently, so there was no one place to correct it. **Eight digits
  remove the question rather than answering it consistently.**
- **`pantheon::DATE_WIDTH` is the one statement of the width** and every reader now takes
  it from there — the spine, both hand-rolled core checks (Album's and Fasti's `check_day`),
  and all three consumers. Pensum and Rationes already delegated to `Key::parse().classify()`
  and so moved for free; that is the shape to copy, not a fresh `len() == 8`.
- **A six-digit date is refused, never widened** (`key_from_at`, §5.4). Guessing `20` at
  the CLI would be the same century assumption one layer down, and it is exactly wrong for
  the dates a hand types by hand. **There is no compat mode**, as in Wave 9 — and here
  reading both widths would be worse than ambiguous, it would be *silently wrong*: keys sort
  lexicographically, so a series holding both `260601` and `20260715` folds to the **older**
  line as its present (`'0' < '6'`), breaking I1 with no error anywhere.
- **`format_version` is `2`** — the first bump, and it is what §16 step 11 was waiting on:
  `pan migrate` is **no longer blocked**, because a prior format now exists to rewrite from.
  The literal lives in **17 places** (twelve apps' `version_json`, each core's
  `schema::<C>(2)` call, `pan doctor`'s own entry, `porticus`'s Title line). That duplication
  is now a real hazard — a missed one reads as a disagreeing app — and folding it into one
  spine constant is the obvious follow-up.
- **The migration is `cargo xtask migrate-dates --root <tree> [--apply]`**, dry-run by
  default. **Its safety is `is_record_file`, not the extension.** The live tree held
  **23,995 `.json` files of which 148 were records** — the rest dependency lockfiles under
  project nodes, and an npm lockfile carries `"from"` keys a date pass would rewrite. This
  is the Wave 8 `rename-prefix` trap exactly, and the same answer: bound the walk to the
  node tree (§5.2 — a record's filename begins with its meta dir's name), never a skip list.
- **It rewrites text, never re-serializes.** A parse-and-re-emit would reflow every record
  in the tree, and a migration that churns files it did not need to touch is one nobody can
  review (I6). Only the seven dated field names plus `key` are touched, so an Annales
  *reading* of `250601` — a measurement, not a date — is left alone.
- **Verified on a copy of the live tree before anything was recommended**: 158 dates across
  59 files, idempotent on re-run, still valid JSON, and read back through `pen`/`fas`/`ann`.
  Keep that order for the next format change — dry-run, copy, apply, read back.

### Step 6's durable rules (the chrome)

- **A view declares intent; Porticus runs the flow** (P-II). A view says which `Action`s it
  offers and Porticus owns the key→action binding, the confirm policy, and every prompt. If you
  find yourself giving a view a raw key handler or its own confirm, stop — that is the one thing
  the layer exists to prevent.
- **A chord is not its key.** Raw mode delivers `Ctrl-D` as `Char('d')` with a CONTROL modifier
  (P§10 says the same of `Ctrl-C`). The router drops CONTROL/ALT/SUPER before the keymap sees
  them; **SHIFT must survive**, because `A`/`D`/`X` arrive shifted. Without this every control
  chord fired its bare letter's action — a bare `Ctrl-D` marked a record done.
- **Name the root on every subprocess — read *and* write.** A write carries `-y` because a
  relay's child writes down a pipe, where a mutation without it exits `5` (§7.3): the confirm is
  the TUI's modal, never a CLI exemption. Both carry `-C <root>` because `$PANTHEON_ROOT` is the
  caller's ambient state (§6.2) while the root a screen was *given* is the fact — without it a
  tool opened with `-C` folds one tree and writes to another, silently. Porticus adds `-C` to
  every relay centrally; **a lens's own reads are its own to root** (`tessera::read` takes one,
  and Atrium holds the root for its tiles, its agenda fold, and its `count_at`). Both halves of
  this were real bugs, found one after the other.
- **`count_at` is one node-local fold, memoized per frame (P§6).** The dim is `count_at > 0`
  and the badge is `count_at`, over the *same* per-frame memo in `Asking` — so a held node is
  folded once, not once for the dim and again for the badge. **`count_at` MUST fold node-local**
  (`fold_local`/`find_entities_local`/`find_documents_local`), the records *at* the node and not
  its subtree: the rail asks it of every visible node, and a subtree fold re-read a branch once
  per ancestor — the O(depth·records) cost that made walking a core's tree slow. This *replaced*
  the old `any_at`/`count_at` split: with a node-local count there is no costly question to make
  cheap, so `App::any_at` was removed and the badge is now node-local where it once summed the
  subtree (a deliberate change; `pan` was already node-local, and no test pinned the sum). The
  content pane (`rows_at`) still folds the subtree — it is one fold per frame, never the rail's
  per-node cost — so a parent's badge (its own records) can read lower than the list below it.
- **`None` from `rows` is a draw-view; `Some(vec![])` is an empty row-view** (P§3). The first is
  *about the selected node*, so the node is its target; the second honestly has an empty set.
  Conflating them made `e` on a draw-view silently do nothing.
- **`Terminal::clear()` does a cursor round-trip** and fails wherever nothing answers. `suspend`
  rebuilds the terminal instead — empty buffers, no question asked. A clear here could commit a
  write and then kill the screen reporting it.
- **Drive the screen in tests with `porticus::drive`**, not a pty. A pty proves the lifecycle but
  has no size, so it draws no cells and echoes scripted input in cooked mode before the app takes
  raw mode. `drive` runs the same `handle` and the same relay, returns the final frame, and
  really performs writes. Three defects reached `main` past a full green suite and were caught
  only by driving a real binary; two more fell out within minutes of `drive` existing. **Every
  instrument now has a `tests/screen.rs` that drives its real `App`** — see the shape rule in
  the workspace layout, which exists to make that possible.

## Non-goals (§18) — do not build

No undo/history layer, no central store or cache, no reverse index, no file watcher, no autonomy/boldness
knob, no per-app editor env var, no thirteenth core verb. When a feature feels convenient, check §18 first.
