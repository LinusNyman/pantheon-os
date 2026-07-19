//! The app model (P§2): Porticus owns the loop; the instrument is a value it drives.
//!
//! **The split.** Porticus owns all of the *feel* — terminal setup and teardown, the
//! event loop, the tree pane, the chrome keymap, the confirm policy, search and
//! filter, the overlays, and the palette. The instrument owns only *content*: its
//! identity, its lineup, its per-node count, its writer, and the action→invocation
//! mapping. Everything that differs between tools lives in those five; everything
//! about how a tool *behaves* lives here — so a change to the confirm flow, the search
//! timing, or a prompt is one edit that moves all twelve.

use pantheon::Code;

use crate::action::{Action, Invocation, Target, Writer};
use crate::ident::Ident;
use crate::view::View;

/// What an instrument provides. Five methods, and none of them is about appearance.
pub trait App {
    /// Identity → title, header, accent (P§8).
    fn ident(&self) -> Ident;

    /// One to nine views; `[0]` opens on launch and number keys map to lineup order
    /// (P§3). [`run`](crate::run) rejects an empty lineup — there is no `[0]` — and a
    /// tenth view, which would have no switch key.
    fn lineup(&mut self) -> Vec<Box<dyn View>>;

    /// This instrument's items at a node — the count badge (P§6).
    ///
    /// Derived on the frame it is shown and never stored (I1).
    fn count_at(&mut self, node: &Code) -> usize;

    /// The dim only (P§6). Override where a count is costly: the badge is then exact
    /// where it shows and the dim cheap everywhere.
    fn any_at(&mut self, node: &Code) -> bool {
        self.count_at(node) > 0
    }

    /// How a relayed write reaches a core (P§7).
    fn writer(&self) -> Writer;

    /// Build the core-verb invocation — *the same command a hand would type*.
    ///
    /// Only the app knows its verb grammar, because only the app authors the write
    /// (I2). Porticus owns the confirm and the relay and knows none of it.
    ///
    /// `None` → the action does not apply to this target: Porticus no-ops it and says
    /// so on the status line, rather than relaying something the core will refuse.
    fn on_action(&mut self, action: Action, target: &Target) -> Option<Invocation>;

    /// Run one relayed invocation.
    ///
    /// The default shells out, which is a **lens's** path (§12): it links no core (I5)
    /// and the write crosses the JSON boundary (I4).
    ///
    /// A **core's own TUI** overrides this to call its write verb *in-process* through
    /// the shared verb machinery — the very code the CLI runs, so validation and the
    /// plan token are one implementation rather than a re-do (P§7). Declaring
    /// [`Writer::InProcess`] without overriding this would shell out to itself, which
    /// works but re-does the work; the two are meant to move together.
    ///
    /// # Errors
    /// If the child cannot be spawned — for a lens, that a core is not on `PATH`.
    fn execute(&mut self, invocation: &Invocation) -> std::io::Result<crate::action::Relayed> {
        crate::action::relay(invocation)
    }

    /// The cores this instrument relays to, for the `PATH` probe (§12, P§7).
    ///
    /// A core absent from `PATH` makes its actions *unavailable* — dimmed before the
    /// key is pressed — rather than a relay that fails when tried. A core's own TUI
    /// writes in-process and needs no probe, so this is empty for one and the lens's
    /// whole reach for the other.
    fn relays_to(&self) -> Vec<String> {
        Vec::new()
    }
}
