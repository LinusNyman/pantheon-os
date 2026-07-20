//! `pan` — the structural CLI over the spine (§5.5). Works one layer down from
//! `data`: codes, files, refs, node annotations. A bare `pan` prints help until its
//! TUI exists (§7.3, step 6). Every mutation confirms; every read follows the hand.

// The bin shares the spine's conventional pedantic allows (see pantheon/src/lib.rs).
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::too_many_lines)]

use std::ffi::OsString;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};

use pantheon::mint::NewSpec;
use pantheon::{
    Annotations, Code, CoreRegistry, Error, Plan, Ref, Result, build_tree, plan_mv, plan_mv_file,
    plan_new, plan_rename, plan_rm, read_annotations, resolve_all, resolve_code, resolve_root,
    set_annotations, validate,
};

// The screen rides the `tui` feature; drop it and the structural CLI stands alone (§14).

#[derive(Parser)]
#[command(
    name = "pan",
    version,
    about = "PantheonOS structural CLI — codes, files, refs, node annotations (§5.5).",
    disable_help_subcommand = true
)]
pub(crate) struct Cli {
    /// The tree root; else $PANTHEON_ROOT, else a usage error (§6.2).
    #[arg(short = 'C', long = "root", global = true, value_name = "DIR")]
    root: Option<PathBuf>,
    /// Force output format; default follows the hand (TTY vs pipe, §7.3).
    #[arg(short = 'f', long = "format", global = true, value_enum)]
    format: Option<Format>,
    /// Apply a mutation without prompting (§7.3).
    #[arg(short = 'y', long = "yes", global = true)]
    yes: bool,
    /// Compute and print the plan without applying (§7.3).
    #[arg(short = 'n', long = "dry-run", global = true)]
    dry_run: bool,
    /// A plan token from a prior dry-run; honored on apply (§7.3).
    #[arg(short = 'p', long = "plan", global = true, value_name = "TOKEN")]
    plan: Option<String>,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Json,
    Table,
}

#[derive(Clone, Copy, ValueEnum)]
enum Shell {
    Bash,
    Zsh,
    Fish,
}

#[derive(Subcommand)]
enum Cmd {
    /// Mint a node: `new <parent> <char> <label>` or `new <parent> --def <def>` (§5.5).
    New {
        parent: String,
        ch: Option<String>,
        label: Option<String>,
        #[arg(long = "def", value_name = "DEFINITION")]
        def: Option<String>,
    },
    /// Emit the ontology (sub)tree as JSON (§5.5).
    Tree { code: Option<String> },
    /// Locate and dereference entity refs; accepts many, walks once (§5.5).
    Resolve { refs: Vec<String> },
    /// The tree's own lint, reported by path (§5.5).
    Validate,
    /// Emit a node's absolute path for a shell shim; bare lands at the root (§5.5).
    Cd { target: Option<String> },
    /// Emit a `pan` shell wrapper that performs the `cd` (§5.5).
    Init { shell: Shell },
    /// Read or edit a node's annotations (§5.2).
    Annotate {
        code: String,
        #[arg(long = "set", value_name = "KEY=VAL")]
        set: Vec<String>,
    },
    /// The seven placement rules plus a node's keywords (§5.5).
    Constitution { code: Option<String> },
    /// Installed apps, versions, and token collisions (§5.5).
    Doctor,
    /// Rewrite the tree from one format version to the next (§5.5).
    Migrate,
    /// Re-home one file to another node (§5.5).
    #[command(name = "mv-file")]
    MvFile {
        file: PathBuf,
        #[arg(long = "to")]
        to: String,
    },
    /// Re-home a node (§5.5).
    Mv {
        code: String,
        #[arg(long = "to")]
        to: String,
    },
    /// Remove a node (§5.5).
    Rm { code: String },
    /// Structural rename of a node (§5.5).
    Rename {
        code: String,
        #[arg(long = "char")]
        ch: Option<String>,
        #[arg(long)]
        label: Option<String>,
        #[arg(long = "def")]
        def: Option<String>,
    },
    /// Bulk code-prefix rewrite over a subtree (§5.5).
    #[command(name = "rename-prefix")]
    RenamePrefix {
        old: String,
        new: String,
        code: Option<String>,
    },
    /// Bulk literal substitution across labels, filenames, slugs (§5.5).
    #[command(name = "rename-pattern")]
    RenamePattern {
        from: String,
        to: String,
        code: Option<String>,
    },
    /// `pan <code>` — code ↔ path, label, symbol, keywords (§5.5).
    ///
    /// Hidden because a hand never types it: [`with_lookup_verb`] inserts it when the
    /// first word is not a verb, exactly as a core's pre-pass inserts `add` (§7.3).
    #[command(hide = true)]
    Lookup { code: String },
}

