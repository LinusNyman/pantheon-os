//! The capability check (§9.2, §9.5 step 2) — **the whole guard.**
//!
//! Nothing marks a record as Auspex's: a task a rule wrote and a task you typed are
//! the same task (§9.5). So the only thing bounding a rule you got wrong is its grant,
//! and this module is where a proposal meets it. Read it in isolation; that is why it
//! is its own file.
//!
//! A grant is **default-deny**: a rule declaring no `writes=` may propose, but nothing
//! it proposes lands. A capability names one core, one node, an optional series, and
//! the verbs allowed there — `core@home[/series]:verbs` — and **a failed check rejects
//! the whole rule's batch**, not the one proposal (§9.5): a rule that got one write
//! wrong is not to be trusted with the rest.

use serde::Deserialize;
use serde_json::Value;

/// One proposed core call (§9.3). The fields a grant is checked against are the first
/// four; the rest ride along to [`crate::apply`].
///
/// `deny_unknown_fields` is deliberately **absent**: a core's `data` is the owning
/// core's to validate (§6.4, I5), and a proposal may carry fields a future core adds.
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Proposal {
    pub core: String,
    pub verb: String,
    pub home: String,
    #[serde(default)]
    pub series: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub refs: Vec<String>,
    #[serde(default)]
    pub data: Option<Value>,
}

impl Proposal {
    /// A one-line label for a report — what this proposal would touch.
    pub fn label(&self) -> String {
        let mut at = format!("{}@{}", self.core, self.home);
        if let Some(series) = &self.series {
            at.push('/');
            at.push_str(series);
        }
        format!("{}:{}", at, self.verb)
    }
}

/// One capability: `core@home[/series]:verbs`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Capability {
    pub core: String,
    pub home: String,
    /// A named series authorizes **minting** it (§18's one exception); a bare grant
    /// may only append (§9.2).
    pub series: Option<String>,
    pub verbs: Vec<String>,
}

impl Capability {
    /// Parse one `writes=` entry, failing closed.
    ///
    /// A grant Auspex cannot read is not a grant it may act on: an unparseable entry is
    /// an error that rejects the rule, never a silently-empty one that would read as
    /// default-deny and hide the typo.
    fn parse(entry: &str) -> Result<Self, String> {
        let (loc, verbs) = entry.split_once(':').ok_or_else(|| {
            format!("{entry:?} names no verbs — a grant is `core@home[/series]:verbs` (§9.2)")
        })?;
        let (core, home_series) = loc.split_once('@').ok_or_else(|| {
            format!("{entry:?} has no `@` — a grant is `core@home[/series]:verbs` (§9.2)")
        })?;
        let (home, series) = match home_series.split_once('/') {
            Some((home, series)) => (home, Some(series.to_string())),
            None => (home_series, None),
        };
        let verbs: Vec<String> = verbs
            .split(',')
            .map(|v| canonical_verb(v.trim()).to_string())
            .filter(|v| !v.is_empty())
            .collect();
        if core.is_empty() || home.is_empty() || verbs.is_empty() {
            return Err(format!(
                "{entry:?} is not a `core@home[/series]:verbs` grant (§9.2)"
            ));
        }
        Ok(Self {
            core: core.to_string(),
            home: home.to_string(),
            series,
            verbs,
        })
    }

    /// Whether this capability authorizes `proposal`. Core, home, and verb must match;
    /// the series slot, if the grant names one, must match too.
    fn authorizes(&self, proposal: &Proposal) -> bool {
        if self.core != proposal.core || self.home != proposal.home {
            return false;
        }
        if !self
            .verbs
            .iter()
            .any(|v| v == canonical_verb(&proposal.verb))
        {
            return false;
        }
        // A grant naming a series binds the proposal to that series — `annales@csa/x`
        // authorizes writing `x` and no other collection at that node. A bare grant
        // does not constrain the series.
        //
        // §9.2's other half of the series slot — that it *licenses minting* (the one
        // write allowed to bring its own container) — is not wired here yet, because
        // minting a series means writing a reading, and a reading carries `data` that
        // no core CLI can accept (I5, [`crate::apply`]); every minting proposal is
        // therefore refused before it reaches a grant check. The slot binds today; it
        // will license `-c` when a dataless mint or a core `--data` path exists.
        match &self.series {
            Some(granted) => proposal.series.as_deref() == Some(granted.as_str()),
            None => true,
        }
    }
}

/// A rule's whole grant: the capabilities from its `writes=` header (§9.2).
pub(crate) struct Grant {
    caps: Vec<Capability>,
}

