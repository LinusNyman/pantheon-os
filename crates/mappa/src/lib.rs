//! Mappa — locus places (where) (§8.2). Places stored as a **partitioned register**:
//! one `.json` object per place, its kind and slug in the filename (§6.1).
//! Referenced everywhere as `mappa:<slug>`.
//!
//! Two filename kinds, both partitioned: `location` (an addressable point — a house,
//! a café, a virtual room) and `region` (an area with an extent — a forest you own, a
//! district). The split is **point-vs-area**, a real `data`-shape difference — a point
//! carries coordinates, an area carries bounds — and **not** small-vs-large: scale is
//! the tree's cut (Habitat → Orbis), and a kind vocabulary that re-cut it would only
//! duplicate the tree in a field (§8.2, I9).
//!
//! Mappa holds *places*, not your history among them. **Where you've been** is an
//! Annales `log` line referencing `mappa:<place>` — a dated fact about your movement
//! (§8.6) — and **where you live** is a Fasti residence `span` referencing a
//! `location` (§8.4): residence is your *relationship* to a place, a period rather
//! than a place-type, so it is never a Mappa kind. Nothing here models movement, and
//! neither of those cores is linked — a ref is all that crosses (I5).
//!
//! Build order step 7 — the register the vertical slice did not need, built against a
//! contract a screen has now exercised (§16).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use pantheon::{Core, Error, Result, Shape};

// The CLI and the screen are the lib's, and `main.rs` is the ~30-line clap shell §14
// asks for. They live here rather than in the bin for one reason: an integration test
// links the *lib*, so a screen in the bin is a screen no test can reach — and step 6
// found three defects that only driving a screen caught (P§3, §14).
//
// **What that must not cost is I4.** A core's CLI JSON is the only thing that crosses a
// component boundary, and a verb reachable as a Rust function would be a second door
// into this core. So the verbs stay `pub(crate)`: the only things public here are
// [`run_cli`] — the whole CLI, entered exactly as the binary enters it — and
// [`MappaApp`], which relays through that same CLI. Neither is a way to call a verb
// directly, and nothing else is exposed.
mod cli;
// The screen rides the `tui` feature; drop it and the core is headless (§14).
#[cfg(feature = "tui")]
mod screen;

pub use cli::run_cli;
#[cfg(feature = "tui")]
pub use screen::MappaApp;

/// A point on the globe, in decimal degrees (§8.2) — what makes a `location`
/// addressable to a machine, beside the `address` that makes it addressable to a
/// hand.
///
/// Decimal degrees rather than a formatted string: the number is the datum, and a
/// hand wanting degrees-minutes-seconds renders them from it (I1). No datum field —
/// what a hand types is WGS84, and a second one would be a knob (§18).
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct Coordinates {
    pub lat: f64,
    pub lon: f64,
}

/// The extent of a `region` (§8.2) — its two corners, southwest then northeast.
///
/// A bounding box rather than a polygon: an extent here says roughly where a district
/// or a forest *is*, and a polygon is geometry a GIS owns. Storing one would make
/// Mappa a mapping engine rather than a register of places.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct Bounds {
    pub south: f64,
    pub west: f64,
    pub north: f64,
    pub east: f64,
}

/// One place — the `data` half of an entity file (§6.1).
///
/// Its `refs` ride in the envelope, and its home, core, kind, and slug are the file's
/// location and name (I3), so none of them is stored here. Nor is a display name: a
/// name and its slug are one thing and are never allowed to differ (§5.4), so
/// `rename` is the only way a name changes and there is no second copy to drift.
///
/// One flat struct rather than an enum over the two kinds, for the reason §7.1 and
/// §7.2 give together: the enum §7.1 asks for is a **dispatch type** for a core
/// declaring two *storage shapes*, and Mappa's two tokens are one shape. Making it an
/// enum would turn `edit -k location→region` into a record transformation, when §7.2
/// says it is a file rename and nothing more — and would have `schemars` emit a
/// tagged union when §18 forbids writing a variant tag. §8.2's "real `data`-shape
/// difference" is what earns the kind split against a scale vocabulary; it does not
/// ask for two Rust types. A point simply leaves `bounds` absent, and an area
/// `coordinates`.
// `Eq` is unavailable and unwanted: a degree is an `f64`, and the only equality this
// record needs is the structural one an overwrite's before/after diff already reads
// off the JSON.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, Default, PartialEq)]
pub struct Place {
    /// Where a point *is* (§8.2). A `region` leaves it absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coordinates: Option<Coordinates>,
    /// The extent of an area (§8.2). A `location` leaves it absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<Bounds>,
    /// The postal or street address — the same place, addressed to a hand.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// A virtual room's address (§8.2): a meeting link is a place you can be.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The zone the place keeps time in — a property of the place, never of a
    /// reading taken there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    /// A hand's remark on this place.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// The core (§7.1): a record type, a name, its tokens, and its `validate`.
/// Everything else — the twelve verbs, storage dispatch, resolution — the spine
/// provides generically.
pub struct Mappa;

impl Mappa {
    /// The token a bare `add` files under. Hardcoded, never a setting: §18 keeps
    /// per-core defaults out of configuration, and Mappa's is `location` — the
    /// commoner half of the split, and the one a `region` is corrected *from*.
    pub const DEFAULT_KIND: &'static str = "location";

