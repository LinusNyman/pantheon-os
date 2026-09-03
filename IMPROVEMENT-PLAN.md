# Improvement plan — the user-testing pass

`BUILD-PLAN.md` covered the *build order* (§16 steps 1–10). This document covers the phase that
follows it: **step 11 is deferred, and the work now is user-testing each app and improving it, one
instrument at a time** (see CLAUDE.md status). It is a worklist, not spec — but several items **do
require a spec edit**, and where they do the item names the chapter and the exact change, because in
this repo *the spec is law and design is downstream of it*. Each item is written to be picked up on
its own: it states the problem, the spec change (if any), the code anchors, the approach, and what
"done" means.

Do them roughly top to bottom — later waves lean on earlier ones (the lens waves assume the chrome
bugs are fixed; the studium/speculum items assume the lens-write item). Within a wave the items are
independent.

## Guardrails that bind every item

These are the invariants each change is most likely to trip. Cite them; do not quietly cross them.

- **§18 / P§11 — no config, no knobs, hardcoded chrome.** The theme, keymap, layout, and font are
  hardcoded on purpose. A "revamp" edits the palette *in code* (`theme.rs`), never adds a
  `porticus.toml` or a theme setting. The **only** hand-written TOML the OS admits is node
  annotations, document frontmatter, and the one §19.3 carve-out (`[code]_curriculum.toml`, reference
  data). If an item wants persistence, it is either reference data homed in the tree (like curriculum)
  or it is **view state** (like Speculum's horizon), never a behaviour config.
- **§18 — no thirteenth core verb.** `add · edit · rename · move · rm · list · get · series · where ·
  schema · help · version` is a core's *whole* vocabulary, stated three times (§7.3, spec §18,
  CLAUDE.md). `grep` collides head-on with this — see **L2** for the way around it.
- **I1 — derived-out.** No screen stores a rendered value. Insights folds fresh each frame; a
  scroll offset is cursor state, not a stored present.
- **I2 / §12 — relay-only writes.** A lens **never originates** a write but **may relay a
  human-initiated one** (§12 line 5). "Lenses are read-only" is a false premise — the ask in Wave 4
  is to *widen* what they relay, not to grant them a new power.
- **I5 — no cross-core imports.** A lens reaches cores over `PATH` as subprocesses; it links no core
  lib. Every new lens relay is another `Invocation` shelled out, dimmed when its core is off `PATH`.
- **P-II — feel is Porticus's, one edit for twelve.** Chrome items (Wave 0–1) change *one* crate
  (`porticus`) and move all twelve apps at once. That is the point of the layer, not a blast radius to
  fear.

## Traceability — your asks → items

| Your ask | Item |
|---|---|
| Can't scroll out of the app | **C1** |
| Adding "just says name the record", no field | **C2** |
| Scroll should start at middle on big screens | **C3** |
| Search broken, slow, no live top answers | **C4** |
| Revamped classical look | **C5** (+ **C7**) |
| Remove tagline by the app name, top-left | **C6** |
| Full-page title with a nice block/classical font | **C7** |
| `ls` scoping: bare returns nothing, `ls a` errors, `-H a` is subtree-only | **L1** |
| A `grep` cmd (or just use `rg`) | **L2** |
| Error messages render as raw JSON at a terminal | **L3** |
| Pensum todos: sort by node, node first, spaces not `_` | **P1** |
| Insights show only 1–2 letters; `pen done` shows nothing | **P2** |
| Drop redundant `core`/`kind` columns from the table | **P3** |
| Lenses shouldn't be read-only (Atrium/Studium full access) | **N1** |
| Studium: study subtree only, node-scoped todos, switch programmes | **N2** |
| Speculum: survey myself / log stuff (like the old aoaq) | **N3** |
| A plugin surface (a Canvas sync tool for Studium) | **X1** (+ **X0** the ingest wall) |
| Everything else missing / unbuilt | **Wave 6** (audit) |
| Use TOML where it's easier | folded into **N2**, **X1** + Guardrails |

---

## Wave 0 — Chrome correctness (bugs; `porticus`, all twelve apps)

### C1 — You can scroll off the end of a list
**Problem.** `motion` grows `state.row` with `saturating_add(1)` and **never clamps to `rows.len()`**
(`runtime.rs:872`). Display clamps with `.min(rows.len()-1)` (`runtime.rs:464`), so the cursor
*looks* stuck at the bottom while the real counter drifts past the end — after over-scrolling you
press Up many times before anything moves. Contrast `Rail::down`, which *does* clamp
(`rail.rs:159-161`).
**Spec.** None — pure bug against P§6.
**Approach.** Clamp the Down/Content branch in `motion` (`runtime.rs:863-887`) to the live
`rows.len()-1`, mirroring the rail. Verify the row count is the *filtered* length when a search is
active.
**Done.** A `tests/frame.rs` (or `porticus::drive`) case that presses Down past the end and asserts
the cursor + `first` stay pinned to the last row; existing screen tests stay green.
**Size.** S · one crate.

