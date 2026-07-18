## 6. Storage model

### 6.1 Three storage shapes

Everything lives scattered through the tree under its home node. A record file takes one of three shapes, told apart by extension (§5.2); each core declares a shape per token (§7.1), fixed by what its primitive *is* — a thing that endures is **partitioned**, a thing sampled over time is a **series**, a thing written is a **document** (the three bullets below). Cores of one sphere usually land on the same shape — Contextus partitions, Actio accumulates series, Ego writes documents — but that is a consequence of grouping like primitives, not a rule the storage layer enforces: Rationes declares a `balance` series beside its partitioned holdings, and Fasti a partitioned `span` beside its `event` series (§8.3–§8.4).

- **Partitioned register** — **one `.json` object per entity**, kind and slug in the filename (`csa__person__john_appleseed.json`), or kind alone for an entity promoted to its own node (`csa_john_appleseed__person.json`, slug = the node's definition, §5.1–§5.2). Mutable: `edit` rewrites the one object; `move` is a file `mv` to another node's meta dir; `rm` deletes the file; changing an entity's `kind` renames it (§7.2). File uniqueness is the filesystem's, but a slug is not a filename: two kinds spell two files and one ref, so `add` globs the node's meta dir and refuses a slug another kind already holds (exit `3`, §5.4, §18); cross-node slug collisions stay a soft `pan validate` finding (§5.4). For things that *are*: people (Album), places (Mappa), holdings (Rationes), spans of time (Fasti). An entity was never a sample, so history you care about is a **list-valued field inside the object** — a person's `away` periods accumulate in a `data.away` array, corrected by editing the object.
- **Series** — a `.jsonl` collection, **one file per series, many keyed lines** (`cso__log__meetings.jsonl`, `cso__event__standups.jsonl`, `cso__task.jsonl`). Each line is keyed — a date (`YYMMDD`, plus a time when a hand gives one, §7.3) for a reading, the record's own name for a register line (a task); the key is normalized on `add`, never hand-typed as a slug (§5.4). A hand-named series is **minted explicitly** (`add -c`, §7.3) — a plain `add` refuses an unknown series rather than auto-creating one (a determined-name series, like Pensum's fixed `task`, is minted by its determinant instead); thereafter `add` appends a new keyed line, and `edit <key>` and `rm <key>` change or drop that one line **in place** — a correction rewrites the keyed line, it does not stack a second (I1). The fold reads the collection, taking the current value per key. A series carries **one record shape** — there are no in-record kinds; a line that would need its own kind is either a distinct series or an entity (a Fasti event references its span rather than encoding a variant, §8.4). For things *measured, logged, or listed*: weight and facts (Annales), events (Fasti), tasks (Pensum), balances (Rationes).
- **Document** — a text file, **one file per document**, `+++` TOML frontmatter (the envelope: `type`, `tags`) over a free-prose body (`ecv_doctors_appointment.md`). The body is opaque — not a serde record, so a fold never parses it; it reads the frontmatter to list and filter (`type`/`tags`). Extension is open across a small fixed set (`.md`, plus `.txt` and `.mdx`), since the payload is prose, not a machine format — but still classified by extension alone (§5.2). Loose in the node dir as `[code]_[slug].[ext]`; `edit` rewrites the file in place, `move` re-homes it, `rm` drops it, like a partitioned file. For things *thought or written*: notes, quotes, principles (Tabella).

### 6.2 On-disk layout (`$PANTHEON_ROOT` = the tree)

An example layout:

```
$PANTHEON_ROOT/
├── a_actio/
│   ├── a_c_cura/                        # code ac
│   │   └── ac__/                        #   meta dir
│   │       ├── ac__.toml
│   │       └── ac__task.jsonl           #   tasks concerning cura (Pensum series, one per node), homed here
│   └── a_o_opus/
│       └── ao_f_fabrica/                # code aof
│           └── aof__/
│               ├── aof__span__mvp_phase.json    #   Fasti span: when this work was active (referenceable as fasti:mvp_phase)
│               └── aof__event__milestones.jsonl #   Fasti events (series), each line may ref a span
├── c_contextus/
│   └── c_s_societas/
│       └── cs_a_amicitia/               # code csa
│           ├── csa_trip_idea.md         # loose doc → Tabella handles it
│           ├── csa__/
│           │   ├── csa__.toml
│           │   ├── csa__person__alex.json  # a friend (register: one object, kind=person)
│           │   ├── csa__person__mara.json  # another, refs alex
│           │   └── csa__group__book_club.json    # an informal set of people (kind=group, §8.1)
│           └── csa_john_appleseed_/     # a friend promoted to a node (def-prefix, code csa_john_appleseed)
│               ├── csa_john_appleseed_example.md      # a loose doc about him
│               └── csa_john_appleseed__/              #   his meta dir
│                   ├── csa_john_appleseed__person.json   #   his Album record (slug = john_appleseed)
│                   └── csa_john_appleseed__log__calls.jsonl  # a call log homed at him
└── e_ego/
    └── e_c_corpus/
        └── ec_v_valetudo/               # code ecv
            └── ecv__/
                ├── ecv__.toml
                ├── ecv__log__weight.jsonl    # Annales weight series (keyed by YYMMDD)
                └── ecv__function__weigh_in.sh    # an Auspex rule scoped here (this node and below, §9.1)
```

