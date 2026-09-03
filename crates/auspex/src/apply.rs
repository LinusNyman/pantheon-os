//! Applying a proposal (§9.5 step 5): build the core call and spawn it.
//!
//! Auspex writes nothing itself — it links no core (I5). It applies a proposal by
//! **spawning the same core CLI a hand would type**, exactly as Porticus relays a
//! human's write, with two additions the grant authorizes: `-y`, because Auspex's
//! confirm is the capability check granted when the rule was authored, not a prompt no
//! hook could answer; and `PANTHEON_NO_HOOKS=1`, so the write does not wake Auspex
//! again (§9.4, §9.5).
//!
//! ## The wall a proposal used to hit, and what took its place (I5)
//!
//! §9.3 has always shown proposals carrying a `data` object, and Auspex refused every
//! one of them — not because a rule may not propose one, but because **no core's CLI
//! could accept a whole record**: every `add` built from typed positionals and flags,
//! and Auspex cannot know a core's shape without linking it. Refusing loudly was the
//! honest side of a real gap between the proposal format and the cores.
//!
//! The gap is closed by the cores rather than here: `add --data` ingests a record
//! validated against the core's own schema (§7.3), so `data` now rides through as that
//! flag. **Auspex still types only what a hand could type** — the rule it always kept.
//! Two things follow and neither is incidental: the flag is `add`'s alone, so `data` on
//! any other verb is still refused rather than dropped; and the grant is untouched,
//! because `writes=` governs *where* a rule may write while `data` says only what is in
//! the record (§9.5).

use std::path::Path;
use std::process::{Command, Stdio};

use crate::grant::{Proposal, canonical_verb};

/// Apply one authorized proposal by spawning its core.
///
/// `short` is the core's binary (`pen`), resolved from the proposal's core name.
/// `root` is the tree `aus` was given — passed as `-C` so the core writes the same
/// tree Auspex read, never the ambient one (the 2b `PANTHEON_ROOT` lesson, one layer
/// on). `now` keys a date-keyed add (§9.3).
pub(crate) fn apply(
    proposal: &Proposal,
    short: &str,
    root: &Path,
    now: &str,
) -> Result<(), String> {
    let args = argv(proposal, now)?;
    let out = Command::new(short)
        .arg("-C")
        .arg(root)
        .args(&args)
        .arg("-y")
        .env("PANTHEON_NO_HOOKS", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("could not run {short}: {e}"))?;

    if out.status.success() {
        Ok(())
    } else {
        // The core refused an authorized, deduped proposal — a bad home, an
        // unresolvable ref, a shape it will not take. Its own `{"error":{…}}` on
        // stderr says which; quote the message, drop the envelope.
        Err(format!(
            "{short} refused {}: {}",
            proposal.label(),
            core_error(&out.stderr)
        ))
    }
}

/// A proposal into the core CLI argv a hand would type (§9.3), or a refusal.
///
/// `verb`, `-H home`, then the universal fields the record takes — the same set
/// Porticus builds a relay from, and the same set §9.3 says a proposal carries beyond
/// `core`/`home`. A field a verb cannot express is refused rather than guessed.
///
/// **`data` rides through as `add --data`** (§9.3). It was refused outright for as long
/// as no core's CLI could accept a whole record — §9.3 showed proposals carrying `data`
/// and the cores could take none of them, which is the gap the ingest flag closed
/// (§7.3). Auspex still types only what a hand could: the flag is the core's own, the
/// core validates the record against its own schema, and the grant governs *where* the
/// write lands exactly as before — `data` says what is in the record, never which node
/// it reaches (§9.5).
fn argv(p: &Proposal, now: &str) -> Result<Vec<String>, String> {
    let verb = canonical_verb(&p.verb);
    if p.data.is_some() && verb != "add" {
        return Err(format!(
            "{}: `data` is the record's content, which only `add` writes — `{verb}` \
             carries none (§9.3, §7.3)",
            p.label()
        ));
    }
    let mut args = vec![verb.to_string(), "-H".to_string(), p.home.clone()];

    match verb {
        "add" => match (&p.name, &p.key) {
            // A fresh add of a named record carries a name, which Auspex hands the core
            // to normalize into the key (§9.3) — it can no more type a slug than a hand.
            (Some(name), None) => args.push(name.clone()),
            // A date-keyed add carries neither and keys by `now` (§9.3).
            (None, None) => {
                args.push("--at".to_string());
                args.push(now.to_string());
            }
            _ => {
                return Err(format!(
                    "{}: an add carries a name or nothing, never a key (§9.3)",
                    p.label()
                ));
            }
        },
        "rm" => args.push(key_of(p, "rm drops")?),
        "rename" => {
            // A rename names both: the key it targets and the name it becomes (§9.3).
            args.push(key_of(p, "rename targets")?);
            let name = p.name.clone().ok_or_else(|| {
                format!("{}: a rename names the name it becomes (§9.3)", p.label())
            })?;
            args.push(name);
        }
        // `edit` sets a new value, and the ingest flag is `add`'s alone — an `edit`
        // would need per-field merge semantics, which is a different design (§7.2).
        // `move` needs the record's *current* home, which the proposal (whose home is
        // the destination, §9.3) does not give. Both wait — refused loudly rather than
        // applied wrong.
        other => {
            return Err(format!(
                "{}: `aus run` does not apply `{other}` yet — a rule proposing it is ahead of the engine (§9.5)",
                p.label()
            ));
        }
    }

    for reference in &p.refs {
        args.push("-r".to_string());
        args.push(reference.clone());
    }
    if let Some(series) = &p.series {
        args.push("--series".to_string());
        args.push(series.clone());
    }
    // Last, so the record's own content follows everything that says where it goes.
    // Serialized here rather than passed through verbatim: the proposal was parsed, so
    // this is the object Auspex actually read, not whatever bytes surrounded it.
    if let Some(data) = &p.data {
        args.push("--data".to_string());
        args.push(
            serde_json::to_string(data)
                .map_err(|e| format!("{}: its `data` will not serialize: {e} (§9.3)", p.label()))?,
        );
    }
    Ok(args)
}

fn key_of(p: &Proposal, verb: &str) -> Result<String, String> {
    p.key.clone().ok_or_else(|| {
        format!(
            "{}: {verb} the key it names, and this proposal gives none (§9.3)",
            p.label()
        )
    })
}

/// The `msg` out of a core's `{"error":{"code":…,"msg":…}}`, or the raw stderr if it
/// is not that shape.
fn core_error(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    serde_json::from_str::<serde_json::Value>(text.trim())
        .ok()
        .and_then(|v| v["error"]["msg"].as_str().map(str::to_string))
        .unwrap_or_else(|| text.trim().to_string())
}
