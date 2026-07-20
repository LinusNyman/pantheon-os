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

use crate::grant::{Grant, Proposal};

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
    init_tracing();

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
        Cmd::Plan { scope } => cmd_plan(cli, scope.as_deref()),
        Cmd::Test { rule } => cmd_test(cli, rule),
        Cmd::Run { scope, trigger } => cmd_run(cli, scope.as_deref(), trigger.as_deref()),
        Cmd::Help => Ok(Response::Json(help_json())),
        Cmd::Version => Ok(Response::Json(version_json())),
    }
}

/// Auspex's diagnostics, **stderr only** (§13).
///
/// Inherited when a hand runs `aus`, and discarded when a core spawns the hook
/// detached — `hook::wake` nulls the child's streams, so this writes into the void
/// there, which is the intent rather than an accident. Never a file: a log the engine
/// kept would be the rule state and the audit sink §18 forbids.
///
/// Off unless `RUST_LOG` asks, so an ordinary run says nothing on stderr and a piped
/// caller sees only the contract on stdout.
fn init_tracing() {
    use tracing_subscriber::{EnvFilter, fmt};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    // `try_init` rather than `init`: a second call must not panic, and the in-process
    // relay re-enters `run_cli`'s neighbours from the screen.
    let _ = fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(filter)
        .try_init();
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

// ── the propose protocol (§9.3) ──────────────────────────────────────────────

/// Evaluate every rule in scope and print what they propose, applying nothing (§9.6).
///
/// **Triggerless, always.** `aus plan` takes no `--trigger` (§9.6), so no single write
/// authored this wake — which is exactly the case §9.3 says `watch=` cannot filter, so
/// every rule in scope evaluates.
fn cmd_plan(cli: &Cli, scope: Option<&str>) -> Result<Response> {
    let root = resolve_root(cli.root.as_deref())?;
    let at = scope.map(Code::parse).transpose()?;
    let rules = crate::discover(&root, at.as_ref())?;
    let now = today()?;

    let mut failed = false;
    let mut rows = Vec::with_capacity(rules.len());
    for rule in &rules {
        let context = context_for(rule, &now, "manual", None);
        let outcome = crate::rule::evaluate(
            &rule.path,
            &root,
            &context.to_string(),
            crate::rule::DEADLINE,
        );
        rows.push(row_for(rule, &outcome, &mut failed));
    }
    Ok(outcome_response(rows, failed))
}

/// Run one rule against a fixture and print its proposals (§9.6).
///
/// The fixture is `aus`'s own stdin, handed to the rule unchanged — the point is to
/// drive a rule with a context you chose (a particular `now`, a particular trigger) and
/// assert on what comes back. **At a terminal with nothing piped, the context `plan`
/// would have built is synthesized instead** of blocking on a stdin that will never
/// close: the same TTY rule that decides table-versus-JSON and opens an editor (§7.3,
/// I8), and the same gate Tabella reads a document body behind.
fn cmd_test(cli: &Cli, rule_name: &str) -> Result<Response> {
    let root = resolve_root(cli.root.as_deref())?;
    let rules = crate::discover(&root, None)?;
    let rule = one_named(&rules, rule_name)?;

    let mut fixture = String::new();
    if !contract::stdin_is_terminal() {
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut fixture)
            .map_err(|e| Error::runtime(format!("could not read the fixture from stdin: {e}")))?;
    }
    // Blank counts as no fixture, not as an empty one — `aus test foo </dev/null` is a
    // hand asking to run the rule, not asking to hand it nothing. Handing a rule an
    // empty stdin would make it fail at parsing a context it was never given.
    let context = if fixture.trim().is_empty() {
        context_for(rule, &today()?, "manual", None).to_string()
    } else {
        fixture
    };

    let mut failed = false;
    let outcome = crate::rule::evaluate(&rule.path, &root, &context, crate::rule::DEADLINE);
    let row = row_for(rule, &outcome, &mut failed);
    Ok(outcome_response(vec![row], failed))
}

/// Resolve a rule name to exactly one rule (§7.3's inference rule).
///
/// Zero is not found; more than one lists the candidates and stops rather than picking
/// — a rule name is unique per node, not per tree, so two nodes may honestly hold a
/// `weigh_in` and neither is the obvious one.
fn one_named<'a>(rules: &'a [crate::Rule], name: &str) -> Result<&'a crate::Rule> {
    let matches: Vec<&crate::Rule> = rules.iter().filter(|r| r.name == name).collect();
    match matches.as_slice() {
        [] => Err(Error::not_found(format!(
            "no rule named {name:?} — `aus ls` shows what the tree has (§9.1)"
        ))),
        [only] => Ok(only),
        many => {
            let at: Vec<&str> = many.iter().map(|r| r.scope.as_str()).collect();
            Err(Error::usage(format!(
                "{name:?} is a rule at {} — name a scope to choose (§7.3)",
                at.join(", ")
            )))
        }
    }
}

