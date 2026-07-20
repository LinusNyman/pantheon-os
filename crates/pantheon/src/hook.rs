//! The Auspex wake (§9.4): how a write reaches the one reactive writer (I2).
//!
//! There is no daemon, no timer, and no cache dir. **Every core, after a successful
//! write, spawns `aus run --trigger <core>@<home>` detached and forgets it**; a
//! core's TUI opening spawns a bare `aus run` instead, a wake no single write
//! authored having no write to name (§9.3). Those hooks and a hand's own `aus run`
//! are the only wakes there are.
//!
//! **The spine does not name Auspex a dependency** (I5): it looks for `aus` on
//! `PATH` and, finding nothing, does nothing. That is the state steps 1–7 ran in.
//!
//! ## One wake per command, not per write
//!
//! A write *notes* its trigger here ([`note_write`]); the wake fires once at the end
//! of the process ([`wake_if_noted`], called from [`crate::contract::dispatch`]). A
//! verb writing three lines therefore wakes Auspex once rather than three times.
//!
//! The note also decides whether there *is* a trigger. §9.3 says one is absent
//! "wherever no single write authored the wake", so two notes disagreeing — a
//! `move` between homes, a batch spanning several — collapse to [`Noted::Several`]
//! and the wake goes out triggerless. A trigger names a write; a wake that is not
//! one stays silent rather than naming a write that never happened.

use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::code::Code;

/// What this process has written so far, as far as a trigger is concerned.
enum Noted {
    /// Every write so far agreed on one `core@home`.
    One(String),
    /// Writes landed at more than one `core@home`; no single write authored the
    /// wake, so it carries no trigger (§9.3).
    Several,
}

static PENDING: Mutex<Option<Noted>> = Mutex::new(None);

/// Set by a process that must never wake Auspex — see [`suppress`].
static SUPPRESSED: AtomicBool = AtomicBool::new(false);

/// Silence every wake from *this* process, for as long as it runs.
///
/// **Auspex calls this at startup**, and it is the reason the hook does not chase its
/// own tail: [`crate::contract::dispatch`] fires for every instrument and
/// `porticus::run` wakes on every screen open, so without this `aus`'s own rules
/// browser would spawn `aus run` the moment it opened. §9.4 scopes that wake to "a
/// core's TUI opening", and Auspex is not a core.
///
/// The twin of `PANTHEON_NO_HOOKS`, which Auspex sets on the core commands it spawns
/// (§9.5): one mechanism says *not this process*, the other *not that child*. Both
/// are needed — the env var cannot silence the process that sets it without also
/// silencing everything it inherits from, and a rule's own reads must stay hookless
/// for their own reasons.
pub fn suppress() {
    SUPPRESSED.store(true, Ordering::Relaxed);
}

/// Note a successful write so the wake at the end of the process can name it.
///
/// Called from the [`Store`](crate::store::Store) mutators, which are generic over
/// [`Core`](crate::core::Core) — so the spine forwards `C::NAME` without knowing any
/// core (I5), and one call site serves all seven.
pub fn note_write(core: &str, home: &Code) {
    let trigger = format!("{core}@{}", home.as_str());
    let Ok(mut pending) = PENDING.lock() else {
        // A poisoned lock means another thread panicked mid-note. The wake is
        // best-effort by design (§9.4), so losing it is not worth a panic here.
        return;
    };
    *pending = Some(match pending.take() {
        None => Noted::One(trigger),
        Some(Noted::One(first)) if first == trigger => Noted::One(first),
        Some(_) => Noted::Several,
    });
}

/// Note a batch whose writes no single trigger describes — a rename cascade
/// (§5.4), which touches whatever nodes hold a ref and so names none of them.
///
/// Notes rather than wakes, so a verb that both rewrites a record *and* cascades its
/// refs still wakes Auspex exactly once, from [`wake_if_noted`]. A caller that does
/// not reach [`crate::contract::dispatch`] — `pan`'s node-level cascade, when §10.1
/// lands — must call [`wake_if_noted`] in its own tail.
pub fn note_batch() {
    let Ok(mut pending) = PENDING.lock() else {
        return;
    };
    *pending = Some(Noted::Several);
}