/// What a command produced.
pub(crate) enum RunOk {
    /// A contract value rendered per the hand, exit `0`.
    Json(Value),
    /// A contract value rendered per the hand, with a specific exit code (§7.3).
    JsonExit(Value, u8),
    /// Raw text for a shell to consume (`cd`, `init`), exit `0`.
    Raw(String),
}

/// `pan`'s verbs (§5.5). A closed reserved set, like a core's twelve: a verb wins over
/// a node code, which is the ambiguity rule §7.3 already states.
const VERBS: &[&str] = &[
    "tree",
    "resolve",
    "cd",
    "init",
    "constitution",
    "doctor",
    "migrate",
    "validate",
    "annotate",
    "new",
    "rename",
    "mv",
    "mv-file",
    "rm",
    "rename-prefix",
    "rename-pattern",
    "lookup",
    "help",
];

/// The global flags that take a separate value — what the verb scan steps over to find
/// the first *word*. (`--flag=value` needs no entry: it is one token.)
const VALUE_FLAGS: &[&str] = &["-C", "--root", "-f", "--format"];

/// Insert the implicit `lookup` verb where the first word is a code (§5.5, §7.3).
///
/// `pan <code>` is an implicit verb exactly as `add` is a core's, so it wants the same
/// treatment: a pre-pass rather than clap's `external_subcommand`, which swallowed the
/// **rest of the line** — flags included. `pan csa -f table` silently ignored `-f`,
/// because clap handed `["csa", "-f", "table"]` to the subcommand as opaque words and
/// the global flags were never parsed. A universal flag that is quietly dropped is
/// worse than one refused (§7.3).
pub(crate) fn with_lookup_verb(raw: impl Iterator<Item = OsString>) -> Vec<OsString> {
    let argv: Vec<OsString> = raw.collect();
    let mut at = 1;
    while let Some(token) = argv.get(at).map(|t| t.to_string_lossy().into_owned()) {
        if !token.starts_with('-') {
            break;
        }
        at += usize::from(VALUE_FLAGS.contains(&token.as_str())) + 1;
    }
    // Nothing but flags: `--help`/`--version`, or a bare short (the TUI, §7.3).
    let Some(word) = argv.get(at) else {
        return argv;
    };
    if VERBS.contains(&word.to_string_lossy().as_ref()) {
        return argv;
    }
    let mut argv = argv;
    argv.insert(at, OsString::from("lookup"));
    argv
}

/// Run `pan` exactly as the binary runs it (§7.3) — parse `argv`, dispatch, and return
/// the process's exit code. The bin is a shell over this and holds nothing of its own.
#[must_use]
pub fn run_cli() -> ExitCode {
    let cli = Cli::parse_from(with_lookup_verb(std::env::args_os()));
    match run(&cli) {
        Ok(RunOk::Json(value)) => {
            pantheon::contract::emit(&value, as_json(&cli));
            ExitCode::from(0)
        }
        Ok(RunOk::JsonExit(value, code)) => {
            pantheon::contract::emit(&value, as_json(&cli));
            ExitCode::from(code)
        }
        Ok(RunOk::Raw(text)) => {
            print!("{text}");
            ExitCode::from(0)
        }
        Err(e) => {
            eprintln!("{}", e.to_error_json());
            ExitCode::from(e.exit_code().as_u8())
        }
    }
}

