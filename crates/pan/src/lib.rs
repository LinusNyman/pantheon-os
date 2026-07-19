//! Pantheon's structural tool (§10) — the tree, and what is wrong with it.
//!
//! The CLI and the screen live here rather than in the bin for one reason: an
//! integration test links the *lib*, so a screen in the bin is a screen no test can
//! reach — and step 6 found three defects that only driving a screen caught (P§3, §14).
//! `main.rs` is the ~30-line clap shell §14 asks for.
//!
//! **What that must not cost is I4.** A verb reachable as a Rust function would be a
//! second door into this tool, so the verbs stay `pub(crate)`. The only things public
//! here are [`run_cli`] — the whole CLI, entered exactly as the binary enters it — and
//! [`PanApp`], which relays through that same CLI.

mod cli;
// The screen rides the `tui` feature; drop it and the structural CLI stands alone (§14).
#[cfg(feature = "tui")]
mod screen;

pub use cli::run_cli;
#[cfg(feature = "tui")]
pub use screen::PanApp;
