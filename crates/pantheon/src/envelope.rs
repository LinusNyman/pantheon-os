//! The record envelope (§5.4): references, the two JSON on-disk record bodies, the
//! document frontmatter, and the series key.
//!
//! A record's `home`, `core`, and — for a partitioned entity — its `kind` and
//! `slug` are its file's location and name, never stored inside (I3), so the
//! envelope structs carry none of them. The spine validates only what is
//! cross-cutting (refs, node path), so its own record type leaves `data` an
//! unparsed [`RawValue`](serde_json::value::RawValue): the spine never knows a
//! core's record shape (I5).

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{Error, Result};

/// The one reference form a record has to another record: `core:slug` (§5.4, I3,
/// I9). Location-independent — it survives a re-home untouched. (De)serializes as
/// the bare string `"core:slug"`, so the on-disk `refs` array is `["album:mara"]`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Ref {
    pub core: String,
    pub slug: String,
}

impl Ref {
    /// Parse `core:slug`, normalizing each half (§5.1).
    pub fn parse(s: &str) -> Result<Ref> {
        let (core, slug) = s
            .split_once(':')
            .ok_or_else(|| Error::usage(format!("reference {s:?} is not `core:slug`")))?;
        let core = crate::name::normalize_token(core, "reference core")?;
        let slug = crate::name::normalize_token(slug, "reference slug")?;
        Ok(Ref { core, slug })
    }

    #[must_use]
    pub fn to_token(&self) -> String {
        format!("{}:{}", self.core, self.slug)
    }
}

impl std::fmt::Display for Ref {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.core, self.slug)
    }
}

impl Serialize for Ref {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_token())
    }
}

impl<'de> Deserialize<'de> for Ref {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ref::parse(&s).map_err(serde::de::Error::custom)
    }
}

/// A partitioned-entity file body (§5.4). `kind`/`slug`/`home` are the file's name
/// and location, not present here (I3).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Entity<T> {
    #[serde(default = "Vec::new")]
    pub refs: Vec<Ref>,
    pub data: T,
}

/// One `.jsonl` series line (§5.4). The `key` — a date for a reading, or the
/// record's name (its slug) for a register line — varies line to line, so it rides
/// in the envelope rather than the filename.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Line<T> {
    pub key: Key,
    #[serde(default = "Vec::new")]
    pub refs: Vec<Ref>,
    pub data: T,
}

/// A document's TOML frontmatter envelope (§6.1, §6.6). The body rides alongside as
/// opaque text and is never deserialized. No `home` key (I3).
///
/// This is Tabella's whole `Core::Record` (§7.1) — hence `JsonSchema`, which the
/// `schema` verb needs. There is no `refs` field, deliberately: a document's
/// frontmatter carries `type` and `tags` and nothing else, which is why `-r` is a
/// usage error on a Document core (§7.3) and why the rename cascade skips documents
/// outright (§5.4).
#[derive(Serialize, Deserialize, schemars::JsonSchema, Clone, Debug, Default, PartialEq, Eq)]
pub struct Frontmatter {
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    // `default`, not `default = "Vec::new"`: the two are identical for a `Vec`, but
    // schemars' derive cannot infer `T` through the path form.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// The best-effort classification of a series [`Key`] (§5.4). Best-effort by design
/// — a name whose slug is all digits is indistinguishable from a date by shape, so
/// the authoritative reading is the owning core's, which knows whether its series is
/// date- or name-keyed (I5).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyShape {
    /// `YYMMDD`.
    Date,
    /// `YYMMDD` plus a time.
    DateTime,
    /// A record name (a slug).
    Name,
}

/// A series line key (§5.4). A spine-opaque string: a date key is stable identity
/// and kept as-is; any other key is normalized to its slug (§5.1) on the way in.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Key(String);

impl Key {
    /// Parse a key. A date-shaped key is kept verbatim (it is its own identity,
    /// §5.4); any other key is normalized to a slug.
    pub fn parse(s: &str) -> Result<Key> {
        if classify_key(s) == KeyShape::Name {
            Ok(Key(crate::name::normalize_token(s, "key")?))
        } else {
            Ok(Key(s.to_string()))
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn classify(&self) -> KeyShape {
        classify_key(&self.0)
    }
}

impl std::fmt::Display for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for Key {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Key {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Key::parse(&s).map_err(serde::de::Error::custom)
    }
}

fn classify_key(s: &str) -> KeyShape {
    let bytes = s.as_bytes();
    let all_digits = |r: &[u8]| !r.is_empty() && r.iter().all(u8::is_ascii_digit);
    if s.len() == 6 && all_digits(bytes) {
        return KeyShape::Date;
    }
    if s.len() > 6 && all_digits(&bytes[..6]) {
        let rest = s[6..].strip_prefix('T').unwrap_or(&s[6..]);
        if all_digits(rest.as_bytes()) {
            return KeyShape::DateTime;
        }
    }
    KeyShape::Name
}

/// The spine's own entity record: `data` left unparsed (I5). The spine deserializes
/// this to validate refs and node paths without knowing a core's record shape.
pub type RawEntity = Entity<Box<serde_json::value::RawValue>>;

/// The spine's own series line: `data` left unparsed (I5).
pub type RawLine = Line<Box<serde_json::value::RawValue>>;