/// Whether to emit JSON: `-f json`, or (no `-f`) a non-terminal stdout (§7.3).
/// `pan`'s surface, as JSON (§7.3).
///
/// A system tool carries its own structural set rather than a core's twelve (§5.5),
/// which is the shape of what it is and no licence for a core to grow one (§18).
fn help_json() -> Value {
    json!({
        "name": "pantheon",
        "short": "pan",
        "about": "the structure: codes, files, refs, node annotations (§5.5, §10)",
        "verbs": [
            "tree", "resolve", "cd", "init", "constitution", "doctor", "migrate",
            "validate", "annotate", "new", "rename", "mv", "mv-file", "rm",
            "rename-prefix", "rename-pattern",
        ],
        "bare": "opens the structural TUI at a terminal; emits this down a pipe",
    })
}

fn as_json(cli: &Cli) -> bool {
    match cli.format {
        Some(Format::Json) => true,
        Some(Format::Table) => false,
        None => !io::stdout().is_terminal(),
    }
}

pub(crate) fn run(cli: &Cli) -> Result<RunOk> {
    let Some(cmd) = &cli.cmd else {
        // A bare short opens the TUI at a terminal; **piped, it emits help as JSON**
        // (§7.3). `pan` had been the one exception — it answered a pipe with prose
        // where every core answers with the contract — and the TTY rule governs it too.
        if as_json(cli) {
            return Ok(RunOk::Json(help_json()));
        }
        #[cfg(feature = "tui")]
        {
            let root = resolve_root(cli.root.as_deref())?;
            crate::screen::open(&root).map_err(|e| Error::runtime(e.to_string()))?;
            return Ok(RunOk::Raw(String::new()));
        }
        // Headless: there is no screen to open, so help is the whole answer (§14).
        #[cfg(not(feature = "tui"))]
        return Ok(RunOk::Raw(
            "pan — PantheonOS structural CLI. Built without the `tui` feature; run \
             `pan --help` for commands.\n"
                .to_string(),
        ));
    };
    match cmd {
        Cmd::New {
            parent,
            ch,
            label,
            def,
        } => cmd_new(cli, parent, ch.as_deref(), label.as_deref(), def.as_deref()),
        Cmd::Tree { code } => cmd_tree(cli, code.as_deref()),
        Cmd::Resolve { refs } => cmd_resolve(cli, refs),
        Cmd::Validate => cmd_validate(cli),
        Cmd::Cd { target } => cmd_cd(cli, target.as_deref()),
        Cmd::Init { shell } => Ok(RunOk::Raw(init_wrapper(*shell))),
        // (Tree handled above with Option<&str>.)
        Cmd::Annotate { code, set } => cmd_annotate(cli, code, set),
        Cmd::Lookup { code } => cmd_lookup(cli, code),
        Cmd::Constitution { code } => cmd_constitution(cli, code.as_deref()),
        // Infallible: an app that is absent, errors, or answers with nonsense is simply
        // not installed, which is a finding rather than a failure (§5.0, §5.5).
        Cmd::Doctor => Ok(cmd_doctor()),
        Cmd::Migrate => Err(not_implemented(
            "migrate",
            "a later step; no prior format version yet",
        )),
        // Step 3 landed the *record*-level cascade (§5.4): renaming a record re-slugs
        // it and rewrites the `core:slug` refs pointing at it. These six are the
        // *node*-level one (§10.1), which is a different and larger job — a node's
        // code is its path, so a rename rewrites every child directory name and file
        // prefix under the branch, plus every rule header naming the code (§9.2).
        Cmd::MvFile { file, to } => cmd_mv_file(cli, file, to),
        Cmd::Mv { code, to } => cmd_mv(cli, code, to),
        Cmd::Rm { code } => cmd_rm(cli, code),
        Cmd::Rename {
            code,
            ch,
            label,
            def,
        } => cmd_rename(cli, code, ch.as_deref(), label.as_deref(), def.as_deref()),
        Cmd::RenamePrefix { .. } => Err(not_implemented("rename-prefix", NODE_CASCADE)),
        Cmd::RenamePattern { .. } => Err(not_implemented("rename-pattern", NODE_CASCADE)),
    }
}

