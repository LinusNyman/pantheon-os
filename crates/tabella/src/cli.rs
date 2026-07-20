//! `tab` — Tabella's CLI (§7). stdout is JSON when piped, a table on a TTY (§7.3).
//!
//! The bin owns only what is Tabella's own: its positionals (`[home] <name> [prose…]`)
//! and the flags its primitive needs (`--type`, `--tag`, `--ext`, `-e`). Everything
//! downstream — reading the hand, confirming a mutation, *finding* a home and a slug,
//! planning the rename cascade, shaping a record into the contract's JSON — is
//! `pantheon`, so every core produces that JSON the same way (I4) and no core reaches
//! into another's records (I5).
//!
//! Three things here are Tabella's alone, and each traces to the Document shape:
//! `-f raw` (the `cat` case, the only per-shape output format, §7.2); the refusal of
//! `-r`, `-k`, `-c`, and `-a`, none of which a document's envelope can use (§7.3);
//! and the editor form as `edit`'s *default* rather than a flagged case, since a
//! document is opened in place — it already *is* the text (§7.3, §8.7).

// This module shares the spine's conventional pedantic allows (see pantheon/src/lib.rs).
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::too_many_arguments)]

use std::ffi::OsString;
use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};

use crate::Tabella;
use pantheon::document::Document;
use pantheon::envelope::Ref;
use pantheon::validate::{Finding, FindingCode, Severity, findings_json};
use pantheon::{
    Checkpoint, Code, Core, DocExt, DocumentAddr, DocumentRef, Error, Frontmatter, RecordChange,
    Response, Result, Store, contract, resolve_root,
};

/// The twelve verbs (§7.3). A closed reserved set: a verb wins over a node code,
/// which is what makes `add` safe to leave implicit (the ambiguity rule, §7.3).
const VERBS: &[&str] = &[
    "add", "edit", "rename", "move", "mv", "rm", "list", "ls", "get", "series", "where", "schema",
    "help", "version",
];

/// What a headless build prints for a bare short (§14, §7.3).
#[cfg(not(feature = "tui"))]
const BARE: &str = "tab — Tabella (ego · meaning). Built without the `tui` feature; run `tab --help` for the verbs.\n";

#[derive(Parser)]
#[command(
    name = "tab",
    version,
    about = "Tabella — captured meaning: notes, quotes, principles, reflections (§8.7).",
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
    /// Compute and print the change without writing (§7.3).
    #[arg(short = 'n', long = "dry-run", global = true)]
    dry_run: bool,
    /// A plan token from a prior dry-run; honored on apply (§7.3).
    #[arg(short = 'p', long = "plan", global = true, value_name = "TOKEN")]
    plan: Option<String>,
    /// State the home explicitly (§7.3).
    #[arg(short = 'H', long = "home", global = true, value_name = "CODE")]
    home: Option<String>,
    /// Which extension a document is written under: `md` (default), `txt`, or `mdx`
    /// (§6.1). Only `add` creates a file, so only `add` takes it.
    #[arg(long = "ext", global = true, value_name = "EXT")]
    ext: Option<String>,
    /// Accepted and refused: Tabella declares no tokens, and that emptiness is what
    /// names it a Document core (§7.1).
    #[arg(short = 'k', long = "kind", global = true, value_name = "K")]
    kind: Option<String>,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

/// A Document core adds `raw` to the universal pair — the only per-shape output
/// format in the spec (§7.2).
#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Format {
    Json,
    Table,
    Raw,
}

