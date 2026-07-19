//! `atr` — Atrium, the home dashboard: the day folded onto one screen (§12, P§9).
//!
//! A **lens**. What makes a lens a lens is *reach*: it composes across cores, reading
//! each one's JSON off `PATH` and deriving the overlap at render — which is exactly
//! what a core's own TUI may never do (I5, §12).
//!
//! It owns no primitive and holds no data. Every figure is folded from the cores' JSON
//! on each refresh and stored nowhere (I1), and it **never originates a write** (I2).
//! It may *relay* a human-initiated write — you mark a task done here — by shelling out
//! to the same verb a hand would type (§7.2), never by linking a core's lib (I5).
//!
//! **A lens is a CLI too** (I4, §4), and what it emits is the present behind its
//! mosaic. At a terminal the bare short opens the screen; down a pipe it emits those
//! figures as JSON — so an LLM reads what a human sees (I8). It owns no records, so it
//! grows no verbs: the bare short is the whole surface, plus `help` and `version`.

// The screen and everything that draws it ride the `tui` feature; a headless build is
// the fold without the chrome (§12, §14). The modules themselves are declared in
// `lib.rs`, which is where the feature gate now sits.
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use pantheon::{contract, resolve_root};
use serde_json::{Value, json};

/// The cores Atrium reaches. Discovered, never required: a tile whose core isn't
/// installed is simply absent, and so is the relay that would have written to it
/// (§12). A lens degrades to what it finds.
pub(crate) const PENSUM: &str = "pen";
pub(crate) const ALBUM: &str = "alb";
const ANNALES: &str = "ann";
pub(crate) const TABELLA: &str = "tab";

#[derive(Parser)]
#[command(
    name = "atr",
    version,
    about = "Atrium — the home dashboard (the hearth)",
    disable_help_subcommand = true
)]
pub(crate) struct Cli {
    /// Operate on a different `$PANTHEON_ROOT` (§7.3).
    #[arg(short = 'C', long = "root", global = true)]
    root: Option<PathBuf>,
    /// Override the default view (§7.3).
    #[arg(short = 'f', long = "format", global = true)]
    format: Option<Format>,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum Format {
    Json,
    Table,
}

/// A lens owns no records, so it grows no verbs (§12, §18). Of the universal flags
/// only those naming no record apply; the rest are usage errors by the rule that
/// already refuses `-c` to Album (§7.3).
#[derive(clap::Subcommand)]
enum Cmd {
    /// The surface, as JSON (§7.3).
    Help,
    /// This tool's name, short, and version, as JSON (§7.3).
    Version,
}

/// Run `atr` exactly as the binary runs it (§7.3) — parse `argv`, dispatch, and return
/// the process's exit code. The bin is a shell over this and holds nothing of its own.
#[must_use]
pub fn run_cli() -> ExitCode {
    let cli = Cli::parse();
    let as_json = contract::format_is_json(cli.format.map(|f| matches!(f, Format::Json)));
    match run(&cli, as_json) {
        Ok(Some(value)) => {
            contract::emit(&value, as_json);
            ExitCode::from(0)
        }
        Ok(None) => ExitCode::from(0),
        Err(e) => {
            eprintln!(r#"{{"error":{{"code":1,"msg":{}}}}}"#, json!(e.to_string()));
            ExitCode::from(1)
        }
    }
}

pub(crate) fn run(cli: &Cli, as_json: bool) -> anyhow::Result<Option<Value>> {
    match cli.cmd {
        Some(Cmd::Version) => return Ok(Some(version_json())),
        Some(Cmd::Help) => return Ok(Some(help_json())),
        None => {}
    }

    let root = resolve_root(cli.root.as_deref())?;

    // The TTY rule governs here as everywhere (§7.3): a screen has nothing to draw
    // down a pipe, so a piped lens emits the figures behind its mosaic instead.
    if as_json {
        return Ok(Some(figures(&root)));
    }

    #[cfg(feature = "tui")]
    {
        crate::screen::open(&root)?;
        Ok(None)
    }
    // Headless: there is no screen to open, so the fold is the whole answer (§12).
    #[cfg(not(feature = "tui"))]
    {
        Ok(Some(figures(&root)))
    }
}

/// The present behind the mosaic — its tiles' folds, the figures themselves (§12).
///
/// This is the same derivation the screen draws, so an LLM reads what a human sees
/// (I8). Nothing consumes a lens in turn (§18), so this JSON answers a hand.
fn figures(root: &std::path::Path) -> Value {
    json!({
        "open_tasks": count(root, PENSUM, &["list"]),
        "people": count(root, ALBUM, &["list"]),
        "logs": count(root, ANNALES, &["list"]),
        "documents": count(root, TABELLA, &["list"]),
    })
}

/// A fold's row count, or `null` where the core is not installed.
///
/// `null` rather than `0`: a zero is a fold that ran and found nothing, and an absent
/// core is not the same answer (§12).
fn count(root: &std::path::Path, short: &str, args: &[&str]) -> Value {
    match tessera::read(root, short, args) {
        Some(Value::Array(rows)) => json!(rows.len()),
        _ => Value::Null,
    }
}

fn version_json() -> Value {
    json!({
        "name": "atrium",
        "short": "atr",
        "version": env!("CARGO_PKG_VERSION"),
        "format": 1,
    })
}

fn help_json() -> Value {
    json!({
        "name": "atrium",
        "short": "atr",
        "about": "the home dashboard — the day folded onto one screen",
        // A lens owns no records, so it grows no verbs (§12, §18).
        "verbs": ["help", "version"],
        "flags": ["-h", "-V", "-f", "-C", "-q", "-v"],
        "bare": "opens the mosaic at a terminal; emits its figures as JSON down a pipe",
    })
}