fn cmd_new(
    cli: &Cli,
    parent: &str,
    ch: Option<&str>,
    label: Option<&str>,
    def: Option<&str>,
) -> Result<RunOk> {
    let root = resolve_root(cli.root.as_deref())?;
    let spec = match (def, ch, label) {
        (Some(definition), None, None) => NewSpec::Def { definition },
        (None, Some(ch), Some(label)) => NewSpec::Triple { ch, label },
        _ => {
            return Err(Error::usage(
                "usage: pan new <parent> <char> <label>  |  pan new <parent> --def <definition>",
            ));
        }
    };
    let (plan, node) = plan_new(&root, parent, spec)?;
    run_plan(cli, &root, &plan, json!({ "created": [node] }))
}

/// The shared structural-verb flow (§10.1, §7.3) — the ladder `cmd_new` first spelled
/// out, now run by every node mutator. `--dry-run` emits the plan and its token; no `-y`
/// off a terminal is the checkpoint (print the plan, exit 5); otherwise a supplied token
/// is checked against the freshly computed one (stale review → exit 3) and the plan is
/// applied, returning `applied`.
fn run_plan(cli: &Cli, root: &std::path::Path, plan: &Plan, applied: Value) -> Result<RunOk> {
    if cli.dry_run {
        return Ok(RunOk::Json(plan.to_json()));
    }
    let applying = cli.yes || (io::stdout().is_terminal() && confirm(&plan.to_json()));
    if !applying {
        // Not a terminal, no `-y`: the structural checkpoint — print the plan, exit 5.
        return Ok(RunOk::JsonExit(plan.to_json(), 5));
    }
    if let Some(token) = &cli.plan {
        plan.check_token(token)?;
    }
    plan.apply(root)?;
    Ok(RunOk::Json(applied))
}

/// `pan rm <code>` — remove an empty node (§10.1). Refused if the node holds anything but
/// its meta scaffold; the refusal is the spine's ([`plan_rm`]).
fn cmd_rm(cli: &Cli, code: &str) -> Result<RunOk> {
    let root = resolve_root(cli.root.as_deref())?;
    let code = Code::parse(code)?;
    let (plan, removed) = plan_rm(&root, &code)?;
    run_plan(cli, &root, &plan, json!({ "removed": [removed] }))
}

/// `pan rename <code> [--char C] [--label L]` — rename a node, cascading the code change
/// over its branch (§10.1). A definition-prefix node renames with `--def`.
fn cmd_rename(
    cli: &Cli,
    code: &str,
    ch: Option<&str>,
    label: Option<&str>,
    def: Option<&str>,
) -> Result<RunOk> {
    let root = resolve_root(cli.root.as_deref())?;
    let code = Code::parse(code)?;
    let (plan, record) = plan_rename(&root, &code, ch, label, def)?;
    run_plan(cli, &root, &plan, json!({ "renamed": [record] }))
}

/// `pan mv <code> --to <parent>` — re-home a node, cascading the code change (§10.1).
fn cmd_mv(cli: &Cli, code: &str, to: &str) -> Result<RunOk> {
    let root = resolve_root(cli.root.as_deref())?;
    let code = Code::parse(code)?;
    let (plan, record) = plan_mv(&root, &code, to)?;
    run_plan(cli, &root, &plan, json!({ "moved": [record] }))
}

/// `pan mv-file <file> --to <code>` — re-home one record/series/rule file (§10.1, §7.2).
fn cmd_mv_file(cli: &Cli, file: &std::path::Path, to: &str) -> Result<RunOk> {
    let root = resolve_root(cli.root.as_deref())?;
    let to = Code::parse(to)?;
    let (plan, record) = plan_mv_file(&root, file, &to)?;
    run_plan(cli, &root, &plan, json!({ "moved": [record] }))
}

fn cmd_tree(cli: &Cli, code: Option<&str>) -> Result<RunOk> {
    let root = resolve_root(cli.root.as_deref())?;
    let code = code.map(Code::parse).transpose()?;
    let tree = build_tree(&root, code.as_ref())?;
    Ok(RunOk::Json(tree.to_json()))
}

