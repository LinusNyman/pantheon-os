//! Node-level structural operations (§10.1): `rm`, `rename`, `mv`, `mv-file`, and the
//! bulk `rename-prefix` / `rename-pattern`. Each builds a [`Plan`](crate::plan::Plan) of
//! directory and file renames — a node's code *is* its path (§5.2), so changing a code
//! rewrites every descendant directory name and `[code]` filename prefix under the
//! branch. The plans are dry-run-first and non-atomic (§10.1): a crash is diagnosed from
//! the tree by `pan validate`, never from an in-flight log (§18).
//!
//! `pan new` (minting) lives in [`mint`](crate::mint); these are the operations over an
//! existing tree. The record-level *ref* cascade a definition-prefix node rename triggers
//! is [`cascade`](crate::cascade)'s, reused here.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::cascade::plan_cascade;
use crate::classify::{FileClass, classify};
use crate::code::{CharToken, Code};
use crate::core::CoreRegistry;
use crate::envelope::Ref;
use crate::mint::{normalize_char, prefix_shadows};
use crate::plan::{Change, Plan};
use crate::tree::{Node, TreeRoot, build_tree, child_node_names, resolve_node};
use crate::{Error, Result, name};

/// Plan the removal of a node (§10.1). **Refused** if the node holds anything but its
/// own meta scaffold — a child node, a record, a series, a document, a rule, or homed
/// bulk — because a node's contents are the last thing that should drop without a word
/// (§10.1). An empty node (only its `[code]__` meta dir, itself holding at most the
/// annotation `[code]__.toml`) is removed whole.
pub fn plan_rm(root: &Path, code: &Code) -> Result<(Plan, Value)> {
    let (_nn, path) = resolve_node(root, code)?;

    let occupants = node_occupants(&path, code)?;
    if !occupants.is_empty() {
        return Err(Error::validation(format!(
            "node {} is not empty — it holds {}; `rm` refuses a node with children, records, \
             documents, rules, or bulk (§10.1). Empty or move them first.",
            code.as_str(),
            occupants.join(", ")
        )));
    }

    let rel = rel_path(root, &path);
    let plan = Plan::new(
        "rm",
        vec![Change::Remove {
            rel_path: rel.clone(),
        }],
    );
    let removed = json!({ "code": code.as_str(), "path": rel.to_string_lossy() });
    Ok((plan, removed))
}

/// What a node holds beyond its meta scaffold — the names that make `rm` refuse. Empty
/// means removable. The meta dir itself and its annotation `[code]__.toml` are scaffold,
/// not contents, so they never appear here.
fn node_occupants(path: &Path, code: &Code) -> Result<Vec<String>> {
    let meta_name = format!("{}__", code.as_str());
    let annotation = format!("{}__.toml", code.as_str());
    let mut found = Vec::new();

    // Anything in the node dir other than its meta dir: a child node, a loose document,
    // or homed bulk.
    for entry in std::fs::read_dir(path)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if name != meta_name {
            found.push(name);
        }
    }
    // Anything in the meta dir other than the annotation: a record, series, or rule.
    let meta = path.join(&meta_name);
    if meta.is_dir() {
        for entry in std::fs::read_dir(&meta)? {
            let name = entry?.file_name().to_string_lossy().into_owned();
            if name != annotation {
                found.push(name);
            }
        }
    }
    found.sort();
    Ok(found)
}

/// A path relative to the tree root, for a [`Change`].
fn rel_path(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}

// ── rename & mv: the recode engine ───────────────────────────────────────────

