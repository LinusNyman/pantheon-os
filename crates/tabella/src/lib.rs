//! Tabella — documents (meaning) (§8.7). Prose: notes, quotes, principles,
//! reflections. Ego's **Document** shape (§6.1): one loose text file per document,
//! `[code]_[slug].md` (also `.txt`/`.mdx`), edited in place.
//!
//! Not a store of its own — it handles every loose `[code]_*` document in the open
//! node dir, hand-written ones included. Home is the path (I3); the `+++` TOML
//! frontmatter carries `type` and `tags` only, and is all a fold reads.
//!
//! A note homes at what it is *about*, not the activity that spawned it (I3, §2):
//! interview notes at the interviewee's node, a principle in Anima, a reflection on a
//! project at that project.

use pantheon::{Core, DocExt, Error, Frontmatter, Result};

/// Tabella — the Document core (§8.7).
pub struct Tabella;

impl Tabella {
    /// The extension a bare `add` writes. Hardcoded, never a setting (§18) — the same
    /// reasoning as Album's default `person`.
    pub const DEFAULT_EXT: DocExt = DocExt::Md;
}

impl Core for Tabella {
    /// The frontmatter **is** the record (§7.1).
    ///
    /// For the other two shapes `Record` is the `data` half of an envelope
    /// (`Entity<T>`, `Line<T>`). A document has no `data` half distinct from its
    /// envelope — the body is the payload and it is opaque prose, never deserialized
    /// — so the envelope itself is the record. One definition of the `+++` shape,
    /// living in the spine that reads the fence (§6.6).
    type Record = Frontmatter;

    const NAME: &'static str = "tabella";

    // `kinds()` is deliberately **not** overridden. The trait's default empty slice is
    // the entire Document declaration (§7.1) — "the one shape a core states by
    // declaring nothing" — which is why these filenames carry no `__` segment, why
    // `-k` is a usage error here, and what the resolver keys off to route every loose
    // document to this core by extension alone (§5.0).

    /// `type` is a note-kind but a frontmatter **field**, never a token (§8.7) — so it
    /// is validated as a field. Its vocabulary is *open*: a quote, a principle, a
    /// reflection, a portrayal, "or any the user defines", so there is no allow-list
    /// to check against, unlike Album's closed `person`/`organization`/`group`.
    ///
    /// The body is never checked. It is prose (§7.1).
    fn validate(record: &Frontmatter) -> Result<()> {
        if record.r#type.as_ref().is_some_and(|t| t.trim().is_empty()) {
            return Err(Error::validation(
                "`type` is the note-kind, so it cannot be blank — omit it instead (§8.7)",
            ));
        }
        for (i, tag) in record.tags.iter().enumerate() {
            if tag.trim().is_empty() {
                return Err(Error::validation("a tag cannot be blank (§8.7)"));
            }
            if record.tags[..i].contains(tag) {
                return Err(Error::validation(format!(
                    "tag {tag:?} is given twice; tags are a set (§8.7)"
                )));
            }
        }
        Ok(())
    }
}