    /// This core's two tokens, in the order `help` and errors should list them.
    pub const KINDS: [&'static str; 2] = ["location", "region"];
}

impl Core for Mappa {
    type Record = Place;

    const NAME: &'static str = "mappa";

    /// Two tokens, one shape. What a place *is* — a point or an area — is a filename
    /// segment, corrected by `edit -k`: a visible structural act, not a silent field
    /// flip (§7.2).
    fn kinds() -> &'static [(&'static str, Shape)] {
        &[
            ("location", Shape::Partitioned),
            ("region", Shape::Partitioned),
        ]
    }

    /// Checks beyond the envelope (§7.1).
    ///
    /// Kind-blind by construction: the trait hands `validate` a record and not the
    /// token it was filed under, which is right — the kind is the filename's. So a
    /// `location` carrying `bounds` is not refused here, and must not be: the refusal
    /// would need the token, and the token is exactly what `edit -k` moves. What is
    /// checked is what a record can be wrong about on its own — a degree off the
    /// globe, an extent inverted, a blank field standing in for an absent one.
    fn validate(record: &Place) -> Result<()> {
        for (name, value) in [
            ("--address", &record.address),
            ("--url", &record.url),
            ("--timezone", &record.timezone),
            ("--note", &record.note),
        ] {
            if value.as_ref().is_some_and(|v| v.trim().is_empty()) {
                return Err(Error::validation(format!("{name} is blank (§8.2)")));
            }
        }
        // A room you can enter is a place, and its address is a URL — so it is one
        // token you could hand to a browser (§8.2).
        if let Some(url) = &record.url
            && url.chars().any(char::is_whitespace)
        {
            return Err(Error::validation(format!(
                "--url {url:?} holds whitespace: a virtual room's address is one token (§8.2)"
            )));
        }
        if let Some(at) = &record.coordinates {
            check_degrees(at.lat, LAT, "latitude")?;
            check_degrees(at.lon, LON, "longitude")?;
        }
        if let Some(extent) = &record.bounds {
            check_degrees(extent.south, LAT, "south")?;
            check_degrees(extent.west, LON, "west")?;
            check_degrees(extent.north, LAT, "north")?;
            check_degrees(extent.east, LON, "east")?;
            if extent.north < extent.south {
                return Err(Error::validation(format!(
                    "bounds put north below south ({} < {}): an extent runs southwest to \
                     northeast (§8.2)",
                    extent.north, extent.south
                )));
            }
            // `east < west` is **not** checked: a region straddling the antimeridian
            // legitimately wraps (Fiji, Chukotka), and refusing it would refuse a real
            // place to keep the arithmetic tidy.
        }
        Ok(())
    }
}

/// The poles bound a latitude; the antimeridian bounds a longitude.
const LAT: f64 = 90.0;
const LON: f64 = 180.0;

/// A degree sits on the globe or the record is wrong (§8.2). Written as a range test
/// rather than two comparisons so a `NaN` — which loses every comparison it is given
/// — is refused rather than quietly stored.
fn check_degrees(value: f64, limit: f64, which: &str) -> Result<()> {
    if (-limit..=limit).contains(&value) {
        Ok(())
    } else {
        Err(Error::validation(format!(
            "{which} {value} is off the globe: it runs -{limit}..{limit} in decimal degrees (§8.2)"
        )))
    }
}