// ── applying proposals (§9.5) ────────────────────────────────────────────────

/// The write that woke a run: a core and a home (§9.3).
struct Trigger {
    core: String,
    home: String,
}

/// Evaluate the rules in scope and **apply** what they propose (§9.4, §9.6).
///
/// The one verb that filters by `watch=`: a `--trigger core@home` names the write
/// that woke this run, and a rule evaluates only if it watches that core (§9.3). A
/// hand's bare `aus run` names no trigger, so every rule in scope evaluates — and a
/// TUI-open hook is a bare run too (§9.4), which is why a run with no trigger honestly
/// signs `manual`: it cannot tell the two apart, and the spec makes them the same.
fn cmd_run(cli: &Cli, scope: Option<&str>, trigger: Option<&str>) -> Result<Response> {
    let root = resolve_root(cli.root.as_deref())?;
    let at = scope.map(Code::parse).transpose()?;
    let rules = crate::discover(&root, at.as_ref())?;
    let now = today()?;
    let trigger = trigger.map(parse_trigger).transpose()?;
    let sign = if trigger.is_some() { "hook" } else { "manual" };
    // Learned once, not per proposal: the name→short map a proposal's `core` needs to
    // become a spawnable binary (§5.0). A core absent here is one a proposal cannot
    // reach, reported per proposal.
    let registry = pantheon::CoreRegistry::discover();

    let mut failed = false;
    let mut rows = Vec::new();
    for rule in &rules {
        if !watched(rule, trigger.as_ref()) {
            continue;
        }
        let context = context_for(rule, &now, sign, trigger.as_ref());
        let outcome = crate::rule::evaluate(
            &rule.path,
            &root,
            &context.to_string(),
            crate::rule::DEADLINE,
        );
        rows.push(applied_row(
            rule,
            outcome,
            &root,
            &now,
            &registry,
            &mut failed,
        ));
    }
    Ok(outcome_response(rows, failed))
}

/// Whether a rule evaluates for this wake (§9.3).
///
/// With no trigger, every rule in scope does — a wake that is not a single write has
/// nothing for `watch=` to filter against. With one, a rule evaluates iff its `watch`
/// set names the trigger's core, an empty `watch` meaning any.
fn watched(rule: &crate::Rule, trigger: Option<&Trigger>) -> bool {
    match trigger {
        None => true,
        Some(trigger) => {
            rule.header.watch.is_empty() || rule.header.watch.iter().any(|c| c == &trigger.core)
        }
    }
}

fn parse_trigger(raw: &str) -> Result<Trigger> {
    let (core, home) = raw
        .split_once('@')
        .ok_or_else(|| Error::usage(format!("--trigger is `core@home`, got {raw:?} (§9.3)")))?;
    if core.is_empty() || home.is_empty() {
        return Err(Error::usage(format!(
            "--trigger is `core@home`, got {raw:?} (§9.3)"
        )));
    }
    Ok(Trigger {
        core: core.to_string(),
        home: home.to_string(),
    })
}

/// One rule's whole outcome — evaluated, checked against its grant, deduped, applied.
///
/// The batch is the unit of trust. A malformed proposal, a grant it does not parse
/// against, an unauthorized write, or two proposals colliding on one key each reject
/// **the entire rule's batch** (§9.5): a rule that got one write wrong is not to be
/// trusted with the rest. Only once the whole batch passes does anything apply.
fn applied_row(
    rule: &crate::Rule,
    outcome: crate::rule::Outcome,
    root: &std::path::Path,
    now: &str,
    registry: &pantheon::CoreRegistry,
    failed: &mut bool,
) -> Value {
    let mut row = json!({ "rule": rule.name, "scope": rule.scope.as_str() });

    let writes = match outcome {
        crate::rule::Outcome::Failed(why) => {
            *failed = true;
            row["error"] = json!(why);
            return row;
        }
        crate::rule::Outcome::Proposed(writes) => writes,
    };

    // A proposal that is not a well-formed core call is a rule bug, and rejects the
    // batch — Auspex will not apply a half-understood set (§9.3).
    let proposals: std::result::Result<Vec<Proposal>, _> = writes
        .iter()
        .map(|v| serde_json::from_value::<Proposal>(v.clone()))
        .collect();
    let Ok(proposals) = proposals else {
        *failed = true;
        row["rejected"] = json!("a proposal is not a `core`/`verb`/`home` call (§9.3)");
        return row;
    };

    // The grant is the whole guard (§9.5). A grant Auspex cannot parse fails closed —
    // it rejects the batch rather than reading as an empty (deny-all) grant.
    let grant = match Grant::parse(&rule.header.writes) {
        Ok(grant) => grant,
        Err(why) => {
            *failed = true;
            row["rejected"] = json!(format!("its grant will not parse: {why}"));
            return row;
        }
    };

    // Step 2 — capability check. One unauthorized proposal rejects the batch.
    for proposal in &proposals {
        if let Err(why) = grant.authorize(proposal) {
            *failed = true;
            row["rejected"] = json!(why);
            return row;
        }
    }

    // Step 4 — two proposals on one key is a rule error, not an upsert (§9.5).
    if let Some(collision) = one_key_twice(&proposals, now) {
        *failed = true;
        row["rejected"] = json!(collision);
        return row;
    }

    // Step 5 — apply in order, with `-y` and `PANTHEON_NO_HOOKS=1` inside `apply`.
    let mut applied = Vec::new();
    let mut errors = Vec::new();
    for proposal in &proposals {
        let Some(short) = short_of(registry, &proposal.core) else {
            errors.push(format!(
                "{}: no core named {:?} is installed (§5.0)",
                proposal.label(),
                proposal.core
            ));
            continue;
        };
        match crate::apply::apply(proposal, short, root, now) {
            Ok(()) => applied.push(proposal.label()),
            Err(why) => errors.push(why),
        }
    }
    row["applied"] = json!(applied);
    if !errors.is_empty() {
        *failed = true;
        row["errors"] = json!(errors);
    }
    row
}