#[derive(Subcommand)]
enum Cmd {
    /// Write a document (§8.7).
    ///
    /// Tokens are `[home] <name> [prose…]`: `tab ecv trip_idea "Two weeks in Rome."`
    /// · `tab trip_idea` (homed at $PWD). With no prose given and stdin on a pipe,
    /// the body is read from stdin. A document needs no prior container — the record
    /// `add` creates *is* the document (§18).
    Add {
        tokens: Vec<String>,
        /// The note-kind: a quote, a principle, a reflection, or any you define
        /// (§8.7). A frontmatter field, never a token.
        #[arg(long = "type", value_name = "T")]
        r#type: Option<String>,
        /// A tag; repeatable (§8.7).
        #[arg(long = "tag", value_name = "TAG")]
        tag: Vec<String>,
        /// Open the new document in the hand's own editor once written (§7.3).
        #[arg(short = 'e', long = "edit")]
        edit: bool,
        /// Accepted and refused: a document's frontmatter carries no refs (§6.1).
        #[arg(short = 'r', long = "ref", value_name = "REF")]
        refs: Vec<String>,
        /// Accepted and refused: Tabella keeps no series to mint (§7.1).
        #[arg(short = 'c', long = "create")]
        create: bool,
        /// Accepted and refused: a document has no key to date (§7.1).
        #[arg(short = 'a', long = "at", value_name = "WHEN")]
        at: Option<String>,
    },
    /// Change a document in place, by slug (§7.2).
    ///
    /// Given **no** flags this is the **editor form**, which for a document is the
    /// ordinary case rather than a special one: the file itself opens in
    /// `$VISUAL`/`$EDITOR`/`vi`, because it already *is* the text (§7.3, §8.7).
    /// Piped, it spawns nothing and prints `{"path":…}`. Given `--type`/`--tag` it is
    /// an ordinary mutation and confirms. `--type ""` clears the key.
    Edit {
        slug: String,
        #[arg(long = "type", value_name = "T")]
        r#type: Option<String>,
        #[arg(long = "tag", value_name = "TAG")]
        tag: Vec<String>,
        /// Accepted and refused: a document's frontmatter carries no refs (§6.1).
        #[arg(short = 'r', long = "ref", value_name = "REF")]
        refs: Vec<String>,
    },
    /// Rename a document and cascade every ref pointing at it (§7.2, §5.4).
    Rename { slug: String, new: String },
    /// Re-home a document to another node (§7.2) — a file `mv` between **node dirs**,
    /// not meta dirs, since a document lives loose in the open one (§6.1).
    #[command(alias = "mv")]
    Move {
        slug: String,
        #[arg(long = "to", value_name = "CODE")]
        to: String,
    },
    /// Remove a document by slug — irreversible (§7.2, §18).
    Rm { slug: String },
    /// Every document across the subtree, frontmatter only (§7.2).
    ///
    /// A fold never reads bodies (§7.1). Tabella declares no read flags of its own,
    /// so selecting on `type` or `tags` is the caller's, over the emitted JSON (I4).
    #[command(alias = "ls")]
    List,
    /// One document by slug — frontmatter and body (§7.2). `-f raw` emits the bare
    /// body, for a pager or `$EDITOR`.
    Get { slug: String },
    /// Accepted and refused: a document is one text file, not a collection (§7.1).
    Series { tokens: Vec<String> },
    /// Resolve a slug to its home code, by walking Tabella's own files (§7.3).
    Where { slug: String },
    /// Self-description: name, tokens and shapes, record schema, format version (§7.2).
    Schema,
    /// This tool's name, short, and version, as JSON (§7.3).
    Version,
    /// The verbs, as JSON (§7.3).
    Help,
}

/// Run `tab` exactly as the binary runs it (§7.3) — parse `argv`, dispatch, and
/// return the process's exit code. The bin is a shell over this and holds nothing of
/// its own.
#[must_use]
pub fn run_cli() -> ExitCode {
    let cli = Cli::parse_from(with_default_verb(std::env::args_os()));
    // `raw` is not a third rendering of a contract value — it is the **body**, and
    // only `get` has one (§7.2). So it says nothing about how a JSON value would
    // render and leaves that to the hand: it maps to `None`, not to `false`, or
    // `-f raw` down a pipe would be read as `table` and start pretty-printing.
    let as_json = contract::format_is_json(match cli.format {
        Some(Format::Json) => Some(true),
        Some(Format::Table) => Some(false),
        Some(Format::Raw) | None => None,
    });
    contract::dispatch(run(&cli, as_json), as_json)
}

/// The flags that take a separate value — what the verb scan must step over to find
/// the first *word* on the line. Only the globals appear here; `--type` and `--tag`
/// are per-verb and none of them may precede the verb anyway.
///
/// `-k`/`--kind` is listed despite Tabella declaring no tokens (§7.1): a flag that is
/// *accepted* must still be stepped over, or `tab -k x note` would read `x` as the
/// verb and insert `add` in the wrong place.
const VALUE_FLAGS: &[&str] = &[
    "-C", "--root", "-f", "--format", "-p", "--plan", "-H", "--home", "-k", "--kind", "--ext",
];

