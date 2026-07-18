## 2. The ontology

The ontology **is the directory tree on disk** — not a file describing it. A node exists because its directory exists; its code, char, and label are read off the directory name (§5.1). `mkdir` mints a node, `mv` re-homes a branch — for a human, an LLM, or a script alike. Per-node meta files *annotate* (symbol, keywords); they never define shape. There is no `ontology.toml` and no fixed schema of nodes: **the tree is emergent and wholly yours** — the tools ship no ontology and impose no starting shape, governed only by the reality test (I7).

**Emergent is not arbitrary.** Whatever roots you mint are your **spheres**, and the tools mandate no particular set — but shaping a tree well is not a free-for-all. The reference tree below is one worked attempt at carving a life close to its natural joints: its top is meant to be near-**found** — the same first cuts most people would reach — while the depths are frankly **made**, chosen drawers only you can judge. The reality test (I7) marks exactly where found gives way to made.

**The reference tree (used throughout this document).** So the mechanics have something concrete to point at, this spec illustrates with the author's tree. Its top level is not a list of spheres but a **2×2 — self / world crossed with being / doing**:

|            | being *(what is)*   | doing *(the operation)* |
| ---------- | ------------------- | ----------------------- |
| **self**   | **Ego** (`e`)       | **Actio** (`a`)         |
| **world**  | **Contextus** (`c`) | **Motus** (no node)     |

- **Ego** (`e`) — the self, a substrate and two processes over it: **Corpus** (`ec`) the body (Valetudo `ecv`, Robur `ecr`, Forma `ecf`), **Mens** (`em`) the condition of the estimator (Sanitas, Acies, Habitus), **Anima** (`ea`) the orienting self (Mores `eam`, Vocatio `eav`, Religio `ear`).
- **Contextus** (`c`) — the world as entities you reference, in three registers: **Societas** (`cs`) people and agents (Necessitudo → Gens/Familia, Amicitia, Officium, Civitas), **Locus** (`cl`) places (Habitat, Statio, Urbs, Orbis, Rete), **Res** (`cr`) holdings (Bona, Pecunia, Iura). Album, Mappa, and Rationes home their entities here.
- **Actio** (`a`) — the one operation, cut only by direction: **Cura** (`ac`) tending the self (Corporis → Cibus/Somnus/Exercitatio, Mentis, Animae), **Scientia** (`as`) taking the world in (Disciplina, Indagatio), **Opus** (`ao`) writing onto the world (Fabrica, Administratio). Fasti, Pensum, and Annales express its three tenses — placement, intention, fact (§8.4–8.6, Appendix A) — homing wherever their subject lives, not only here (cores aren't sphere-locked, §6.2).
- **Motus** — the world's own doing; **named to complete the square and never minted** (§8.4). You are its patient, never its author, so it would hold nothing — and a node holding nothing is not real (I7), so no directory bears the name and no code addresses it. Every trace of it you can keep is a *sample*, and a sample is Actio. It is a name in the square, not a node in the tree; like every node here it is the reference tree's own, urged on nobody.

A code names the whole path down from its sphere: in this tree `csa` is Contextus → Societas → Amicitia (friendships); `ecv` is Ego → Corpus → Valetudo (health). The rules that make a code resolve to exactly one path — whether a compact letter/number string or a definition-prefix segment — are §5.1; the full on-disk layout is §6.2.

**The constitution.** What makes a node real is not a schema — there is none — but seven placement rules, emitted by `pan constitution` (§5.5) so a human and an LLM file alike:

1. **Home only** (I3) — one home per record; the path *is* the home.
2. **Sort by what it is** — by essence, never by material or by where it surfaces. (This dissolves a "looks", "digital", "media", or "music" node: those are surfaces and formats, not kinds.)
3. **States in being, change in doing** — a *node* for a being (person, place, thing, state) belongs in a being-branch, a node for a doing in Actio; beings never nest under doings. This cuts the **tree**, never the cores: a record still homes at what it is *about* (rule 1), so a log of your calls with a friend homes at the friend though Annales is Actio's instrument. The cores group by sphere as instruments of one (§4), which is no lock on where their records land (§6.2, §18).
4. **Fields, not nodes** — closeness, role, motive, obligation, origin, format, otium/negotium *color* a record; they are never branches.
5. **Relationships are edges** (I9) — an entity is filed once; membership, association, and provenance are references. Nest only when X is *part of the substance of* Y; reference when X *belongs to / relates to / came from* Y — and never reproduce one branch's structure inside another.
6. **Aboutness, not provenance** — a record homes at what it is *about*, not at the activity or context that produced it; origin is an edge, reconstructed by query.
7. **The reality test** (I7) — a node is real only if distinct things are filed there and it is reviewed apart; a blank sub-level is a finished answer, not a gap.

This governs *your* tree exactly as it governs the reference tree. (On the Latin register and the deity personifications, see Appendix A.)
