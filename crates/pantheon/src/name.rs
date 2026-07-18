//! Name normalization (§5.1) — one rule for every typed token: code char, label,
//! slug, kind, series/rule name, filename. Deterministic, total, and idempotent,
//! which is what lets `pan` auto-apply it as a fix (§10.2).

use unicode_normalization::UnicodeNormalization;

/// Normalize a token to its single legal form (§5.1): lowercase; NFC; keep Unicode
/// alphabetic-or-numeric characters and `_`; fold space and `-` to `_`; drop every
/// other punctuation or symbol; collapse runs of `_` to one; strip leading and
/// trailing `_`. Returns `None` iff the token normalizes to empty.
///
/// The pipeline order — NFC → lowercase → NFC → filter → collapse → strip — is
/// load-bearing: `to_lowercase` can de/recompose in some scripts, so NFC brackets
/// it on both sides to keep the result idempotent
/// (`normalize(normalize(x)) == normalize(x)`). NFC is not optional: macOS and
/// Linux disagree on decomposed vs composed bytes (§5.1).
#[must_use]
pub fn normalize(input: &str) -> Option<String> {
    let lowered: String = input.nfc().flat_map(char::to_lowercase).collect();

    let folded: String = lowered
        .as_str()
        .nfc()
        .filter_map(|c| {
            if c.is_alphanumeric() || c == '_' {
                Some(c)
            } else if c == ' ' || c == '-' {
                Some('_')
            } else {
                None
            }
        })
        .collect();

    // Collapse runs of `_` and strip leading/trailing in one pass.
    let mut out = String::with_capacity(folded.len());
    let mut prev_underscore = true; // start true so a leading `_` is dropped
    for c in folded.chars() {
        if c == '_' {
            if !prev_underscore {
                out.push('_');
            }
            prev_underscore = true;
        } else {
            out.push(c);
            prev_underscore = false;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }

    if out.is_empty() { None } else { Some(out) }
}

/// [`normalize`] or a usage error (exit `2`) naming the token — for CLI argument
/// validation, where an empty result is the caller's fault.
pub fn normalize_token(input: &str, what: &str) -> crate::Result<String> {
    normalize(input)
        .ok_or_else(|| crate::Error::usage(format!("{what} normalizes to empty: {input:?}")))
}

/// Whether `s` is already in normal form — a cheap check for resolve/validate that
/// avoids allocating a fix when the token is already legal.
#[must_use]
pub fn is_normalized(s: &str) -> bool {
    normalize(s).as_deref() == Some(s)
}