/// `add` is the default verb (§7.3): where the first word on the line is not one of
/// the reserved verbs, it begins an `add`. Verbs are a closed set and win over any
/// other reading of that token — the ambiguity rule (§7.3).
pub(crate) fn with_default_verb(raw: impl Iterator<Item = OsString>) -> Vec<OsString> {
    let mut argv: Vec<OsString> = raw.collect();
    // Step over leading flags (and their values) to find the first word.
    let mut at = 1;
    while at < argv.len() {
        let token = argv[at].to_string_lossy().into_owned();
        if token == "--" {
            at += 1;
            break;
        }
        if !token.starts_with('-') {
            break;
        }
        at += usize::from(VALUE_FLAGS.contains(&token.as_str())) + 1;
    }
    // Nothing but flags: a bare short (the TUI, §7.3) or `--help`/`--version`.
    let Some(word) = argv.get(at) else {
        return argv;
    };
    if VERBS.contains(&word.to_string_lossy().as_ref()) {
        return argv;
    }
    argv.insert(at, OsString::from("add"));
    argv
}

pub(crate) fn run(cli: &Cli, as_json: bool) -> Result<Response> {
    let Some(cmd) = &cli.cmd else {
        // A bare short opens the TUI at a terminal; piped, it emits help (§7.3) — a
        // screen has nothing to draw down a pipe.
        if as_json {
            return Ok(Response::Json(help_json()));
        }
        #[cfg(feature = "tui")]
        {
            let root = resolve_root(cli.root.as_deref())?;
            crate::screen::open(&root).map_err(|e| Error::runtime(e.to_string()))?;
            return Ok(Response::Raw(String::new()));
        }
        // Headless: there is no screen to open, so help is the whole answer (§14).
        #[cfg(not(feature = "tui"))]
        return Ok(Response::Raw(BARE.to_string()));
    };

    // Flags a Document core's shape cannot use are usage errors (exit `2`, §7.3).
    // Taken by clap and refused here, so each refusal wears the contract's error
    // envelope and says *why*, rather than clap's own "unexpected argument" (I4).
    if cli.kind.is_some() {
        return Err(Error::usage(
            "tabella declares no tokens, so -k names nothing: an empty `kinds()` is what \
             makes it a Document core, and why its filenames carry no `__` segment (§7.1)",
        ));
    }
    // `-f raw` is the bare body of one document (§7.2), so it is meaningful on `get`
    // alone: a fold reads no bodies, and a write emits a record rather than prose.
    if matches!(cli.format, Some(Format::Raw)) && !matches!(cmd, Cmd::Get { .. }) {
        return Err(Error::usage(
            "-f raw emits a document's bare body, which only `get` reads: a fold never \
             reads bodies and a write emits a record (§7.1, §7.2)",
        ));
    }
    // `--ext` names the file a write creates, and only `add` creates one. A rename
    // cannot change a file's extension, and there is no verb that converts one (§7.2).
    if cli.ext.is_some() && !matches!(cmd, Cmd::Add { .. }) {
        return Err(Error::usage(
            "--ext names the file `add` creates; no other verb changes an extension, \
             since the extension is the shape (§5.2, §7.2)",
        ));
    }

    match cmd {
        Cmd::Add {
            tokens,
            r#type,
            tag,
            edit,
            refs,
            create,
            at,
        } => {
            refuse_shapeless_flags(refs, *create, at.as_deref())?;
            cmd_add(cli, tokens, r#type.as_deref(), tag, *edit)
        }
        Cmd::Edit {
            slug,
            r#type,
            tag,
            refs,
        } => {
            refuse_shapeless_flags(refs, false, None)?;
            cmd_edit(cli, slug, r#type.as_deref(), tag)
        }
        Cmd::Rename { slug, new } => cmd_rename(cli, slug, new),
        Cmd::Move { slug, to } => cmd_move(cli, slug, to),
        Cmd::Rm { slug } => cmd_rm(cli, slug),
        Cmd::List => cmd_list(cli),
        Cmd::Get { slug } => cmd_get(cli, slug),
        Cmd::Series { .. } => Err(Error::usage(
            "tabella keeps no series: a document is one text file, read with `get` or \
             `list` (§7.1, §8.7)",
        )),
        Cmd::Where { slug } => cmd_where(cli, slug),
        Cmd::Schema => Ok(Response::Json(serde_json::to_value(pantheon::schema::<
            Tabella,
        >(1))?)),
        Cmd::Version => Ok(Response::Json(version_json())),
        Cmd::Help => Ok(Response::Json(help_json())),
    }
}

/// The three universal flags a document's envelope cannot use (§7.3). `-r` is the
/// shape's own refusal: the frontmatter carries `type` and `tags` and no `refs`, so
/// there is nothing for a reference to attach to (§6.1) — which is also why the
/// rename cascade skips documents entirely (§5.4).
fn refuse_shapeless_flags(refs: &[String], create: bool, at: Option<&str>) -> Result<()> {
    if !refs.is_empty() {
        return Err(Error::usage(
            "a document's frontmatter carries `type` and `tags` and no refs, so there is \
             nothing for -r to attach to; a note points by *living* at what it is about \
             (§6.1, §7.3, I3)",
        ));
    }
    if create {
        return Err(Error::usage(
            "tabella keeps no series, so -c mints nothing: the document `add` creates \
             *is* the record (§7.1, §18)",
        ));
    }
    if at.is_some() {
        return Err(Error::usage(
            "tabella keeps no series, so -a keys nothing: a document is not a sample, \
             and its name is its key (§5.4, §7.1)",
        ));
    }
    Ok(())
}

// ── the verbs ───────────────────────────────────────────────────────────────

fn cmd_add(
    cli: &Cli,
    tokens: &[String],
    r#type: Option<&str>,
    tags: &[String],
    open_editor: bool,
) -> Result<Response> {
    refuse_under_rule(cli, "add")?;
    let ctx = Ctx::open(cli)?;
    let ext = parse_ext(cli.ext.as_deref())?;
    let target = contract::resolve_document_target(
        &ctx.store,
        &contract::DocumentQuery {
            home: cli.home.as_deref(),
            positionals: tokens,
            pwd: None,
        },
    )?;

    if open_editor && !target.body.is_empty() {
        return Err(Error::usage(
            "-e opens the document in an editor, so it cannot also be given prose on the \
             command line: that names two sources for one buffer (§7.3)",
        ));
    }

    // Within a node the check is one `read_dir`, so it is hard: two extensions spell
    // two files but only one ref, which the filesystem permits and the ref namespace
    // does not — §5.4's kind trap in the extension dimension (§18).
    if let Some(held) = &target.existing
        && held.ext != ext
    {
        return Err(Error::validation(format!(
            "{} already holds {:?} as a .{}: two extensions spell two files but only one \
             `tabella:{}`, so the ref would be ambiguous (§5.4, §18)",
            target.home.as_str(),
            target.slug,
            held.ext,
            target.slug
        )));
    }

    // An overwrite keeps the file's own frontmatter TOML and line endings, so a hand's
    // comments and any key Tabella does not read survive it (§6.6, I6).
    let prior = match &target.existing {
        Some(held) => Some((held.clone(), ctx.store.read_document(held)?)),
        None => None,
    };
    let frontmatter = Frontmatter {
        r#type: r#type.map(ToOwned::to_owned),
        tags: tags.to_vec(),
    };
    Tabella::validate(&frontmatter)?;
    let document = Document {
        frontmatter,
        front_raw: prior.as_ref().and_then(|(_, d)| d.front_raw.clone()),
        body: body_text(&target.body)?,
        crlf: prior.as_ref().is_some_and(|(_, d)| d.crlf),
    };
    let addr = DocumentAddr {
        home: target.home.clone(),
        slug: target.slug.clone(),
        ext,
    };
    let after = addr_json(&addr, &document);

    // A fresh `add` runs free; landing on an existing document is an overwrite — a
    // mutation, shown and confirmed before it commits (§7.3, I1).
    match &prior {
        Some((held, before)) => {
            let change = change(
                "add",
                &addr,
                Some(contract::document_json(Tabella::NAME, held, before)),
                Some(after.clone()),
                None,
            );
            if let Some(pending) = review(cli, &change)? {
                return Ok(pending);
            }
        }
        // Every write verb takes `--dry-run` (§7.2), fresh or not.
        None if cli.dry_run => {
            let change = change("add", &addr, None, Some(after), None);
            return Ok(Response::Json(change.to_json()));
        }
        None => {}
    }

    let written = ctx.store.write_document(&addr, &document)?;
    // Across nodes the check is a walk, so it stays soft: the record itself goes to
    // stdout, the warning to stderr (§5.4, §18).
    warn_duplicates(&ctx, &written)?;
    if open_editor {
        return editor_form(&ctx, &written);
    }
    Ok(Response::Json(contract::document_json(
        Tabella::NAME,
        &written,
        &document,
    )))
}

fn cmd_edit(cli: &Cli, slug: &str, r#type: Option<&str>, tags: &[String]) -> Result<Response> {
    refuse_under_rule(cli, "edit")?;
    let ctx = Ctx::open(cli)?;
    let dref = ctx
        .store
        .locate_document(&normalize_slug(slug)?, ctx.scope().as_ref())?;

    // Given no frontmatter flag, `edit` *is* the editor form. For every other core
    // that form is the flagged case — a field named with no value — but a document
    // has one buffer and it is the whole file, so there is no field to name (§7.3).
    if r#type.is_none() && tags.is_empty() {
        return editor_form(&ctx, &dref);
    }

    let before = ctx.store.read_document(&dref)?;
    let mut document = before.clone();
    if let Some(t) = r#type {
        // `--type ""` clears the key; the fence keeps whatever else it holds.
        document.frontmatter.r#type = (!t.trim().is_empty()).then(|| t.to_string());
    }
    if !tags.is_empty() {
        document.frontmatter.tags = tags.to_vec();
    }
    Tabella::validate(&document.frontmatter)?;

    let addr = DocumentAddr {
        home: dref.home.clone(),
        slug: dref.slug.clone(),
        ext: dref.ext,
    };
    let change = change(
        "edit",
        &addr,
        Some(contract::document_json(Tabella::NAME, &dref, &before)),
        Some(addr_json(&addr, &document)),
        None,
    );
    if let Some(pending) = review(cli, &change)? {
        return Ok(pending);
    }
    ctx.store.write_document(&addr, &document)?;
    Ok(Response::Json(contract::document_json(
        Tabella::NAME,
        &dref,
        &document,
    )))
}

