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
- `18-non-goals.md` — **what must NOT be built.** Read early; it fences the design.

`docs/src/APPENDIX-A-NAMES.md` explains the Latin naming.

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

Every tool has a three-char short and is both a CLI emitting JSON and a Porticus TUI. Cores land
**CLI-first**: a bare short prints `help` until its TUI exists (§7.3).

## Workspace layout

Single public Cargo workspace (monorepo forced by I5). Members: `crates/*` and `xtask`.

- `crates/pantheon` — the spine lib. `crates/pan` — the thin bin over it, its own crate.
- `crates/porticus`, `crates/tessera` — the peer libs.
- `crates/<core>` — one crate per core; `lib.rs` holds the logic, `main.rs` the thin bin.
- `xtask/` — workspace automation (run via `cargo xtask`).
- `docs/` — the mdBook spec. `deny.toml`, `dist-workspace.toml`, `release-plz.toml` — supply chain & release.

## Status — build order steps 1–4 are done (§16)

**Built and green:** `pantheon` + `pan` (step 1), `annales` (2), `album` (3), `pensum` (4).
That is two of the three storage shapes — Partitioned, and Series in *both* its hand-named
and nameless forms — plus the `core:slug` resolver, the record-level rename cascade, and the
record lock under real contention. **Document is the shape still unbuilt** (§6.1), which is
what step 5 is for, and why `Store` has no document path at all.

**Still scaffold** — a stub printing a not-implemented line: `tabella` (step 5), `porticus`
and `tessera` and `atrium` (6), `mappa`/`rationes`/`fasti` (7), `auspex` (8), `speculum` and
`studium` (9). Next real work is **step 5, Tabella** — the Document shape, the one file→core
mapping resting on extension alone.

Two things a later step must not be surprised by:

- **`pan`'s node-level cascade (§10.1) is still stubbed.** Its six structural mutators
  (`mv`, `rm`, `rename`, `rename-prefix`, `rename-pattern`, `mv-file`) return not-implemented.
  The *record*-level cascade (§5.4) is done and is what the cores use.
- **`Store::write_line` mints any `Shape::Series { named: false }` series on first write**
  (§7.3: a determined series is minted by its determinant). For Pensum the determinant is the
  node, so that is right. **Rationes' `balance` is determined by a holding *entity*** — so `rat`
  must check that entity exists in its own bin before writing. The store links no core and
  cannot know (I5).

## Commands (match CI exactly — see `.github/workflows/ci.yml`)

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -W clippy::pedantic -D warnings
cargo build --workspace --bins                                 # REQUIRED before tests: a core's contract
                                                               # test drives another tool's binary (`alb` writes,
                                                               # `pan resolve` reads back), and cargo builds no
                                                               # bin for a crate that is not under test
cargo nextest run --workspace --all-features --no-tests=pass   # falls back to `cargo test` if nextest absent
cargo build --workspace                                        # CI also cross-builds 5 targets
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
- **Format follows the hand:** TTY → table, piped → JSON, same code path.

## Non-goals (§18) — do not build

No undo/history layer, no central store or cache, no reverse index, no file watcher, no autonomy/boldness
knob, no per-app editor env var, no thirteenth core verb. When a feature feels convenient, check §18 first.
