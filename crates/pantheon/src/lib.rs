//! Pantheon — the spine (§5). Addressing, resolution, the record envelope, the
//! `Core` substrate (§7.1), and write-time validation. Everything links this;
//! nothing points sideways (I5).
//!
//! Module map: [`name`] normalization (§5.1) · [`code`] addressing (§5.1) ·
//! [`shape`] the three storage shapes (§7.1) · [`envelope`] records & refs (§5.4) ·
//! [`classify`] the file→core map (§5.2) · [`document`] the `+++` fence over opaque
//! prose (§6.6) · [`core`] the `Core` trait & PATH
//! discovery (§7.1) · [`schema`] the discovery surface (§7.2) · [`root`] root
//! resolution (§6.2) · [`tree`] the walk (§5.0) · [`resolve`] `core:slug` →
//! record (§5.0) · [`validate`] the cross-cutting lint (§5.5) · [`lock`] the record
//! write primitive (§6.4) · [`store`] the verb machinery (§7.1) · [`contract`] the
//! verb runner every core's CLI ends in (§7.1, §7.3) · [`plan`] planned
//! transactions (§10.1) · [`meta`] node annotations (§5.2) · [`error`] exit codes
//! (§7.3).

// Pedantic is on in CI (`-W clippy::pedantic -D warnings`); we satisfy it. The five
// lints allowed below are the conventional pedantic-noise set: doc/naming-preference
// checks that would otherwise bury the spec references our doc comments deliberately
// carry (§5.1, I3, PANTHEON_ROOT, …). Errors are the exit-code contract (§7.3),
// documented centrally rather than per-fn.
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::doc_markdown)]

pub mod cascade;
pub mod classify;
pub mod code;
pub mod contract;
pub mod core;
pub mod document;
pub mod envelope;
pub mod error;
pub mod lock;
pub mod meta;
pub mod mint;
pub mod name;
pub mod node_ops;
pub mod plan;
pub mod resolve;
pub mod root;
pub mod schema;
pub mod shape;
pub mod store;
pub mod table;
pub mod tree;
pub mod validate;

pub use cascade::{Cascade, RefRewrite, occupied_slug, plan_cascade};
pub use classify::{DocExt, FileClass, classify};
pub use code::{CharToken, Code, CodeForm, NodeName};
pub use contract::{
    Checkpoint, DocumentQuery, DocumentTarget, Edited, EntityQuery, EntityTarget, RecordChange,
    RegisterQuery, RegisterTarget, Response, SeriesTarget, peel_home,
};
pub use core::{Core, CoreRegistry, DiscoveredCore};
pub use document::{Document, read_frontmatter};
pub use envelope::{Entity, Frontmatter, Key, KeyShape, Line, RawEntity, RawLine, Ref};
pub use error::{Error, ExitCode, Result};
pub use lock::with_record_lock;
pub use meta::{Annotations, read_annotations, set_annotations};
pub use mint::{NewSpec, plan_new};
pub use name::normalize;
pub use node_ops::plan_rm;
pub use plan::{Change, Outcome, Plan};
pub use resolve::{RefOutcome, Resolution, resolve_all};
pub use root::resolve_root;
pub use schema::{CoreSchema, TokenSchema, schema};
pub use shape::Shape;
pub use store::{
    DocumentAddr, DocumentRef, EntityAddr, EntityForm, EntityRef, PresentLine, SeriesRef, Store,
};
pub use tree::{Node, TreeRoot, build_tree, resolve_code, resolve_node};
pub use validate::{Finding, FindingCode, Severity, validate};
