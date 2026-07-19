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

Every tool has a three-char short and is both a CLI emitting JSON and a Porticus TUI. **A bare
short opens the screen at a TTY and emits `help` as JSON down a pipe** (§7.3) — a screen has
nothing to draw down a pipe. A crate built `--no-default-features` has no screen to open, so its
bare short prints a banner pointing at `--help` instead; that is the only path the old
CLI-first behaviour survives on.

## Workspace layout

Single public Cargo workspace (monorepo forced by I5). Members: `crates/*` and `xtask`.

- `crates/pantheon` — the spine lib (~5.7k lines; nearly all the logic). `crates/pan` — the bin over
  it, its own crate, now with its own `screen.rs` too.
- `crates/porticus` (~3.8k lines), `crates/tessera` — the peer libs. Porticus links `ratatui` whole;
  Tessera takes `ratatui-core` and links no Porticus, now or ever.
- `crates/<core>` — one crate per core, **three files**. **`lib.rs` is the thin file, `main.rs` the
  fat one** — the reverse of what §14's "~30-line clap shell" suggests, and the shape all four
  built cores share. `lib.rs` (66–148 lines) holds only the record struct(s) and `impl Core`;
  `main.rs` (784–954) holds the clap `Cli`, the twelve verbs, `Ctx`, the editor form, and the tail
  helpers; `screen.rs` (197–244, behind `tui`) holds `impl App` and the folds its views close over.
  Put verb logic in `main.rs` — the spine already owns everything a core would otherwise share.
- `xtask/` — workspace automation (run via `cargo xtask`).
- `docs/` — the mdBook spec. `deny.toml`, `dist-workspace.toml`, `release-plz.toml` — supply chain & release.

## Status — build order steps 1–6 are done (§16); the slice is closed

**Built and green:** `pantheon` + `pan` (step 1), `annales` (2), `album` (3), `pensum` (4),
`tabella` (5), `porticus` + `tessera` + `atrium` (6). **All three storage shapes exist** —
Partitioned, Series in *both* its hand-named and nameless forms, and Document — plus the
`core:slug` resolver, the record-level rename cascade, and the record lock under contention.

**The vertical slice closed at step 6**, which is what it was for: a real screen renders
derived-out (I1) and relays a human write back through a core (I2, §12) — `d` on an Atrium row
runs `pen edit … --done -y` and `pen list` reads it back from another process. Six instruments
have TUIs (`pan`, `atr`, `alb`, `ann`, `pen`, `tab`); the table renderer fills §7.3's "TTY →
table"; `cargo xtask seed` mints a tree to look at.

**Still scaffold** — a stub printing a not-implemented line: `mappa`/`rationes`/`fasti` (7),
`auspex` (8), `speculum` and `studium` (9). **Next real work is step 7** — the three cores the
slice did not need, against a contract a screen has now exercised.

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
- **`Calendar` and `Timeline`** wait for Fasti at step 7 — nothing would fill them before it.

### Things a later step must not be surprised by

- **`pan`'s node-level cascade (§10.1) is still stubbed.** Its six structural mutators
  (`mv`, `rm`, `rename`, `rename-prefix`, `rename-pattern`, `mv-file`) return not-implemented,
  so `r`/`m`/`x` are **dark** in `pan`'s TUI — `on_action` returns `None` and Porticus greys
  them (P§7). The *record*-level cascade (§5.4) is done and is what the cores use.
- **`Store::write_line` mints any `Shape::Series { named: false }` series on first write**
  (§7.3: a determined series is minted by its determinant). For Pensum the determinant is the
  node, so that is right. **Rationes' `balance` is determined by a holding *entity*** — so `rat`
  must check that entity exists in its own bin before writing. The store links no core and
  cannot know (I5). **This bites at step 7.**
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
- **Format follows the hand:** TTY → table, piped → compact JSON, one code path
  (`contract::emit` → `pantheon::table`). The renderer lives in the **spine**, not Porticus: a
  bin built `--no-default-features` drops the chrome and is still a CLI that must table (§14).
  It knows no core (I5) — columns are whatever keys the value carries, with `data`'s hoisted —
  and it **declines what it cannot honestly flatten**: `pan tree` nests nodes, `schema` nests a
  schema, so those fall back to pretty JSON. The flatness test is deliberately *not* recursive.
  No contract snapshot covers any of this, because every contract test pipes.

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
  only by driving a real binary; two more fell out within minutes of `drive` existing.

## Non-goals (§18) — do not build

No undo/history layer, no central store or cache, no reverse index, no file watcher, no autonomy/boldness
knob, no per-app editor env var, no thirteenth core verb. When a feature feels convenient, check §18 first.
