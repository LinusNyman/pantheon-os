//! Terminal lifecycle (P§10) — the one piece of the shell no instrument may override.
//!
//! `enter` puts the terminal into raw mode + the alternate screen; a **`Drop` guard**
//! restores it, so a panic anywhere in an instrument still tears down cleanly, and a
//! **panic hook** beside it prints the message *after* the restore, which the
//! alternate screen would otherwise swallow.
//!
//! Because the guard rides on unwinding, a TUI-bearing binary must not build
//! `panic = "abort"` — a profile that runs no destructor. Porticus is a library and
//! cannot pin the final binary's profile, so the workspace `[profile.release]` pins
//! `panic = "unwind"` (§14). This module is why.
//!
//! What does not unwind, no guard can catch: `SIGTERM` and an `abort` profile run no
//! destructor, and Porticus claims no more than it does. `Ctrl-C` is *not* among them
//! while Porticus owns the terminal — raw mode delivers it as a key event rather than
//! a signal — but across a [`Screen::suspend`] cooked mode is restored and it reverts
//! to `SIGINT`, uncaught like the rest.

use std::io::{Stdout, stdout};
use std::sync::Once;

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

/// Installed exactly once per process. `enter` runs again after every
/// [`Screen::suspend`], and a hook that stacked per call would run the restore once
/// per suspension on a later panic.
static HOOK: Once = Once::new();

/// The terminal, owned. Dropping it restores — that is the whole contract.
pub struct Screen {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl Screen {
    /// Take the terminal: panic hook (once), raw mode, alternate screen.
    ///
    /// # Errors
    /// If raw mode, the alternate screen, or sizing the terminal fails.
    pub fn enter() -> std::io::Result<Self> {
        HOOK.call_once(|| {
            let previous = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                // Restore *first*: the alternate screen would swallow the message,
                // and a hand that cannot read the panic cannot report it.
                leave();
                previous(info);
            }));
        });
        raise()?;
        Ok(Self {
            terminal: Terminal::new(CrosstermBackend::new(stdout()))?,
        })
    }

    /// The terminal, for a draw.
    pub fn terminal(&mut self) -> &mut Terminal<CrosstermBackend<Stdout>> {
        &mut self.terminal
    }

    /// Hand the bare terminal to a child, then take it back — the guard run backwards
    /// (P§10).
    ///
    /// An editor relay (§7.3, P§5) needs cooked mode and the real screen. The restore
    /// path here is [`leave`], the *same* function `Drop` calls, so a child that dies
    /// or a panic mid-session lands the terminal clean by the same route as `q`.
    ///
    /// Suspension is Porticus's alone: an instrument that wants an editor asks for the
    /// relay and never spawns a process across a screen it does not own.
    ///
    /// # Errors
    /// If the terminal cannot be retaken after the child returns.
    pub fn suspend<T>(&mut self, child: impl FnOnce() -> T) -> std::io::Result<T> {
        leave();
        let outcome = child();
        raise()?;
        // The child owned the screen; nothing on it is ours to believe.
        self.terminal.clear()?;
        Ok(outcome)
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        leave();
    }
}

/// Take the terminal.
fn raise() -> std::io::Result<()> {
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)
}

/// Give it back. **One implementation**, reached by `Drop`, by `suspend`, and by the
/// panic hook alike — so there is no path that restores differently, or not at all.
///
/// Errors are swallowed deliberately: this runs while unwinding and on the way out,
/// where there is nothing useful left to do with one and a second panic would be
/// worse than a terminal left imperfect.
fn leave() {
    let _ = disable_raw_mode();
    let _ = execute!(stdout(), LeaveAlternateScreen);
}
