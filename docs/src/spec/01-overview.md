## 1. Overview

**PantheonOS is a suite of terminal tools over one idea: your life, modeled as a directory tree you can read, edit, and reason about by hand — and so can an LLM, and so can a script.** There is no database and no app. The ontology *is* the filesystem: a node of your life (health, a friendship, a project) is a directory, and its records are plain files under it (eg. JSON). Three hands — you, an LLM on your behalf, and deterministic code — act on exactly the same files through exactly the same grammar (I8).

**What it's for.** Keeping the durable facts of a life legible and queryable without lock-in: who you know, where things are, what you own, what you owe, what you do, what happened, what's next. You add a reading (`ann run 10 43`), the tools file it at the right node, and it stands as one sample of your reality (I1).

**The shape of the system:**

- **The tree** (§4–6) — the ontology as directories. A node's name carries its code, character, and label; its meta dir holds the records. Nothing describes the tree but the tree.
- **The spine, `pantheon`** (§5) — the one library everything links: addressing, resolution, the record envelope, validation. Every other component points at it and nothing points sideways (I5).
- **The cores** (§8) — one small CLI each, owning one primitive. Entities (contextus): `album` (people), `mappa` (places), `rationes` (holdings); the three tenses of doing (actio): `fasti` (placement), `pensum` (intention), `annales` (fact); the self (ego): `tabella` (documents). Each speaks the same verbs and emits JSON — that JSON is the only contract (I4).
- **Auspex** (§9) — the one component allowed to turn a read into a write automatically (I2). Rules are files in the tree; they *propose*, and only Auspex applies (§9.3).
- **Lenses & UI** (§11–12) — dashboards (`speculum`, `atrium`, `studium`) built from composable tiles that fold different aspects of life into a coherent view, from which every write relays back through the cores (§12).

**Who acts, and how safely.** Reads and appends are frictionless; anything destructive always confirms before it commits — a prompt for you at a terminal, a structured checkpoint for an LLM (§7.3). That is what lets an LLM work directly on your data without a special path or a gatekeeper: it's your hand, held to the same contract you are.

**The feel.** Terse to type (`alb`, `ann`, `pan`), legible to read (open any file and understand it), and honest about state (the tree is the only truth — no hidden store, no cache of record data, nothing you can't see and edit). Written in Rust, shipped as standalone binaries; install one tool or all of them.

The rest of this document is the specification. §2 sketches the ontology — the tools fix none of its content, only its shape, since the tree is yours to define. The invariants (§3) are law, and every design choice below is downstream of them; §18 fences off what must *not* be built, and is worth reading early for all that it sits last. Appendix A collects the naming and etymology notes, so the technical sections stay lean.
