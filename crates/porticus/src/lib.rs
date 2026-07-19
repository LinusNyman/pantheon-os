//! Porticus — the shared TUI chrome (`docs/src/PORTICUS-SPEC.md`, cited `P§n`).
//!
//! The portico every instrument stands behind: one shell, one keymap, one look. It
//! owns the runtime — terminal lifecycle, event loop, global keymap, shared screens,
//! and the theme — so each of the twelve TUIs is a thin *provider* rather than a
//! re-implementation of the same chrome.
//!
//! # What binds it
//!
//! - **I1 — derived-out.** Every screen recomputes from readings each frame. Porticus
//!   stores no rendered value: not a tile total, not a tree count, not a folded
//!   present. A count badge is derived on refresh, never written to disk.
//! - **I2 — relay-only writes.** No screen ever *originates* a write. A keystroke that
//!   mutates is the human's hand relayed through a core's write verb; Porticus
//!   supplies the confirm, not the authorship ([`action`]).
//! - **I3 / I8 — one node, one look, three hands.** A node renders identically in
//!   every instrument, because the render reads the tree rather than a per-tool config.
//! - **I4 — JSON is the boundary.** A screen consumes rows and is blind to their
//!   origin. Porticus never imports a core to render it.
//! - **I5 — hub-and-spoke.** Porticus → Pantheon, full stop. It links no core and no
//!   Tessera; a grid of tiles is a lens's own draw-view, never a catalog view here.
//! - **§18 — no config, no knobs.** Keymap, palette, and layout are hardcoded. There
//!   is no `porticus.toml`, no theme setting, no rebindable key.
//!
//! # P-II — feel is owned, not just look
//!
//! Porticus owns *behaviour*, not only appearance: whether a mutation is confirm-first
//! or direct, how errors surface, the navigation model. All of it lives here, so **one
//! change reshapes every app at once** — that is the whole reason the layer exists. A
//! view contributes *what* is shown and *which* actions exist; it never decides *how*
//! an interaction feels.

pub mod action;
pub mod app;
pub mod ident;
pub mod keymap;
pub mod overlay;
pub mod rail;
pub mod runtime;
pub mod term;
pub mod theme;
pub mod view;
pub mod views;

pub use action::{Action, Invocation, RecordRef, Relayed, Target, Writer};
pub use app::App;
pub use ident::Ident;
pub use runtime::{as_text, drive, keys, render_once, run};
pub use theme::Theme;
pub use view::{Grid, GridCell, Handled, Layout, Nav, Row, View, ViewId};