/// The binary short for a core's name, from what discovery learned (§5.0).
fn short_of<'a>(registry: &'a pantheon::CoreRegistry, core: &str) -> Option<&'a str> {
    registry
        .cores()
        .iter()
        .find(|c| c.name == core)
        .map(|c| c.short.as_str())
}

/// The first key two proposals in a batch would both land on, if any (§9.5 step 4).
///
/// A record's identity is its `core@home/series` plus the key Auspex derives — an
/// explicit `key`, or the slug a `name` normalizes to (§5.1), or `now` for a date-keyed
/// line. Two proposals sharing that would upsert one over the other with nothing to
/// show for the first, so the batch is refused instead.
fn one_key_twice(proposals: &[Proposal], now: &str) -> Option<String> {
    let mut seen = std::collections::HashSet::new();
    for proposal in proposals {
        let id = proposal
            .key
            .clone()
            .or_else(|| proposal.name.as_deref().and_then(pantheon::normalize))
            .unwrap_or_else(|| now.to_string());
        let series = proposal
            .series
            .as_deref()
            .map(|s| format!("/{s}"))
            .unwrap_or_default();
        let slot = format!("{}@{}{series}:{id}", proposal.core, proposal.home);
        if !seen.insert(slot.clone()) {
            return Some(format!(
                "two proposals land on {slot} — one would discard the other, so the \
                 batch is a rule error (§9.5)"
            ));
        }
    }
    None
}

/// The context handed to a rule on stdin (§9.3).
///
/// A `trigger` rides along only where one write authored the wake — §9.3 says it is
/// **absent** otherwise rather than present and empty, a wake that is not a write
/// staying silent instead of naming one that never happened. `plan` and `test` pass
/// `None`; only a triggered `run` passes `Some`.
fn context_for(rule: &crate::Rule, now: &str, sign: &str, trigger: Option<&Trigger>) -> Value {
    let mut ctx = json!({
        "sign": sign,
        "rule": rule.name,
        "scope": rule.scope.as_str(),
        "now": now,
    });
    if let Some(trigger) = trigger {
        ctx["trigger"] = json!({ "core": trigger.core, "home": trigger.home });
    }
    ctx
}

/// Today, as the `YYMMDD` a rule keys by.
///
/// **`now` is a date and stays one** (§9.3): that is what makes a rule idempotent, the
/// same proposal on the same day being the same key to upsert rather than stack. A
/// `now` carrying the wake's clock would mint a fresh key every hook and stack forever.
///
/// The spine's `today()` is private; `key_from_at(None)` is its one public door, and
/// the same one Pensum uses for a bare `--done`.
fn today() -> Result<String> {
    Ok(contract::key_from_at(None)?.to_string())
}

/// One rule's outcome as a row, flipping `failed` if it did not run.
fn row_for(rule: &crate::Rule, outcome: &crate::rule::Outcome, failed: &mut bool) -> Value {
    let mut row = json!({
        "rule": rule.name,
        "scope": rule.scope.as_str(),
    });
    match outcome {
        crate::rule::Outcome::Proposed(writes) => {
            // An empty `writes` is a real result — most wakes find nothing to say — so
            // it is present and empty rather than absent (§9.5 step 1).
            row["writes"] = Value::Array(writes.clone());
        }
        crate::rule::Outcome::Failed(why) => {
            *failed = true;
            row["error"] = json!(why);
        }
    }
    row
}

/// The rows, with the worst outcome folded into the exit code.
///
/// Every rule is reported either way — §9.5's "skipped and reported; others are
/// unaffected" is about not aborting the batch. But a command that met a broken rule
/// should not claim success, which is how `pan validate` and `pan resolve` both behave:
/// the findings ride in the JSON and the exit code carries the worst of them.
fn outcome_response(rows: Vec<Value>, failed: bool) -> Response {
    let value = Value::Array(rows);
    if failed {
        Response::JsonExit(value, 1)
    } else {
        Response::Json(value)
    }
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