### C2 — Add prompt appears but takes no input ("name the [core] record")
**Problem.** The multi-field add form *exists and is tested* (`open_add_form` `runtime.rs:1392`,
`handle_form_key` `runtime.rs:1295`, `frame.rs:171` `a_opens_the_default_add_form`). The literal
string **"name the … record"** is **not** Porticus — it is a **core-CLI refusal**
(`pantheon/src/contract.rs:663` and `:763`, also `pensum/src/cli.rs:623`, `rationes/src/cli.rs:317`).
It reaches the status line **only when a bare, nameless `add` was relayed past the form**. So on some
instrument `a` is relaying instead of opening the form — either the focused view does not offer
`Action::Add` (dark key, `begin` no-ops at `runtime.rs:906`) or a relay path is being taken.
**Spec.** None — bug.
**Approach.** First **reproduce** to pin which instrument/view shows it (note `runtime.rs` is
currently dirty in the working tree — check whether a local edit regressed the form path at
`runtime.rs:951-959`). Then ensure that view lists `Action::Add` in `actions()` and that `a` routes
to `open_add_form`, not a relay. `pan` is *correctly* dark here (`pan/src/screen.rs:473`) — don't
"fix" that.
**Done.** Driving `a` on the affected instrument opens a typeable form and mints a record; a
`tests/screen.rs` case proves it end-to-end for that crate.
**Size.** S–M · likely one crate + a porticus assertion.

---

## Wave 1 — Chrome UX (`porticus`, all twelve apps)

### C3 — Scrolling should begin before the cursor hits the edge (scrolloff)
**Problem.** The viewport is bottom-anchored: `first = cursor.saturating_sub(height-1)`
(`runtime.rs:465`, and identically the tree at `rail.rs:250`). There is **no scrolloff / centering
anywhere** — on a tall terminal the list stays top-pinned until the cursor reaches the very bottom
line, then scrolls one-per-step.
**Spec.** P§6 says nothing about a scroll margin. Add one sentence to P§6 ("Search, filter & the
tree") stating the keep-cursor-off-the-edge margin, so the behaviour is documented where the rest of
the scroll model lives.
**Approach.** Introduce a scrolloff constant (e.g. `min(height/2, N)`) and recompute `first` so the
cursor keeps a margin from top and bottom until the list ends. Apply to both `draw_rows`
(`runtime.rs:462`) and `Rail::draw` (`rail.rs:250`) so list and tree feel identical (I3).
**Done.** A drive test on a tall viewport asserts the window moves while the cursor is still mid-pane.
**Size.** S · one crate.

### C4 — Search is slow and shows no live top answers
**Problem.** Three things stack up:
1. Rows are **re-folded every frame** (`runtime.rs:374`, `views[active].rows(node)`) and then rescanned
   O(n) in `filtered()` (`runtime.rs:684`) — a full fold + full rescan on every keystroke.
2. `filtered()` does a plain `label.to_lowercase().contains(needle)` with **no ranking**
   (`runtime.rs:684-693`), so nothing floats the best matches up.
3. In the **default rail focus** after a view switch (`runtime.rs:819`), typing only calls
   `Rail::seek` (`rail.rs:224`) — it *jumps* the cursor, it does **not** narrow the list. So "top
   answers while typing" never appears until you `Tab` to content focus.
**Spec.** P§6 already says search "matches live" and "filters rows incrementally" — the intent is
right, only the implementation lags. Add one clause to P§6 that search results are **ranked** (prefix
> word-boundary > substring) so "top answers" is a documented behaviour, not an accident of order.
**Approach.** (a) Memoize the per-frame fold so keystrokes don't re-fold (cache the `Vec<Row>` for the
active view+node, invalidated on the P§6 refresh events — launch/nav/switch/relay). (b) Give
`filtered()` a rank key and sort by it. (c) Decide rail-search behaviour: either narrow the tree too,
or make the incremental **content** filter the primary search surface. Keep it Porticus-owned (P-II)
— no per-app search.
**Done.** Typing in search visibly shrinks/ranks the list within one frame; a large-list drive test
stays responsive; no per-keystroke re-fold in a profile/trace.
**Size.** M · one crate.

### C5 — Revamp the look so it reads as classical
**Problem.** The palette is *already* the classical scheme (ink-ground / parchment-text, minium
accent, Trajan letter-spacing — `theme.rs:1-45`, P§8), but it isn't landing as "classical" to the
eye. This is a **visual-design pass**, not a re-architecture.
**Spec.** None. P§8 is the theme's spec and the change stays **inside** it — richer values, same
model. **Must stay hardcoded** (§18/P§11): edit `theme.rs`, add no setting.
**Approach.** Concrete levers, all in `theme.rs:13-38` + the draw sites: border weight/color
(`chrome()` `theme.rs:69`), section rules and spacing in the header/overlays, how much the accent is
used vs held back (P§8 says accent = *name + focus only*; honour that restraint), and the six-color
sphere palette (`theme.rs:31-38`) for legibility. Pairs with **C7** (the figlet title is the single
biggest "classical" signal). Consider a short before/after screenshot set via the `run` skill.
**Done.** A deliberate, reviewed palette/border pass; `--no-default-features` still builds; screens
still legible in the drive tests. This one is taste — expect iteration.
**Size.** M · one crate · subjective.

