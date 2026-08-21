//! Addressing (§5.1): the [`Code`] parser and node-directory-name reader.
//!
//! A node directory is named `[parent code]_[defining char]_[definition]` (the
//! *triple* form) or `[parent code]_[definition]_` with a trailing `_` (the
//! *definition-prefix* form). A full code is the parent's code plus this node's
//! defining char; any code reconstructs its path and vice versa. A code carries
//! single `_` (definition-prefix) but never `__` (the file-field separator, §5.2)
//! and never a trailing `_` (that is a directory device, not part of the code).

use unicode_normalization::char::is_combining_mark;

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
        // A combining mark is part of the letter before it, not a character of its own: on
        // a decomposed filesystem `ö` arrives as `o` + U+0308, and rejecting the mark
        // rejects the whole code. The tree holds both spellings — a `pan new` mint writes
        // NFC, macOS writes NFD (D8) — so the parser has to read either.
        if let Some(bad) = s
            .chars()
            .find(|c| !(c.is_alphanumeric() || *c == '_' || is_combining_mark(*c)))
        {
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

    /// Left-to-right scan of a *compact* code (§5.1): **every token is exactly one
    /// character**, a letter or a digit, so the scan is a plain `chars()` walk with
    /// nothing to look ahead at. Errors on a definition-prefix code (its internal `_`
    /// cannot be tokenized from the string alone — it is resolved by a walk), on an
    /// opening digit, or on a character no char slot may hold.
    pub fn tokenize_compact(&self) -> Result<Vec<char>> {
        if !self.is_compact() {
            return Err(Error::usage(format!(
                "code {:?} is definition-prefix; it cannot be tokenized from the string alone (§5.1)",
                self.0
            )));
        }
        let mut tokens = Vec::new();
        for (i, c) in self.0.chars().enumerate() {
            if c.is_ascii_digit() {
                if i == 0 {
                    return Err(Error::usage(format!(
                        "code {:?} opens with a digit (§5.1)",
                        self.0
                    )));
                }
            } else if !c.is_alphabetic() {
                return Err(Error::usage(format!(
                    "code {:?} has an illegal character {c:?}",
                    self.0
                )));
            }
            tokens.push(c);
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
        Some(Code(tokens[..tokens.len() - 1].iter().collect()))
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
    pub ch: Option<char>,
    pub label: String,
}

/// Parse a node directory name, given its parent's code (`None` at the root, where
/// a node has no parent prefix). A trailing `_` marks the definition-prefix form;
/// otherwise the first character is the defining char and the rest is the label
/// (§5.1). This is the single point that tells the two forms apart at read time.
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
    code_str.push(ch);
    Ok(NodeName {
        code: Code(code_str),
        form: CodeForm::Triple,
        ch: Some(ch),
        label,
    })
}

/// Split a triple remainder `[char]_[label]` into its defining char and label. The
/// char is **one** character — a letter or a digit (§5.1) — and the label keeps its
/// internal `_` whole.
fn split_triple(rem: &str, dirname: &str) -> Result<(char, String)> {
    let mut it = rem.char_indices();
    let (_, first) = it.next().expect("non-empty remainder");
    if first.is_alphabetic() || first.is_ascii_digit() {
        if let Some((sep_idx, sep)) = it.next() {
            if sep == '_' {
                let label = rem[sep_idx + 1..].to_string();
                if label.is_empty() {
                    return Err(Error::validation(format!(
                        "directory {dirname:?} has an empty label"
                    )));
                }
                return Ok((first, label));
            }
        }
    }
    Err(Error::validation(format!(
        "directory {dirname:?} is malformed: expected `[char]_[label]` with a one-character char (§5.1)"
    )))
}
