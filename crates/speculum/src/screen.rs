//! The screen: Speculum as a Porticus app (P§2).
//!
//! Everything here rides the `tui` feature — a headless lens keeps the folds and drops
//! the chrome (§12, §14), so nothing in this file may be reachable without it.
//!
//! Speculum leads with its [`Mosaic`] — tiles counting every core the mirror reflects —
//! and reviews through the [`Horizon`], a dated cross-core list the hand widens and
//! narrows. It relays a human write (I2, §12) by shelling out to the same verb a hand
//! would type: `e` fixes a reading in place (the editor form — a balance corrected,
//! §8.3), `x` drops one. The write crosses the JSON boundary over `PATH` (I4, I5).

use pantheon::Code;
use porticus::view::Row;
use porticus::{Action, App, Ident, Invocation, RecordRef, Target, View, Writer};
use serde_json::Value;

use crate::cli::{ALBUM, ANNALES, FASTI, PENSUM, RATIONES, TABELLA};
use crate::horizon::Horizon;
use crate::mosaic::Mosaic;

/// The cores whose **dated points** the horizon folds — logs, events, balances. Each
/// row their `list` returns carries `core`/`home`/`key` (§7.2); a row whose key is a
/// date lands on the horizon, one whose key is a slug (a task, a place, a span) does
/// not. The order is the routing priority (see [`Speculum::core_of`]).
const DATED: &[&str] = &[ANNALES, FASTI, RATIONES];

/// Open the mosaic.
///
/// # Errors
/// If the tree cannot be walked or the terminal cannot be taken.
pub fn open(root: &std::path::Path) -> anyhow::Result<()> {
    porticus::run(&mut Speculum::new(root), root)
}

/// The root the screen is drawing.
///
/// Held rather than left to `$PANTHEON_ROOT`: a lens opened with `-C` must fold the
/// tree it was pointed at, not the caller's ambient one (§6.2, §7.3).
pub struct Speculum {
    root: std::path::PathBuf,
}

impl Speculum {
    /// Public so a test can build the **real** lens and drive it — the same object
    /// `open` runs, with the same tiles and the same **subprocess** relay, so a driven
    /// write crosses the JSON boundary exactly as it does in a hand's terminal
    /// (I4, I5, §12).
    #[must_use]
    pub fn new(root: &std::path::Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }

    /// Which core owns the record at `(home, key)`.
    ///
    /// A cross-core lens must route a relay to the right binary, but a [`RecordRef`]
    /// carries no core — so the owner is recovered by re-reading the dated sources and
    /// matching `home`/`key` (I5, §12). The first source that holds it wins, which is
    /// why [`DATED`]'s order is a priority: were two cores to file a record at the same
    /// node under the same date-key, the relay would go to the earlier one. (That the
    /// `RecordRef` has no core slot is a Porticus gap this works around; see the crate's
    /// PR notes.)
    fn core_of(&self, home: &str, key: &str) -> Option<&'static str> {
        for short in DATED {
            if let Some(Value::Array(rows)) = tessera::read(&self.root, short, &["list"])
                && rows.iter().any(|row| {
                    row["home"].as_str() == Some(home) && row["key"].as_str() == Some(key)
                })
            {
                return Some(short);
            }
        }
        None
    }
}

impl App for Speculum {
    fn ident(&self) -> Ident {
        Ident {
            name: "speculum",
            short: "spe",
            tagline: "the mirror",
            symbol: '☽',
            accent: porticus::ident::accent::MOON_SILVER,
        }
    }

    fn lineup(&mut self) -> Vec<Box<dyn View>> {
        let for_horizon = self.root.clone();
        vec![
            // A lens leads with its mosaic — the dashboard, not the tree (P§3).
            Box::new(Mosaic::of(vec![
                Box::new(tessera::Count::of(
                    &self.root,
                    "open tasks",
                    PENSUM,
                    &["list"],
                )),
                Box::new(tessera::Count::of(&self.root, "people", ALBUM, &["list"])),
                Box::new(tessera::Count::of(
                    &self.root,
                    "holdings",
                    RATIONES,
                    &["list"],
                )),
                Box::new(tessera::Count::of(
                    &self.root,
                    "placements",
                    FASTI,
                    &["list"],
                )),
                Box::new(tessera::Count::of(&self.root, "logs", ANNALES, &["list"])),
                Box::new(tessera::Count::of(
                    &self.root,
                    "documents",
                    TABELLA,
                    &["list"],
                )),
            ])),
            // The review: dated points across every core, on a window the hand widens
            // and narrows. Each row carries its own home so a relay reaches the right
            // node, and `e`/`x` fix or drop a reading in place (P§3, P§7).
            Box::new(
                Horizon::of(move || dated_rows(&for_horizon))
                    .offering(&[Action::Edit, Action::Remove]),
            ),
        ]
    }

