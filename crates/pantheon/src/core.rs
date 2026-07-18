//! The `Core` substrate (§7.1) and runtime core discovery (§5.0).
//!
//! A core declares only what is core-specific — a record type, its name, its
//! tokens, and a `validate`. The spine calls no specific core's `validate` (that
//! runs inside the core's own bin, I5); it learns an installed core's tokens by
//! running its `schema` verb over PATH — never by linking it.

use std::collections::HashMap;
use std::process::Command;

use crate::Shape;
use crate::schema::CoreSchema;

/// A core: a record type plus its primitive, its tokens, and its `validate` (§7.1).
/// The spine defines the trait; each core implements it. Everything else — the
/// twelve verbs, storage dispatch, resolution — the spine provides generically.
pub trait Core {
    /// The `data` shape; an enum where a core declares more than one token.
    type Record: serde::Serialize + serde::de::DeserializeOwned + schemars::JsonSchema;
    /// The `core:` half of a `core:slug` reference — e.g. `"album"`.
    const NAME: &'static str;
    /// Token → shape pairs; empty names a Document core (§7.1).
    fn kinds() -> &'static [(&'static str, Shape)] {
        &[]
    }
    /// Checks beyond the envelope (§7.1) — schema-level invariants the core owns.
    fn validate(record: &Self::Record) -> crate::Result<()>;
}

/// The seven cores' shorts (§8), the fixed set `discover` probes on PATH.
const KNOWN_CORE_SHORTS: &[&str] = &["alb", "map", "rat", "fas", "pen", "ann", "tab"];

/// One installed core as learned from its `schema` verb (§5.0, §7.2).
#[derive(Clone, Debug)]
pub struct DiscoveredCore {
    pub name: String,
    pub short: String,
    pub kinds: Vec<(String, Shape)>,
    pub format_version: u32,
}

/// The file→core map built once per command from PATH discovery (§5.0). Kinds are
/// globally unique across cores, so a filename's `[kind]` segment names its owner;
/// the registry is what lets the spine resolve and enforce per-core uniqueness
/// without importing a core (I5).
pub struct CoreRegistry {
    cores: Vec<DiscoveredCore>,
    by_kind: HashMap<String, usize>,
}

impl CoreRegistry {
    /// Build a registry from a known set of cores — the deterministic path tests and
    /// snapshots use, so resolution and validation run with zero installed cores.
    #[must_use]
    pub fn from_cores(cores: Vec<DiscoveredCore>) -> Self {
        let mut by_kind = HashMap::new();
        for (i, c) in cores.iter().enumerate() {
            for (kind, _) in &c.kinds {
                by_kind.entry(kind.clone()).or_insert(i);
            }
        }
        Self { cores, by_kind }
    }

    /// Discover installed cores by running each known short's `schema` verb on PATH
    /// (§5.0). A core that is absent, errors, or emits unparseable JSON is skipped;
    /// with none installed the registry is empty.
    #[must_use]
    pub fn discover() -> Self {
        let cores = KNOWN_CORE_SHORTS
            .iter()
            .filter_map(|s| discover_one(s))
            .collect();
        Self::from_cores(cores)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cores.is_empty()
    }

    #[must_use]
    pub fn cores(&self) -> &[DiscoveredCore] {
        &self.cores
    }

    /// The core owning a kind token, if any.
    #[must_use]
    pub fn core_of_kind(&self, kind: &str) -> Option<&DiscoveredCore> {
        self.by_kind.get(kind).map(|&i| &self.cores[i])
    }

    /// A kind's shape, if the kind is owned by an installed core.
    #[must_use]
    pub fn shape_of_kind(&self, kind: &str) -> Option<Shape> {
        self.core_of_kind(kind)
            .and_then(|c| c.kinds.iter().find(|(k, _)| k == kind).map(|(_, s)| *s))
    }

    /// A core's declared tokens, by core name.
    #[must_use]
    pub fn kinds_of(&self, core: &str) -> Option<&[(String, Shape)]> {
        self.cores
            .iter()
            .find(|c| c.name == core)
            .map(|c| c.kinds.as_slice())
    }

    /// Kinds declared by more than one core — a totality violation `pan doctor`
    /// reports (§5.5). Sorted by kind for a stable rendering.
    #[must_use]
    pub fn token_collisions(&self) -> Vec<(String, Vec<String>)> {
        let mut owners: HashMap<&str, Vec<&str>> = HashMap::new();
        for c in &self.cores {
            for (kind, _) in &c.kinds {
                owners
                    .entry(kind.as_str())
                    .or_default()
                    .push(c.name.as_str());
            }
        }
        let mut out: Vec<(String, Vec<String>)> = owners
            .into_iter()
            .filter(|(_, v)| v.len() > 1)
            .map(|(k, v)| (k.to_string(), v.into_iter().map(str::to_string).collect()))
            .collect();
        out.sort();
        out
    }
}

fn discover_one(short: &str) -> Option<DiscoveredCore> {
    let out = Command::new(short).arg("schema").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let cs: CoreSchema = serde_json::from_slice(&out.stdout).ok()?;
    Some(DiscoveredCore {
        name: cs.name,
        short: short.to_string(),
        kinds: cs.tokens.into_iter().map(|t| (t.token, t.shape)).collect(),
        format_version: cs.format_version,
    })
}
