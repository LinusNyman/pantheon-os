//! Album — societas agents (who) (§8.1). People and the bodies they form, stored as
//! a **partitioned register**: one `.json` object per agent, its kind and slug in the
//! filename (§6.1). Referenced everywhere as `album:<slug>`.
//!
//! Three filename kinds, all partitioned: `person` (an individual), `organization`
//! (a formal body — a company, a school, a state), `group` (an informal set — a
//! family, a friend group, a book club). Homed under Societas by the **nature of the
//! bond**, one agent one file — but not sphere-locked (§6.2, I7).
//!
//! Closeness, role, origin, and gender are **fields, not nodes** (§2). The kind says
//! what an agent *is* and is corrected only by the file-rename `edit -k` (§7.2); a
//! form of address like *Mr/Ms* is **derived** from gender at render time, never
//! stored (I1, §18). Where you met someone is provenance and which context they
//! belong to is an edge; neither is ever their home (I3, I9) — a membership is a
//! reference to the group entity (`refs: ["album:book_club"]`), read from either end
//! and never a nesting.
//!
//! Build order step 3 — the first partitioned register: the second shape, `core:slug`
//! refs, the resolver's filename path, and the rename cascade (§16, §5.4).