    fn count_at(&mut self, node: &Code) -> usize {
        // Never reached — Speculum's views are both Full, so no tree rail asks (P§6).
        // Kept honest anyway: this node's dated points across the cores the horizon
        // folds, derived and never stored (I1).
        let mut total = 0;
        for short in DATED {
            if let Some(Value::Array(rows)) =
                tessera::read(&self.root, short, &["list", "-H", node.as_str()])
            {
                total += rows
                    .iter()
                    .filter(|row| row["key"].as_str().is_some_and(is_date_key))
                    .count();
            }
        }
        total
    }

    fn writer(&self) -> Writer {
        // A lens shells out to the core binary on `PATH` (§12): it links no core (I5),
        // and the write crosses the JSON boundary like every other (I4).
        Writer::Subprocess
    }

    fn relays_to(&self) -> Vec<String> {
        // The cores the horizon can write to: an absent one dims its actions rather
        // than failing when tried (§12, P§7).
        DATED.iter().map(|short| (*short).to_string()).collect()
    }

    fn on_action(&mut self, action: Action, target: &Target) -> Option<Invocation> {
        // Only the app knows its verb grammar, because only the app authors the write
        // (I2). Porticus owns the confirm and the relay and knows none of this.
        let Target::Row(RecordRef { home, key }) = target else {
            // Speculum adds nothing: it owns no primitive, so a new record is a core's
            // to create, not a mirror's (§12).
            return None;
        };
        let short = self.core_of(home.as_str(), key)?;
        let home = home.as_str();
        match action {
            // `e` with no value inline is the editor form (§7.3): the reading opens in
            // the hand's own editor — a balance fixed in place (§8.3, §12).
            Action::Edit => Some(Invocation::new(short, ["edit", "-H", home, key.as_str()])),
            Action::Remove => Some(Invocation::new(short, ["rm", "-H", home, key.as_str()])),
            _ => None,
        }
    }
}

/// The dated points across every core the horizon folds, as rows.
///
/// Read off each core's `list` JSON and nothing else — the contract is the only thing
/// that crosses (I4). Each row keeps the home the core reported, which is what lets a
/// cross-node horizon relay each write to its own node (P§7). A row is kept only where
/// its key is a date; the rest — a task's slug, a span's name — belong to no horizon.
fn dated_rows(root: &std::path::Path) -> Vec<Row> {
    let mut out = Vec::new();
    for short in DATED {
        let Some(Value::Array(rows)) = tessera::read(root, short, &["list"]) else {
            continue;
        };
        for row in rows {
            let Some(key) = row["key"].as_str() else {
                continue;
            };
            if !is_date_key(key) {
                continue;
            }
            let Some(home) = row["home"].as_str().and_then(|h| Code::parse(h).ok()) else {
                continue;
            };
            let core = row["core"].as_str().unwrap_or(short);
            let what = row["series"]
                .as_str()
                .or_else(|| row["kind"].as_str())
                .unwrap_or("");
            let refs = row["refs"].as_array().map_or(String::new(), |refs| {
                let names: Vec<&str> = refs.iter().filter_map(Value::as_str).collect();
                if names.is_empty() {
                    String::new()
                } else {
                    format!("   {}", names.join(", "))
                }
            });
            let home_str = home.as_str();
            out.push(Row {
                label: format!("{core:<9} {what}   {home_str}{refs}"),
                target: Target::Row(RecordRef {
                    home,
                    key: key.to_string(),
                }),
                when: Some(key.to_string()),
            });
        }
    }
    out
}

/// Whether a key names a date — six leading digits (`YYMMDD`), optionally trailed by a
/// time (`260703T1400`). A slug is not a date, so it is not on the horizon (§6.1).
fn is_date_key(key: &str) -> bool {
    key.len() >= 6 && key.as_bytes()[..6].iter().all(u8::is_ascii_digit)
}