fn cmd_rename(cli: &Cli, slug: &str, new: &str) -> Result<Response> {
    refuse_under_rule(cli, "rename")?;
    let ctx = Ctx::open(cli)?;
    let dref = ctx
        .store
        .locate_document(&normalize_slug(slug)?, ctx.scope().as_ref())?;
    let document = ctx.store.read_document(&dref)?;
    let new = pantheon::name::normalize_token(new, "name")?;

    let from = Ref::parse(&format!("{}:{}", Tabella::NAME, dref.slug))?;
    let to = Ref::parse(&format!("{}:{new}", Tabella::NAME))?;

    // The occupied-slug refusal is made **here**, not by the cascade. `plan_cascade`
    // gates that check on the caller's own tokens, and Tabella declares none (§7.1) —
    // and it walks meta dirs, where no document lives (§5.2). Neither of its two gates
    // can see a document, so the check would silently never fire; this is it, tree-wide
    // and hard, in the cascade's own words (§7.2, §5.4).
    if let Some(held) = ctx.store.find_documents(None, Some(&new))?.first() {
        return Err(pantheon::occupied_slug(&to, &held.home));
    }
    // The cascade still runs: other cores' records may hold `tabella:<old>`, and
    // rewriting those is exactly what it is for (§5.4).
    let cascade = pantheon::plan_cascade(ctx.store.root(), &[], &from, &to)?;

    let addr = DocumentAddr {
        home: dref.home.clone(),
        slug: new.clone(),
        ext: dref.ext,
    };
    let change = change(
        "rename",
        &addr,
        Some(contract::document_json(Tabella::NAME, &dref, &document)),
        Some(addr_json(&addr, &document)),
        Some(cascade.to_json()),
    );
    if let Some(pending) = review(cli, &change)? {
        return Ok(pending);
    }

    // The record's own file moves first, so a crash mid-cascade leaves refs dangling
    // on the *old* slug — which `pan validate` reports naming exactly the files that
    // still need fixing (§5.4, §10.1).
    let moved = ctx.store.relocate_document(&dref, &addr)?;
    cascade.apply(ctx.store.root())?;
    Ok(Response::Json(json!({
        "renamed": { "from": dref.slug, "to": new },
        "cascade": cascade.to_json(),
        "record": contract::document_json(Tabella::NAME, &moved, &document),
    })))
}

