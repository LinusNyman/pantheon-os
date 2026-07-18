//! Addressing (§5.1): the [`Code`] parser and node-directory-name reader.
//!
//! A node directory is named `[parent code]_[defining char]_[definition]` (the
//! *triple* form) or `[parent code]_[definition]_` with a trailing `_` (the
//! *definition-prefix* form). A full code is the parent's code plus this node's
//! defining char; any code reconstructs its path and vice versa. A code carries
//! single `_` (definition-prefix) but never `__` (the file-field separator, §5.2)
//! and never a trailing `_` (that is a directory device, not part of the code).

use crate::{Error, Result};

/// A validated full code — e.g. `csa` (triple) or `csa_john_appleseed`
/// (definition-prefix). Never carries `__`, a leading/trailing `_`, or an opening
/// digit.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Code(String);

/// The two ways a node names itself (§5.1).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CodeForm {
    /// `[parent]_[char]_[label]` — a compact, all-token code (no internal `_`).
    Triple,
    /// `[parent]_[definition]_` — the definition doubles as the label; no char slot.
    DefinitionPrefix,
}

impl CodeForm {
    /// The contract JSON tag for this form.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            CodeForm::Triple => "triple",
            CodeForm::DefinitionPrefix => "definition_prefix",
        }
    }
}

/// One level's defining char (§5.1): a single alphabetic character, or a
/// fixed-width two-digit numeric (`01`..`99`). Stored as its string form so a
/// leading zero survives text sort and round-trip (`01` ≠ `1`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum CharToken {
    /// A single Unicode-alphabetic character.
    Alpha(char),
    /// Exactly two ASCII digits.
    Numeric(String),
}

impl CharToken {
    /// The token's on-disk / in-code string: `"a"` or `"01"`.
    #[must_use]
    pub fn as_code_str(&self) -> String {
        match self {
            CharToken::Alpha(c) => c.to_string(),
            CharToken::Numeric(n) => n.clone(),
        }
    }

    fn push_to(&self, out: &mut String) {
        match self {
            CharToken::Alpha(c) => out.push(*c),
            CharToken::Numeric(n) => out.push_str(n),
        }
    }
}