impl Grant {
    /// Parse a rule's `writes=` entries, failing closed on any one that will not read.
    pub fn parse(writes: &[String]) -> Result<Self, String> {
        let caps = writes
            .iter()
            .map(|w| Capability::parse(w))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { caps })
    }

    /// The capability authorizing `proposal`, or a rejection naming what was refused.
    ///
    /// Default-deny is the empty-grant case: with no capabilities, every proposal is
    /// refused, which is a read-only rule proposing into the void (§9.2).
    pub fn authorize<'a>(&'a self, proposal: &Proposal) -> Result<&'a Capability, String> {
        self.caps
            .iter()
            .find(|cap| cap.authorizes(proposal))
            .ok_or_else(|| {
                format!(
                    "proposes {}, which its grant does not allow (§9.2)",
                    proposal.label()
                )
            })
    }
}

/// Fold a verb's aliases to the canonical form so a grant and a proposal agree (§7.3).
pub(crate) fn canonical_verb(verb: &str) -> &str {
    match verb {
        "mv" => "move",
        "ls" => "list",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn proposal(v: Value) -> Proposal {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn a_bare_grant_authorizes_its_verb_at_its_node() {
        let grant = Grant::parse(&["pensum@acm:add".to_string()]).unwrap();
        assert!(
            grant
                .authorize(&proposal(
                    json!({"core":"pensum","verb":"add","home":"acm","name":"x"})
                ))
                .is_ok()
        );
        // Wrong node, wrong core, wrong verb — each refused.
        assert!(
            grant
                .authorize(&proposal(
                    json!({"core":"pensum","verb":"add","home":"ecv","name":"x"})
                ))
                .is_err()
        );
        assert!(
            grant
                .authorize(&proposal(
                    json!({"core":"annales","verb":"add","home":"acm"})
                ))
                .is_err()
        );
        assert!(
            grant
                .authorize(&proposal(
                    json!({"core":"pensum","verb":"rm","home":"acm","key":"x"})
                ))
                .is_err()
        );
    }

    #[test]
    fn an_empty_grant_denies_everything() {
        let grant = Grant::parse(&[]).unwrap();
        assert!(
            grant
                .authorize(&proposal(
                    json!({"core":"pensum","verb":"add","home":"acm"})
                ))
                .is_err()
        );
    }

    #[test]
    fn a_series_grant_binds_the_proposal_to_that_series() {
        let grant = Grant::parse(&["annales@csa/stale_contact:add".to_string()]).unwrap();
        assert!(
            grant
                .authorize(&proposal(
                    json!({"core":"annales","verb":"add","home":"csa","series":"stale_contact"})
                ))
                .is_ok()
        );
        // The same grant does not authorize a *different* series at that node.
        assert!(
            grant
                .authorize(&proposal(
                    json!({"core":"annales","verb":"add","home":"csa","series":"other"})
                ))
                .is_err()
        );
    }

    #[test]
    fn a_bare_grant_does_not_bind_a_series() {
        let grant = Grant::parse(&["annales@csa:add".to_string()]).unwrap();
        assert!(
            grant
                .authorize(&proposal(
                    json!({"core":"annales","verb":"add","home":"csa","series":"log"})
                ))
                .is_ok()
        );
    }

    #[test]
    fn several_capabilities_are_semicolon_separated() {
        let grant = Grant::parse(&[
            "pensum@acm:add".to_string(),
            "annales@csa/stale_contact:add".to_string(),
        ])
        .unwrap();
        assert!(
            grant
                .authorize(&proposal(
                    json!({"core":"pensum","verb":"add","home":"acm"})
                ))
                .is_ok()
        );
        assert!(
            grant
                .authorize(&proposal(
                    json!({"core":"annales","verb":"add","home":"csa","series":"stale_contact"})
                ))
                .is_ok()
        );
    }

    #[test]
    fn a_grant_takes_several_verbs_and_folds_aliases() {
        let grant = Grant::parse(&["pensum@acm:add,rm,mv".to_string()]).unwrap();
        for verb in ["add", "rm", "move"] {
            assert!(
                grant
                    .authorize(&proposal(
                        json!({"core":"pensum","verb":verb,"home":"acm","key":"x"})
                    ))
                    .is_ok(),
                "{verb} is granted"
            );
        }
        // `mv` in the grant authorizes a `move` proposal — one muscle memory (§7.3).
        assert!(
            grant
                .authorize(&proposal(
                    json!({"core":"pensum","verb":"mv","home":"acm","key":"x"})
                ))
                .is_ok()
        );
        assert!(
            grant
                .authorize(&proposal(
                    json!({"core":"pensum","verb":"edit","home":"acm","key":"x"})
                ))
                .is_err()
        );
    }

    #[test]
    fn a_malformed_grant_fails_closed() {
        // No verbs, no `@`, empty — each rejects the rule rather than reading as an
        // empty (default-deny) grant that would hide the mistake.
        for bad in ["pensum@acm", "pensumacm:add", ":add", "pensum@:add"] {
            assert!(
                Grant::parse(&[bad.to_string()]).is_err(),
                "{bad:?} must be rejected, not silently empty"
            );
        }
    }
}