fn cmd_move(cli: &Cli, slug: &str, to: &str) -> Result<Response> {
    refuse_under_rule(cli, "move")?;
    let ctx = Ctx::open(cli)?;
    let dref = ctx
        .store
        .locate_document(&normalize_slug(slug)?, ctx.scope().as_ref())?;
    let document = ctx.store.read_document(&dref)?;

    let addr = DocumentAddr {
        home: Code::parse(to)?,
        slug: dref.slug.clone(),
        ext: dref.ext,
    };
    if let Some(held) = ctx.store.document_slug_taken_at(&addr.home, &addr.slug)? {
        return Err(Error::validation(format!(
            "{} already holds {:?} as a .{} document (§5.4)",
            addr.home.as_str(),
            addr.slug,
            held.ext
        )));
    }

    let change = change(
        "move",
        &addr,
        Some(contract::document_json(Tabella::NAME, &dref, &document)),
        Some(addr_json(&addr, &document)),
        None,
    );
    if let Some(pending) = review(cli, &change)? {
        return Ok(pending);
    }
    // No ref changes: a ref carries no path, so it survives a re-home untouched (§5.4).
    // The filename's code prefix does change, since it is derived from the destination.
    let moved = ctx.store.relocate_document(&dref, &addr)?;
    Ok(Response::Json(json!({
        "moved": { "from": dref.home.as_str(), "to": addr.home.as_str() },
        "record": contract::document_json(Tabella::NAME, &moved, &document),
    })))
}

