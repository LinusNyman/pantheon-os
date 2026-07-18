## 9. Auspex — the rules engine

Auspex reads the cores' readings for signs and turns them into **intentions**: it converts *passio* into proposed *actio*. It writes records, never deeds — most often a task, which proposes that *you* act; Auspex itself never acts on the world. It is the only component with agency and the only reactive writer (I2). *(On the name, see [Appendix A](./APPENDIX-A-NAMES.md).)*

### 9.1 Rules are files

A rule *is* a file in a meta dir — no registry, no `rules.toml`. Auspex discovers rules the way Pantheon discovers nodes: by walking the tree.

```
csa__/csa__function__stale_contact.py     # scope csa, name stale_contact, Python
ecv__/ecv__function__weigh_in.sh          # scope ecv, shell
crp__/crp__function__balance_check        # no extension — the shebang names the language
csa_john_appleseed__/csa_john_appleseed__function__birthday.py   # scoped at a def-prefix node (§5.1)
```

The filename carries everything: code prefix = **scope**, the reserved **`function`** token in the kind slot marks it a rule, the segment after `function__` = **name**, extension = **language**. `function` is filename-only — a rule *is* an Auspex function — and the reserved word by which the walker (§5.2) and `aus` recognize a rule even with no extension; the tool is still Auspex/`aus`, and the in-file header key stays `auspex:` (§9.2). `touch` mints a rule, `rm` removes one, and re-scoping is a move plus a prefix rewrite (`pan mv-file`, §5.5) — by any hand (I8).

**Where the file sits is the whole of its scope.** A rule's code means here exactly what it means in every other fold — this node and everything under it (§6.3) — and no header key narrows it, so a rule's scope is never half in its name and half in its text, and `mv` re-scopes it completely. Scope is what an `aus run [scope]` evaluates, never what a rule may touch: where its writes land is the header's to say, one node at a time (`writes=`, §9.2), so a rule scoped at `csa` may be granted `pensum@acm` and reach a node its scope never covers. A rule that cares about only one node of its subtree says so itself, against the `trigger` it is handed (§9.3): that is an `if` a rule writes, not a grant Auspex keeps. A rule scoped at a sphere — a root node such as `e`, `c`, `a` (§5.1) — is therefore evaluated for that whole sphere, and that is the widest scope there is: the tree root holds no meta dir, so no rule can sit above the spheres (§6.2, §18).

### 9.2 The header

A rule declares when it runs and what it may touch; its *scope* — which runs evaluate it — is its filename's to say (§9.1), never the header's. The declaration is a **comment header** on the first line — or the second and no further, when a shebang takes the first — parsed *without executing the file* (`#` for Python/shell/Ruby, `//` for JS/Rust). Discovery never runs code: rule files are code an LLM may have authored, and a rule's capabilities must be readable before it is trusted to run. **A rule is therefore always text** — there is no compiled form, since a binary could declare its header only by being executed, which is the one thing discovery must not do.

```python
#!/usr/bin/env python3
# auspex: watch=annales writes=pensum@acm:add;annales@csa/stale_contact:add
```

| Key | Meaning | Default |
|---|---|---|
| `watch` | evaluate when the triggering write touches these cores | any |
| `writes` | **capabilities**: `core@home[/series]:verbs`, `;`-separated | *none* |
| `desc` | one-line human description | — |

**`writes` is default-deny.** A rule declaring nothing is read-only: it may propose, but nothing it proposes lands. A capability names **one node**, never its subtree, so a grant reads as exactly where writes land; several are `;`-separated (`writes=pensum@acm:add;annales@csa/stale_contact:add`). Every mutating verb is a capability like any other — `rm`, `move`, and `rename` included (`writes=pensum@acm:rm`), the last cascading refs when it lands (§7.2). A capability may name a **series** after its node, and an `add` that would **mint** one must: `annales@csa/stale_contact:add` brings that log into being where it is missing — the one write allowed to bring its own container with it (§18) — while a bare `annales@csa:add` may only append to a series that already exists. That slot is what holds the exception to the size §18 claims for it: a rule's series is authored once, in the grant, so a rule cannot mint a log per run and silt up a node with them, and the `core@home` bounds *where* while the series bounds *what*. Auspex checks every proposal against the declaration before applying — and that check is the only thing standing between a rule and your records (§9.5).

### 9.3 The propose protocol

