//! Porticus — the shared TUI chrome (PORTICUS-SPEC.md). Owns the runtime:
//! terminal lifecycle, event loop, global keymap, shared screens, and the theme.
//! Each instrument is a thin `App` provider (P§2); Porticus owns the *feel* (P-II).
//!
//! Depends on Pantheon only (I5). Scaffold.
