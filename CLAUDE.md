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
**CLI-first** until their TUI exists (§7.3) — and "prints `help`", the phrasing §16 uses, is loose
about what the code does: *piped* emits `help_json()`, but a **TTY** gets the one-line `BARE`
banner pointing at `--help`. It is the TTY arm step 6 replaces with the TUI.

## Workspace layout

Single public Cargo workspace (monorepo forced by I5). Members: `crates/*` and `xtask`.

- `crates/pantheon` — the spine lib (~5.4k lines; nearly all the logic). `crates/pan` — the bin over
  it, its own crate.
- `crates/porticus`, `crates/tessera` — the peer libs.
- `crates/<core>` — one crate per core. **`lib.rs` is the thin file, `main.rs` the fat one** — the
  reverse of what §14's "~30-line clap shell" suggests, and the shape all four built cores share.
  `lib.rs` (66–148 lines) holds only the record struct(s) and `impl Core`; `main.rs` (771–941) holds
  the clap `Cli`, the twelve verbs, `Ctx`, the editor form, and the tail helpers. Put verb logic in
  `main.rs` — the spine already owns everything a core would otherwise share.
- `xtask/` — workspace automation (run via `cargo xtask`).
- `docs/` — the mdBook spec. `deny.toml`, `dist-workspace.toml`, `release-plz.toml` — supply chain & release.

## Status — build order steps 1–5 are done (§16)

**Built and green:** `pantheon` + `pan` (step 1), `annales` (2), `album` (3), `pensum` (4),
`tabella` (5). **All three storage shapes now exist** — Partitioned, Series in *both* its
hand-named and nameless forms, and Document — plus the `core:slug` resolver, the record-level
rename cascade, and the record lock under real contention. `pan doctor` is wired (§5.5) and
reports the file→core map total, which is what Tabella makes demonstrable: it declares no
tokens, so its files reach it by extension alone (§7.1).

**Still scaffold** — a stub printing a not-implemented line: `porticus` and `tessera` and
`atrium` (6), `mappa`/`rationes`/`fasti` (7), `auspex` (8), `speculum` and `studium` (9).
Next real work is **step 6, the first screen** — which is also the gate for the whole
vertical slice, and where you circle back and fix whatever the screen exposed in steps 1–5.

Seven things a later step must not be surprised by (the durable rules that came out of
step 5 live in Conventions below, not here — this list is about what is *unfinished*):

- **`pan`'s node-level cascade (§10.1) is still stubbed.** Its six structural mutators
  (`mv`, `rm`, `rename`, `rename-prefix`, `rename-pattern`, `mv-file`) return not-implemented.
  The *record*-level cascade (§5.4) is done and is what the cores use.
- **`pan constitution` (§5.5) is stubbed too, and its message says "step 6"** — unlike the six
  above, which point at §10.1 rather than a step number. It is the one `pan` verb step 6 owes.
- **`pan`'s bare short ignores the TTY rule.** It returns `RunOk::Raw` unconditionally, so a
  *piped* `pan` emits prose where every core emits `help_json()`. §7.3 governs `pan` too.
- **The `tui` feature is decorative.** Nine crates declare `default = ["tui"]` with
  `porticus`/`tessera` as optional deps, and there is **not one `#[cfg(feature = "tui")]` in the
  workspace** — today they link two empty crates and use nothing from them. Whoever writes the
  first real TUI writes the `cfg` blocks, or the feature keeps meaning nothing. `atrium` has the
  opposite bug: it takes both libs *unconditionally*, so `--no-default-features` cannot strip the
  screen from a lens the way §14 and §12 say it must.
- **`pan` carries a second copy of `emit`** (`crates/pan/src/render.rs`), byte-identical to
  `pantheon::contract::emit`. The table renderer §7.3 owes has two plug points, not one; collapse
  them before filling either.
- **`Store::write_line` mints any `Shape::Series { named: false }` series on first write**
  (§7.3: a determined series is minted by its determinant). For Pensum the determinant is the
  node, so that is right. **Rationes' `balance` is determined by a holding *entity*** — so `rat`
  must check that entity exists in its own bin before writing. The store links no core and
  cannot know (I5).
- **`plan_cascade` cannot refuse an occupied slug for a Document core**, and this is by design
  rather than a bug: it gates that check on the caller's own tokens, and Tabella declares none —
  and it walks meta dirs, where no document lives. So **Tabella makes the check itself**
  (`find_documents` tree-wide, then `pantheon::occupied_slug` for the shared wording). Any
  future Document core must do the same, or a rename will silently produce two records with
  one name. `tabella/tests/contract.rs::refusal_rename_onto_an_occupied_slug` guards it.

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
- **All TOML is `toml_edit`'s, and frontmatter is never re-serialized** (§6.6). `pantheon::document`
  owns the `+++` fence; `Document` carries `front_raw`, the fence's original TOML, and a rewrite edits
  *that* `DocumentMut` and re-emits. Rebuilding the fence from `Frontmatter`'s two fields instead would
  silently destroy a hand's comments, its key ordering, and every key Tabella does not read — the exact
  thing §6.6 keeps `toml_edit` for. Same rule for `[code]__.toml` (`meta.rs`).
- **A fold never reads bodies** (§6.1, §7.1, §7.2, §8.7 — the spec says it four times). `list` uses
  `document::read_frontmatter`, which stops at the closing fence. Reading the whole file and discarding
  the prose satisfies the letter and not the thing.
- **Format follows the hand:** TTY → pretty, piped → compact, same code path (`contract::emit`).
  §7.3 says TTY → *table*, and **there is no table renderer yet** — a TTY currently gets
  pretty-printed JSON. That is step 6's job, with the rest of the chrome.

## Non-goals (§18) — do not build

No undo/history layer, no central store or cache, no reverse index, no file watcher, no autonomy/boldness
knob, no per-app editor env var, no thirteenth core verb. When a feature feels convenient, check §18 first.
