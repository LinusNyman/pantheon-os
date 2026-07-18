//! `pan` — the structural CLI over the spine (§5.5). Works one layer down from
//! `data`: codes, files, refs, node annotations. A bare `pan` prints help until its
//! TUI exists (§7.3, step 6). Every mutation confirms; every read follows the hand.

// The bin shares the spine's conventional pedantic allows (see pantheon/src/lib.rs).
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::too_many_lines)]

use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};

use pantheon::mint::NewSpec;
use pantheon::{
    Annotations, Code, CoreRegistry, Error, Ref, Result, build_tree, plan_new, read_annotations,
    resolve_all, resolve_code, resolve_root, set_annotations, validate,
};

mod render;

#[derive(Parser)]
#[command(
    name = "pan",
    version,
    about = "PantheonOS structural CLI — codes, files, refs, node annotations (§5.5).",
    disable_help_subcommand = true
)]
struct Cli {
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
    #[command(external_subcommand)]
    Lookup(Vec<String>),
}

/// What a command produced.
enum RunOk {
    /// A contract value rendered per the hand, exit `0`.
    Json(Value),
    /// A contract value rendered per the hand, with a specific exit code (§7.3).
    JsonExit(Value, u8),
    /// Raw text for a shell to consume (`cd`, `init`), exit `0`.
    Raw(String),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(RunOk::Json(value)) => {
            render::emit(&value, as_json(&cli));
            ExitCode::from(0)
        }
        Ok(RunOk::JsonExit(value, code)) => {
            render::emit(&value, as_json(&cli));
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
fn as_json(cli: &Cli) -> bool {
    match cli.format {
        Some(Format::Json) => true,
        Some(Format::Table) => false,
        None => !io::stdout().is_terminal(),
    }
}

fn run(cli: &Cli) -> Result<RunOk> {
    let Some(cmd) = &cli.cmd else {
        // A bare `pan` is the TUI; it prints help until the TUI lands (§7.3, step 6).
        return Ok(RunOk::Raw(
            "pan — PantheonOS structural CLI. The TUI lands at step 6; run `pan --help` for commands.\n"
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
        Cmd::Lookup(args) => cmd_lookup(cli, args),
        Cmd::Constitution { .. } => Err(not_implemented("constitution", "step 6")),
        Cmd::Doctor => Err(not_implemented("doctor", "step 2+, once cores exist")),
        Cmd::Migrate => Err(not_implemented(
            "migrate",
            "a later step; no prior format version yet",
        )),
        Cmd::MvFile { .. } => Err(not_implemented("mv-file", "step 3")),
        Cmd::Mv { .. } => Err(not_implemented("mv", "step 3")),
        Cmd::Rm { .. } => Err(not_implemented("rm", "step 3")),
        Cmd::Rename { .. } => Err(not_implemented("rename", "step 3")),
        Cmd::RenamePrefix { .. } => Err(not_implemented("rename-prefix", "step 3")),
        Cmd::RenamePattern { .. } => Err(not_implemented("rename-pattern", "step 3")),
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
    plan.apply(&root)?;
    Ok(RunOk::Json(json!({ "created": [node] })))
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

fn cmd_lookup(cli: &Cli, args: &[String]) -> Result<RunOk> {
    let root = resolve_root(cli.root.as_deref())?;
    let raw = args
        .first()
        .ok_or_else(|| Error::usage("expected a code"))?;
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

fn not_implemented(verb: &str, when: &str) -> Error {
    Error::runtime(format!("`pan {verb}` is not implemented yet ({when})"))
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