### C6 — Remove the tagline next to the app name (top-left)
**Problem.** You see a tagline by the name in the top-left. **In the committed tree the tagline is
only rendered in the Title overlay** (`runtime.rs:544`, `format!("{} {}", symbol, tagline)`) — the
header (`draw_header` `runtime.rs:287-294`) shows *only* the tracked name-word + breadcrumb + tabs. So
either you mean the `+` Title splash, or a **local uncommitted `runtime.rs` change** put it in the
header (that file is dirty right now).
**Spec.** None. Note P§4 legitimately lists the tagline as part of the **Title** overlay, so removing
it there is a spec-visible choice; removing a stray header render is not.
**Approach.** **Verify the render site first** (`git diff crates/porticus/src/runtime.rs`,
`grep -n tagline`). If it's a header render, drop that span. If you want it gone from the Title splash
too, drop `runtime.rs:544` (and optionally the `Ident.tagline` field, `ident.rs:19`, plus each core's
`tagline:` in `screen.rs`).
**Done.** No tagline by the name-word; Title overlay renders as intended by whatever you decide.
**Size.** S · one crate.

### C7 — Full-page title with a block/classical font
**Problem.** The Title overlay is a **small centered 3-line box** (`runtime.rs:539-549`, sized
`centred(area,72,…)` `runtime.rs:621`) showing the tracked name-word — **not** a full-page banner.
P§8 specifies the banner as figlet's **`roman`** face (big Trajan caps), rendered at runtime from
`ident.name`. It was deferred at step 6 for a licensing reason only: the face needs a vendored
third-party `.flf` that `cargo deny` can't vet (CLAUDE.md:302).
**Spec.** P§8 already *wants* this — no spec change to the intent. If you pick a font other than
`roman` (a "block" face), update P§8's "figlet `roman`" line to the chosen face.
**Approach.** Resolve the licensing call first (it's the whole reason it's deferred): pick a figlet
font under a `cargo deny`-clean license (public-domain / MIT / OFL) and vendor it, or render the caps
without a third-party `.flf`. Then rewrite the `Overlay::Title` arm (`runtime.rs:539-549`) to build
the multi-line banner and size it to the **full `area`** instead of `centred(…,72,…)`
(`runtime.rs:621`); source from `ident.name` (`ident.rs:68`). Keep it a summoned splash (P§4), not a
launch screen.
**Done.** `+` opens a full-page classical banner; `cargo deny check licenses` passes; headless build
unaffected.
**Size.** M · one crate · gated on a licensing decision (do late).

---

## Wave 2 — CLI contract (the seven cores)

### L1 — `list`/`ls` scoping, defaults, and node addressing
**Keep `ls` as-is** — it already works: every core carries `#[command(alias = "ls")]` and lists both
in `VERBS` (e.g. `album/src/cli.rs:166`, `:34`); §7.3 already writes `list` (`ls`). A rename to make
`ls` primary is wide, spec-visible churn (7 enums + 7 `VERBS` + the `&["list"]` callers in
`atrium/src/cli.rs:122` / `speculum/src/cli.rs:125` + ~6 spec refs) for no new capability — **don't**.
**The real problem is scoping, and it's a contract issue.** `cmd_list` always does a **subtree fold**
from the home (`pensum/src/cli.rs:486-498`: `fold(home, kind)` is subtree; `home = -H` else the node at
`$PWD`, falling to whole-tree when outside the tree). Three symptoms fall straight out, and the pattern
is **identical in all seven cores**:
- **`pen ls` returns nothing** — bare, `home` is the `$PWD` node; run anywhere but where the tasks
  live and its subtree is empty. The default depends on `$PWD` in a way that isn't obvious.
- **`pen ls a` errors** — `List` takes no positional (`Cmd::List { all }`, `pensum/src/cli.rs:141`),
  so clap rejects `a` as unexpected. You must spell `-H a`.
- **`pen ls -H a` is subtree-only** — there is **no node-local option**; you always get `a` + all its
  descendants, never just `a`'s own records.