A rule is a **pure function of the tree**: context on stdin, proposals on stdout. It never writes.

```
stdin  ← {"sign":"hook","rule":"stale_contact","scope":"csa","now":"260710",
          "trigger":{"core":"annales","home":"csa_john_appleseed"}}
stdout → {"writes":[{"core":"pensum","verb":"add","home":"acm",
                     "name":"Reach out to Alex","refs":["album:alex"]},
                    {"core":"annales","verb":"add","home":"csa","series":"stale_contact",
                     "data":{"proposed":"reach_out_to_alex"}}]}
```

`sign` is `hook` when a core spawned the run and `manual` when you called it yourself. `trigger` names the core and home of the write that woke it, and is **absent wherever no single write authored the wake** — a manual run, a core's TUI opening (§9.4), a rename cascade whose batch touched several cores and homes (§5.4) — where `watch=` therefore filters nothing and every rule in scope evaluates. A trigger names a write; a wake that is not one stays silent rather than naming a write that never happened. The trigger *names* the write, it never carries it: a rule that wants the record reads it through the core CLI like anything else.

A **proposal is a core call in JSON**: `core`, `verb`, and `home`, then what that call needs. `series` names the collection a series write targets — an Annales log, a Fasti event set — and is absent where the core's series is determined (Pensum's one `task` per node, §7.3). A fresh `add` of a named record carries a **name, never a key**: Auspex normalizes it (§5.1) into the record's key — `reach_out_to_alex`, reachable as `pensum:reach_out_to_alex` — so a rule can no more hand-type a slug than a hand can (§5.4). A date-keyed `add` carries neither and keys by `now`: the key is what the giver gives (§7.3), and a rule gives none. **`now` is a date and stays one** — that is what makes a rule idempotent, the same proposal on the same day being the same key for Auspex to upsert rather than stack (below); a `now` carrying the wake's clock would mint a fresh key every hook and stack forever. The corollary is that one wake writes **one line per date-keyed series**: two such proposals in a batch land on one key, and applying them in order would discard the first without a trace, so Auspex rejects the batch rather than silently keeping the last (§9.5). A rule with several facts about one day proposes one line carrying them all, or proposes name-keyed records instead — a task per contact, which is what `stale_contact` does (§9.4). A proposal targeting an *existing* record — `edit`, `rm`, `move`, `rename` — names its `key` instead, since that record already has one; a `rename` names both (the `key` it targets, the `name` it becomes), and a `move`'s `home` is its destination. `refs` and `data` ride along where the record takes them, and are the owning core's to validate (§6.4).

A rule reads whatever it needs through the core CLIs (`alb ls -H csa -f json`) — the JSON contract is the API. **Enforced, not conventional**: Auspex executes rules with `PANTHEON_RULE=1` in the environment, and core binaries refuse **every write verb** under it — a fresh `add` included, since a rule's only path into the tree is a proposal (exit `6`). `aus` refuses `run`, `plan`, and `test` under it by the same law: a rule reads the tree through the core CLIs and never by re-entering the engine, and an `aus plan` inside a rule would re-evaluate that rule and recurse without bound (exit `6`).

Consequences of proposing rather than writing:

- **`aus plan` is real** — run the rule, print stdout, apply nothing; works for any rule in any language.
- **Idempotence** — a rule proposes a deterministic **name**, which normalizes to the same key every wake; Auspex **upserts** on that key, so a rule firing on every wake keeps one record instead of stacking duplicates. Auspex itself holds nothing between runs (§9.4); a rule that wants a memory proposes a log and reads it back, like any other hand.
- **One place for policy** — capability checks, validation, and dedupe all live in Auspex, not in forty scripts.
- **Testability** — `aus test <rule> < fixture.json` hands a rule its context and asserts on the JSON out; nothing is applied, so no sandbox is needed.

The protocol governs writes **to the tree** only. A rule is arbitrary code — it may send mail or call an API — it simply cannot put anything in the tree except by proposing. A rule cannot branch on the effect of its own writes: it proposes a batch, Auspex applies in order, the next wake sees the new state.

### 9.4 Waking — hooks, no daemon, no cache

There is no background process and no cache dir. **Every core, after a successful write, spawns `aus run --trigger <core>@<home>` detached and forgets it**; a core's TUI opening spawns a bare `aus run` instead, a wake no single write authored having no write to name (§9.3). Auspex walks for rules in scope, applies, exits. Those hooks and your own `aus run` are the only wakes there are — no timer, and nothing that fires while you are away.