fn cmd_resolve(cli: &Cli, refs: &[String]) -> Result<RunOk> {
    let root = resolve_root(cli.root.as_deref())?;
    let reg = CoreRegistry::discover();
    let refs: Vec<Ref> = refs.iter().map(|r| Ref::parse(r)).collect::<Result<_>>()?;
    let outcomes = resolve_all(&root, &reg, &refs)?;
    let value = pantheon::resolve::outcomes_json(&outcomes);
    let code = resolve_exit_code(&outcomes);
    Ok(RunOk::JsonExit(value, code))
}

fn resolve_exit_code(outcomes: &[pantheon::RefOutcome]) -> u8 {
    use pantheon::RefOutcome::{Ambiguous, Unresolved};
    if outcomes.iter().any(|o| matches!(o, Unresolved(_))) {
        4
    } else if outcomes.iter().any(|o| matches!(o, Ambiguous(_))) {
        2
    } else {
        0
    }
}

fn cmd_validate(cli: &Cli) -> Result<RunOk> {
    let root = resolve_root(cli.root.as_deref())?;
    let reg = CoreRegistry::discover();
    let findings = validate(&root, &reg)?;
    let has_error = findings
        .iter()
        .any(|f| f.severity == pantheon::Severity::Error);
    let value = pantheon::validate::findings_json(&findings);
    Ok(RunOk::JsonExit(value, u8::from(has_error) * 3))
}

fn cmd_cd(cli: &Cli, target: Option<&str>) -> Result<RunOk> {
    let root = resolve_root(cli.root.as_deref())?;
    let Some(target) = target else {
        return Ok(RunOk::Raw(format!("{}\n", root.display())));
    };
    let path = if target.contains(':') {
        // A core:slug lands at the entity's current home node (§5.4).
        let reference = Ref::parse(target)?;
        let reg = CoreRegistry::discover();
        match resolve_all(&root, &reg, std::slice::from_ref(&reference))?
            .into_iter()
            .next()
        {
            Some(pantheon::RefOutcome::Resolved(r)) => resolve_code(&root, &r.home)?,
            _ => return Err(Error::not_found(format!("no home for {target:?}"))),
        }
    } else {
        resolve_code(&root, &Code::parse(target)?)?
    };
    Ok(RunOk::Raw(format!("{}\n", path.display())))
}

/// The seven placement rules (§2), plus a node's keywords where one is named (§5.5).
///
/// Emitted so **a human and an LLM file alike** (I8). The rules are the constitution of
/// *your* tree exactly as they are of the reference tree — there is no schema, and this
/// is what stands in for one.
///
/// The text is held here rather than read from the spec at runtime: nothing outside the
/// tree is Pantheon's to depend on (§18), and a doc file is not shipped beside a binary.
/// It is prose a hand reads, so it lives where the verb is.
const PLACEMENT_RULES: &[(&str, &str)] = &[
    (
        "home only",
        "One home per record; the path *is* the home (I3).",
    ),
    (
        "sort by what it is",
        "By essence, never by material or by where it surfaces — a \"media\" or \
         \"digital\" node sorts by format, which is a surface, not a kind.",
    ),
    (
        "states in being, change in doing",
        "A node for a being (person, place, thing, state) belongs in a being-branch, a \
         node for a doing in Actio; beings never nest under doings. This cuts the tree, \
         never the cores — a record still homes at what it is about.",
    ),
    (
        "fields, not nodes",
        "Closeness, role, motive, obligation, origin, format colour a record; they are \
         never branches.",
    ),
    (
        "relationships are edges",
        "An entity is filed once; membership, association and provenance are references \
         (I9). Nest only when X is part of the substance of Y; reference when X belongs \
         to, relates to, or came from Y — and never reproduce one branch's structure \
         inside another.",
    ),
    (
        "aboutness, not provenance",
        "A record homes at what it is about, not at the activity or context that \
         produced it; origin is an edge, reconstructed by query.",
    ),
    (
        "the reality test",
        "A node is real only if distinct things are filed there and it is reviewed apart \
         (I7); a blank sub-level is a finished answer, not a gap.",
    ),
];