**Spec.** §7.2 documents `list` as "folds across the subtree" — so subtree is the *documented* default,
and this item **adds** a node-local mode + fixes the bare default + adds node addressing. Update
§7.2/§7.3 to record the local-vs-subtree control and the bare-`ls` default.
**Approach.** (a) **Node-local flag** (e.g. `-l`/`--here`) using the spine's existing node-local fold
(`fold_local`, already used by the TUI's `count_at`); subtree stays the default per §7.2. (b) **Accept a
positional home** on `list` so `pen ls a` means node `a` — `list` has no other positional, so ambiguity
is low; at minimum improve the error to point at `-H`. (c) **Fix the bare default** to something
predictable — recommend subtree from `$PANTHEON_ROOT` when set ("all my tasks"), else the `$PWD` node —
and document it. Do it once in the shared shape; refreeze the snapshots (contract tests pipe, so the
JSON diff is the review surface).
**Done.** `pen ls` reliably shows tasks; `pen ls a` lists node `a`; `-l` limits to node-local; subtree
still reachable; snapshots refrozen; §7.2/§7.3 updated.
**Size.** M–L · all seven cores + spine + spec.

### L2 — Content search: use `rg`; `pan grep` is optional
**You don't need a new verb.** The tree is plain, legible files by design (I6; §18: *"Every byte …
sits in the tree at its node, legible"*, *"anything else can fold the same JSON the same way"*), so
content search is `rg`/`grep` over `$PANTHEON_ROOT` **today, zero code** — bodies included:
`rg 'pattern' "$PANTHEON_ROOT"`, scoped by glob (`-g '*.md'`), or one body via `tab get <slug> -f raw
| rg …`.
**Why not a core `grep` verb.** It would be a **thirteenth verb** — barred by §7.3, spec §18, CLAUDE.md
— *and* semantically uneven (only Tabella has bodies; other cores store structured `data`, not prose),
*and* an O(tree) live scan (no reverse index, §18).
**If you want contract-aware search later**, it goes on **`pan`, not a core** (`pan`/`aus` carry their
own structural verbs, §18-exempt): a `pan grep` reading bodies through the spine (`find_documents`
`store.rs:1170` + `read_document` `store.rs:1299`) and emitting structured `{home, core, slug, matches}`
— cleaner output than raw `rg`, but a convenience, not a capability you lack.
**Recommendation.** **Don't schedule it.** Use `rg`. Revisit `pan grep` only if raw-grep noise
(matching JSON punctuation / filenames / frontmatter keys) actually bites; then it's a §5.5/§10 + §18
spec note first, then a `Cmd::Grep` in `pan`.
**Size.** `rg`: zero. `pan grep` (if ever): M–L · spine + `pan` + spec.

### L3 — Errors render as raw JSON at a terminal
**Problem.** `dispatch`'s error branch **always** prints the JSON envelope and never consults the
format: `Err(e) => eprintln!("{}", e.to_error_json())` (`pantheon/src/contract.rs:90-93`). So
"format follows the hand" (§7.3, I8) — which the *success* path honours via `emit`/`format_is_json`
(`contract.rs:49,58`) — is **not applied to errors**: at a TTY you get `{"error":{"code":…,"msg":…}}`
instead of a readable line. Every core + `pan` + `aus` ends in `dispatch`, so it's one bug for the
whole suite.
**Spec.** §7.3 specifies the `{"error":{code,msg}}` envelope as the **piped** contract. Add a clause
that the error format follows the hand like every other output: **TTY → `error: <msg>` (human,
optionally with the code/§ ref); pipe → the JSON envelope unchanged.**
**Approach.** Gate the `Err` arm in `dispatch` on `as_json` (already computed and passed in): piped
keeps `to_error_json()`; at a TTY print a human line to stderr. Exit codes unchanged. Contract
snapshots pipe, so they're unaffected — add a TTY-path assertion if one is missing.
**Done.** Interactive errors read as `error: name the pensum record`; piped output stays the JSON
envelope; exit codes intact.
**Size.** S · one spot in the spine · whole suite.

---

## Wave 3 — Per-app display

### P1 — Pensum todos read badly
**Problem.** One line builds every Pensum row — `row()` at `pensum/src/screen.rs:218-224`:
`label: format!("{key}   {}", home.as_str())`. That is the whole complaint: **(a)** todo-key first,
node second; **(b)** node shown *after*; **(c)** `key` is the normalized slug, so underscores render
raw (`reach_out_to_alex`). Neither `rows_at` (`screen.rs:202`) nor `all_rows` (`screen.rs:210`) sorts;
the `Agenda` sorts by `when` then `label`, and tasks carry `when: None`, so they sort by task text,
never grouped by node (`agenda.rs:67`). The **same row shape** is in Atrium (`atrium/src/screen.rs:140`)
and Studium (`studium/src/screen.rs:243`).
**Spec.** None — display only, and I3 *wants* one render everywhere.
**Approach.** Reorder to node-first, de-underscore the task for display (`_`→space), and sort rows by
node then task. Because three apps share the shape, consider a **shared Porticus label helper** (a
`prettify(slug)` that swaps `_`→space) so the de-underscoring is one implementation for twelve (I3) —
but keep the *target* keyed on the real slug (search matches the visible label, the write uses the
stored key). Decide node-first ordering per view: the `TreeFile` rail is already node-scoped, so the
win is biggest on the cross-node `Agenda`.
**Done.** Pensum's list reads `actio · mars   reach out to alex`-style, grouped by node; Atrium/Studium
inherit it; screen tests updated.
**Size.** S–M · pensum (+ a small porticus helper, + atrium/studium).

### P2 — Insights show 1–2 letters and `pen done` shows nothing
**Problem.** Two separate defects:
1. **1–2 letters** — Pensum's "open by node" bars label with `home.as_str()`, the **node code**
   (`a`/`ac`/`acm`, 1–3 chars, §5.1) not the human `node.label` (`pensum/src/screen.rs:237`). *And*
   Porticus clamps bar width to `.clamp(1,8)` (`insights.rs:228`), so ratatui truncates any label to
   ~8 cols regardless. The rail has the real labels (`rail.rs:281`); the panels fold does not.
2. **`pen done` blank** — `panels()` **skips done tasks** (`if *is_done { continue }`
   `pensum/src/screen.rs:233`) and there is **no "done by node"** chart, only a scalar
   `Chart::Stat("done", …)` (`screen.rs:250`). So completing a task removes it from the only per-node
   chart and yields no insight.
**Spec.** P§3 defines the `Insights`/`Chart` vocabulary. If the fix needs node-scoping, note that
Porticus's `Insights` is a **Full draw-view that ignores the node cursor** (`insights.rs:88,102`) — a
genuinely "by node" insight needs the cursor node passed into the fold or the labels sourced from the
tree. That may warrant a small P§3 clause about how a chart gets legible node labels.
**Approach.** (a) Source bar labels from `node.label`, not the code — either give `panels()` access to
the tree, or resolve code→label in the fold. (b) Reconsider the `bar_width` clamp
(`insights.rs:228-236`) — allow wider labels or rotate/wrap. (c) Add a "done by node" (or open-vs-done
stacked) chart so completing a task *shows*. The label-source issue is **cross-core** — every core's
`panels()` uses the same `Chart::Bars` pattern (`annales/screen.rs:65`, `rationes:83`, `fasti:105`,
`mappa:76`, `tabella:77`, `album:76`) — so fix it once, centrally, if you can.
**Done.** Bars carry legible node names; marking done produces a visible per-node change; the
label-source fix is centralized.
**Size.** M · porticus + pensum (+ the other cores if centralized).

### P3 — Drop redundant columns from the table
**Problem.** The table renderer emits **a column per key** and never suppresses a column that is
constant across all rows (`table.rs:106` `columns()`; `ENVELOPE_ORDER` at `:22` lists `core`,`kind`,…).
So `pen ls` shows a **`core`** column (always `pensum` — you invoked `pen`) and a **`kind`** column
(always `task` — Pensum has one kind), both eating width and carrying no information. The JSON contract
still needs those keys (I4) — this is a **TTY-table-only** concern.
**Spec.** §7.3 (format follows the hand) / §8.7 — a display refinement, JSON unchanged. Note that the
TTY table elides constant columns.
**Approach.** In `grid()` (`table.rs:74`), **drop any envelope column whose value is identical across
every row** — general, no per-core knowledge (I5-clean): `core` always drops; `kind` drops for
single-kind cores but **stays for two-shape cores** (Fasti `span`/`event`, Rationes `holding`/`balance`,
where it varies) — exactly right, for free. Guards: **never drop the identity column** (`slug`/`key`)
or a hoisted `data.*` column even when constant (a single-row list must still show what it is), and only
suppress with more than one row. Escape hatch for the full set: `-f json`, or add a `-v`/`--verbose` that
forces every column at the TTY (thread it through `emit`).
**Done.** `pen ls` shows only the task-bearing columns; Fasti still shows `kind`; piped JSON unchanged;
a documented way to see all columns.
**Size.** M · spine (`table.rs`) + maybe a verbose flag · all apps at once.

---

## Wave 4 — Lenses: widen the write surface, then scope

### N1 — Lenses should let you act on all cores, not just a few
**Problem.** The lenses are **not** read-only (Atrium relays `Done/Edit/Remove` to `pen`
`atrium/src/screen.rs:77`; Studium relays a grade `add_form` `studium/src/screen.rs:111`). But each
offers a **narrow** action set on a **few** cores. What makes a lens able to write is exactly three
things and nothing more: the **Action set a view `offering(...)`s** + the **`on_action` verb mapping**
+ **`Writer::Subprocess` with the core in `relays_to`** (`porticus/src/action.rs:243`). To reach "all
cores, all the usual edits" you widen those three.
**Spec.** §12 already permits relaying human-initiated writes (line 5) — so **no new power**, only a
clarification. Add a sentence to §12 that a lens may relay the **full standard action set** across
**every core it reaches on `PATH`** (add/edit/done/remove/rename/move), each dimmed when its core is
absent (already the P§7 rule). This unblocks N2 and N3.
**Approach.** For Atrium (and the others), broaden each view's `offering(...)`, extend `on_action` to
build the right `Invocation` per `(Action, core)`, add `add_form`s where an `Action::Add` is offered,
and list every relayed core in `relays_to`. Reuse Studium's `add_form` + `Target::Node` pattern
(`studium/src/screen.rs:111-151`) as the template. Keep authorship in `on_action` only (a View holds
no Writer, `view.rs:1-13`).
**Done.** Atrium can add/edit/done/remove across the cores it folds, each action dimmed when its core
is off `PATH`; `tests/relay.rs`-style proofs for the new relays; §12 clarified.
**Size.** M · per-lens (do Atrium first as the reference, then reuse).

### N2 — Studium: scope to the studies subtree, node-scope its todos, switch programmes
**Problem.** Studium folds the **whole tree** — `Mosaic::faces` calls `figures(root, None)`
(`mosaic.rs:35`), `tasks` runs `pen list` with **no `-H`** (`screen.rs:235`), and only the `Courses`
view is scoped, and only to the transient rail cursor (`screen.rs:177`). There is **no study-root**
and **no programme-switch** state — §19.4 says outright *"nothing is stored to remember the choice"*;
scoping today is per-invocation `-H`/`-C` only. The per-programme `[code]_curriculum.toml` machinery
already exists (`curriculum.rs:151-195`, longest-prefix `governing()`), and `programmes(spans)` already
derives programme spans (`fold.rs:267`) — but nothing surfaces them as a chooser.
**Spec.** This is the **§19 change** the item requires. Amend §19.4/§19.6: Studium scopes to a
**studies subtree** and supports an **interactive programme switch** across discovered programme
spans / curricula, folding tasks and figures **within the active programme's node**, not tree-wide.
Reconcile with §19.4's "node-agnostic scope" wording — the CLI `-H` lever stays; the *screen* gains a
switch. Keep it inside §18's carve-out: the studies-root and active-programme are either **reference
data** (a curriculum file at a studies root) or **view state** (like Speculum's horizon), **never** a
behaviour config.
**Approach.** (a) Give the Mosaic/tasks fold a scope — a studies root and/or the active programme's
node — instead of `None` (`mosaic.rs:35`, `screen.rs:235` add `-H`). (b) Build a **programme switcher**
as a Tier-3 nav-key on the Mosaic (mirror Speculum's `w/n/[/]/t` horizon control,
`speculum/src/horizon.rs:189`), cycling the programmes `programmes()`/`curriculum::discover()` already
find; the active programme is view state. (c) Scope the Tasks view to the active programme's node so
unrelated todos vanish. Decide the studies-root mechanism (a curriculum file at a studies node is the
§18-clean option).
**Done.** Studium opens on the studies subtree, shows only that programme's tasks/figures, and a key
cycles programmes; GPA/credits recompute per active programme; §19 updated.
**Size.** L · studium + spec · the largest lens item.

### N3 — Speculum: survey yourself / log readings (the old aoaq)
**Problem.** Speculum folds a `Mosaic` + a dated `Horizon` and relays **only `Edit`/`Remove` on
existing rows** — `on_action` matches only `Target::Row`, and its `else` arm returns `None` ("a new
record is a core's to create, not a mirror's", `screen.rs:161-178`). There is **no `Add`, no
`add_form`, no `Target::Node`** — so no way to *log a new reading* (a mood, a metric, a
self-survey line).
**Spec.** §12 permits relayed writes; logging a reading is `ann <log> --at <date> <value>` (§8.6,
§19.8). Add a short §12 / Speculum note that the review lens may **relay a logging write** — a
human-initiated Annales reading — so "survey myself" is a documented relay, not a lens originating
data (still I2-clean: the hand asks, the core writes).
**Approach.** Add an `Action::Add` + `add_form` + `(Action::Add, Target::Node)` → `ann add …`/`ann
<log> --at …` mapping, modeled on Studium's `add_form` (`studium/src/screen.rs:111-151`). Decide the
survey UX: a set of log fields (the aoaq questions) that append dated Annales readings. Depends on
**N1** (the widened lens-write surface).
**Done.** From Speculum you can log a reading into an Annales series and see it fold back into the
horizon; relay proof test; §12/Speculum note added.
**Size.** M · speculum + spec · after N1.

---

## Wave 5 — Connectors: sync external sources into the tree (the plugin surface)

The plugin you want is a **Canvas sync tool** — pull your LMS files, deadlines, and grades straight
into the tree. That is not an in-app view plugin (loading external code into Porticus fights §18's
*"static linking only, no shared libraries"* — Rust has no stable ABI), and it is not a new core or
lens. It is a **connector**: a standalone binary that ingests an external source and **writes into the
existing cores through their CLIs** (I4), linking no core lib (I5). It is the **code hand** of I8 —
automating what a hand would type — run by you, not woken by a hook, so it is *not* a second Auspex
and needs no `PANTHEON_RULE` authority; it writes with your authority because you invoke it.

**Why it mostly fits already.** Standalone-binary + JSON-contract + `PATH`-discovery *is* the suite's
whole extension model. A connector shells `tab add`, `fas add`, `ann …`, and drops bulk files (§6.5)
at the right course nodes. Studium then folds those records like any other (§19.6) — **the connector
needs no Studium change**, because Studium already reads records uniformly across programmes (§19.7).
So the work is not "make Studium pluggable"; it is "make ingesting external structured data possible,
then write the Canvas connector."

### X0 — Close the `--data` / JSON-record ingest wall (the foundation)
**Problem.** Every core `add` builds a record from **typed positionals and flags only** — there is
**no way to feed a whole record** (arbitrary document frontmatter, a rich reading). CLAUDE.md names
this exact wall: *"no core's CLI can ingest an arbitrary record … closing it means giving cores a
`--data`/JSON-record path first (none exists)."* It is why Auspex **refuses** a `data`-bearing
proposal today, and it blocks any importer that carries more than a name + a couple of fields.
**Spec.** This is a **flag on `add`, not a thirteenth verb** — a core's own flags are explicitly
permitted (§18: *"that, plus the core's own flags, is a core's whole vocabulary"*) — so it stays
spec-clean, but it is spine + every-core surface and must be **snapshot-frozen** (I4). Update §7.3
(the shared grammar) and §8 to document the ingest flag and its validation.
**Approach.** Add a `--data <json>` (or `--json`) path to `add` that validates the record against the
core's `schema` and writes it, same envelope and plan token as a typed `add`. It must honour every
existing check (name normalization §5.1, within-node slug refusal §18, `deny_unknown_fields`). This
also **unblocks Auspex's minting-licensing half** (the `/series` grant slot) — do it here once, both
consumers benefit.
**Done.** `add --data '{…}'` on all seven cores, validated against each core's
own published schema; Auspex applies a `data` proposal (`apply.rs`'s refusal is gone, and the
`/series` mint-licensing half unblocked with it); §7.3, §9.3 and §18 updated. **No snapshot
churned** — the `help` snapshots list verbs and kinds, not flags — so the anticipated refreeze cost
nothing. Two things the plan did not foresee: only the two-shape cores carried
`deny_unknown_fields`, so the ingest path needed an unknown-key check of its own or a typo'd field
would write an empty record at exit `0`; and on a two-shape core the record has to name its own
shape, since the positional pattern that usually discriminates is unavailable once the figure is
inside the record. **X1 is unblocked.**
**Size.** L · spine + all seven cores + spec · the reusable unlock.

### X1 — The Canvas connector
**Problem.** You want Canvas files/deadlines/grades in your tree, and a repeatable way to add more
sources later.
**Spec.** Add a **connector** shape to §4 (the architecture): external tooling that writes via the
contract, links no core (I5), and is discovered like any tool (`version -f json`, so `pan doctor` can
see it) *or* run privately as a script. A short §19.6 note that a study life's Canvas import is a
connector, not a lens or a daemon (§19.6 already says export/git are *"the hand's own over an ordinary
directory"* — this is the ingest twin). Record the **course→node mapping** as a §18 carve-out:
reference data (`canvas_course_id = …` in `[code]_curriculum.toml`, or a sibling `[code]_canvas.toml`)
homed at the course/programme node — a datum a tool reads, never a behaviour knob.
**Approach.** A new binary (`crates/canvas`, or out-of-tree) that: authenticates to Canvas (**token
via env var / OS keychain — never stored in the tree**, §18 "nothing hidden, nothing outside the
tree"), reads the course→node map, then for each course pulls **files → bulk at the node** (§6.5)
and/or Tabella documents (via **X0**'s `--data`), **assignments/exams → `fas add` events** (§8.4,
referencing the enrolment span), **grades → `ann` grade facts** (§19.2), **enrolments → `fas` spans**.
Depends on `pantheon` for the contract types; shells the core CLIs (I4), links no core (I5). Idempotent
re-sync = the core's overwrite (a stable slug per Canvas item), exactly as Auspex's re-runs are.
**Done.** `canvas sync` (hand-run) lands your Canvas materials as records/bulk at the right nodes, and
Studium folds them with no change; re-running is idempotent; §4/§18/§19.6 updated.
**Size.** L · new crate + spec · gated on **X0**.

**Later connectors reuse this.** A bank feed → Rationes readings, a calendar → Fasti events — the same
connector shape over X0's ingest path. Documenting X1's contract *is* the plugin surface.

---

## Wave 6 — Everything else missing (from the audit)

An audit swept `crates/` + the spec for every stub, deferral, and spec-claimed-but-unbuilt feature.
**The tree is exceptionally clean:** zero `TODO`/`FIXME`/`HACK`, zero `#[ignore]` tests, zero
`#[allow(dead_code)]`, and exactly **one** hard stub. So "missing" here is mostly *deliberately-deferred
lens breadth* (Studium) and a handful of spec'd-but-unbuilt cascade/chrome features — each documented
in-place. Split by whether to schedule it.

### Buildable now — **all done**; kept as the record of what each was

Every item below is built (G8 landed with Wave 4; G1–G7 in the Wave 6 pass). The three
stale comments named at the end of this section are corrected. What each settled that a
later change must not undo is recorded in CLAUDE.md's Wave 6 section, not here.

- **G1 ✅ — Studium §19.5 terms/periods + the "P6" absolute period label.** `curriculum.rs:52-54`
  parses the scales but **silently drops** `periods_per_year`/`terms`/`periods`; none of §19.5 exists
  — no `(study_year−1)×periods_per_year + n` label, no study-year derivation, no multi-period `P3–P4`
  interval read. The data is already in the file; nothing reads it. *studium · M.*
- **G2 ✅ — Studium §19.6 derivations that are absent.** People (an **Album** contacts fold) and
  Reflections (**Tabella** `type=reflection`) are **entirely unread** — Studium touches only `fas`/`ann`
  (`fold.rs:21`). Deadlines/exams is **partial**: only a single `next_exam` (`fold.rs:208`), no "next
  28 days" list, no Calendar drop, and the exam-vs-deadline split is deferred (`fold.rs:207`). The
  lineup has only three views (`screen.rs:64`) against §19.6's six tabs — no Timeline/Calendar/
  study-time/people/reflections views. *studium · L · pairs with N2.*
- **G3 ✅ — Studium's missing relays + first-grade mint.** §19.8 lists five relays; only two are wired
  (`pen edit --done`, `ann add` — `screen.rs:132`). **Close-enrolment** (`fas edit --to`),
  **log-study-time** (`ann … --at`), and **place-exam** (`fas add` event) are unbuilt, and recording a
  **first** grade is deferred (`screen.rs:110` — only retake/correction today; a first mint needs
  `ann -c` or **X0**'s ingest path). *studium · M · folds into N1/N2; the first-grade mint may wait on X0.*
- **G4 ✅ — Auspex `writes=` rule-header cascade on recode.** `pan`'s node cascade renames rule *files*
  by prefix but never rewrites the invalidated `writes=core@home` grant tokens *inside* a header
  (`node_ops.rs:405`), plus the paired "dead code in a header" validate finding is unbuilt. The comment
  says "no header parser exists yet" — **stale**: it does now (`auspex/src/grant.rs:63`), so the parser
  can be reused. *pantheon + pan + auspex · M.*
- **G5 ✅ — Same-core ref-chip follow (P§3).** Chips are display-only (`entity_card.rs:24`, echoed in
  every core screen). A **cross-core** follow is barred forever by I5, but a **same-core** jump is "a
  natural later jump." *porticus · S–M.*
- **G6 ✅ — `pan` TUI `m` (move) is dark.** No destination prompt (`pan/src/screen.rs:124`, pinned dark by
  `pan/tests/screen.rs:129`). The pick-a-home tree modal **now exists** (`runtime.rs:257`, `Overlay::Tree`)
  — wire it to a `pan mv` relay. *pan · S.*
- **G7 ✅ — Validate genuine-choice candidate fixes (§10.2).** The single-fix apply landed (`pan` screen
  `d` relays the rename), but the spine emits a fix only for the one unambiguous rename shape;
  **multi-candidate findings carry `None` and offer nothing** (`validate.rs:25`). Needs the spine to
  produce candidate lists. *pantheon + pan · M.*
- **G8 ✅ — `RecordRef` gains a `core` slot.** Speculum re-reads all dated cores to recover which binary
  owns a row (`speculum/src/screen.rs:62`, `core_of`) — a documented Porticus gap; two cores filing the
  same date-key at one node would misroute the relay. Adding a `core` to `RecordRef` hardens it and
  simplifies every lens relay. *porticus + pantheon · M · helps N1/N3.*

### Blocked-by-design — do NOT schedule until the release phase

- **`pan migrate`** (`pan/src/cli.rs:305`, the one hard stub) — shape-directed and idempotent, it can
  only rewrite *from* a prior format version, so it waits on the first release (§16 step 11).
- **The `--data` ingest wall / Auspex's `/series` minting-licensing half** — this is **X0** above,
  promoted to a scheduled foundation because a connector needs it; until X0 lands, Auspex rightly
  refuses `data`-bearing proposals (`auspex/src/apply.rs:71`).
- **The figlet banner** — this is **C7**, gated on the `.flf` licensing call, not code.

### Already resolved — don't re-file (CLAUDE.md/older notes are stale here)

`Pick` is now the tree-as-modal (`Overlay::Tree`), not a line prompt; the single-fix validate apply
landed; `Calendar`/`Timeline` exist. The three **stale comments/spec lines** this section used to
name are all corrected: `docs/src/spec/19-studium.md` now says a `curriculum.toml` routes to `Bulk`,
`node_ops.rs` no longer claims "no header parser exists yet" (the spine owns one), and `validate.rs`
no longer says the mutators "are still stubbed".

---

## Suggested order

Wave 0 (C1, C2) → Wave 1 (C3, C4, C5, C6) → Wave 2 (L1, L3; **L2 = just use `rg`**, nothing to build) →
Wave 3 (P1, P2, P3) → Wave 4 (N1 → N2, N3) → **Wave 6 (G1–G8, done)** → Wave 5 (X0 → X1). **L3** (human errors) and **P3** (drop
redundant columns) are tiny, high-daily-value CLI wins — pull them early with C1/C2 if you want the
terminal usable first. **Wave 6** items are independent and slot in wherever they fit — G2/G3 alongside
N2 (all Studium), G8 before N1/N3 (it hardens every lens relay). **C7** (figlet
title) is cosmetic and gated on a licensing call — slot it whenever the license is settled, ideally
alongside C5. **X0** (the ingest wall) is foundational and also unblocks Auspex, so it can be pulled
earlier if a connector or a rich importer becomes urgent. Each item is self-contained; hand Claude
Code one at a time, and start it by reading the spec chapter the item names.
