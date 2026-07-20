//! Applying a proposal (§9.5 step 5): build the core call and spawn it.
//!
//! Auspex writes nothing itself — it links no core (I5). It applies a proposal by
//! **spawning the same core CLI a hand would type**, exactly as Porticus relays a
//! human's write, with two additions the grant authorizes: `-y`, because Auspex's
//! confirm is the capability check granted when the rule was authored, not a prompt no
//! hook could answer; and `PANTHEON_NO_HOOKS=1`, so the write does not wake Auspex
//! again (§9.4, §9.5).
//!
//! ## The wall a proposal can hit (I5)
//!
//! A proposal may carry a `data` object (§9.3), but **no core's CLI can accept an
//! arbitrary record** — every `add` builds its record from typed positionals and flags,
//! and Auspex cannot know a core's shape without linking it. So a `data`-bearing
//! proposal is *refused here rather than mis-stored*: Auspex applies only what a hand
//! could have typed — a name, a key, a series, refs, a date. That is a real boundary
//! between §9.3's proposal format and what the cores accept today, and refusing loudly
//! is the honest side of it.

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
/// `core`/`home`. A `data` object, or a field a verb cannot express, is refused rather
/// than guessed.
fn argv(p: &Proposal, now: &str) -> Result<Vec<String>, String> {
    if p.data.is_some() {
        return Err(format!(
            "proposes {} carrying `data`, which no core's CLI can accept — a rule \
             proposes only what a hand could type (§9.3, I5)",
            p.label()
        ));
    }

    let verb = canonical_verb(&p.verb);
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
        // `edit` sets a new value, which is `data` no CLI can carry; `move` needs the
        // record's *current* home, which the proposal (whose home is the destination,
        // §9.3) does not give. Both wait — refused loudly rather than applied wrong.
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
