//! Output rendering (§7.3, I8): format follows the hand — piped stdout gets JSON,
//! a terminal gets a human-readable rendering. Same data, one code path.

use serde_json::Value;

/// Emit a contract value. `as_json` (a pipe, or `-f json`) prints compact JSON; a
/// terminal gets pretty JSON for now — a true table is polish (§7.3), and the data,
/// which is the contract, is identical either way.
pub fn emit(value: &Value, as_json: bool) {
    if as_json {
        println!("{value}");
    } else {
        match serde_json::to_string_pretty(value) {
            Ok(pretty) => println!("{pretty}"),
            Err(_) => println!("{value}"),
        }
    }
}
