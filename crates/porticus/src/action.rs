//! Writes — relay, never originate (P§7, I2).
//!
//! Screens mutate, but they never author. A write is the human's hand passing through
//! a core's verb (§7.2); Porticus's only job is to make that safe and uniform.
//!
//! Authorship stays with the app — only it knows its verb grammar, so only it can
//! build the invocation. The *flow* stays here: which key means which action, whether
//! that action confirms first, and how the relay runs (P-II).

use std::process::Command;

use pantheon::Code;

/// The closed set of standard actions (P§5 Tier 2).
///
/// Closed on purpose: Porticus binds each to one key, once, so a shared verb keeps a
/// shared key across all twelve instruments. A view declares *which* of these it
/// offers and never what key they sit on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// `a` — add at the current node.
    Add,
    /// `e` — edit the focused row. May leave the screen (the editor form, §7.3).
    Edit,
    /// `d` — done / toggle the focused row.
    Done,
    /// `x` — remove the focused row.
    Remove,
    /// `r` — rename the focused row, cascading its refs (§5.4).
    Rename,
    /// `m` — re-home the focused row.
    Move,
    /// `A` — quick add by code, at any node, without navigating.
    QuickAdd,
    /// `D` — done / toggle every item at the current node.
    DoneAll,
    /// `X` — remove every item at the current node. The heaviest friction (P§5).
    RemoveAll,
}

impl Action {
    /// Whether this action opens the Confirm overlay before committing (P§5).
    ///
    /// **This is Porticus's call for the whole suite, not the app's** — change it here
    /// and the feel shifts in all twelve at once, which is exactly P-II's point.
    ///
    /// A single focused-row change (`d`, `e`) is itself the acknowledgement §7.3
    /// requires: you targeted one visible row and, for `e`, typed the new value. So it
    /// relays direct and the TUI stays fluid. Every remove and every bulk action opens
    /// the overlay over a computed `--dry-run`. These are all still *final* (§18 keeps
    /// no undo); what makes the first kind direct is that it is bounded to one visible
    /// row, never that it could be taken back.
    #[must_use]
    pub fn confirms(self) -> bool {
        match self {
            Action::Done | Action::Edit | Action::Add | Action::QuickAdd => false,
            Action::Remove
            | Action::Rename
            | Action::Move
            | Action::DoneAll
            | Action::RemoveAll => true,
        }
    }

    /// The label Help shows (P§4), generated from the live binding so it cannot drift.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Action::Add => "add",
            Action::Edit => "edit",
            Action::Done => "done / toggle",
            Action::Remove => "remove",
            Action::Rename => "rename",
            Action::Move => "move",
            Action::QuickAdd => "quick add by code",
            Action::DoneAll => "done / toggle all here",
            Action::RemoveAll => "remove all here",
        }
    }
}

/// A record's address — its home rides *with* it, since an Agenda's rows are
/// cross-node and each must relay to its own node (P§7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordRef {
    pub home: Code,
    /// The record's key or slug (§5.4) — its identity and its name at once.
    pub key: String,
}

/// What an action acts on (P§3).
///
/// This *is* §7.3's home rule made concrete. Acting on an existing record is a
/// [`Target::Row`], which carries its own home. A **new** add is a [`Target::Node`],
/// whose home Porticus resolves by layout: the tree cursor for a Rail view, the node
/// picker for a Full view, which has no cursor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Row(RecordRef),
    Node {
        node: Code,
        /// A dated Full view's cell date, so `a` on a calendar keeps the day you
        /// pointed at rather than defaulting to today (§7.3).
        at: Option<String>,
    },
}

/// A built CLI invocation — the same command a hand would type (§7.2).
///
/// The app returns one of these from `on_action`; Porticus relays it and never reads
/// its grammar. Note what is absent: no environment, no cwd, no stdin. A relay is a
/// command, not a session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invocation {
    /// The core's three-char short — `pen`, `alb` (§7.3).
    pub short: String,
    pub args: Vec<String>,
}

