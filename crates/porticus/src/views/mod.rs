//! The catalog (P§3) — the views Porticus ships, so a look is written once for twelve
//! (I3).
//!
//! **Composition, not inheritance.** An instrument declares a lineup by picking from
//! here and slotting in its own. "Shared with a twist" falls straight out: a catalog
//! view is *parameterised by the instrument*, so one implementation serves every core
//! that wants it.
//!
//! The seam is the constructor. `::of(fold)` **captures the instrument's fold** — a
//! closure over its own store — and the view calls it in `rows`/`draw` to build its
//! rows **fresh each frame**. Folded on demand, never a value frozen at construction,
//! which would be a stored present (I1). So the [`App`](crate::App) trait grows no
//! method per catalog view (P§2 stays closed), and the app folds its **own** JSON
//! in-process — no side channel to a core, no second contract (I4).
//!
//! **No mosaic here.** A grid of tiles is nothing but Tessera, and Porticus links no
//! Tessera (§11) — so the mosaic is a draw-view each lens writes from the tiles it
//! links itself. The catalog holds only what Porticus can draw with Pantheon and
//! `ratatui` alone.

mod agenda;
mod tree_file;

pub use agenda::Agenda;
pub use tree_file::TreeFile;