fn cmd_rm(cli: &Cli, slug: &str) -> Result<Response> {
    refuse_under_rule(cli, "rm")?;
    let ctx = Ctx::open(cli)?;
    let dref = ctx
        .store
        .locate_document(&normalize_slug(slug)?, ctx.scope().as_ref())?;
    let document = ctx.store.read_document(&dref)?;
    let addr = DocumentAddr {
        home: dref.home.clone(),
        slug: dref.slug.clone(),
        ext: dref.ext,
    };
    let change = change(
        "rm",
        &addr,
        Some(contract::document_json(Tabella::NAME, &dref, &document)),
        None,
        None,
    );
    if let Some(pending) = review(cli, &change)? {
        return Ok(pending);
    }
    ctx.store.remove_document(&dref)?;
    Ok(Response::Json(json!({ "deleted": dref.slug })))
}

fn cmd_list(cli: &Cli) -> Result<Response> {
    let ctx = Ctx::open(cli)?;
    let folded = ctx.store.fold_documents(ctx.locus().as_ref())?;
    Ok(Response::Json(contract::document_fold_json(
        Tabella::NAME,
        &folded,
    )))
}

fn cmd_get(cli: &Cli, slug: &str) -> Result<Response> {
    let ctx = Ctx::open(cli)?;
    let dref = ctx
        .store
        .locate_document(&normalize_slug(slug)?, ctx.scope().as_ref())?;
    let document = ctx.store.read_document(&dref)?;
    // The `cat` case (§7.2): the bare body, for a pager or `$EDITOR`. `dispatch`
    // prints it with no trailing newline added, so it is byte-for-byte the prose.
    if matches!(cli.format, Some(Format::Raw)) {
        return Ok(Response::Raw(document.body));
    }
    Ok(Response::Json(contract::document_json(
        Tabella::NAME,
        &dref,
        &document,
    )))
}

fn cmd_where(cli: &Cli, slug: &str) -> Result<Response> {
    let ctx = Ctx::open(cli)?;
    let dref = ctx
        .store
        .locate_document(&normalize_slug(slug)?, ctx.scope().as_ref())?;
    let mut out = identity(&dref);
    let rel = dref
        .path
        .strip_prefix(&ctx.root)
        .unwrap_or(&dref.path)
        .to_string_lossy()
        .into_owned();
    out["path"] = Value::String(rel);
    Ok(Response::Json(out))
}

// ── shared plumbing ─────────────────────────────────────────────────────────

struct Ctx {
    root: PathBuf,
    store: Store<Tabella>,
    /// The explicit `-H`, if any.
    home: Option<Code>,
}

impl Ctx {
    fn open(cli: &Cli) -> Result<Ctx> {
        let root = resolve_root(cli.root.as_deref())?;
        let store = Store::new(root.clone());
        let home = cli.home.as_deref().map(Code::parse).transpose()?;
        Ok(Ctx { root, store, home })
    }

    /// What a slug lookup is scoped to. `-H` narrows it; otherwise the whole tree,
    /// because a slug is unique **per core, not per node** (§5.4) — narrowing to $PWD
    /// would make `tab get trip_idea` mean different notes in different directories.
    fn scope(&self) -> Option<Code> {
        self.home.clone()
    }

