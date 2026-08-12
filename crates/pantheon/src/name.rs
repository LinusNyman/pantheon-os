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

/// The spelling a name is written to disk in: **NFD** (D8).
///
/// [`normalize`] answers in NFC because a token is also a *key* — a slug in a record, a
/// definition in a `core:slug` ref — and those are compared, sorted and shipped between
/// machines. A **filename** is a different object with a different authority over it:
/// Syncthing on macOS treats NFD as the canonical on-disk form, and given a composed name
/// it either renames it back (`autoNormalize=true`, which silently reverted Phase 1 twice)
/// or refuses to index it at all (`autoNormalize=false`, which dropped 1,367 files out of
/// the backup). So the token stays composed, the path it becomes is decomposed, and this
/// function is the boundary between them.
///
/// Unconditional rather than `cfg(target_os = "macos")`: one tree is read from more than
/// one host, and a mint whose output depends on who ran it is a mint that cannot be tested.
#[must_use]
pub fn fs_spelling(s: &str) -> String {
    s.nfd().collect()
}

/// Whether `s` is already in normal form, **compared under NFC** (D8) — a cheap check for
/// resolve/validate that avoids allocating a fix when the token is already legal.
///
/// Unicode normalization is the one difference this does not count. `övning` composed and
/// decomposed are the same token: APFS is normalization-insensitive and opens the same
/// directory for either, so a distinction the filesystem does not make is not `pan`'s to
/// enforce. Everything else §5.1 folds — case, punctuation, spacing, runs of `_` — is still
/// a violation and still reported.
///
/// Comparing bytes here is what raised ~60 `non_normalized_name` findings against a tree
/// that is deliberately NFD, each one carrying a suggested fix that recomposed the name and
/// so dropped that file out of the backup. The tree's spelling is not a defect.
#[must_use]
pub fn is_normalized(s: &str) -> bool {
    normalize(s).is_some_and(|n| n.as_str().nfc().eq(s.nfc()))
}
