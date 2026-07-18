## 17. Worked example (end to end)

Log today's weight:

```
$ ann ecv weight 78.4
```

`ann` = Annales, `add` implicit, `ecv` = home (Valetudo), `weight` = series, today's date = key (§7.3). To a TTY it prints a table; piped, the same call emits JSON. A fresh key runs free — no confirm, since only a mutation stops (§7.3).

The `weight` log had to exist first — minted once with `ann ecv weight -c` (§7.3); this call is a plain append, and a mistyped series is a not-found error (exit `4`), not a new log. Because that log is the only `weight` series in the tree, `ann weight 78.4` would land the same reading with its home found tree-wide, and `ann ecv 78.4` would too while Valetudo holds just the one log of Annales' own — three forms onto one file, none of them able to invent it.

- The reading appends to `…/ec_v_valetudo/ecv__/ecv__log__weight.jsonl` — node, core, and series *are* the path and filename, never fields in the line (I3, §5.4).
- Atrium's weight tile folds `ann series weight` — JSON, since a child's stdout is a pipe (§7.3) — to its latest key on the next data refresh: no stored "current weight" (I1, §5.0, §11.2).
- The write spawns `aus run --trigger annales@ecv`, detached (§9.4). Auspex's `weigh_in` rule, scoped at `ecv`, sees a reading today and proposes nothing. Had none landed in 30 days it would have proposed a Pensum task — landing only at the one node its header grants (`writes=`, default-deny, §9.2), upserted on the key Auspex derives from its proposed name (§9.3), and indistinguishable from a task you typed, since no record carries its author (§9.5). Nothing fires on a timer: that proposal appears at the next write that wakes it, a trigger naming the wake and never narrowing which rules evaluate (§9.1, §9.3).
- Speculum's monthly horizon windows the same file into a trend (`ann series weight --from … --to …`, §7.2). Same JSON, three consumers, zero duplication.

The same act by the other two hands (I8). An **LLM** types what you type: a fresh reading is no mutation, so `ann ecv weight 78.4` runs free for it too, and the checkpoint appears only where it *corrects* one. Re-weighing today at 78.9 is a second `add` on today's key — an overwrite, so away from a terminal it exits `5` and prints the would-be change with its plan token; the LLM shows you, then re-runs with `-y --plan <token>`, which lands only if nothing moved underneath in between (§7.3). The keyed line is rewritten, never stacked (I1). A **human**, offline, opens `ecv__log__weight.jsonl` and types the envelope by hand — `{"key":"260703","refs":[],"data":{…}}` (§5.4). Nothing watches (§18), so nothing checks it on the spot: Annales catches a `data` that won't deserialize the next time it reads the file (exit `3`, §13), and the spine's own checks — prefix, node, refs — run when a hand asks for them (`pan validate`, §5.5). Three hands, one file, one grammar, one validation.
