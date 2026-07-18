//! The `schema` surface (§7.2): the JSON every core emits so the spine can discover
//! its tokens and record shape over PATH (§5.0), and each token's [`Shape`]. Generic
//! over [`Core`], so every core produces it identically.

use serde::{Deserialize, Serialize};

use crate::Shape;
use crate::core::Core;

/// A core's declared schema — what `<short> schema` prints and `pan doctor` /
/// resolution read back (§7.2).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CoreSchema {
    pub name: String,
    pub tokens: Vec<TokenSchema>,
    pub record: schemars::Schema,
    pub format_version: u32,
}

/// One token and the shape it names. `shape` is flattened, so a token serializes as
/// `{"token":"person","shape":"partitioned"}` (§7.1).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TokenSchema {
    pub token: String,
    #[serde(flatten)]
    pub shape: Shape,
}

/// Build a core's [`CoreSchema`] from its trait declaration (§7.2).
#[must_use]
pub fn schema<C: Core>(format_version: u32) -> CoreSchema {
    let tokens = C::kinds()
        .iter()
        .map(|(token, shape)| TokenSchema {
            token: (*token).to_string(),
            shape: *shape,
        })
        .collect();
    CoreSchema {
        name: C::NAME.to_string(),
        tokens,
        record: schemars::schema_for!(C::Record),
        format_version,
    }
}