fn cmd_constitution(cli: &Cli, code: Option<&str>) -> Result<RunOk> {
    let rules: Vec<Value> = PLACEMENT_RULES
        .iter()
        .enumerate()
        .map(|(i, (name, rule))| json!({ "n": i + 1, "name": name, "rule": rule }))
        .collect();

    // A node's keywords are what the constitution is *for* at that node: they are the
    // annotation written for an LLM to file by (§5.2, §6.6).
    let node = match code {
        None => Value::Null,
        Some(code) => {
            let root = resolve_root(cli.root.as_deref())?;
            let code = Code::parse(code)?;
            let ann = read_annotations(&root, &code).unwrap_or_default();
            json!({
                "code": code.as_str(),
                "keywords": ann.keywords,
                "explanation": ann.explanation,
            })
        }
    };

    Ok(RunOk::Json(json!({ "rules": rules, "node": node })))
}

fn cmd_annotate(cli: &Cli, code: &str, set: &[String]) -> Result<RunOk> {
    let root = resolve_root(cli.root.as_deref())?;
    let code = Code::parse(code)?;
    if set.is_empty() {
        return Ok(RunOk::Json(read_annotations(&root, &code)?.to_json()));
    }
    let mut pairs = Vec::new();
    for item in set {
        let (key, value) = item
            .split_once('=')
            .ok_or_else(|| Error::usage(format!("--set expects KEY=VAL, got {item:?}")))?;
        if !matches!(key, "symbol" | "keywords" | "deity" | "explanation") {
            return Err(Error::usage(format!(
                "unknown annotation key {key:?}; expected symbol|keywords|deity|explanation"
            )));
        }
        pairs.push((key.to_string(), value.to_string()));
    }
    set_annotations(&root, &code, &pairs)?;
    Ok(RunOk::Json(read_annotations(&root, &code)?.to_json()))
}

fn cmd_lookup(cli: &Cli, raw: &str) -> Result<RunOk> {
    let root = resolve_root(cli.root.as_deref())?;
    let code = Code::parse(raw)?;
    let path = resolve_code(&root, &code)?;
    let ann = read_annotations(&root, &code).unwrap_or_else(|_| Annotations::default());
    Ok(RunOk::Json(json!({
        "code": code.as_str(),
        "path": path.display().to_string(),
        "symbol": ann.symbol,
        "keywords": ann.keywords,
    })))
}

fn confirm(plan_json: &Value) -> bool {
    eprintln!(
        "{}",
        serde_json::to_string_pretty(plan_json).unwrap_or_else(|_| plan_json.to_string())
    );
    eprint!("apply this plan? [y/N] ");
    let _ = io::stderr().flush();
    let mut line = String::new();
    io::stdin().read_line(&mut line).is_ok() && matches!(line.trim(), "y" | "Y" | "yes")
}

/// What the six structural mutators are waiting on (§10.1). Named once, because a
/// core's refusal points a hand at `pan rename --def` and `pan mv` — those are the
/// permanently correct answers (§7.2), so the message they arrive at should say what
/// is actually missing rather than a step number that has since gone by.
const NODE_CASCADE: &str = "the node-level path cascade, §10.1";

fn not_implemented(verb: &str, when: &str) -> Error {
    Error::runtime(format!("`pan {verb}` is not implemented yet ({when})"))
}

// ── doctor: what is installed, and whether the file→core map is total (§5.5) ──

/// The twelve apps `doctor` probes (§7.3). Versions come off **every** app's
/// `version -f json`, a verb every layer has; the token map beside them comes off each
/// **core**'s `schema`. The split is what lets `doctor` see all twelve: a lens owns no
/// records and so declares no `schema` (§12), and reading versions off that surface
/// would blind the check to the very apps §15.5 makes it for.
const KNOWN_SHORTS: &[&str] = &[
    "alb", "map", "rat", "fas", "pen", "ann", "tab", "aus", "spe", "atr", "stu",
];

