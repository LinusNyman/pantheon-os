//! Speculum — the review lens, as a lib (§12, P§9).
//!
//! The CLI, the mosaic, the horizon and the screen live here rather than in the bin
//! because an integration test links the *lib*: a screen in the bin is a screen no test
//! can reach. `main.rs` is the ~30-line clap shell §14 asks for.
//!
//! A lens links **no core** (I5) and never originates a write (I2). What it may do is
//! *relay* a human-initiated one by shelling out to the same verb a hand would type —
//! which is why [`Speculum`] being constructible matters more here than anywhere else:
//! driving it exercises §12's cross-process relay end to end, over real core binaries
//! discovered on `PATH`.
//!
//! **The I4 guard.** The lib exposes exactly two things — [`run_cli`] and [`Speculum`].
//! `Cli`, `run`, and every verb stay `pub(crate)`, so the JSON stays the only contract:
//! a verb reachable as a Rust function would be a second door into the lens.

mod cli;
#[cfg(feature = "tui")]
mod horizon;
#[cfg(feature = "tui")]
mod mosaic;
#[cfg(feature = "tui")]
mod screen;

pub use cli::run_cli;
#[cfg(feature = "tui")]
pub use screen::Speculum;