/// `pan rename <code>` (§10.1). `--label` renames a triple node's label (no code change,
/// so nothing under it moves but the node dir itself); `--char` changes its defining char,
/// which **changes the code** and cascades over the whole branch. A definition-prefix
/// node has no char and no label slot — its rename is `--def`, which also re-slugs its
/// entity and is handled separately (it cascades `core:slug` refs, §10.1).
///
/// Returns the plan and a `{from, to}` record echoed on apply.
pub fn plan_rename(
    root: &Path,
    code: &Code,
    ch: Option<&str>,
    label: Option<&str>,
    def: Option<&str>,
) -> Result<(Plan, Value)> {
    let (nn, path) = resolve_node(root, code)?;

    if def.is_some() {
        // `--def` re-slugs the entity and cascades refs, which needs the registry to map
        // kind → core — so it is `plan_rename_def`'s, reached through `cmd_rename` (§10.1).
        return Err(Error::usage(
            "a definition-prefix rename (`--def`) goes through the registry-aware path (§10.1)",
        ));
    }
    let Some(current_ch) = nn.ch.clone() else {
        return Err(Error::usage(format!(
            "node {} is definition-prefix; rename it with --def, not --char/--label (§5.1)",
            code.as_str()
        )));
    };
    if ch.is_none() && label.is_none() {
        return Err(Error::usage(
            "usage: pan rename <code> [--char C] [--label L]",
        ));
    }

    let (parent_code, parent_path) = parent_of(root, code, &path);

    // The new char and label — each defaulting to the current one.
    let new_ch = match ch {
        Some(c) => char_token_from(&normalize_char(c)?),
        None => current_ch.clone(),
    };
    let new_label = match label {
        Some(l) => name::normalize_token(l, "label")?,
        None => nn.label.clone(),
    };

    // A no-op is *both* char and label unchanged — a label-only rename keeps the code.
    if new_ch == current_ch && new_label == nn.label {
        return Err(Error::validation(format!(
            "rename is a no-op: {} already has that char and label",
            code.as_str()
        )));
    }
    let new_code = child_code(parent_code.as_ref(), &new_ch);
    if new_ch != current_ch {
        refuse_collision(&parent_path, parent_code.as_ref(), &new_code, code)?;
    }

    let new_dirname = triple_dirname(parent_code.as_ref(), &new_ch, &new_label);
    let new_top_rel = rel_path(root, &parent_path).join(&new_dirname);
    let changes = plan_recode(root, code, &new_code, &new_top_rel)?;

    let plan = Plan::new("rename", changes);
    let record = json!({ "from": code.as_str(), "to": new_code.as_str() });
    Ok((plan, record))
}

