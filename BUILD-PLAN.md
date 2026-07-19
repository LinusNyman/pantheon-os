# Build plan — how to go about building PantheonOS

Spec §16 (`docs/src/spec/16-build-order.md`) fixes the **order** and the **why**. This document is
the **how**: the working method for moving through that order without painting later steps into a corner.
It is process, not spec — edit it as we learn.

## The two principles

### 1. "Done" splits in two — nail the contract, leave the polish

Every step has two kinds of done, and they are not equal:

- **Contract-done** — the JSON output (I4), the verb grammar, the file→core mapping, the record
  envelope, the exit codes. This is what every *later* step is built against. Get it wrong and the
  cost cascades across the whole build.
- **Polish-done** — internals, error message wording, edge cases, ergonomics, TUI chrome.

**Rule: bring each step to contract-done against its spec chapter before moving on. Let polish-done
stay rough.** Spend the planning budget on the contract surface, not the guts. The spec already bakes
this in — cores land **CLI-first**, a bare short prints `help` until its TUI exists (§7.3). The rough
v1 is intended; the roughness is just sequenced so contracts solidify first and chrome arrives at step 6.

Freeze each contract with an `insta` snapshot the moment it is right (§7.2). After that, any drift is a
visible diff in review — that is the mechanism that lets "rough internals" stay safe.

### 2. The slice is ONE milestone; breadth is independent

§16 steps 1–6 are a single **vertical slice**: spine → three storage shapes → first screen. "The slice
closes here" (§16 step 6). Do **not** call step 1 truly done until step 6 renders from it end-to-end.
The screen is the integration test that proves the contract was right.

- **Steps 1–6 — one milestone, one closing demo.** Move *through* them fast and rough, but do not
  move *on* until a real screen reads real records through the cores. Expect to reach back and fix an
  earlier core's contract when the screen exposes a hole — that is the design working, not a failure.
- **Steps 7–10 — genuinely independent.** These hang off a proven contract, so here the instinct
  "plan one, build v1, ship, next" is exactly right. Each core/lens is its own small milestone.

## Per-step cadence

For each step, the same loop:

1. **Plan the contract first (~½ page).** Before any code, write this step's JSON shapes + verb
   grammar, checked against its §8 (core) or §5/§9 (system) chapter. This is the "plan for each step."
2. **Build v1, CLI-only.** Implement over the `Core` trait (§7.1). Bare short = `help`. No TUI yet.
3. **Verify the contract end-to-end.** `pan tree` / `resolve` / `validate` must read back what the
   core wrote. Snapshot the JSON with `insta`. Run the full CI command set locally (see CLAUDE.md).
4. **Gate:** contract-done + green CI + snapshot committed → move on. Skip polish.
5. **Commit** as a small, reviewable unit. Keep the repo public-and-green from the first commit (§16 preamble).

## The steps, annotated

Grouped into the three phases the method implies. Each step's own detail is in §16 — here is what
"contract-done" and the gate mean at each.

### Phase A — the vertical slice (steps 1–6, one milestone)

1. **Pantheon + `pan`** — the spine (addressing, resolution, record envelope, `Core` substrate,
   write-time validation) and the thin bin over it. *Contract-done:* `pan new` mints a tree; `pan tree`
   / `resolve` / `validate` read it back; the `Core` trait compiles and is snapshot-testable. First
   binary → `dist-workspace.toml` and CI's `dist plan` land here.
2. **Annales** — simplest core, one hand-named `log` series. *Contract-done:* proves I1, the Series
   shape, and the twelve-verb contract on one core.
3. **Album** — first partitioned register. *Contract-done:* the second shape, `core:slug` refs, the
   resolver's filename path, the rename cascade (§5.4).
4. **Pensum** — the nameless `task` series (determined-name path) + the record lock under contention (§6.4).
5. **Tabella** — the Document shape (no tokens, extension-only mapping); makes `pan doctor`'s totality
   claim demonstrable. *Contract-done:* the third shape, the `+++` codec, the editor form in place,
   `-f raw`, and `pan doctor` reporting the map total with one core declaring nothing.
6. **Porticus + Tessera + Atrium** — the first screen. *Gate for the whole slice:* a real Atrium screen
   renders derived-out (I1) and relays a human write back through a core (I2). **Now circle back** and
   fix whatever the screen exposed in steps 1–5. The slice closes: three shapes, one contract, one screen.

### Phase B — breadth against the proven contract (steps 7–9, independent milestones)

7. **Mappa, Rationes, Fasti** — the cores the slice did not need. Fasti brings the two-token core
   (`span` + `event`), Rationes the determined-name series.
8. **Auspex** — the first rule (`plan` before `run`), the propose protocol, the capability grant. Lands
   after every core exists so the capability surface (`core@home`) is written once against the whole.
9. **Speculum, Studium** — the cross-core lenses; not buildable before step 7.

### Phase C — ship (step 10)

10. **Releases** — `release-plz`, per-crate tags, first tagged `dist` run — once an app is worth installing.

## What to defer (don't let it block a step)

- TUIs for cores 2–5 (they come at step 6 / are chrome — §7.3).
- Error-message polish, verbose/quiet niceties, `-f` view tuning.
- Cross-target build fiddling beyond what CI already does.
- Anything in §18 — if it feels convenient, it is probably a non-goal.

## What must NOT slip to "later"

- The JSON contract, verb grammar, exit codes, and file→core mapping of the step you are on.
- Green CI (fmt + pedantic clippy + tests + audit/deny) on every commit.
- The `insta` snapshot that freezes the contract.
