//! The Document shape's file body (§6.1, §6.6): a `+++`-fenced TOML [`Frontmatter`]
//! over opaque prose.
//!
//! **Frontmatter needs no parser of its own** (§13). The fence is found by scanning
//! for it and the TOML between is `toml_edit`'s, like every other TOML in the system
//! (§6.6) — one fence, one parser, one format-preserving path. A frontmatter crate
//! would buy delimiter configuration the spec has already spent (`+++` is fixed, YAML
//! is a non-goal, §18) at the price of a second, non-preserving TOML parser.
//!
//! **The body is opaque.** It is never deserialized, and a fold never reads it — a
//! rule the spec states four separate times (§6.1, §7.1, §7.2, §8.7). That is what
//! [`read_frontmatter`] exists for: it stops at the closing fence, so `list` walks a
//! subtree of documents without their prose ever entering memory.
//!
//! **A document need not carry frontmatter at all.** Tabella handles *every* loose
//! `[code]_*.md` in place (§8.7), hand-written ones included, so a file with no fence
//! reads as an empty envelope over a whole-file body rather than as an error.

use std::io::{BufRead, BufReader};
use std::path::Path;

use toml_edit::{Array, DocumentMut};

use crate::envelope::Frontmatter;
use crate::{Error, Result};

/// The frontmatter fence (§6.6). Fixed — there is no delimiter setting (§18).
pub const FENCE: &str = "+++";

/// A whole document: the frontmatter envelope and the prose beneath it (§6.1).
///
/// `home` and `slug` are the file's location and name and are not here (I3), and
/// there is no `refs` field — a document holds no outbound references (§7.3).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Document {
    /// `type` and `tags` — all a fold reads (§8.7).
    pub frontmatter: Frontmatter,
    /// The fence block's TOML exactly as it was on disk, or `None` where the file
    /// carries no fence. **Load-bearing**: a rewrite re-parses this rather than
    /// re-serializing [`Frontmatter`], so a hand's comments, its key ordering, and
    /// any key Tabella does not read (`author = "…"`) survive an edit (§6.6, I6, I8).
    pub front_raw: Option<String>,
    /// The prose below the closing fence, byte-for-byte. Never parsed.
    pub body: String,
    /// The line ending the file already uses, so a rewrite does not convert it.
    pub crlf: bool,
}

impl Document {
    /// Render back to file text, editing the original TOML in place (§6.6).
    ///
    /// The fence lines and the blank line beneath them follow the file's own ending;
    /// the TOML block is `toml_edit`'s own output, which preserves whatever the hand
    /// wrote in the parts an edit did not touch.
    pub fn to_text(&self) -> Result<String> {
        let nl = if self.crlf { "\r\n" } else { "\n" };
        let mut doc: DocumentMut = match &self.front_raw {
            Some(src) => src
                .parse()
                .map_err(|e| Error::validation(format!("frontmatter: {e}")))?,
            None => DocumentMut::new(),
        };
        match &self.frontmatter.r#type {
            Some(t) => doc["type"] = toml_edit::value(t.as_str()),
            None => {
                doc.remove("type");
            }
        }
        let mut tags = Array::new();
        for tag in &self.frontmatter.tags {
            tags.push(tag.as_str());
        }
        doc["tags"] = toml_edit::value(tags);
        let mut toml = doc.to_string();
        if !toml.ends_with('\n') {
            toml.push('\n');
        }
        Ok(format!("{FENCE}{nl}{toml}{FENCE}{nl}{nl}{}", self.body))
    }
}

/// A fence line is `+++` alone, at column zero; trailing whitespace is tolerated.
fn is_fence(line: &str) -> bool {
    line.trim_end() == FENCE
}

/// Split off the next line, returning it without its terminator and the rest.
/// CRLF and LF both terminate; a final line needs no terminator.
fn next_line(text: &str) -> (&str, &str) {
    match text.find('\n') {
        Some(i) => (
            text[..i].strip_suffix('\r').unwrap_or(&text[..i]),
            &text[i + 1..],
        ),
        None => (text, ""),
    }
}

/// Drop exactly one newline, so the blank line [`render`] writes after the closing
/// fence is not mistaken for the body's own first line.
fn strip_one_newline(text: &str) -> &str {
    text.strip_prefix("\r\n")
        .or_else(|| text.strip_prefix('\n'))
        .unwrap_or(text)
}

/// Split raw document text into its frontmatter TOML source and its body.
///
/// `None` for the TOML means the file carries no fence at all — legal, and the whole
/// text is body (§8.7). An opening fence with no closing one is malformed and is a
/// validation failure (exit `3`).
pub fn split(text: &str) -> Result<(Option<&str>, &str)> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let (first, mut rest) = next_line(text);
    if !is_fence(first) {
        return Ok((None, text));
    }
    let toml = rest;
    let mut len = 0usize;
    loop {
        if rest.is_empty() {
            return Err(Error::validation(
                "unterminated `+++` frontmatter fence (§6.6)",
            ));
        }
        let (line, after) = next_line(rest);
        if is_fence(line) {
            return Ok((Some(&toml[..len]), strip_one_newline(after)));
        }
        len += rest.len() - after.len();
        rest = after;
    }
}

/// Parse whole document text. Reads the body, so this is `get`'s path, never a fold's.
pub fn parse(text: &str) -> Result<Document> {
    let (toml, body) = split(text)?;
    Ok(Document {
        frontmatter: match toml {
            Some(src) => from_toml(src)?,
            None => Frontmatter::default(),
        },
        front_raw: toml.map(ToOwned::to_owned),
        body: body.to_string(),
        crlf: text
            .split_once('\n')
            .is_some_and(|(l, _)| l.ends_with('\r')),
    })
}

/// Read a document's frontmatter and **stop at the closing fence** — the fold path
/// (§7.2: "a fold never reads bodies"). A file with no fence yields the empty
/// envelope after one line has been read.
pub fn read_frontmatter(path: &Path) -> Result<Frontmatter> {
    let mut reader = BufReader::new(std::fs::File::open(path)?);
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Ok(Frontmatter::default());
    }
    if !is_fence(line.strip_prefix('\u{feff}').unwrap_or(&line)) {
        return Ok(Frontmatter::default());
    }
    let mut toml = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Err(Error::validation(format!(
                "unterminated `+++` frontmatter fence in {} (§6.6)",
                path.display()
            )));
        }
        if is_fence(&line) {
            return from_toml(&toml);
        }
        toml.push_str(&line);
    }
}

fn from_toml(src: &str) -> Result<Frontmatter> {
    let doc: DocumentMut = src
        .parse()
        .map_err(|e| Error::validation(format!("frontmatter: {e}")))?;
    Ok(Frontmatter {
        r#type: crate::meta::get_str(&doc, "type"),
        tags: crate::meta::get_str_array(&doc, "tags"),
    })
}