/// `pan rename <code> --def <definition>` (§10.1, §5.4) — rename a **definition-prefix**
/// node's definition. The definition doubles as the entity's slug, so this both recodes
/// the branch (like any rename) *and* cascades every `core:slug` ref pointing at the
/// entity(ies) promoted here — the one node rename that touches refs.
///
/// The ref cascade covers records **outside** the renamed subtree, which is where a ref
/// to a person overwhelmingly comes from. A rare self-reference from within the subtree
/// (a sub-record pointing at its own ancestor entity) is left for `pan validate` to
/// report as a dangling ref — its path shifts under the recode, the same inconsistency a
/// crash mid-cascade leaves and that §10.1 diagnoses from the tree.
pub fn plan_rename_def(
    root: &Path,
    code: &Code,
    new_def: &str,
    registry: &CoreRegistry,
) -> Result<(Plan, Value)> {
    let (nn, path) = resolve_node(root, code)?;
    if nn.ch.is_some() {
        return Err(Error::usage(format!(
            "node {} is a triple node; rename it with --char/--label, not --def (§5.1)",
            code.as_str()
        )));
    }
    let old_def = nn.label.clone();
    let new_def = name::normalize_token(new_def, "definition")?;
    if new_def == old_def {
        return Err(Error::validation(format!(
            "rename is a no-op: {} already has that definition",
            code.as_str()
        )));
    }

    // The parent code is the old code minus its `_{old_def}` suffix (the def may itself
    // carry `_`, so this is the only sound split).
    let parent_code = code
        .as_str()
        .strip_suffix(&format!("_{old_def}"))
        .filter(|p| !p.is_empty())
        .map(Code::parse)
        .transpose()?;
    let new_code = match &parent_code {
        Some(p) => Code::parse(&format!("{}_{new_def}", p.as_str()))?,
        None => Code::parse(&new_def)?,
    };

    let parent_path = path.parent().unwrap_or(root).to_path_buf();
    refuse_collision(&parent_path, parent_code.as_ref(), &new_code, code)?;

    // 1. Recode the branch (dirs and files), exactly like any rename.
    let new_dirname = def_dirname(parent_code.as_ref(), &new_def);
    let new_top_rel = rel_path(root, &parent_path).join(&new_dirname);
    let mut changes = plan_recode(root, code, &new_code, &new_top_rel)?;

    // 2. Cascade the entity's refs — one cascade per distinct core hosting an
    //    entity-as-node file here (its slug is this node's definition, §5.2).
    let old_branch = rel_path(root, &path);
    let meta = path.join(format!("{}__", code.as_str()));
    for core_name in cores_hosting_entities(&meta, code, registry)? {
        let (Ok(from), Ok(to)) = (
            Ref::parse(&format!("{core_name}:{old_def}")),
            Ref::parse(&format!("{core_name}:{new_def}")),
        ) else {
            continue;
        };
        let own_kinds: Vec<&str> = registry
            .kinds_of(&core_name)
            .unwrap_or(&[])
            .iter()
            .map(|(k, _)| k.as_str())
            .collect();
        let cascade = plan_cascade(root, &own_kinds, &from, &to)?;
        for rewrite in &cascade.rewrites {
            // A within-branch record's path shifts under the recode; leave it to
            // `pan validate` rather than chase a moving target.
            if rewrite.rel_path.starts_with(&old_branch) {
                continue;
            }
            changes.push(Change::RewriteRefs {
                rel_path: rewrite.rel_path.clone(),
                is_series: rewrite.is_series,
                from: from.to_token(),
                to: to.to_token(),
            });
        }
    }

    let plan = Plan::new("rename", changes);
    let record = json!({ "from": code.as_str(), "to": new_code.as_str() });
    Ok((plan, record))
}

/// The distinct core names hosting an entity-as-node file (`[code]__[kind].json`) in a
/// definition-prefix node's meta dir — whose refs a `--def` rename must cascade.
fn cores_hosting_entities(
    meta: &Path,
    code: &Code,
    registry: &CoreRegistry,
) -> Result<Vec<String>> {
    let mut cores: Vec<String> = Vec::new();
    if meta.is_dir() {
        for entry in std::fs::read_dir(meta)? {
            let fname = entry?.file_name().to_string_lossy().into_owned();
            if let FileClass::EntityNode { kind, .. } = classify(&fname, false, code) {
                if let Some(core) = registry.core_of_kind(&kind) {
                    if !cores.contains(&core.name) {
                        cores.push(core.name.clone());
                    }
                }
            }
        }
    }
    Ok(cores)
}

