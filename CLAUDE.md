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

## Status — build order steps 1–7 are done (§16); all seven cores exist

**Built and green:** `pantheon` + `pan` (step 1), `annales` (2), `album` (3), `pensum` (4),
`tabella` (5), `porticus` + `tessera` + `atrium` (6), `mappa` + `rationes` + `fasti` (7).
**All three storage shapes exist** — Partitioned, Series in *both* its hand-named and nameless
forms, and Document — plus the `core:slug` resolver, the record-level rename cascade, and the
record lock under contention.

**The vertical slice closed at step 6**, which is what it was for: a real screen renders
derived-out (I1) and relays a human write back through a core (I2, §12) — `d` on an Atrium row
runs `pen edit … --done -y` and `pen list` reads it back from another process. Nine instruments
have TUIs (`pan`, `atr`, `alb`, `ann`, `pen`, `tab`, `map`, `rat`, `fas`); the table renderer
fills §7.3's "TTY → table"; `cargo xtask seed` mints a tree to look at.

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

**Still scaffold** — a stub printing a not-implemented line: `auspex` (8), `speculum` and
`studium` (10). **Next real work is step 8** — Auspex, the one reactive writer (I2, §9).
Step 9 is now a **cleanups** pass — the deferrals steps 1–8 left (pan's node cascade §10.1,
`pan migrate`, validate's candidate fixes §10.2, the figlet banner, `Pick` as tree-as-modal,
nested `data` render) plus the chrome debts hands-on use surfaced (**no add form** — `a`
relays a nameless `add` with no field prompt; responsive top/bottom split and a narrower
rail; passive overlays yielding to a nav key; header showing the node not the whole trail).
Lenses and releases shift to 10 and 11.

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

- **`pan`'s node-level cascade (§10.1) is still stubbed.** Its six structural mutators
  (`mv`, `rm`, `rename`, `rename-prefix`, `rename-pattern`, `mv-file`) return not-implemented,
  so `r`/`m`/`x` are **dark** in `pan`'s TUI — `on_action` returns `None` and Porticus greys
  them (P§7). The *record*-level cascade (§5.4) is done and is what the cores use.
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
- **The dim asks `any_at`, the badge asks `count_at`** (P§6). Two questions on purpose: an
  instrument whose count is costly overrides `any_at` and the dim stays cheap. Collapse them and
  that override becomes unreachable. The default `any_at` counts, so a node holding records is
  folded twice a frame — the cost P§6 tells a costly instrument to override away.
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
