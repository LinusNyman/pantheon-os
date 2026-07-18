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

- `crates/pantheon` — spine lib + `pan` bin. `crates/porticus`, `crates/tessera` — the peer libs.
- `crates/<core>` — one crate per core; `lib.rs` holds the logic, `main.rs` the thin bin.
- `xtask/` — workspace automation (run via `cargo xtask`).
- `docs/` — the mdBook spec. `deny.toml`, `dist-workspace.toml`, `release-plz.toml` — supply chain & release.

**Status: everything is scaffold.** All crate sources are stubs printing a not-implemented line.
Build order step 1 (Pantheon spine + `pan`) is the next real work.

## Commands (match CI exactly — see `.github/workflows/ci.yml`)

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -W clippy::pedantic -D warnings
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
- **The contract is frozen by `insta` snapshots** (`key` and plan tokens redacted). Any change to a core's
  JSON output is a visible snapshot diff in review — regenerate deliberately, never blindly `cargo insta accept`.
- **Name normalization is one rule** (§5.1): lowercase, NFC, alphanumeric+`_`, fold space/`-` to `_`,
  collapse and strip `_`. NFC is not optional (macOS/Linux byte disagreement). Apply on write, compare NFC on read.
- **Exit codes are contract** (§7.3): `0` ok · `1` runtime · `2` usage · `3` validation · `4` not found ·
  `5` confirm required · `6` write refused under a rule. Errors print `{"error":{"code":…,"msg":…}}` to stderr.
- **Format follows the hand:** TTY → table, piped → JSON, same code path.

## Non-goals (§18) — do not build

No undo/history layer, no central store or cache, no reverse index, no file watcher, no autonomy/boldness
knob, no per-app editor env var, no thirteenth core verb. When a feature feels convenient, check §18 first.