/// Wake Auspex if anything was written, naming the write where one authored it.
///
/// The tail of [`crate::contract::dispatch`], so every core reaches it identically.
pub fn wake_if_noted() {
    let Ok(mut pending) = PENDING.lock() else {
        return;
    };
    match pending.take() {
        None => {}
        Some(Noted::One(trigger)) => wake(Some(&trigger)),
        Some(Noted::Several) => wake(None),
    }
}

/// Spawn `aus run [--trigger <core>@<home>]` detached, and forget it (§9.4).
///
/// Never waits, never reads the child's output, and never propagates its failure:
/// the writing core's exit code is its own, and a rule that errors is Auspex's to
/// report. Silent where `aus` is not installed (I5) and where `PANTHEON_NO_HOOKS`
/// is set — the latter is what Auspex's own applies carry, so a rule cannot recurse.
pub fn wake(trigger: Option<&str>) {
    if SUPPRESSED.load(Ordering::Relaxed) || std::env::var_os("PANTHEON_NO_HOOKS").is_some() {
        return;
    }
    let mut cmd = Command::new("aus");
    cmd.arg("run");
    if let Some(trigger) = trigger {
        cmd.arg("--trigger").arg(trigger);
    }
    // No stdio to inherit, so the child cannot write over a table the caller is still
    // printing — and §13 is explicit that Auspex's own `tracing` output is discarded
    // here, being kept only when a hand runs `aus` itself.
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    detach(&mut cmd);
    // `aus` absent from PATH surfaces as a spawn error, which is exactly the no-op
    // §9.4 asks for — so there is no separate PATH probe to keep in step with this
    // call. Nothing waits on the child: the writing core's exit code is its own.
    let _ = cmd.spawn();
}

/// Cut the child loose from the caller's process group, per §13.
///
/// Without this the hook is only half detached: the child stays in the caller's
/// **process group**, so a `Ctrl-C` at the terminal signals it too and can kill a rule
/// mid-write — the one thing a detached wake must not be exposed to. Nulled stdio and
/// an un-awaited handle do not give this on their own.
#[cfg(unix)]
fn detach(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    cmd.process_group(0);
}

/// Windows' counterpart: no console, and no inheritance of the caller's.
#[cfg(windows)]
fn detach(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    /// `DETACHED_PROCESS` — the child gets no console of its own and does not join
    /// the parent's (§13).
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    cmd.creation_flags(DETACHED_PROCESS);
}

/// Anywhere else, nulled stdio and an un-awaited child are the whole of what std
/// offers — which is still the fire-and-forget §9.4 asks for, minus the signal
/// isolation the two platforms above can give.
#[cfg(not(any(unix, windows)))]
fn detach(_cmd: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(s: &str) -> Code {
        Code::parse(s).unwrap()
    }

    fn take() -> Option<Noted> {
        PENDING.lock().unwrap().take()
    }

    /// One test, not three: [`PENDING`] is process-wide state and Cargo threads the
    /// tests within a binary, so three cases resetting it would race each other.
    #[test]
    fn what_a_process_notes_decides_whether_the_wake_names_a_write() {
        // A read-only command notes nothing, so nothing wakes (§9.4).
        assert!(take().is_none());

        // A verb writing twice to one home still has one write to name (§9.3).
        note_write("pensum", &code("acm"));
        note_write("pensum", &code("acm"));
        assert!(matches!(take(), Some(Noted::One(t)) if t == "pensum@acm"));

        // Two homes — a `move` — leave no single write to name, so the wake goes
        // out triggerless rather than naming one of them (§9.3).
        note_write("pensum", &code("acm"));
        note_write("pensum", &code("ecv"));
        assert!(matches!(take(), Some(Noted::Several)));

        // Two cores at one home collapse the same way.
        note_write("pensum", &code("acm"));
        note_write("annales", &code("acm"));
        assert!(matches!(take(), Some(Noted::Several)));

        // And taking the note clears it: the wake fires once (§9.4).
        assert!(take().is_none());
    }
}
