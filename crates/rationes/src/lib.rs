//! Rationes — res holdings (what) (§8.3). What you hold, stored as a **partitioned
//! register**: one `.json` object per holding, its kind and slug in the filename
//! (§6.1). Referenced everywhere as `rationes:<slug>`.
//!
//! Three filename kinds, all partitioned: `account`, `asset` (a good), `claim` (a
//! right, licence, subscription, or debt owed to you). Homed under Res by what the
//! holding *is* — but **not sphere-locked** (§6.2, I7): where an org is modelled as
//! its own node, *its* account lives at that node, and the tree-walking fold still
//! sums it into net worth wherever it sits. The org tie is a reference
//! (`refs: ["album:<org>"]`), never a home (I3, I9).
//!
//! **The kind decides whether a holding carries a balance series.** `account` and
//! `asset` may have `[code]__balance__<slug>.jsonl` — one keyed line per date, a
//! changing balance or valuation; `claim` has none, because an expiry date is a
//! field and not a thing sampled over time.
//!
//! That series is Rationes' one `Series` token, declared **`Series { named: false }`**
//! — determined, not named (§7.1): its name slot carries its holding's slug, so it is
//! minted by the holding's creation rather than by `-c`, has nothing to mistype
//! (§7.3), is reached *through* the holding rather than referenced on its own (§5.4),
//! and moves with it (§7.2).
//!
//! **Net worth is a fold, never a field** (I1): the latest balance of the kinds that
//! have one, summed at read time by `rat list --net`. Your passport is a `claim` and
//! carries no balance, so it is not part of it.
//!
//! Build order step 7 — the first core to declare **two shapes**, which is what makes
//! its `Record` an enum (§7.1, §16).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use pantheon::{Core, Error, KeyShape, Result, Shape};

/// One holding — the `data` half of an entity file (§6.1).
///
/// Its `refs` ride in the envelope, and its home, core, kind, and slug are the file's
/// location and name (I3), so none of them is stored here. Nor is a balance: a
/// balance is sampled, so it lives in the companion series and the present is derived
/// from it (I1). There is no `current_value` field and there will not be one.
///
/// One flat struct across the three kinds, for Album's reason (§8.1): the three
/// tokens are one shape and one primitive, and `edit -k` is a file rename rather than
/// a record transformation (§7.2). A `claim` simply leaves `currency` absent, and an
/// `account` leaves `expires` absent.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Holding {
    /// The unit its balance is read in — `usd`, `eur`, `shares`. A fold sums *by* it
    /// rather than across it: adding dollars to shares would be a lie (§8.3, I1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    /// When the holding lapses (`YYMMDD`) — a `claim`'s expiry, which §8.3 makes a
    /// **field** precisely because it is one date and not a series of them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires: Option<String>,
    /// A hand's remark on this holding — the editor form's buffer (§7.3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// One balance reading — the `data` half of a series line (§5.4).