    /// What a fold is scoped to. Unlike a lookup this *is* the locus: `cd
    /// e_c_corpus/ && tab ls` lists the notes filed there (§7.3). Outside the tree
    /// there is nothing to narrow by, so the fold spans the forest.
    fn locus(&self) -> Option<Code> {
        self.home
            .clone()
            .or_else(|| contract::code_at_path(&self.root, None).ok())
    }
}

/// The editor form (§7.3). A **document is opened in place** — it already *is* the
/// text (§8.7) — with none of the buffer-of-one-value indirection the other shapes
/// need, since there is no machine-owned JSON to keep out of a hand's hands (I6, §6.6).
///
/// The session is the review: it mints no plan token and needs no `-y`, because the
/// hand is already looking at the thing it is changing.
fn editor_form(ctx: &Ctx, dref: &DocumentRef) -> Result<Response> {
    // Piped, it spawns nothing and prints the file's path, by the same law that sends
    // a table to a TTY and JSON down a pipe: the LLM hand gets a path to open with its
    // own tools rather than a blocked process it cannot drive (I8).
    if !contract::stdout_is_terminal() {
        return Ok(Response::Json(
            json!({ "path": dref.path.display().to_string() }),
        ));
    }
    match contract::edit_file(&dref.path)? {
        // Text that comes back unchanged writes nothing (§7.3).
        contract::Edited::Unchanged => Ok(Response::Json(contract::document_json(
            Tabella::NAME,
            dref,
            &ctx.store.read_document(dref)?,
        ))),
        // Nothing is written back: the editor already saved the file, which is what
        // "in place" means. What remains is to re-read what came back — text that is
        // invalid exits `3` (§7.3), and the hand's own save stands, since §18 keeps no
        // prior copy to restore and restoring one would be the undo layer it forbids.
        // `pan validate` reports it until a hand fixes it at the source (I6, §5.5).
        contract::Edited::Changed(text) => {
            let document = pantheon::document::parse(&text)
                .map_err(|e| Error::validation(format!("{}: {e}", dref.path.display())))?;
            Tabella::validate(&document.frontmatter)?;
            Ok(Response::Json(contract::document_json(
                Tabella::NAME,
                dref,
                &document,
            )))
        }
    }
}

/// A document's body, from the positionals or — where none were given and stdin is a
/// pipe — from stdin, so `tab add ecv note < draft.md` works (I8). A trailing newline
/// is ensured; §6.6's blank line under the fence is `Document::to_text`'s.
fn body_text(positionals: &[String]) -> Result<String> {
    let mut text = positionals.join(" ");
    if text.is_empty() && !contract::stdin_is_terminal() {
        std::io::stdin()
            .read_to_string(&mut text)
            .map_err(|e| Error::runtime(format!("could not read the body from stdin: {e}")))?;
    }
    let text = text.trim_end();
    Ok(if text.is_empty() {
        String::new()
    } else {
        format!("{text}\n")
    })
}

