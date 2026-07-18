//! Error type and exit codes (§7.3). Exit codes are contract — machines never
//! parse prose: `0` ok · `1` runtime · `2` usage · `3` validation · `4` not found
//! · `5` confirm required · `6` write refused. Every error prints
//! `{"error":{"code":…,"msg":…}}` to stderr, where `code` is the numeric exit
//! code.

use serde_json::json;

/// Process exit codes — the contract every PantheonOS binary shares (§7.3).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum ExitCode {
    /// The command succeeded.
    Ok = 0,
    /// An I/O or serialization fault, or any otherwise-unclassified failure.
    Runtime = 1,
    /// The caller's fault: a missing root, a malformed code, a flag a shape can't take.
    Usage = 2,
    /// A validation failure: a slug collision, a stale plan token, a record that won't deserialize.
    Validation = 3,
    /// No such node, ref, or series.
    NotFound = 4,
    /// A mutation needs confirmation: re-run with `-y` (§7.3). Carried as a plan, not an error.
    ConfirmRequired = 5,
    /// A write verb refused under `PANTHEON_RULE=1` (§9.3).
    WriteRefused = 6,
}

impl ExitCode {
    #[must_use]
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

/// A spine error carrying the exit code its variant maps to (§7.3).
///
/// Exit `5` (confirmation required) is deliberately *not* an [`Error`] — it is an
/// [`crate::plan::Outcome::Plan`] awaiting review. An error is a failure; a pending
/// plan is data the caller shows and then re-runs with `-y`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// `1` — a runtime fault (I/O, serialization, or unclassified).
    #[error("{0}")]
    Runtime(String),
    /// `2` — a usage error.
    #[error("{0}")]
    Usage(String),
    /// `3` — a validation failure.
    #[error("{0}")]
    Validation(String),
    /// `4` — not found.
    #[error("{0}")]
    NotFound(String),
    /// `6` — a write refused under a rule.
    #[error("{0}")]
    WriteRefused(String),
}

impl Error {
    #[must_use]
    pub fn exit_code(&self) -> ExitCode {
        match self {
            Error::Runtime(_) => ExitCode::Runtime,
            Error::Usage(_) => ExitCode::Usage,
            Error::Validation(_) => ExitCode::Validation,
            Error::NotFound(_) => ExitCode::NotFound,
            Error::WriteRefused(_) => ExitCode::WriteRefused,
        }
    }

    /// The stderr contract shape (§7.3): `{"error":{"code":<u8>,"msg":<string>}}`.
    #[must_use]
    pub fn to_error_json(&self) -> serde_json::Value {
        json!({ "error": { "code": self.exit_code().as_u8(), "msg": self.to_string() } })
    }

    pub fn runtime(msg: impl Into<String>) -> Self {
        Error::Runtime(msg.into())
    }
    pub fn usage(msg: impl Into<String>) -> Self {
        Error::Usage(msg.into())
    }
    pub fn validation(msg: impl Into<String>) -> Self {
        Error::Validation(msg.into())
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Error::NotFound(msg.into())
    }
    pub fn write_refused(msg: impl Into<String>) -> Self {
        Error::WriteRefused(msg.into())
    }
}

/// A missing path is `NotFound` (exit `4`); every other I/O fault is `Runtime`.
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        match e.kind() {
            std::io::ErrorKind::NotFound => Error::NotFound(e.to_string()),
            _ => Error::Runtime(e.to_string()),
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Runtime(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
