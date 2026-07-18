# Appendix A — On the names

*Section and invariant numbers (`§5.1`, `I2`) point at `PANTHEONOS-SPEC.md`; `P§` at `PORTICUS-SPEC.md`. Nothing here is normative — the specs are.*

The suite is named in a Latin / Roman-mythological register, chosen for terseness at the terminal and for memorability: an inflected language gives twelve distinct three-letter shorts — `alb` `map` `rat` `fas` `pen` `ann` `tab` `spe` `atr` `stu` `pan` `aus` (P§9) — each with a mnemonic standing behind it. None of it is behavioral. The names are read by hands, never by tools: no code branches on one, and the one reserved word a filename carries — Auspex's `function` (§9.1) — sits outside the register entirely. Most names want only a gloss; the few carrying a conceptual load follow it.

| Name | Latin sense | Why it fits |
|---|---|---|
| **Pantheon** | the temple to all the gods, each in its own niche under one dome | the spine every tool stands in (§5), and one suite over twelve binaries |
| **Album** | the whitened board on which Rome posted its public registers — the *album senatorium*, the *album iudicum* | a register, one entry per agent, legible at a glance (§8.1, I6) |
| **Mappa** | a cloth; the *mappa mundi* made it the word for a map | places, and only places — never your history among them (§8.2) |
| **Rationes** | accounts, reckonings — *rationes conficere*, to keep the books | holdings and their balances (§8.3) |
| **Fasti** | the Roman calendar — days marked *fastus* or *nefastus* — and the year-registers kept alongside it | placement: what sits on the timeline (§8.4) |
| **Pensum** | the wool weighed out to a spinner as a day's task; hence one's allotted work | intention: the doing allotted and not yet done (§8.5) |
| **Annales** | the year-by-year record (*annus*) — what happened, set down as it happened | fact: the durable record (§8.6) |
| **Tabella** | the small wax tablet you wrote a note, a letter, or a vote on | one loose document per thought (§8.7) |
| **Auspex** | the bird-watcher (*avis* + *spec-*) who read the signs | see below |
| **Porticus** | the colonnade fronting a building, giving a row of them one face | one shell, one keymap, one look across all twelve (§11.1) |
| **Tessera** | a small cube of stone in a mosaic; also a token or tally | one tile, drawn small; many make the view (§11.2) |
| **Speculum** | a mirror | review: it shows you back to yourself, reflecting rather than emitting (§12, P§9) |
| **Atrium** | the hall you enter a Roman house through — traditionally *ater*, "blackened", for the hearth-soot that once darkened it | the home dashboard: the room everything passes through (§12) |
| **Studium** | zeal, application, study | the studies lens (§12) |

The one short that had to give ground is `pan`: Homebrew's core tap already holds a `pan`, so the formula is `pantheon-pan` while the binary keeps its three letters (§15.4).

**Auspex — why it proposes, never acts.** An *auspex* read bird-signs — noticing omens, never causing them, and saying what they meant. The rules engine (§9) is built the same way: it reads the cores' readings for signs and converts *passio* (what was measured or logged) into proposed *actio* (a task for you), but never acts on the world itself. I2 makes Auspex the one reactive writer; the name says what kind of writer that is — one that writes records, never deeds, and most often a task, which proposes that *you* act (§9). P§9 gives it the comet: the omen read but never made, the same thought carried by the symbol family.

The office is the *auspex*, never the **augur** — a distinct role, the one *consulted* to interpret, where an auspex observes and announces. Observe, report, never cause is I2's shape exactly, so the register is auspex and the second office does no work here.

**The three tenses of Actio.** Fasti, Pensum, and Annales (§8.4–§8.6) are the placement, intention, and fact tenses of *Actio*:

- **Fasti — placement.** What sits on the timeline: spans (a period — a project's active window, a residence, a career stage) and events (a dated occurrence, each optionally pointing at the span it belongs to). The calendar is a view folded from these, not a stored thing (§8.4).
- **Pensum — intention.** A future doing: the task not yet done.
- **Annales — fact.** What happened: the durable record, the purest expression of I1.

**Personifications (deities).** A node's meta file (`[code]__.toml`, §5.2, §6.6) may carry an optional `deity` field — a Roman or Greek figure personifying the node (*Prometheus* for `e_ego`, §6.6). It is a memorization aid and nothing more: `pan annotate` reads it back (§5.5), nothing branches on it, and like every key in that file it annotates the node rather than defining it (§5.0).

**The symbols are their own register.** The instruments' marks — the seven classical planets, the Moon's nodes, a star for the frame, a comet for the omen — are Porticus's, set out with the accent palette in P§9. They follow the same rule the names do: mnemonic, hardcoded, never read for behavior (§18).

**A note on the Latin generally.** The node names throughout (Actio, Contextus, Ego, Motus, Cura, Amicitia, Valetudo, Societas, Locus, Res, Anima, …) are illustrations from one author's tree, not a fixed ontology the tools impose. The tree *is* the ontology — emergent and wholly yours, governed only by the reality test (§2, §5.0, I7). The reference tree shows a *style* of ontology, never a required one.