/// `--ext`, checked against the small fixed set §6.1 admits. The payload is prose, not
/// a machine format, which is why the set is open at all — but it is still closed at
/// three, and classification rests on the extension alone (§5.2).
fn parse_ext(raw: Option<&str>) -> Result<DocExt> {
    let Some(raw) = raw else {
        return Ok(Tabella::DEFAULT_EXT);
    };
    DocExt::from_ext(raw.trim().trim_start_matches('.')).ok_or_else(|| {
        Error::usage(format!(
            "--ext takes one of {}; got {raw:?} (§6.1)",
            DocExt::ALL
                .iter()
                .map(|e| e.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    })
}

/// A slug held at another node is a *soft* finding: the check is a tree walk, which is
/// the cost the softness exists to avoid (§5.4, §18). The record still goes to stdout;
/// the warning rides stderr in the same shape `pan validate` emits, so a machine hand
/// reads one shape from both surfaces (I4, I8).
fn warn_duplicates(ctx: &Ctx, written: &DocumentRef) -> Result<()> {
    let elsewhere = ctx
        .store
        .duplicate_document_slugs_elsewhere(&written.home, &written.slug)?;
    if elsewhere.is_empty() {
        return Ok(());
    }
    let findings: Vec<Finding> = elsewhere
        .iter()
        .map(|other| Finding {
            code: FindingCode::DuplicateSlug,
            severity: Severity::Warning,
            rel_path: other
                .path
                .strip_prefix(&ctx.root)
                .unwrap_or(&other.path)
                .to_path_buf(),
            msg: format!(
                "{}:{} also names a record at {} — a ref meeting both lists them rather than \
                 guessing; a fuller name tells them apart (§5.4, §7.3)",
                Tabella::NAME,
                written.slug,
                other.home.as_str()
            ),
            // A cross-node duplicate is a genuine choice — which record takes the
            // fuller name is the hand's — so there is no single legal correction (§10.2).
            fix: None,
        })
        .collect();
    eprintln!("{}", findings_json(&findings));
    Ok(())
}

/// A write verb is refused outright under `PANTHEON_RULE=1` (exit `6`, §9.3): the one
/// reactive writer is Auspex, and a rule may not borrow a hand's authority (I2).
fn refuse_under_rule(cli: &Cli, verb: &str) -> Result<()> {
    // `--dry-run` computes without writing, so a rule may still plan (§7.3).
    if contract::under_rule() && !cli.dry_run {
        return Err(contract::refused_under_rule(verb));
    }
    Ok(())
}

/// Run one mutation past the confirm rule (§7.3). `Some` is a pending response the
/// caller returns as-is; `None` means go ahead and write.
fn review(cli: &Cli, change: &RecordChange) -> Result<Option<Response>> {
    match contract::checkpoint(change, cli.dry_run, cli.yes, cli.plan.as_deref())? {
        Checkpoint::DryRun(value) => Ok(Some(Response::Json(value))),
        Checkpoint::ConfirmRequired(value) => Ok(Some(Response::JsonExit(value, 5))),
        Checkpoint::Apply => Ok(None),
    }
}

fn change(
    verb: &'static str,
    addr: &DocumentAddr,
    before: Option<Value>,
    after: Option<Value>,
    cascade: Option<Value>,
) -> RecordChange {
    RecordChange {
        verb,
        core: Tabella::NAME.to_string(),
        home: addr.home.as_str().to_string(),
        // A Document core declares no tokens (§7.1), so there is no kind to name. The
        // empty string is what the resolver already registers a document with, and it
        // hashes into the plan token as honestly as any other value would.
        kind: String::new(),
        // Tabella keeps no series, so the change body names none (§7.1).
        series: None,
        // A document stores no key — its *name* is the key (§5.4, §18).
        key: addr.slug.clone(),
        before,
        after,
        cascade,
    }
}

/// The contract JSON for a document not yet on disk — an `add`'s result, or a
/// `rename`'s destination. The same shape [`contract::document_json`] emits.
fn addr_json(addr: &DocumentAddr, document: &Document) -> Value {
    json!({
        "core": Tabella::NAME,
        "home": addr.home.as_str(),
        "slug": addr.slug,
        "ext": addr.ext.as_str(),
        "type": document.frontmatter.r#type,
        "tags": document.frontmatter.tags,
        "body": document.body,
    })
}

/// No `kind` key, unlike every other core's: a Document core declares none (§7.1).
fn identity(dref: &DocumentRef) -> Value {
    json!({
        "core": Tabella::NAME,
        "home": dref.home.as_str(),
        "slug": dref.slug,
        "ext": dref.ext.as_str(),
    })
}

/// A slug given on the command line is a typed token, so it is normalized on the way
/// in — `tab get "Trip Idea"` finds `trip_idea` (§5.1).
fn normalize_slug(raw: &str) -> Result<String> {
    pantheon::name::normalize_token(raw, "name")
}

fn version_json() -> Value {
    json!({
        "name": Tabella::NAME,
        "short": "tab",
        "version": env!("CARGO_PKG_VERSION"),
        "format_version": 1,
    })
}

fn help_json() -> Value {
    json!({
        "name": Tabella::NAME,
        "short": "tab",
        "about": "captured meaning: notes, quotes, principles, reflections (§8.7)",
        "verbs": VERBS,
        // Present and empty, not absent: a machine reading `help` across cores reads
        // one shape (I4), and for a Document core empty *is* the answer (§7.1).
        "kinds": Vec::<&str>::new(),
        "exts": DocExt::ALL.iter().map(|e| e.as_str()).collect::<Vec<_>>(),
        "version": env!("CARGO_PKG_VERSION"),
    })
}