///
/// Keyed by date, so it is a **sample**: a correction rewrites the keyed line, it
/// never stacks a second, and the present is the line at the latest key (I1, §6.1).
/// The holding it belongs to is the series file's name slot, so it is not stored here
/// either (I3) — which is what "determined, not named" means on disk (§7.1).
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Balance {
    /// The balance or valuation, in the holding's own currency.
    pub amount: f64,
    /// What a hand typed past the figure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// The two record bodies Rationes files, as one dispatch type (§7.1).
///
/// **A dispatch type, not a disk format.** The filename's token already says which
/// variant a file holds (§5.2), so this is `untagged`: each file stores the bare
/// variant payload and no tag is ever written (§18). `deny_unknown_fields` on both
/// variants is what makes the untagged read *total* rather than merely lucky — a
/// holding cannot be mistaken for a balance in either direction, whichever order
/// serde tries them in.
///
/// This is the first `Record` in the workspace that is an enum, and it is one for
/// exactly the reason §7.1 gives: Rationes declares two **shapes**. Album's three
/// tokens stayed a flat struct because they are one shape (§8.1).
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum Record {
    Balance(Balance),
    Holding(Holding),
}

impl Record {
    /// The holding a partitioned file holds.
    ///
    /// A body that disagrees with its filename's token is a **malformed** record, not
    /// a missing one: the filename is the claim (§5.2), so this fails validation
    /// (exit `3`) rather than reading as absent.
    ///
    /// # Errors
    /// If the record is a balance reading — the file is not what its name says.
    pub fn as_holding(&self) -> Result<&Holding> {
        match self {
            Record::Holding(holding) => Ok(holding),
            Record::Balance(_) => Err(Error::validation(
                "a holding file holds a balance reading: the filename's token says \
                 which record a file is, and this one disagrees (§5.2, §7.1)",
            )),
        }
    }

    /// The balance a series line holds — the mirror of [`Record::as_holding`].
    ///
    /// # Errors
    /// If the record is a holding — the file is not what its name says.
    pub fn as_balance(&self) -> Result<&Balance> {
        match self {
            Record::Balance(balance) => Ok(balance),
            Record::Holding(_) => Err(Error::validation(
                "a balance line holds a holding: the filename's token says which \
                 record a file is, and this one disagrees (§5.2, §7.1)",
            )),
        }
    }
}

/// The core (§7.1): a record type, a name, its tokens, and its `validate`.
/// Everything else — the twelve verbs, storage dispatch, resolution — the spine
/// provides generically.
pub struct Rationes;

impl Rationes {
    /// The token a bare `add` files under. Hardcoded, never a setting (§18).
    pub const DEFAULT_KIND: &'static str = "account";

    /// The three **entity** tokens, in the order `help` and errors should list them.
    pub const KINDS: [&'static str; 3] = ["account", "asset", "claim"];

    /// The one `Series` token (§8.3).
    pub const BALANCE: &'static str = "balance";

    /// The kinds a balance series may hang off (§8.3). A `claim` is deliberately not
    /// among them: a right you hold is not a figure that moves, and net worth folds
    /// only what this list names.
    pub const CARRIES_BALANCE: [&'static str; 2] = ["account", "asset"];

    /// Whether a holding of this kind may carry a balance series (§8.3).
    #[must_use]
    pub fn carries_balance(kind: &str) -> bool {
        Self::CARRIES_BALANCE.contains(&kind)
    }
}

impl Core for Rationes {
    type Record = Record;

    const NAME: &'static str = "rationes";

    /// Four tokens over **two shapes** — the declaration §7.1 says is the only place
    /// a shape is ever named. `balance` is `named: false` because its name slot
    /// carries a determinant (its holding's slug) rather than an identity: the spine
    /// reads that one bit to skip the cross-node uniqueness check a hand-named series
    /// gets, and to know the series is no ref target of its own (§5.4).
    fn kinds() -> &'static [(&'static str, Shape)] {
        &[
            ("account", Shape::Partitioned),
            ("asset", Shape::Partitioned),
            ("claim", Shape::Partitioned),
            ("balance", Shape::Series { named: false }),
        ]
    }

    /// Checks beyond the envelope (§7.1).
    ///
    /// Kind-blind, like Album's (§8.1): the trait hands `validate` a record and not
    /// the token it was filed under. Whether a `claim` may carry a balance at all is
    /// a question about *two* records — the line and the holding it hangs off — so it
    /// is the bin's check on write, not this one's (§6.4).
    fn validate(record: &Record) -> Result<()> {
        match record {
            Record::Holding(holding) => validate_holding(holding),
            Record::Balance(balance) => validate_balance(balance),
        }
    }
}

fn validate_holding(holding: &Holding) -> Result<()> {
    for (name, value) in [("--currency", &holding.currency), ("--note", &holding.note)] {
        if value.as_ref().is_some_and(|v| v.trim().is_empty()) {
            return Err(Error::validation(format!("{name} is blank (§8.3)")));
        }
    }
    if let Some(expires) = &holding.expires {
        // An expiry says *when*, so it has to be a date. Reusing the key's own shape
        // rule keeps one reading of what a date looks like across the suite (§5.4).
        if !matches!(
            pantheon::Key::parse(expires)?.classify(),
            KeyShape::Date | KeyShape::DateTime
        ) {
            return Err(Error::validation(format!(
                "--expires takes the day the holding lapses (YYMMDD), and {expires:?} \
                 is not one (§5.4, §8.3)"
            )));
        }
    }
    Ok(())
}

fn validate_balance(balance: &Balance) -> Result<()> {
    // A non-finite figure serializes as `null` and comes back as a record that will
    // not parse, so it is refused at the door rather than written and mourned (I6).
    if !balance.amount.is_finite() {
        return Err(Error::validation(format!(
            "a balance is a finite figure, and {} is not one (§8.3)",
            balance.amount
        )));
    }
    if balance.note.as_ref().is_some_and(|n| n.trim().is_empty()) {
        return Err(Error::validation(
            "the note is blank; a reading says what it is in its figure, so an empty \
             note says nothing twice (§8.3)",
        ));
    }
    Ok(())
}
