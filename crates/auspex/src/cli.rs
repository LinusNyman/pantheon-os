//! `aus`'s CLI (§9.6). A **system tool**, so it carries its own structural verbs
//! beside `help` and `version` — not a core's twelve, and no licence for a core to
//! grow a thirteenth (§7.3, §18).
//!
//! **No argv pre-pass.** `add` is a core's default verb, not every bin's: "`pan`,
//! `aus`, and the lenses have no implicit verb and need no pre-pass" (§13). So clap
//! parses `argv` directly and a bare short is simply `cmd: None`.
//!
//! **No `schema` verb, and no `Ctx`.** Auspex owns no records, declares no tokens, and
//! holds no `Store` — `pan doctor` reads its version off `version -f json` like every
//! app's, while the token map beside it comes off each *core*'s `schema` (§5.5, §12).

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};

use pantheon::code::Code;
use pantheon::contract::{self, Response};
use pantheon::{Error, Result, resolve_root};

const ABOUT: &str = "Auspex — the rules engine: reads the tree for signs and proposes \
                     intentions (§9).";

#[cfg(not(feature = "tui"))]
const BARE: &str = "aus — Auspex (the omens). Built without the `tui` feature; run \
                    `aus --help` for the verbs.\n";

/// The verbs, for `help`. One list, read by both surfaces — `pan` keeps a second copy
/// inside its `help_json` and the two have already drifted apart.
const VERBS: &[&str] = &["run", "plan", "ls", "test", "help", "version"];

#[derive(Parser)]
#[command(name = "aus", version, about = ABOUT, disable_help_subcommand = true)]
pub(crate) struct Cli {
    /// The tree root; else `$PANTHEON_ROOT`, else a usage error (§6.2).
    #[arg(short = 'C', long = "root", global = true, value_name = "DIR")]
    root: Option<PathBuf>,
    /// Force output format; default follows the hand (TTY vs pipe, §7.3).
    #[arg(short = 'f', long = "format", global = true, value_enum)]
    format: Option<Format>,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Json,
    Table,
}

/// Auspex's structural set (§9.6). `add` and the rest of a core's twelve mean nothing
/// here — Auspex owns no records; it writes other cores' through their own CLIs.
#[derive(Subcommand)]
enum Cmd {
    /// Evaluate the rules in scope and apply what they propose (§9.4).
    Run {
        /// The subtree to evaluate; the whole forest when absent.
        scope: Option<String>,
        /// The write that woke this run, as `core@home` (§9.3). A hand omits it.
        #[arg(long = "trigger", value_name = "CORE@HOME")]
        trigger: Option<String>,
    },
    /// Evaluate and print proposals as JSON; apply nothing (§9.6).
    Plan {
        /// The subtree to evaluate; the whole forest when absent.
        scope: Option<String>,
    },
    /// Every discovered rule: scope and header (§9.6).
    Ls {
        /// The subtree to list; the whole forest when absent.
        scope: Option<String>,
    },
    /// Run one rule against a stdin fixture and print its proposals (§9.6).
    Test {
        /// The rule's name.
        rule: String,
    },
    /// The verbs, as JSON (§7.3).
    Help,
    /// This tool's name, short, and version, as JSON (§7.3).
    Version,
}

/// Run `aus` exactly as the binary runs it (§7.3) — parse `argv`, dispatch, and return
/// the process's exit code. The bin is a shell over this and holds nothing of its own.
#[must_use]
pub fn run_cli() -> ExitCode {
    // Auspex never wakes Auspex. `contract::dispatch` fires the hook for every
    // instrument and `porticus::run` wakes on every screen open, so without this the
    // rules browser would spawn `aus run` the moment it opened — and §9.4 scopes that
    // wake to a *core's* TUI. The twin of the `PANTHEON_NO_HOOKS` Auspex sets on the
    // cores it spawns: one says not this process, the other not that child.
    pantheon::hook::suppress();

    let cli = Cli::parse();
    let as_json = contract::format_is_json(cli.format.map(|f| matches!(f, Format::Json)));
    contract::dispatch(run(&cli, as_json), as_json)
}

pub(crate) fn run(cli: &Cli, as_json: bool) -> Result<Response> {
    let Some(cmd) = &cli.cmd else {
        return bare(cli, as_json);
    };
    match cmd {
        // The three verbs that evaluate a rule are refused to a rule (§9.3): a rule
        // reads the tree through the core CLIs and never by re-entering the engine,
        // and an `aus plan` inside a rule would re-evaluate that rule without bound.
        Cmd::Run { .. } | Cmd::Plan { .. } | Cmd::Test { .. } if contract::under_rule() => {
            Err(refused_under_rule(verb_of(cmd)))
        }
        Cmd::Ls { scope } => cmd_ls(cli, scope.as_deref()),
        Cmd::Help => Ok(Response::Json(help_json())),
        Cmd::Version => Ok(Response::Json(version_json())),
        Cmd::Run { .. } | Cmd::Plan { .. } | Cmd::Test { .. } => Err(not_implemented(cmd)),
    }
}