/// `pan mv <code> --to <parent>` (§10.1) — re-home a node under a new parent. The node's
/// **code changes** (its parent prefix does), so the whole branch cascades; its char,
/// label, and definition are unchanged, so no ref cascade (a def-prefix node keeps its
/// slug on a move, §10.1).
pub fn plan_mv(root: &Path, code: &Code, to_parent: &str) -> Result<(Plan, Value)> {
    // Resolving validates the node exists (exit 4); `plan_recode` re-reads its subtree.
    let (nn, _path) = resolve_node(root, code)?;
    let (new_parent_code, new_parent_path) = resolve_dest_parent(root, to_parent)?;

    // No moving a node into itself or its own subtree.
    if let Some(dest) = &new_parent_code {
        if is_self_or_descendant(dest, code) {
            return Err(Error::validation(format!(
                "cannot move {} under {} — that is the node itself or its own descendant",
                code.as_str(),
                dest.as_str()
            )));
        }
    }
    // A triple node may not become a child of a definition-prefix node (§5.1).
    if nn.ch.is_some() && new_parent_code.as_ref().is_some_and(|p| !p.is_compact()) {
        return Err(Error::usage(
            "a triple node cannot be a child of a definition-prefix node (§5.1)",
        ));
    }

    let new_code = if let Some(ch) = &nn.ch {
        child_code(new_parent_code.as_ref(), ch)
    } else {
        // Def-prefix node: new code is `{new_parent}_{def}`.
        let s = match &new_parent_code {
            Some(p) => format!("{}_{}", p.as_str(), nn.label),
            None => nn.label.clone(),
        };
        Code::parse(&s)?
    };
    if new_code == *code {
        return Err(Error::validation(format!(
            "{} is already under {}",
            code.as_str(),
            to_parent
        )));
    }
    refuse_collision(&new_parent_path, new_parent_code.as_ref(), &new_code, code)?;

    let new_dirname = match &nn.ch {
        Some(ch) => triple_dirname(new_parent_code.as_ref(), ch, &nn.label),
        None => def_dirname(new_parent_code.as_ref(), &nn.label),
    };
    let new_top_rel = rel_path(root, &new_parent_path).join(&new_dirname);
    let changes = plan_recode(root, code, &new_code, &new_top_rel)?;

    let plan = Plan::new("mv", changes);
    let record = json!({ "from": code.as_str(), "to": new_code.as_str() });
    Ok((plan, record))
}

/// `pan mv-file <file> --to <code>` (§10.1, §7.2) — re-home one record, series, or rule
/// file to another node, rewriting its `[code]__` prefix to the target's. The file's
/// remainder (`kind__slug`, `kind__name.jsonl`, `function__name…`) is invariant, so this
/// is a single rename into the target's meta dir. A document (single-`_` name) is moved
/// by its own core, not here.
pub fn plan_mv_file(root: &Path, file: &Path, to_code: &Code) -> Result<(Plan, Value)> {
    let file_abs = if file.is_absolute() {
        file.to_path_buf()
    } else {
        root.join(file)
    };
    if !file_abs.is_file() {
        return Err(Error::not_found(format!("no file at {}", file.display())));
    }
    let basename = file_abs
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let Some((_old_code, rest)) = basename.split_once("__") else {
        return Err(Error::usage(format!(
            "mv-file re-homes a record, series, or rule (a `__`-named file); {basename:?} is not \
             one — a document is re-homed by its core (§7.2)"
        )));
    };

    let (_to_nn, to_path) = resolve_node(root, to_code)?;
    let to_meta = to_path.join(format!("{}__", to_code.as_str()));
    let new_basename = format!("{}__{rest}", to_code.as_str());
    let dest = to_meta.join(&new_basename);

    if dest == file_abs {
        return Err(Error::validation(format!(
            "{basename:?} is already at {} (§7.2)",
            to_code.as_str()
        )));
    }
    if dest.exists() {
        return Err(Error::validation(format!(
            "{} already holds a file named {new_basename:?} — re-home would overwrite it (§5.4)",
            to_code.as_str()
        )));
    }

    let mut changes = Vec::new();
    // The target meta dir is minted lazily on first write; create it if this is the first.
    if !to_meta.is_dir() {
        changes.push(Change::Mkdir {
            code: to_code.clone(),
            rel_path: rel_path(root, &to_meta),
        });
    }
    changes.push(Change::Rename {
        from: rel_path(root, &file_abs),
        to: rel_path(root, &dest),
    });

    let plan = Plan::new("mv-file", changes);
    let record = json!({
        "file": rel_path(root, &file_abs).to_string_lossy(),
        "to": rel_path(root, &dest).to_string_lossy(),
    });
    Ok((plan, record))
}