/// Installed apps, their crate and format versions, format mismatches (§15.5), and
/// any collision across two cores' declared tokens (§5.5).
///
/// **A clean run means the file→core map is total**: every token has exactly one
/// owning core and one shape, which is what lets resolution read a name without
/// importing a core (§5.0). A Document core is the case that makes that claim
/// non-trivial — it contributes no tokens at all, so its files reach it by extension
/// alone (§7.1), and the map is `extension ∪ token` rather than token alone.
fn cmd_doctor() -> RunOk {
    let registry = CoreRegistry::discover();

    // `pan` reports itself rather than asking PATH: it is the running binary, so
    // spawning a copy would answer for whichever `pan` is installed instead of this one.
    let mut apps = vec![json!({
        "short": "pan",
        "name": "pantheon",
        "version": env!("CARGO_PKG_VERSION"),
        "format_version": 1,
    })];
    let mut absent = Vec::new();
    let mut formats: std::collections::BTreeMap<u64, Vec<String>> =
        std::collections::BTreeMap::new();
    formats.entry(1).or_default().push("pan".to_string());

    for short in KNOWN_SHORTS {
        match probe_version(short) {
            Some(value) => {
                if let Some(format) = value.get("format_version").and_then(Value::as_u64) {
                    formats
                        .entry(format)
                        .or_default()
                        .push((*short).to_string());
                }
                apps.push(value);
            }
            None => absent.push(*short),
        }
    }

    // Tokens, and who owns them. A core declaring none is a Document core (§7.1) —
    // reported as such, since its emptiness is a declaration rather than a gap.
    let mut tokens = Vec::new();
    let mut document_cores = Vec::new();
    for core in registry.cores() {
        if core.kinds.is_empty() {
            document_cores.push(core.name.clone());
        }
        for (token, shape) in &core.kinds {
            tokens.push(json!({
                "token": token,
                "core": core.name,
                "shape": shape,
            }));
        }
    }
    tokens.sort_by(|a, b| a["token"].as_str().cmp(&b["token"].as_str()));

    let collisions: Vec<Value> = registry
        .token_collisions()
        .into_iter()
        .map(|(token, cores)| json!({ "token": token, "cores": cores }))
        .collect();

    // A format bump is a breaking change for every app and gets a migration (§15.5),
    // so a *disagreement* is what doctor flags; crate versions drift freely beneath it.
    let agreed = formats.len() <= 1;
    RunOk::Json(json!({
        "apps": apps,
        "absent": absent,
        "format": {
            "agreed": agreed,
            "versions": formats.iter().map(|(v, shorts)| json!({
                "format_version": v,
                "apps": shorts,
            })).collect::<Vec<_>>(),
        },
        "tokens": tokens,
        "document_cores": document_cores,
        "collisions": collisions,
        // The totality claim itself (§5.5), stated rather than left to be inferred.
        "map_total": collisions.is_empty() && document_cores.len() <= 1,
    }))
}

/// One app's `version -f json` over PATH (§7.3). An app that is absent, errors, or
/// emits unparseable JSON is simply not installed — the same tolerance
/// `CoreRegistry::discover` shows a missing core (§5.0).
fn probe_version(short: &str) -> Option<Value> {
    let out = std::process::Command::new(short)
        .args(["version", "-f", "json"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let mut value: Value = serde_json::from_slice(&out.stdout).ok()?;
    value["short"] = Value::String(short.to_string());
    Some(value)
}

fn init_wrapper(shell: Shell) -> String {
    match shell {
        Shell::Bash | Shell::Zsh => "\
pan() {
  if [ \"$1\" = cd ]; then
    shift
    local __pan_dir
    __pan_dir=\"$(command pan cd \"$@\")\" || return $?
    cd \"$__pan_dir\"
  else
    command pan \"$@\"
  fi
}
"
        .to_string(),
        Shell::Fish => "\
function pan
  if test \"$argv[1]\" = cd
    set -l __pan_dir (command pan cd $argv[2..-1])
    or return $status
    cd $__pan_dir
  else
    command pan $argv
  end
end
"
        .to_string(),
    }
}