fn verb_of(cmd: &Cmd) -> &'static str {
    match cmd {
        Cmd::Run { .. } => "run",
        Cmd::Plan { .. } => "plan",
        Cmd::Test { .. } => "test",
        Cmd::Ls { .. } => "ls",
        Cmd::Help => "help",
        Cmd::Version => "version",
    }
}

/// Exit `6` for a verb that would re-enter the engine from inside a rule (§9.3).
///
/// Auspex writes its own wording rather than borrowing
/// [`contract::refused_under_rule`], because the *reason* is not a core's. A core
/// refuses a write because a rule may not borrow a hand's authority; Auspex refuses
/// these three because **an `aus plan` inside a rule would re-evaluate that rule and
/// recurse without bound**. Same law, same exit code, different danger — and the
/// core's message would point a reader at `get` and `where`, which are not verbs `aus`
/// has.
fn refused_under_rule(verb: &str) -> Error {
    Error::write_refused(format!(
        "`{verb}` re-enters the engine and PANTHEON_RULE=1 refuses it: a rule reads the \
         tree through the core CLIs, never by calling `aus` — an `aus plan` inside a \
         rule would re-evaluate that rule without bound (§9.3)"
    ))
}

/// The three verbs that execute a rule, which land with the propose protocol (§9.3).
///
/// Declared rather than hidden, so `--help` tells the truth about what `aus` will be
/// and the refusal above has something to refuse.
fn not_implemented(cmd: &Cmd) -> Error {
    Error::runtime(format!(
        "`aus {}` is not implemented yet — rule execution is the propose protocol's \
         (§9.3), and `ls` is what reads the tree today",
        verb_of(cmd)
    ))
}

/// A bare short opens the screen at a terminal and emits `help` down a pipe (§7.3) —
/// a TUI has nothing to draw on down a pipe.
fn bare(cli: &Cli, as_json: bool) -> Result<Response> {
    if as_json {
        return Ok(Response::Json(help_json()));
    }
    #[cfg(feature = "tui")]
    {
        let root = resolve_root(cli.root.as_deref())?;
        crate::screen::open(&root).map_err(|e| Error::runtime(e.to_string()))?;
        Ok(Response::Raw(String::new()))
    }
    // Headless: there is no screen to open, so help is the whole answer (§14).
    #[cfg(not(feature = "tui"))]
    {
        let _ = cli;
        Ok(Response::Raw(BARE.to_string()))
    }
}

/// Every rule in scope, one flat row each (§9.6).
///
/// `watch` and `writes` stay in their **header form** — the capability string a hand
/// reads before granting (§9.2). Parsing them into a structure is the enforcing verb's
/// job; showing them back unchanged is this one's.
fn cmd_ls(cli: &Cli, scope: Option<&str>) -> Result<Response> {
    let root = resolve_root(cli.root.as_deref())?;
    let at = scope.map(Code::parse).transpose()?;
    let rules = crate::discover(&root, at.as_ref())?;
    let rows: Vec<Value> = rules
        .iter()
        .map(|rule| {
            let mut row = json!({
                "scope": rule.scope.as_str(),
                "name": rule.name,
                "file": rule.file_name(),
                "watch": rule.header.watch,
                "writes": rule.header.writes,
            });
            // Absent keys rather than hollow ones: a rule with no description has no
            // `desc`, and a header that parsed has no `error` (§7.2).
            if let Some(desc) = &rule.header.desc {
                row["desc"] = json!(desc);
            }
            if let Some(declared) = &rule.declared {
                // §9.1: where the file sits is the whole of its scope. A filename
                // naming a different node is a misfiling to report, never a scope.
                row["misfiled_as"] = json!(declared);
            }
            if let Some(error) = &rule.header.error {
                row["error"] = json!(error);
            }
            row
        })
        .collect();
    Ok(Response::Json(Value::Array(rows)))
}

/// A system tool's surface. No `kinds` and no `series` — Auspex owns no records — and
/// no `schema` verb to declare them with (§7.1, §12).
fn help_json() -> Value {
    json!({
        "name": "auspex",
        "short": "aus",
        "about": "the omens: rules that read the tree and propose intentions (§9)",
        "verbs": VERBS,
        "bare": "opens the rules browser at a terminal; emits this down a pipe",
        "version": env!("CARGO_PKG_VERSION"),
        "format_version": 1,
    })
}

fn version_json() -> Value {
    json!({
        "name": "auspex",
        "short": "aus",
        "version": env!("CARGO_PKG_VERSION"),
        "format_version": 1,
    })
}
