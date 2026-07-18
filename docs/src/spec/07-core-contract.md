## 7. The Core Contract

**The contract is the CLI's JSON** (I4). Nothing else crosses a component boundary; per I5 no core imports another. The `Core` trait is internal scaffold so every core produces that JSON the same way.

### 7.1 The trait (internal scaffold)

`pantheon::contract` implements the verb set generically over a `Store<Record>` (the subtree-walk of §6.3). A core declares only what is core-specific:

```rust
enum Shape {
    Partitioned,            // one .json per entity, kind (+slug) in filename (Album, Mappa, Rationes, Fasti spans)
    Series { named: bool }, // one .jsonl per collection, many keyed lines. named: a hand-chosen ref target
                            //   (Annales `log`, Fasti `event`); else determined — a co-located slug (Rationes
                            //   `balance`) or nameless (Pensum `task`), and never a ref target (§5.4)
    Document,               // one text file per document, TOML frontmatter over a prose body (Tabella)
}

trait Core {
    type Record: Serialize + DeserializeOwned + JsonSchema;  // the `data` shape; an enum where a core has more than one
    const NAME: &str;                                        // "album" — the `core:` half of a ref
    fn kinds() -> &'static [(&'static str, Shape)] { &[] }   // token → shape; empty for a Document core
    fn validate(r: &Self::Record) -> Result<()>;             // checks beyond the envelope
}
```

From that, every core gets: `write` (validate, then — by the token's shape — rewrite an entity `.json`, `add`/`edit`/`rm` a keyed line in a `.jsonl` series, or rewrite a document's text file in place, each under the file lock of §6.4), `rename`/`move`/`rm`, `fold` (subtree walk → present), `series` (read a collection), and `schema` (via `schemars`). Pantheon owns resolution and ref validation; a core is a record type plus its primitive, its tokens, and its `validate`.

**`kinds()` names the files, and nothing else names a shape.** Every token a core declares is a legal filename segment and carries the shape it is stored in — for most cores uniformly (Album's `person`/`organization`/`group` are all `Partitioned`; Annales' `log` is `Series`). A core whose primitive brings a second shape with it says so on the token: Fasti's `span` is `Partitioned` and its `event` a `Series`, Rationes' `account`/`asset`/`claim` are `Partitioned` and its `balance` a `Series`. There is no second declaration and no rule to remember — a core has tokens, each token has a shape, and a core's *primitive* is which tokens it chose rather than a field it states: a two-token core has no one primitive to declare, which is why nothing asks it for one.