/// `pan rename-prefix <old> <new> [code]` (§10.1, §10.2) — rewrite a code prefix over a
/// subtree. A **repair**: where a crashed rename or a hand's `mkdir` left files carrying
/// the wrong `[code]` prefix (a `csa__`-named file stranded inside a `cso__/` meta dir),
/// this rewrites every directory and file name under the scope whose code prefix is `old`
/// to `new`. Unlike `rename`, it does *not* touch the scope node's own directory — it
/// fixes contents, not identity. Scope defaults to the whole tree.
///
/// A code-prefix hit would also cascade the rule headers naming it (§9.2); that is
/// deferred with Auspex (no header parser exists yet). Codes are not refs, so no
/// `core:slug` cascade — that is `rename-pattern`'s, for a *slug* hit.
pub fn plan_rename_prefix(
    root: &Path,
    old: &str,
    new: &str,
    scope: Option<&Code>,
) -> Result<(Plan, Value)> {
    if old == new {
        return Err(Error::usage(
            "rename-prefix: old and new prefixes are the same",
        ));
    }
    // Both must be legal codes — a prefix is a code (§5.1).
    Code::parse(old)?;
    Code::parse(new)?;

    let scope_path = match scope {
        Some(code) => resolve_node(root, code)?.1,
        None => root.to_path_buf(),
    };
    let scope_rel = rel_path(root, &scope_path);
    let mut changes = Vec::new();
    walk_prefix(&scope_path, &scope_rel, old, new, &mut changes)?;

    if changes.is_empty() {
        return Err(Error::not_found(format!(
            "no name under the scope carries the code prefix {old:?}"
        )));
    }
    let count = changes.len();
    let plan = Plan::new("rename-prefix", changes);
    let record = json!({ "old": old, "new": new, "renamed": count });
    Ok((plan, record))
}

/// Recursively rename every directory and file under `dir` whose name carries `old` as a
/// code prefix. Top-down, so a renamed directory's children use the new path (like
/// [`plan_recode`]). `dir_abs` is read for enumeration; `dir_rel` is where it lives after
/// any ancestor rename, for the `Change` from-paths.
fn walk_prefix(
    dir_abs: &Path,
    dir_rel: &Path,
    old: &str,
    new: &str,
    changes: &mut Vec<Change>,
) -> Result<()> {
    for entry in std::fs::read_dir(dir_abs)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_dir = entry.file_type()?.is_dir();
        let renamed = swap_code_prefix(&name, old, new);
        let child_rel = dir_rel.join(&renamed);
        if renamed != name {
            push_rename(changes, dir_rel.join(&name), child_rel.clone());
        }
        if is_dir {
            walk_prefix(&entry.path(), &child_rel, old, new, changes)?;
        }
    }
    Ok(())
}

/// Walk the branch rooted at `old_code` and emit a [`Change::Rename`] for every directory
/// and file whose name carries a code in the branch (§10.1). The renamed node's own dir
/// moves to `new_top_rel`; every descendant then follows mechanically — its code's
/// `old_code` prefix becomes `new_code`, and its dir and file names rebuild from that.
///
/// Emitted **top-down** so each `from` is valid when applied in order: an ancestor dir is
/// renamed before a descendant's `from` (which uses the already-renamed ancestor path plus
/// its own still-old name) is reached. A rename whose target equals its source (a name
/// that does not carry the changing code — e.g. under a label-only rename) is skipped.
fn plan_recode(
    root: &Path,
    old_code: &Code,
    new_code: &Code,
    new_top_rel: &Path,
) -> Result<Vec<Change>> {
    let TreeRoot::Subtree(node) = build_tree(root, Some(old_code))? else {
        return Err(Error::not_found(format!(
            "no node with code {}",
            old_code.as_str()
        )));
    };
    let mut changes = Vec::new();
    push_rename(
        &mut changes,
        rel_path(root, &node.path),
        new_top_rel.to_path_buf(),
    );
    recode_contents(&node, old_code, new_code, new_top_rel, &mut changes)?;
    Ok(changes)
}

