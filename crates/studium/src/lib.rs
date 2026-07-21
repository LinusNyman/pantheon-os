//! Studium — the studies lens, as a lib (§12, §19, P§9).
//!
//! The CLI, the folds, the mosaic and the screen live here rather than in the bin because
//! an integration test links the *lib*: a screen in the bin is a screen no test can reach.
//! `main.rs` is the ~30-line clap shell §14 asks for.
//!
//! A lens links **no core** (I5) and never originates a write (I2). What it may do is
//! *relay* a human-initiated one by shelling out to the same verb a hand would type —
//! which is why [`Studium`] being constructible matters more here than anywhere else:
//! driving it exercises §12's cross-process relay end to end, over real core binaries
//! discovered on `PATH`.
//!
//! **What that must not cost is I4.** A verb reachable as a Rust function would be a
//! second door past the JSON contract, so the only things public are [`run_cli`] — the
//! whole CLI, entered as the binary enters it — and [`Studium`], which relays through the
//! core CLIs. `Cli`, `run`, and the folds stay `pub(crate)`/module-private.

// The fold reads a `home` code and never mutates it; the curriculum reader chains
// `Option`s densely. These are the spine's conventional pedantic allows.
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::doc_markdown)]

mod cli;
mod curriculum;
mod fold;
#[cfg(feature = "tui")]
mod mosaic;
#[cfg(feature = "tui")]
mod screen;

pub use cli::run_cli;
#[cfg(feature = "tui")]
pub use screen::Studium;