impl Invocation {
    #[must_use]
    pub fn new(
        short: impl Into<String>,
        args: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            short: short.into(),
            args: args.into_iter().map(Into::into).collect(),
        }
    }

    /// The invocation as a hand would read it — what the Confirm overlay shows and
    /// what the status line names when it fails.
    #[must_use]
    pub fn display(&self) -> String {
        std::iter::once(self.short.as_str())
            .chain(self.args.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The same invocation with `--dry-run`, for the Confirm overlay's computed change
    /// and plan token (§7.3).
    #[must_use]
    pub fn dry_run(&self) -> Self {
        let mut args = self.args.clone();
        args.push("--dry-run".into());
        Self {
            short: self.short.clone(),
            args,
        }
    }

    /// The same invocation, committed.
    ///
    /// **`-y` is mandatory and is not an exemption.** A relay's child writes down a
    /// pipe, where a mutation without `-y` exits `5` (§7.3) — so the acknowledgement
    /// has to be the TUI's modal rather than the CLI's prompt. Porticus supplies the
    /// confirm; it never lets a core skip its own validation (§12, P§7).
    ///
    /// `--plan` rides along where the overlay computed one, so a change that moved
    /// underneath between the review and the commit is refused rather than applied.
    #[must_use]
    pub fn committed(&self, plan: Option<&str>) -> Self {
        let mut args = self.args.clone();
        args.push("-y".into());
        if let Some(token) = plan {
            args.push("--plan".into());
            args.push(token.into());
        }
        Self {
            short: self.short.clone(),
            args,
        }
    }
}

/// How a relayed write reaches a core (P§7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Writer {
    /// A **core's own TUI**: it calls its own write verb in-process, through the very
    /// code the CLI runs, so validation and the plan token are one implementation.
    InProcess,
    /// A **lens**: it shells out to the core binary on `PATH` (§12). It links no core
    /// (I5) and the write crosses the JSON boundary (I4).
    Subprocess,
}

/// What a relay came back with. The status line reads this and nothing else (P§4).
#[derive(Clone, Debug)]
pub struct Relayed {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Relayed {
    #[must_use]
    pub fn ok(&self) -> bool {
        self.code == 0
    }

    /// The parsed contract value, where the core emitted one.
    #[must_use]
    pub fn json(&self) -> Option<serde_json::Value> {
        serde_json::from_str(&self.stdout).ok()
    }

    /// What the status line says (P§4): the core's own `msg` where it gave one, since
    /// a core says why better than Porticus can guess (§7.3).
    #[must_use]
    pub fn message(&self) -> String {
        let reason = serde_json::from_str::<serde_json::Value>(&self.stderr)
            .ok()
            .and_then(|v| v["error"]["msg"].as_str().map(str::to_owned));
        match reason {
            Some(msg) => msg,
            None => match self.code {
                3 => "validation failed".into(),
                4 => "not found".into(),
                5 => "confirmation required".into(),
                6 => "write refused under a rule".into(),
                _ => "the write failed".into(),
            },
        }
    }
}

/// Run a relay as a subprocess and capture it (P§7).
///
/// The emitted record is **captured, never let onto the alternate screen** — a core
/// prints its result to stdout, and a screen Porticus is drawing has no room for it.
///
/// # Errors
/// If the core binary cannot be spawned — which for a lens means it is not on `PATH`,
/// the case §12 degrades rather than fails.
pub fn relay(invocation: &Invocation) -> std::io::Result<Relayed> {
    let out = Command::new(&invocation.short)
        .args(&invocation.args)
        .output()?;
    Ok(Relayed {
        code: out.status.code().unwrap_or(1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

/// Whether a core is on `PATH` (§12).
///
/// A lens **probes before the key is pressed** and dims the action — greyed, dropped
/// from Help — so a missing core makes its action *unavailable* rather than a relay
/// that fails when tried. Graceful degradation is the whole of what makes installing
/// one app real (§15.5).
#[must_use]
pub fn on_path(short: &str) -> bool {
    Command::new(short)
        .arg("version")
        .output()
        .is_ok_and(|out| out.status.success())
}