/// The meta dir, its files, loose documents, and child node dirs of `node` — renamed to
/// carry `node`'s new code — then each child recursed. `node_new_rel` is where the node's
/// own dir already lives after the caller renamed it; the OLD absolute path (`node.path`)
/// is read to enumerate the current contents at plan time.
fn recode_contents(
    node: &Node,
    old_code: &Code,
    new_code: &Code,
    node_new_rel: &Path,
    changes: &mut Vec<Change>,
) -> Result<()> {
    let node_new_code = recode_code(&node.code, old_code, new_code)?;
    let old_meta = format!("{}__", node.code.as_str());
    let new_meta = format!("{}__", node_new_code.as_str());

    // The meta dir and each file inside whose code prefix changes.
    let meta_abs = node.path.join(&old_meta);
    if meta_abs.is_dir() {
        push_rename(
            changes,
            node_new_rel.join(&old_meta),
            node_new_rel.join(&new_meta),
        );
        for entry in std::fs::read_dir(&meta_abs)? {
            let fname = entry?.file_name().to_string_lossy().into_owned();
            let renamed = swap_code_prefix(&fname, node.code.as_str(), node_new_code.as_str());
            push_rename(
                changes,
                node_new_rel.join(&new_meta).join(&fname),
                node_new_rel.join(&new_meta).join(&renamed),
            );
        }
    }

    // Loose documents (and any code-prefixed loose file) in the open node dir.
    for entry in std::fs::read_dir(&node.path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            continue; // the meta dir and child node dirs are handled below / by recursion
        }
        let fname = entry.file_name().to_string_lossy().into_owned();
        let renamed = swap_code_prefix(&fname, node.code.as_str(), node_new_code.as_str());
        push_rename(
            changes,
            node_new_rel.join(&fname),
            node_new_rel.join(&renamed),
        );
    }

    // Child node dirs, recursively.
    for child in &node.children {
        let old_dirname = child
            .path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let new_dirname = match &child.ch {
            Some(ch) => triple_dirname(Some(&node_new_code), ch, &child.label),
            None => def_dirname(Some(&node_new_code), &child.label),
        };
        let child_new_rel = node_new_rel.join(&new_dirname);
        push_rename(
            changes,
            node_new_rel.join(&old_dirname),
            child_new_rel.clone(),
        );
        recode_contents(child, old_code, new_code, &child_new_rel, changes)?;
    }
    Ok(())
}

/// A subtree node's new code: its `old_code` prefix replaced by `new_code`, the tail
/// (its own remaining tokens) kept. `old_code` is always a string prefix of a node in its
/// own subtree, so this is exact.
fn recode_code(node_code: &Code, old_code: &Code, new_code: &Code) -> Result<Code> {
    let tail = node_code
        .as_str()
        .strip_prefix(old_code.as_str())
        .ok_or_else(|| {
            Error::validation(format!(
                "internal: {} is not under {}",
                node_code.as_str(),
                old_code.as_str()
            ))
        })?;
    Code::parse(&format!("{}{tail}", new_code.as_str()))
}

/// Swap a leading `old_code` prefix (at a `_` boundary) for `new_code` in a file or
/// directory name. Records/series/rules/meta use `__`, a document a single `_`; in both
/// the code is exactly the leading `old_code` followed by `_`. A name not beginning with
/// the code at a boundary (homed bulk, a stray file) is returned unchanged.
fn swap_code_prefix(name: &str, old_code: &str, new_code: &str) -> String {
    if let Some(rest) = name.strip_prefix(old_code) {
        if rest.starts_with('_') {
            return format!("{new_code}{rest}");
        }
    }
    name.to_string()
}

/// Push a rename, skipping a no-op (source == target) — a name that does not carry the
/// changing code, as under a label-only rename.
fn push_rename(changes: &mut Vec<Change>, from: PathBuf, to: PathBuf) {
    if from != to {
        changes.push(Change::Rename { from, to });
    }
}