**A `Series` carries one bit beyond its shape: `named`.** A **hand-named** series (`named: true` — Annales' `log`, Fasti's `event`) is a ref target, so its name is unique per core and home-free (`annales:meetings`, §5.4). A **determined** series (`named: false`) has no name of its own: it is a co-located entity's slug (Rationes' `balance`, whose name *is* a holding's slug) or nameless (Pensum's `task`, one per node). That one bit is what the spine needs to read two files that split identically — `ecv__log__meetings.jsonl` and `crp__balance__checking.jsonl` are both `[code]__[kind]__[name].jsonl` (§5.2) — and treat them opposite: it checks a hand-named name for cross-node uniqueness and skips a determined one (its uniqueness is its determinant's, §5.4), and for a determined series *carrying* a name it checks the determinant sits beside it, a stranded one being a `pan validate` finding (§10.2). The bit rides in `schema` (§7.2) at no discovery cost, so the spine reads it without a token's meaning (I5) — `named` is structure, not semantics. Where a core declares two, its `Record` is an enum over them (`enum FastiRecord { Span(..), Event(..) }`), which is a **dispatch type, not a disk format**: the filename's token already says which variant a file holds (§5.2), so each file stores the bare variant payload and no tag is ever written (§18). There are no in-record kinds anywhere (§6.1), so a variant that once rode inside a record is now its own token — a Fasti event references a `span` entity rather than encoding a shape (§8.4). An entity promoted to its own node still carries its kind in the filename, only dropping the slug segment — its slug becomes the node's definition (§5.2).

A `Document` core (Tabella) declares **no tokens at all**: `kinds()` is empty, and its files carry no `__` segment — they are `[code]_[slug].[ext]` (single underscore), mapped to their core by extension→shape (§5.2), unambiguous while Tabella is the sole `Document` core. An empty `kinds()` is therefore what *names* a core Document — the one shape a core states by declaring nothing, and the reason the trait asks for no primitive beside the tokens. Its `Record` is the frontmatter (`type`, `tags`); the prose body rides alongside as opaque text, never deserialized, so a fold reads frontmatter only and there is no keyed-line surface.

Because a filename's token is what maps a file to its owning core (§5.0), **tokens are globally unique across cores** — Album's `person`/`organization`/`group`, Mappa's `location`/`region`, Rationes' `account`/`asset`/`claim`/`balance`, Fasti's `span`/`event`, Annales' `log`, Pensum's `task`, plus Auspex's reserved `function` (§9.1, declared by no core); no two share one. That single-owner guarantee is what lets the spine resolve `core:slug` and enforce per-core slug uniqueness (§5.4) by reading a name alone, without importing a core (I5). The spine reads an installed core's tokens by running its `schema` verb (§7.2) — runtime PATH discovery, never a link — and `pan doctor` reports a collision between two installed cores' declared tokens (§5.5). Legality *within* a vocabulary stays the owning core's own check on write (§6.4): Pantheon links no core and does not police kind legality tree-wide (I5, §5.5).

### 7.2 The verbs

Every core binary exposes the same verbs. stdout is JSON when piped, a table on a TTY (§7.3).

| Verb | Emits | Purpose |
|---|---|---|
| `add --home CODE [--kind K] … [-c]` | the created record | create an entity or a document, or append a reading to an **existing** series (`-c` first mints the series, §7.3) |
| `edit <key> …` | the updated record | change a record in place — an entity or document by slug, a series line by its key |
| `rename <slug> <new>` | the renamed record | change a record's name; renames the file and cascades its refs (§5.4) |
| `move <slug> --to CODE` | the moved record | re-home an entity or a document to another node |
| `rm <key>` | `{deleted: key}` | remove a record — an entity file, a document, or a series line (irreversible) |
| `list [--home CODE] [--kind K]` | array | folded present across the subtree |
| `get <slug>` | one record | current state — an entity, or a document with its body |
| `series [name] [--from D] [--to D]` | array | every record in a collection (the trend across keys), optionally windowed |
| `schema` | JSON Schema | self-description: name, primitive, tokens and their shapes, record schema, format version — the surface the spine's PATH discovery reads (§5.0, §7.1) |

`where`, `help`, and `version` complete the set, with every alias, in §7.3.

`edit <key>` and `rm <key>` apply to **any record** — an entity or a document (by slug), or a series line (by its date/slug key, §6.1). For a partitioned core, `move <slug> --to CODE` is a file `mv` between meta dirs (a document's is a `mv` between node dirs, §6.1), carrying any **determined-name series** the entity determines (a holding's `balance`, §8.3) in the same planned transaction — that series exists only because the entity does, and would otherwise strand at a node its determinant has left (§10.2). `edit <slug> -k <newkind>` **renames the file** (`…__person__john.json` → `…__organization__john.json`): changing what an entity fundamentally *is* is a visible structural act, not a silent field flip. Re-homing a whole series is a `pan` structural move (§10.1). `series` reads one collection whole; a window is a filter on it (`--from`/`--to`), never a second verb.

**`-k` selects within a shape, never across it.** On a write the *form* picks the shape — a series name means a series write, its absence an entity or a document — and `-k` names which of that shape's tokens (§7.1) is meant: `rat add -k balance` is a usage error (exit `2`), since the entity form has no `balance` token, while `rat crp checking 4200` infers `balance` as Rationes' only `Series` token (§7.3). This is what makes `edit -k` a *rename* at all — a rename cannot change a file's extension, and the extension is the shape (§5.2) — so no `-k` ever converts one shape into another. On a read there is no shape to respect and `-k` filters by any token: `fas list -k event`.

**A record's name is its slug**, and neither is stored (§5.4) — so there is nothing to edit, and renaming is its own verb. `rename <slug> <new>` normalizes the new name (§5.1), renames the file, and **cascades every `core:slug` ref** pointing at it (§5.4); a name and its slug are one thing, never allowed to differ. It is a verb rather than a flag on `edit` for the same reason `edit -k` renames a file: rewriting refs across the tree is a structural act and should read as one. Like any mutation it confirms first and shows the affected refs (§7.3). It **refuses an occupied slug** (exit `3`): the walk that finds the refs to cascade is the one that finds a record already holding the new name, and landing on it would rewrite every `album:johnn` into an `album:john` indistinguishable from the refs that always meant the other John. A cross-node duplicate born of `add` is a soft warning because you can still fix it at the source (§5.4); a cascade onto an occupied slug spends the token that told the two apart, and §18 keeps no history to recover it.

**Two refusals**, both where a record's name is a node's. An **entity-as-node**'s slug *is* its node's definition (§5.2), so renaming it means renaming the node dir, which no core may do: `alb rename john_appleseed …` exits `2` and points at `pan rename --def` (§5.5), the single node-rename that cascades refs (§10.1). `alb move john_appleseed --to …` exits `2` the same way, pointing at `pan mv`: an entity-as-node's home *is* its node (I3), so re-homing it is a node move — a file `mv` would strand the node and everything homed at it, and re-slug the record to whatever its new node's definition happened to be.

**Reading a document** needs no new verb. `get <slug>` returns the whole document — frontmatter as structured fields plus the prose body — rendered on a TTY, JSON (`{type, tags, body}`) when piped; `-f raw` emits the bare body text (the `cat` case, for a pager or `$EDITOR`). `list` folds the frontmatter across the subtree (metadata only — a fold never reads bodies, §7.1). `edit <slug>` opens the document itself — in the hand's own editor at a TTY, as a printed path when piped (§7.3) — and rewrites it in place. The universal verb set holds (§7.3); Document adds only the `raw` output format.

These verbs are the whole write surface for humans and LLMs alike (I8); writes are kept safe by validation, `--dry-run`, and the hardcoded confirm on mutations (§7.3) — never by gatekeeping the caller. Reactive writes route through Auspex (I2); an LLM acting on instruction is the user's hand, not a reactive process.

Every write verb takes `--dry-run`. The contract is frozen by `insta` snapshots with `key` and plan tokens redacted, so any change to a core's JSON is a visible diff in review.

### 7.3 The shared grammar

One vocabulary across every binary. Every tool has a three-char short, and the short is the command:

| | | | |
|---|---|---|---|
| `pan` Pantheon | `alb` Album | `map` Mappa | `rat` Rationes |
| `fas` Fasti | `pen` Pensum | `ann` Annales | `tab` Tabella |
| `aus` Auspex | `spe` Speculum | `atr` Atrium | `stu` Studium |

**Shape:** `<short> [verb] [home] [positionals…] [flags]`

Implicit for the common case:

- **Bare short → TUI**, at a terminal. `ann` alone opens Annales' TUI (prints `help` until the TUI exists). The TTY rule governs here as everywhere (below), so a bare short *piped* emits rather than opens: `ann | jq` is `help` as JSON — a TUI has nothing to draw on down a pipe, and `add` is the default verb, so there is no fold for a bare short to mean instead. A **lens**, whose whole surface is its fold, emits its mosaic's figures there (§12).
- **`add` is the default verb, and it fills a container — it never mints one.** `ann ecv weight 78.4` = `ann add -H ecv --series weight 78.4`, where `--series` and the reading's positional values are Annales' own — a core adds the flags its primitive needs; the universal set is below. A reading's home node comes from `pan new` and its **series is minted explicitly**, so a plain `add` refuses to append to a series that doesn't exist (exit `4`) rather than conjuring it. You create a series with **`-c`/`--create`** on a fully-specified `add` — home and series both named, never an inference form — so `ann ecv weight 78.4 -c` mints the `ecv` `weight` log and writes the first reading, and `ann ecv weight -c` mints it empty. This is why a typo can't spawn a junk log — `ann ecv wieght 78.4` with no `-c` is a not-found error, not a new series. `-c` is required wherever a series is **hand-named** (an Annales log, a Fasti event) — the typo-prone case; a series with a *determined* name (Pensum's one fixed `task` per node, Rationes' `balance` named for its holding) has nothing to mistype and is minted by its determinant — the node's first task, the holding's creation — without `-c`. A **partitioned entity** needs no prior container at all, since it *is* the record `add` creates.
- **The key is what you give, never invented.** A reading keys by date (`260703`) and takes a time only when a hand supplies one (`-a 1400` → `260703T1400`); the tool **never auto-suffixes** to dodge a collision. So a series sampled once a day (weight) and one sampled many times (meetings, meals, where you've been, time spent) need no per-series setting and no state anywhere: a second bare `add` on the same date lands on the same key, which makes it an **overwrite** — a mutation, shown and confirmed before it commits (below). That is I1's correction path, and it is the right answer for a weight you take once; give a time and the second reading is a second key, and a fresh `add` runs free.
- **Home and series are each inferable, but only ever *found*, never invented.** A reading needs a home node and a series; give both, or give one and let the tool resolve the other against what already exists — inference finds an existing series, it never creates one:
  - **both** — `ann ecv weight 78.4`: home `ecv`, series `weight`; no inference (the series must exist, or `-c`).
  - **home only** — `ann ecv 78.4`: infer the series *iff* `ecv` holds exactly one of **the tool's own** series files — another core's series at that node is not Annales' to count (§5.0) — zero → nothing to append to (exit `4`), more than one → ambiguous, list them and stop (exit `2`).
  - **series only** — `ann weight 78.4`: no home token, so search the **whole tree** for one of the tool's series named `weight` — exactly one → its node is the home, zero → not found (exit `4`), more than one → conflict, list the candidate homes and stop (exit `2`). `$PWD` never narrows this: a named series with no home is a deliberate "find it anywhere."
  - **neither** — `ann 78.4`: home from `$PWD` (below), series inferred there as in *home only*.

  A lone leading token is classified deterministically — resolves to a node code → **home** (series then inferred at it); otherwise → **series name** (home then inferred tree-wide). The rule is total, and compact codes and word series-names rarely collide, so it rarely surprises; the only conflicts are the multiplicity cases above, which the tool reports and refuses rather than guessing (`-H` or an explicit series resolves them). `-a`/`--at` defaults to today.
- **The locus is `$PWD`.** With no home token *and no series named*, the tool walks up from the working directory to the nearest node dir — triple or definition-prefix (§5.1) — and uses that code: `cd cs_a_amicitia/ && alb ls` lists friends. (A named series with no home ignores `$PWD` and searches tree-wide, above.) No stored cursor; the shell tracks location identically for all three hands. `-H` overrides.

**Universal verbs — identical within a layer**, since a layer is what shares a surface. Every **core** has the same twelve: `add` (default) · `edit` · `rename` · `move` (`mv`) · `rm` · `list` (`ls`) · `get` · `series` · `where` · `schema` · `help` · `version`. Every **lens** has the same surface too, and it is the bare short plus `help` and `version` — a lens owns no records, so it grows no verbs (§12). The two **system tools** carry their own structural sets — `pan`'s at §5.5, `aus`'s at §9.6 — beside `help` and `version`: that is the shape of what they are, and no licence for a core to grow a thirteenth (§18). Aliases are accepted everywhere, so `pan mv` and `alb mv` are one muscle memory, and `pan rename` and `alb rename` name the same act at their own layer. `where <slug>` resolves a slug to its home code by walking the tool's files (§5.0) — the per-core counterpart of `pan resolve`.

**Universal flags — universal where the shape admits it.** A flag its core's shape cannot use is a usage error (exit `2`): Album keeps no series, so `-c` and `-a` mean nothing to it; a document's frontmatter carries no `refs` (§6.1), so neither does `-r`.

| Short | Long | Meaning |
|---|---|---|
| `-h` | `--help` | help; on any verb |
| `-V` | `--version` | version |
| `-n` | `--dry-run` | validate, print what would change (with plan token), write nothing |
| `-H` | `--home CODE` | state the home explicitly |
| `-k` | `--kind K` | which of the core's tokens (§7.1) — within the shape the form already picks on a write, any token when filtering a read (§7.2) |
| `-c` | `--create` | mint the series before `add` writes the first reading; refused on an inference form (§7.3) |
| `-a` | `--at YYMMDD` \| `YYMMDDThhmm` \| `hhmm` | the reading's date, date and time, or a time today — the key is what you give (§7.3) |
| `-r` | `--ref REF` | attach a reference; repeatable |
| `-f` | `--format json\|table` | override the default view (a Document core adds `raw`, §7.2) |
| `-C` | `--root PATH` | operate on a different `$PANTHEON_ROOT` |
| `-y` | `--yes` | skip the confirm on a mutation |
| `-p` | `--plan TOKEN` | confirm the exact change a prior `--dry-run` computed (guards against a stale review) |
| `-q` / `-v` | `--quiet` / `--verbose` | |

**Format follows the hand (I8).** stdout to a TTY → table; piped → JSON. Same data, same code path; `-f` forces either.

**The editor follows the hand too (I8).** An `edit` given no new value is the **editor form**: at a TTY the text opens in the hand's own editor — `$VISUAL`, else `$EDITOR`, else `vi` — and is written back on save; piped, it spawns nothing and prints the file's path (`{"path":…}`, exit `0`), by the same law that sends a table to a TTY and JSON down a pipe. So `$EDITOR "$(tab edit meditationes | jq -r .path)"` is the shell's business rather than a `--print-path` flag, and the LLM hand gets a path to open with its own tools instead of a blocked process it cannot drive. The editor is the environment's, never Pantheon's: there is no `PANTHEON_EDITOR`, no per-core `PENSUM_EDITOR`, and no `--editor` flag — that is a knob (§18) where the OS already has one, and the shell already overrides it per command (`EDITOR=nvim pen edit ecv reach_out_to_alex`). Under `PANTHEON_RULE=1` the verb is refused before any of this (exit `6`, §9.3); a rule that wants a path uses `get` or `where`.

**What opens follows the shape** (§6.1). A **document** is opened in place — it already *is* the text (§8.7). An entity field or a series line opens a buffer holding **only that value**, normalized (§5.1) and folded back into the record on return: the JSON/JSONL record is machine-owned and is never handed to a hand raw (I6, §6.6).

**The editor session is the confirm.** The editor form mints no plan token and needs no `-y` — there is no computed change to review until the human saves, and the session *is* the review (save commits, `:q!` does not). It is the one mutation that never prompts, for exactly the reason the prompt exists elsewhere: the hand is already looking at the thing it is changing. (`-y` is accepted and moot there, so the TUI's blanket relay-with-`-y` holds unchanged — P§7.) An `edit` **given** its value inline (`pen edit ecv reach_out_to_alex "text"`) is an ordinary mutation and confirms by the rule below. Nothing is locked across the session (§6.4): the lock is taken to read and again to write back, since a session runs for minutes and any hand may edit the file directly meanwhile regardless (I8, §5.5). An editor exiting non-zero writes nothing (exit `1`); text that comes back unchanged writes nothing (exit `0`); text that comes back invalid exits `3`.

**Exit codes** (machines never parse prose): `0` ok · `1` runtime error · `2` usage error · `3` validation failure · `4` not found · `5` confirmation required · `6` write refused (write verb under `PANTHEON_RULE=1`, §9.3). Errors print `{"error":{"code":…,"msg":…}}` to stderr.

**Confirming mutations.** There is no autonomy setting — the behavior is hardcoded, one rule for everyone (Pantheon doesn't offer a boldness knob any more than it offers a theme). Verbs are classified: **reads** and a **fresh `add`** (recording a new keyed record — a new reading, a new entity, a new task) run free, since a new key can't destroy an existing one. **Mutations** — `edit`, `rename`, `move`, `rm`, and an `add` that overwrites an existing key — are final (§18: no undo layer) and always confirm before committing:

- **At a terminal**, a mutation needs `-y` on the command or answers a `y/n` prompt.
- **Not at a terminal** (an LLM or script — stdout isn't a TTY), a mutation without `-y` exits `5` and prints the would-be change as JSON, plan token included. The caller shows the user, then re-runs with `-y`. This is the structural checkpoint that lets the LLM hand write safely without Pantheon gatekeeping the caller (I8) — the same kind of rule as "emit JSON to a non-TTY."
- **Auspex** applies with `-y`. It is code, and its authorization is the rule's `writes=` declaration (§9.2) — granted when the rule was authored, checked per proposal (§9.5), with `aus plan` as its dry-run. This is I2's one grant of reactive authorship, not an exemption a hand can borrow: `PANTHEON_RULE=1` refuses every mutating verb to the rule itself (exit `6`, §9.3).

The **plan token** guards against acting on a stale review: `--dry-run` prints a hash of the exact computed change, and `-y --plan <token>` proceeds only if the change it would make *now* still hashes the same — anything moved underneath in between (another hand edited the record, a hook re-homed it) invalidates it, forcing a fresh look. The token is always available on `--dry-run` and always honored when passed; passing it is optional for a human typing `-y`, and the natural path for a cautious caller. Auspex is untouched: it is code, governed by I2 and its own `plan`.

**The ambiguity rule.** A first token could be both a verb and a node code (an `a_d_…` → `ad_d_…` path yields `add`). Verbs are a closed reserved set and win; `-H` or `--` forces the code reading. `pan validate` warns when the ontology mints a code colliding with a reserved verb.
