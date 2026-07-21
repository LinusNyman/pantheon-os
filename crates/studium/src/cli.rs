//! `stu` — Studium, the studies lens: a study life folded onto one screen (§12, §19).
//!
//! A **lens**. What makes a lens a lens is *reach*: it composes across cores — Fasti the
//! enrolment's period, Annales its result and its hours, a curriculum file the scale to
//! weigh it — reading each one's JSON off `PATH` and deriving the overlap at render, which
//! is exactly what a core's own TUI may never do (I5, §12).
//!
//! It owns no primitive and holds no data. Every figure — the **GPA** above all — is
//! folded from the cores' JSON on each refresh and stored nowhere (I1, §19.4), and it
//! **never originates a write** (I2). It may *relay* a human-initiated write — you mark a
//! task done, you record a grade — by shelling out to the same verb a hand would type
//! (§7.2, §19.8), never by linking a core's lib (I5).
//!
//! **A lens is a CLI too** (I4, §4), and what it emits is the present behind its mosaic.
//! At a terminal the bare short opens the screen; down a pipe it emits those figures as
//! JSON — so an LLM reads what a human sees (I8, §19.9). It owns no records, so it grows
//! no verbs: the bare short is the whole surface, plus `help` and `version`.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use pantheon::{contract, resolve_root};
use serde_json::{Value, json};

#[derive(Parser)]
#[command(
    name = "stu",
    version,
    about = "Studium — the studies lens (the GPA, folded)",
    disable_help_subcommand = true
)]
pub(crate) struct Cli {
    /// Operate on a different `$PANTHEON_ROOT` (§7.3).
    #[arg(short = 'C', long = "root", global = true)]
    root: Option<PathBuf>,
    /// Force output format; default follows the hand (TTY vs pipe, §7.3).
    #[arg(short = 'f', long = "format", global = true)]
    format: Option<Format>,
    /// Scope the fold to one node's subtree — one programme's mean (§6.3, §19.4).
    #[arg(short = 'H', long = "home", global = true, value_name = "CODE")]
    home: Option<String>,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum Format {
    Json,
    Table,
}

/// A lens owns no records, so it grows no verbs (§12, §18). Of the universal flags only
/// those naming no record apply; the rest are usage errors by the rule that already
/// refuses `-c` to Album (§7.3).
#[derive(clap::Subcommand)]
enum Cmd {
    /// The surface, as JSON (§7.3).
    Help,
    /// This tool's name, short, and version, as JSON (§7.3).
    Version,
}

/// Run `stu` exactly as the binary runs it (§7.3) — parse `argv`, dispatch, and return the
/// process's exit code. The bin is a shell over this and holds nothing of its own.
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

    // The TTY rule governs here as everywhere (§7.3): a screen has nothing to draw down a
    // pipe, so a piped lens emits the figures behind its mosaic instead (§19.9).
    if as_json {
        return Ok(Some(crate::fold::figures(&root, cli.home.as_deref())));
    }

    #[cfg(feature = "tui")]
    {
        crate::screen::open(&root)?;
        Ok(None)
    }
    // Headless: there is no screen to open, so the fold is the whole answer (§12, §14).
    #[cfg(not(feature = "tui"))]
    {
        Ok(Some(crate::fold::figures(&root, cli.home.as_deref())))
    }
}

fn version_json() -> Value {
    json!({
        "name": "studium",
        "short": "stu",
        "version": env!("CARGO_PKG_VERSION"),
        "format_version": 1,
    })
}

fn help_json() -> Value {
    json!({
        "name": "studium",
        "short": "stu",
        "about": "the studies lens — a study life folded onto one screen, GPA and all",
        // A lens owns no records, so it grows no verbs (§12, §18).
        "verbs": ["help", "version"],
        "flags": ["-h", "-V", "-f", "-C", "-H"],
        "bare": "opens the mosaic at a terminal; emits its figures as JSON down a pipe",
    })
}