impl Code {
    /// Parse and validate a code's *syntax* only — no disk access (§5.1). A
    /// definition-prefix code (internal `_`) is accepted here but does not
    /// tokenize; it is matched level-by-level against the tree (§5.0).
    pub fn parse(s: &str) -> Result<Code> {
        if s.is_empty() {
            return Err(Error::usage("empty code"));
        }
        if s.contains("__") {
            return Err(Error::usage(format!(
                "code {s:?} contains '__' (the reserved file-field separator, §5.2)"
            )));
        }
        if s.starts_with('_') || s.ends_with('_') {
            return Err(Error::usage(format!(
                "code {s:?} has a leading or trailing '_'"
            )));
        }
        let first = s.chars().next().expect("non-empty");
        if !first.is_alphabetic() {
            return Err(Error::usage(format!(
                "code {s:?} opens with {first:?}; a code opens with a letter, never a digit (§5.1)"
            )));
        }
        if let Some(bad) = s.chars().find(|c| !(c.is_alphanumeric() || *c == '_')) {
            return Err(Error::usage(format!(
                "code {s:?} has an illegal character {bad:?}"
            )));
        }
        Ok(Code(s.to_string()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// A compact (all-token) code has no internal `_`; a definition-prefix code does.
    #[must_use]
    pub fn is_compact(&self) -> bool {
        !self.0.contains('_')
    }

    #[must_use]
    pub fn form(&self) -> CodeForm {
        if self.is_compact() {
            CodeForm::Triple
        } else {
            CodeForm::DefinitionPrefix
        }
    }

    /// Left-to-right scan of a *compact* code (§5.1): a letter is a one-char token;
    /// a digit begins a two-digit token. Errors on a definition-prefix code (its
    /// internal `_` cannot be tokenized from the string alone — it is resolved by a
    /// walk), on an opening digit, or on a lone/short digit run.
    pub fn tokenize_compact(&self) -> Result<Vec<CharToken>> {
        if !self.is_compact() {
            return Err(Error::usage(format!(
                "code {:?} is definition-prefix; it cannot be tokenized from the string alone (§5.1)",
                self.0
            )));
        }
        let mut tokens = Vec::new();
        let mut chars = self.0.chars();
        let mut first = true;
        while let Some(c) = chars.next() {
            if c.is_ascii_digit() {
                if first {
                    return Err(Error::usage(format!(
                        "code {:?} opens with a digit (§5.1)",
                        self.0
                    )));
                }
                let d2 = chars.next().ok_or_else(|| {
                    Error::usage(format!(
                        "code {:?} ends with a lone digit; a numeric level is two digits (§5.1)",
                        self.0
                    ))
                })?;
                if !d2.is_ascii_digit() {
                    return Err(Error::usage(format!(
                        "code {:?} has a single digit; a numeric level is two digits (§5.1)",
                        self.0
                    )));
                }
                tokens.push(CharToken::Numeric(format!("{c}{d2}")));
            } else if c.is_alphabetic() {
                tokens.push(CharToken::Alpha(c));
            } else {
                return Err(Error::usage(format!(
                    "code {:?} has an illegal character {c:?}",
                    self.0
                )));
            }
            first = false;
        }
        Ok(tokens)
    }

    /// The parent code by string alone — drop the last token of a compact code.
    /// `None` for a single-token (root) code, or for a definition-prefix code
    /// (whose parent is found by the walk, not the string).
    #[must_use]
    pub fn parent_compact(&self) -> Option<Code> {
        if !self.is_compact() {
            return None;
        }
        let tokens = self.tokenize_compact().ok()?;
        if tokens.len() <= 1 {
            return None;
        }
        let mut s = String::new();
        for t in &tokens[..tokens.len() - 1] {
            t.push_to(&mut s);
        }
        Some(Code(s))
    }
}

impl std::fmt::Display for Code {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A node's identity read off its directory name (§5.1).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NodeName {
    pub code: Code,
    pub form: CodeForm,
    /// The defining char — `None` for a definition-prefix node.
    pub ch: Option<CharToken>,
    pub label: String,
}

/// Parse a node directory name, given its parent's code (`None` at the root, where
/// a node has no parent prefix). A trailing `_` marks the definition-prefix form;
/// otherwise the first token (one letter or two digits) is the defining char and
/// the rest is the label (§5.1). This is the single point that tells the two forms
/// apart at read time.
///
/// A definition-prefix node's children may themselves only be definition-prefix
/// (§5.1); a triple child under a definition-prefix parent is a naming mistake and
/// is reported as a validation failure.
pub fn parse_node_dirname(parent: Option<&Code>, dirname: &str) -> Result<NodeName> {
    // Strip the parent prefix (`parent_`) to get this node's own remainder.
    let rem = match parent {
        Some(p) => {
            let pfx = format!("{}_", p.as_str());
            dirname.strip_prefix(&pfx).ok_or_else(|| {
                Error::validation(format!(
                    "directory {dirname:?} is not a child of {} (missing prefix {pfx:?})",
                    p.as_str()
                ))
            })?
        }
        None => dirname,
    };
    if rem.is_empty() {
        return Err(Error::validation(format!(
            "directory {dirname:?} names an empty node"
        )));
    }

    let parent_is_def_prefix = parent.is_some_and(|p| !p.is_compact());

    // Definition-prefix: a single trailing `_`. (A meta dir ends in `__` and is
    // classified before this is ever called, so a lone trailing `_` is unambiguous.)
    if let Some(def) = rem.strip_suffix('_') {
        if def.is_empty() {
            return Err(Error::validation(format!(
                "directory {dirname:?} has an empty definition"
            )));
        }
        let code = match parent {
            Some(p) => Code(format!("{}_{def}", p.as_str())),
            None => Code(def.to_string()),
        };
        return Ok(NodeName {
            code,
            form: CodeForm::DefinitionPrefix,
            ch: None,
            label: def.to_string(),
        });
    }

    // Otherwise it is a triple. A triple child under a definition-prefix parent is
    // illegal (a one-char triple char can't concatenate onto a `_`-bearing code, §5.1).
    if parent_is_def_prefix {
        return Err(Error::validation(format!(
            "directory {dirname:?} is a triple child under a definition-prefix node; children of a \
             definition-prefix node must themselves be definition-prefix (§5.1)"
        )));
    }

    let (ch, label) = split_triple(rem, dirname)?;
    let mut code_str = parent.map(|p| p.as_str().to_string()).unwrap_or_default();
    ch.push_to(&mut code_str);
    Ok(NodeName {
        code: Code(code_str),
        form: CodeForm::Triple,
        ch: Some(ch),
        label,
    })
}

/// Split a triple remainder `[char]_[label]` into its defining char and label. The
/// char is two ASCII digits or one alphabetic character; the label keeps its
/// internal `_` whole.
fn split_triple(rem: &str, dirname: &str) -> Result<(CharToken, String)> {
    let bytes = rem.as_bytes();
    // Numeric char: two ASCII digits then `_`.
    if bytes.len() >= 3
        && bytes[0].is_ascii_digit()
        && bytes[1].is_ascii_digit()
        && bytes[2] == b'_'
    {
        let ch = CharToken::Numeric(rem[..2].to_string());
        let label = rem[3..].to_string();
        if label.is_empty() {
            return Err(Error::validation(format!(
                "directory {dirname:?} has an empty label"
            )));
        }
        return Ok((ch, label));
    }
    // Alpha char: one letter then `_`.
    let mut it = rem.char_indices();
    let (_, first) = it.next().expect("non-empty remainder");
    if first.is_alphabetic() {
        if let Some((sep_idx, sep)) = it.next() {
            if sep == '_' {
                let label = rem[sep_idx + 1..].to_string();
                if label.is_empty() {
                    return Err(Error::validation(format!(
                        "directory {dirname:?} has an empty label"
                    )));
                }
                return Ok((CharToken::Alpha(first), label));
            }
        }
    }
    Err(Error::validation(format!(
        "directory {dirname:?} is malformed: expected `[char]_[label]` with a one-letter or two-digit char (§5.1)"
    )))
}