// ── small naming helpers ─────────────────────────────────────────────────────

/// A triple node's directory name: `{parent}_{char}_{label}`, or `{char}_{label}` at the
/// root.
fn triple_dirname(parent: Option<&Code>, ch: &CharToken, label: &str) -> String {
    match parent {
        Some(p) => format!("{}_{}_{}", p.as_str(), ch.as_code_str(), label),
        None => format!("{}_{}", ch.as_code_str(), label),
    }
}

/// A definition-prefix node's directory name: `{parent}_{def}_` (trailing `_`), or
/// `{def}_` at the root.
fn def_dirname(parent: Option<&Code>, def: &str) -> String {
    match parent {
        Some(p) => format!("{}_{def}_", p.as_str()),
        None => format!("{def}_"),
    }
}

/// A triple child's full code from its parent's code and its char.
fn child_code(parent: Option<&Code>, ch: &CharToken) -> Code {
    let s = match parent {
        Some(p) => format!("{}{}", p.as_str(), ch.as_code_str()),
        None => ch.as_code_str(),
    };
    Code::parse(&s).expect("a parent code plus a normalized char is a valid code")
}

/// A `CharToken` from a normalized char string (`"a"` or `"01"`).
fn char_token_from(ch: &str) -> CharToken {
    if ch.len() == 2 && ch.bytes().all(|b| b.is_ascii_digit()) {
        CharToken::Numeric(ch.to_string())
    } else {
        CharToken::Alpha(ch.chars().next().unwrap_or('x'))
    }
}

/// Whether `dest` is `code` itself or a node in its subtree — the `mv`-into-own-subtree
/// guard. A compact code's descendants share its string prefix directly; a
/// definition-prefix code's descendants share it followed by a `_` boundary (a sibling
/// `c_seaside` must not read as under `c_sea`).
fn is_self_or_descendant(dest: &Code, code: &Code) -> bool {
    if dest == code {
        return true;
    }
    if code.is_compact() {
        dest.as_str().starts_with(code.as_str())
    } else {
        dest.as_str().starts_with(&format!("{}_", code.as_str()))
    }
}

/// The current parent's code (by string, for a triple node) and directory path.
fn parent_of(root: &Path, code: &Code, path: &Path) -> (Option<Code>, PathBuf) {
    let parent_path = path.parent().unwrap_or(root).to_path_buf();
    (code.parent_compact(), parent_path)
}

/// Resolve a `mv` destination parent (`"root"` re-homes to a sphere).
fn resolve_dest_parent(root: &Path, to_parent: &str) -> Result<(Option<Code>, PathBuf)> {
    if to_parent == "root" {
        return Ok((None, root.to_path_buf()));
    }
    let code = Code::parse(to_parent)?;
    let path = crate::tree::resolve_code(root, &code)?;
    Ok((Some(code), path))
}

/// Refuse a new code that collides with a sibling — excluding the node being renamed
/// itself (§5.3). Mirrors [`mint`](crate::mint)'s mint-time check.
fn refuse_collision(
    parent_path: &Path,
    parent_code: Option<&Code>,
    new_code: &Code,
    self_code: &Code,
) -> Result<()> {
    let nc = new_code.as_str();
    for sibling in child_node_names(parent_path, parent_code)? {
        let sc = sibling.code.as_str();
        if sibling.code == *self_code {
            continue; // the node moving/renaming does not collide with its old self
        }
        if sc == nc {
            return Err(Error::validation(format!(
                "code {nc:?} already exists at this parent (§5.3)"
            )));
        }
        if prefix_shadows(nc, sc) || prefix_shadows(sc, nc) {
            return Err(Error::validation(format!(
                "code {nc:?} collides with sibling {sc:?}: one prefix-shadows the other (§5.3)"
            )));
        }
    }
    Ok(())
}