**Finding the root.** `$PANTHEON_ROOT` is resolved per command, never stored (§18): the `--root` flag (§7.3) wins, else the `$PANTHEON_ROOT` env var — and **there is no default**. Unset and unflagged is a usage error (exit `2`), never a guess: nothing marks the tree root on disk (only the ontology's top nodes sit there, §18), so a tool that fell back to `$HOME/pantheon` could not tell a tree you have not made from a tree it is failing to find, and would answer both by minting neither. The root is *named* — not discovered by walk-up, not conjured by fallback; the §7.3 locus walk finds your current node *within* an already-known tree, never the tree itself. The pointer lives in your shell rc, beside the `pan init` shim (§5.5) — your state, not a tool store.

A record's node, core, and (for entities) kind and slug are its path and filename. Cores are not sphere-locked — a Pensum task homes at the *doing* node it belongs to, a Tabella note at whatever it is *about* — but entities still home by what they are: a person lives in Societas by the nature of the bond, never under the org or context you met them in, and their membership in that org is a *reference* to the org entity (`album:<org>`), an edge, not a nesting (I9, §8.1).

### 6.3 Folds are subtree-walks

A fold is a walk. `alb list` walks the requested subtree and reads every Album entity file it finds — the partitioned `*__[kind]__*.json` and the entity-as-node `*__[kind].json` forms alike (§5.0) — unioning the one-object files; a series fold instead reads a single `.jsonl` and takes the current value per key. The descent visits only node dirs — triple or definition-prefix (§5.1) — and their meta dirs, so bulk is never opened. Cheap for a personal corpus.

### 6.4 Atomicity & concurrency

- **Every record write takes an advisory lock** (`fd-lock`) **on the record file itself** — the entity's `.json`, the series' `.jsonl`, or the document's text file — and writes via temp-file-and-rename (a series `add`/`edit`/`rm`: read the file, apply the change, rename the new copy into place). The lock scope is that one file. It is required because a detached `aus run` hook (§9.4) can legitimately write a file — a proposed task into a node's `task` series — while a user command is mid-edit on the same file; without the lock, one rewrite would clobber the other's line. Pantheon takes the lock automatically. Since temp-file-and-rename swaps the inode, a writer that acquires the lock re-opens the path and confirms it holds the file now there — retrying if another writer's rename slipped in first — before its read-modify-write. **The lock lives on the tree file, not a sidecar or cache** — there is no cache dir (§9.4, §18), so it serializes writers using only the record file itself. What it does **not** close is the window between a resolve and a write: §5.0's map is built once per command, so a ref that resolved before another hand's rename cascade can be appended after it and dangle. Only a tree-wide lock would close that, and §18 leaves nowhere to keep one — so it takes the concession a crash already takes: `pan validate` reports the dangle and you fix it at the source (§5.4, §10.2). The lock serializes tearing, never a read-set.
- File uniqueness needs no check — the filesystem refuses a duplicate filename (§6.1); cross-node slug collisions are a soft `pan validate` finding (§5.4).
- The **owning core** validates its own record on write — schema, and the filename's kind segment against its own vocabulary (§7.1). Pantheon validates what is cross-cutting: ref resolution and node-path validity. Last-write-wins on folds.

No version history is kept: `edit` overwrites the keyed record in place, so a corrected reading leaves no prior copy — you fix the value, you don't accrue a change-log (I1, §18).

### 6.5 Two strata: records and bulk

`$PANTHEON_ROOT` is the whole personal filesystem, so each node carries:

- **Records** — the small text the tools own: `*.json`, `*.jsonl`, `[code]__.toml`, loose `[code]_*` documents (`.md`/`.txt`/`.mdx`). Legible, hand-editable, checked by the owning core on its next read and by `pan validate` on demand (§5.5).
- **Bulk** — media, PDFs, binaries. Homed at their node, but content, not records; not versioned by PantheonOS.

Auspex rule files (`[code]__function__*`, §9.1) share the meta dir but are neither stratum — executable code the tools own, not data.

Records reference bulk **by relative path** (the second reference form): a `cri` rights entry is the metadata record, and `cri_passport_scan.pdf` sits beside it at the same node.

**Media is bulk, not a core.** An image, a video, a voice memo is *content* a record points at — homed by what it is *about* (a photo of a friend beside that friend, a progress photo under Corpus), never a "media" core, which would sort by format against sort-by-what-it-is (§2). A *stream* of such captures (a daily photo log, voice notes) is an Annales series whose payload is a file. Rendering a binary is a Porticus concern where the terminal supports it (§11); otherwise the record's metadata carries it.

### 6.6 Configuration & frontmatter

**Hand-authored annotation is TOML; the record contract is JSON/JSONL** — machine-owned, though hand-editable at the source (I6). All TOML is parsed with `toml_edit` (format-preserving) so comments and ordering survive a rewrite by code or LLM (I8). There is **no global config file** — UI, mutation-confirm behavior (§7.3), numeric pad width (always 2, §5.1), and per-core defaults (e.g. Album's `person`) are all hardcoded, so the tree is the only state. What is genuinely the *environment's* is read per command and stored nowhere — `$PANTHEON_ROOT` (§6.2), and the editor `edit` opens (`$VISUAL`/`$EDITOR`, §7.3); neither is Pantheon's to configure. Two hand-written TOML surfaces remain, both annotation, neither behavioral:

- **`[code]__.toml`** (in the meta dir) — node annotations: symbol, keywords (for LLMs), deity/personification (a memory aid), explanation. Optional; mainly the top ontological layers carry one. Read for placement (`pan constitution`), never for behavior. For example, a top-layer `e_ego` node:

```toml
symbol      = "Ε"
deity       = "Prometheus"          # personification, for memorization
keywords    = ["self", "body", "mind", "identity"]
explanation = "Everything that is me: my body, states, and inner life."
```

- **Document frontmatter** — `+++`-fenced TOML at the head of a Tabella document (`.md`/`.txt`/`.mdx`).

**Frontmatter is TOML**, fenced `+++`:

```markdown
+++
type = "principium"
tags = ["mores", "vocatio"]
+++

Prose starts here.
```

No `home` key — the file's node is its path (I3). `type` and `tags` are fields, not branches.