- **Concurrent `aus run` invocations need no Auspex-level lock.** A batch of fifty writes spawns fifty hooks; each proposes the same deterministic upserts (§9.3), so overlapping runs converge on the same records (last-write-wins per key), and the record lock (§6.4) keeps two of them from tearing one file — at worst redundant work, never corruption.
- **Auspex keeps nothing between runs.** There is no debounce key and no last-run signal: when Auspex last walked is not a fact of your life, so it has no node to home at (I7, §2), and a signal the engine kept would be exactly the rule state §18 forbids. A walk is cheap (§5.0), so a hook-woken rule should be cheap too.
- Auspex's own writes carry `PANTHEON_NO_HOOKS=1`; cores skip the hook when they see it — no recursion.
- Cores do not depend on Auspex (I5): they look for `aus` on `PATH`; not installed → nothing happens.

Time-decay needs no scheduler: "no weight reading in 30 days" is answered by comparing dates whenever a hook asks. A rule that wants to fire less often than the hooks do keeps its own count the same way — in the open, as records. `stale_contact` proposes a log line of every run it acts on (`annales@csa/stale_contact:add`, §9.2 — the grant names the log, so the rule mints it once and never a second), reads it back on the next wake (`ann series stale_contact -H csa`), and returns nothing while the last line is within seven days. The debounce is the rule's own, written in the same records everything else is, and Auspex neither requires it nor knows it is there. A rule's output is a Pensum task, which exists to be *seen*; the auspex observes and announces, and the announcement keeps at its node until you look.

### 9.5 Applying a proposal

1. **Parse** stdout. Empty or absent → no-op, exit 0.
2. **Capability check** — every write matches the header's `writes=`, or the batch is rejected.
3. **Validate** — home exists and refs resolve (Pantheon); the owning core checks schema and kind on write (§6.4). A cross-node slug collision is a soft `pan validate` finding (§5.4), not a hard reject here.
4. **Dedupe** — upsert on the key Auspex derived from the proposed name (§5.1, §9.3), so a rule firing on every wake keeps one record current instead of stacking duplicates. An existing record at that key is overwritten; step 2's grant is what authorized that. **Two proposals in one batch landing on one key is a rule error, not an upsert** — the second would discard the first with nothing to show for it — so the batch is rejected and reported, as a failed capability check is (§9.3).
5. **Apply**, in order, with `-y` and `PANTHEON_NO_HOOKS=1`. Step 4's upsert is an `add` over an existing key — a mutation (§7.3) — so the `-y` is what lets it land: Auspex's authorization is step 2's capability check, granted when the rule was authored, not a prompt no hook could answer.

A rule that errors is skipped and reported; others are unaffected.

**The grant is the whole guard.** Nothing marks a record as Auspex's — a task it wrote and a task you typed are the same task — so an upsert lands on whatever holds that key, yours included. That is the trade, not an oversight: a capability names one core, one node, and the verbs allowed there (§9.2), so the reach of a rule you got wrong is exactly the node you handed it. Read `aus plan` before you grant, and grant narrowly. I2 gives Auspex reactive authorship; the header is where you say how far it goes.

**Provenance is a record you keep, not a field Auspex writes.** No record carries who wrote it, and none needs to: you authored the rule and granted it, so a task it writes is your own hand at one remove (I8) — not a foreign one to be marked and mistrusted. Where you want to know *why* a task appeared, the rule proposes an Annales log line of its own run beside it (§8.6) — an Annales log and no other channel — and you read, fold, and delete it like any other. Auspex neither writes that log nor reads it: it is the rule author's etiquette, and being your own rule, it should tell the truth. That is how the whole mechanism stays clear of the audit sink and the rule state §18 forbids.

### 9.6 CLI

- `aus` — rules browser TUI: what exists and what is scoped where.
- `aus run [scope] [--trigger core@home]` — evaluate the rules in scope and apply. What the hooks call, passing their own trigger; a hand omits it and everything in scope evaluates (§9.3).
- `aus plan [scope]` — evaluate and print proposals as JSON; applies nothing.
- `aus ls` — every discovered rule: scope and header.
- `aus test <rule>` — run one rule against a stdin fixture, print proposals.
