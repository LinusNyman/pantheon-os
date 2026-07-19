//! The overlay stack (P§4). The base is the current view; overlays push above it and
//! pop back, and the working set is fixed and shared across all twelve.
//!
//! `Esc` unwinds in order. `q`'s meaning **follows the top of the stack**: inside a
//! text-entry overlay it types a literal `q` (P-I); over Help, Title, or Confirm it is
//! inert and `Esc` dismisses; only at the bare base does it quit.

use crate::action::{Action, Invocation, Target};

/// Why a line prompt is open — what its answer will be used for.
#[derive(Clone, Debug)]
pub enum Prompt {
    /// `r` — the new name, which will re-slug the record and cascade its refs (§5.4).
    Rename(Target),
    /// `A` — quick add by code: the node, then the content.
    QuickAddCode,
    /// `A`, second leg — the content, at the code just given.
    QuickAddContent(String),
    /// A Full view's `a`, which has no tree cursor to resolve a home from (P§7).
    ///
    /// P§4 wants the tree itself as a modal here; this is the line-entry form of the
    /// same question, resolving a home by code.
    PickHome,
}

/// One overlay.
pub enum Overlay {
    /// `+` — the instrument's name, symbol, tagline, and both versions (§15.5).
    Title,
    /// `?` — generated from the live keymap, so help can never drift from the
    /// bindings (P§4).
    Help,
    /// `/` — incremental match over the focused row-view or the tree (P§6).
    Search { buffer: String },
    /// A line prompt. Porticus prompts for a *line* — never for prose, which is the
    /// hand's own editor's job (P§11).
    Line {
        prompt: Prompt,
        label: String,
        buffer: String,
    },
    /// The computed change and its plan token, before a write commits (P§7, §7.3).
    Confirm {
        action: Action,
        invocation: Invocation,
        /// The plan token from the `--dry-run` relay, where it produced one.
        token: Option<String>,
        /// The change as the core computed it — shown, never interpreted.
        change: String,
        /// Set for `X` (remove-all), the one that demands a distinct, heavier
        /// keystroke: the count named and an explicit key, never a stray `y` (P§5).
        heavy: Option<usize>,
    },
}

impl Overlay {
    /// Whether this overlay holds the keyboard for typing (P-I).
    ///
    /// While one does, every printable key is a literal character and only `Esc` and
    /// `Enter` stay chrome — chrome and typing cannot both own the keyboard. This is
    /// the single carve-out in an otherwise universal reservation, and it exists
    /// because a prompt Porticus itself runs is the one place `?` and `+` must type
    /// rather than fire.
    #[must_use]
    pub fn is_text_entry(&self) -> bool {
        matches!(self, Overlay::Search { .. } | Overlay::Line { .. })
    }

    /// The buffer being typed into, if any.
    pub fn buffer_mut(&mut self) -> Option<&mut String> {
        match self {
            Overlay::Search { buffer } | Overlay::Line { buffer, .. } => Some(buffer),
            _ => None,
        }
    }

    /// The title bar of the overlay's own box.
    #[must_use]
    pub fn heading(&self) -> String {
        match self {
            Overlay::Title => "title".into(),
            Overlay::Help => "help".into(),
            Overlay::Search { .. } => "search".into(),
            Overlay::Line { label, .. } => label.clone(),
            Overlay::Confirm { action, .. } => format!("confirm — {}", action.label()),
        }
    }
}
