//! Node-level structural operations (§10.1): `rm`, `rename`, `mv`, `mv-file`, and the bulk
//! `rename-prefix` (a code-prefix repair) and `rename-pattern` (a literal substitution
//! across record slugs and series names, each hit cascading its refs). Each builds a
//! [`Plan`](crate::plan::Plan) of
//! directory and file renames — a node's code *is* its path (§5.2), so changing a code
//! rewrites every descendant directory name and `[code]` filename prefix under the
//! branch. The plans are dry-run-first and non-atomic (§10.1): a crash is diagnosed from
//! the tree by `pan validate`, never from an in-flight log (§18).
//!
//! `pan new` (minting) lives in [`mint`](crate::mint); these are the operations over an
//! existing tree. The record-level *ref* cascade a definition-prefix node rename triggers
//! is [`cascade`](crate::cascade)'s, reused here.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use unicode_normalization::UnicodeNormalization;

use crate::cascade::plan_cascade;
use crate::classify::{FileClass, RESERVED_KIND_FUNCTION, classify};
use crate::code::Code;
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
    let Some(current_ch) = nn.ch else {
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
        Some(c) => normalize_char(c)?,
        None => current_ch,
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
    let new_code = child_code(parent_code.as_ref(), new_ch);
    if new_ch != current_ch {
        refuse_collision(&parent_path, parent_code.as_ref(), &new_code, code)?;
    }

    // The label as it will be spelled on disk. A **typed** label is a fresh mint and so is
    // written in the tree's spelling (D8); a label nobody touched is carried through byte
    // for byte in the spelling the tree already holds, which is what keeps a bare `--char`
    // recode the pure prefix substitution `ass` → `asd` relied on.
    let disk_label = match label {
        Some(_) => name::fs_spelling(&new_label),
        None => new_label.clone(),
    };
    let new_dirname = triple_dirname(parent_code.as_ref(), new_ch, &disk_label);
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

    // 1. Recode the branch (dirs and files), exactly like any rename. The definition is
    //    decomposed for the *directory* only (D8) — `new_def` itself stays composed,
    //    because it is also this entity's slug and goes out in every `core:slug` ref the
    //    cascade below rewrites.
    let new_dirname = def_dirname(parent_code.as_ref(), &name::fs_spelling(&new_def));
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

    let new_code = if let Some(ch) = nn.ch {
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

    let new_dirname = match nn.ch {
        Some(ch) => triple_dirname(new_parent_code.as_ref(), ch, &nn.label),
        None => def_dirname(new_parent_code.as_ref(), &nn.label),
    };
    let new_top_rel = rel_path(root, &new_parent_path).join(&new_dirname);
    let changes = plan_recode(root, code, &new_code, &new_top_rel)?;

    let plan = Plan::new("mv", changes);
    let record = json!({ "from": code.as_str(), "to": new_code.as_str() });
    Ok((plan, record))
}

/// `pan merge <src> --into <dst>` (§10.1) — dissolve one node into another, recoding its
/// whole branch `src` → `dst`.
///
/// The verb `mv` cannot be. `mv` refuses a code collision (§5.3) and is right to: a silent
/// merge would be worse than a refusal. But that left **no verb that unions two branches**,
/// and a tree assembled from two trees is full of merges — the same node reached twice,
/// once live and once in an archive.
///
/// **The union key is the code, and it recurses.** A `src` child whose recoded code matches
/// a child already at `dst` is *merged into it*, never renamed onto it — which is the whole
/// difference from `mv`. Where the two spell the same code with different labels, **`dst`'s
/// directory survives** and the dropped label is reported rather than quietly lost: one
/// code is one node (§5.3), so one of the two names has to go and only the tree's existing
/// name is not a guess.
///
/// **Everything in the open directory moves**, homed bulk directories included — one
/// rename each, nothing descended into — because what the walk does not carry, the final
/// removal would destroy. And the removal is [`Change::RemoveEmptyDir`], which refuses a
/// directory that is not empty, so anything that arrived meanwhile stops the merge.
///
/// **A file landing on a file is refused, and every one of them is named** — that is
/// [`Plan::preflight`], unchanged, doing what it does for every other verb. Two annotation
/// files are the usual case, and they are a genuine decision: what a merged `[code]__.toml`
/// should say is not the tool's to invent.
///
/// Grants cascade as they do for `mv` — `src`'s codes really do change (§9.2). Refs do
/// not: a definition-prefix node keeps its slug on a move, so nothing points at what
/// changed (§10.1).
pub fn plan_merge(root: &Path, src: &Code, dst: &Code) -> Result<(Plan, Value)> {
    if src == dst {
        return Err(Error::validation(format!(
            "merge is a no-op: {} is already itself",
            src.as_str()
        )));
    }
    if is_self_or_descendant(dst, src) {
        return Err(Error::validation(format!(
            "cannot merge {} into {} — that is the node itself or its own descendant",
            src.as_str(),
            dst.as_str()
        )));
    }
    let (TreeRoot::Subtree(src_node), TreeRoot::Subtree(dst_node)) =
        (build_tree(root, Some(src))?, build_tree(root, Some(dst))?)
    else {
        return Err(Error::not_found(
            "merge takes two nodes, each named by its code",
        ));
    };

    let mut changes = Vec::new();
    let mut rules = Vec::new();
    let mut relabelled = Vec::new();
    let dst_rel = rel_path(root, &dst_node.path);
    merge_into(
        root,
        &src_node,
        dst,
        &dst_rel,
        Some(&dst_node),
        &mut changes,
        &mut rules,
        &mut relabelled,
    )?;
    changes.extend(header_cascade(root, src, dst, &rules)?);

    let record = json!({
        "from": src.as_str(),
        "to": dst.as_str(),
        "relabelled": relabelled,
    });
    Ok((Plan::new("merge", changes), record))
}

/// Dissolve `src` into the node at `dst_code`, whose directory is `dst_rel` and whose
/// already-present form (if the tree holds one) is `dst_existing`. Emits the moves and
/// then the removals of what it emptied; recurses where a child is a node both sides hold.
#[allow(clippy::too_many_arguments)]
fn merge_into(
    root: &Path,
    src: &Node,
    dst_code: &Code,
    dst_rel: &Path,
    dst_existing: Option<&Node>,
    changes: &mut Vec<Change>,
    rules: &mut Vec<(PathBuf, PathBuf)>,
    relabelled: &mut Vec<Value>,
) -> Result<()> {
    let src_rel = rel_path(root, &src.path);
    let src_meta_name = format!("{}__", src.code.as_str());
    let dst_meta_rel = dst_rel.join(format!("{}__", dst_code.as_str()));

    // 1. The meta dir: every record, series, rule and annotation in it, then the dir.
    let src_meta_abs = src.path.join(&src_meta_name);
    if src_meta_abs.is_dir() {
        let mut moves = Vec::new();
        for entry in std::fs::read_dir(&src_meta_abs)? {
            let entry = entry?;
            let fname = entry.file_name().to_string_lossy().into_owned();
            let renamed = swap_code_prefix(&fname, src.code.as_str(), dst_code.as_str());
            let lands_at = dst_meta_rel.join(&renamed);
            if is_rule_name(&fname) {
                rules.push((entry.path(), lands_at.clone()));
            }
            moves.push(Change::Rename {
                from: src_rel.join(&src_meta_name).join(&fname),
                to: lands_at,
            });
        }
        // Minted lazily on first write, so the destination may not have one yet — and
        // the mint has to precede the moves that land in it.
        if !moves.is_empty() && !root.join(&dst_meta_rel).is_dir() {
            changes.push(Change::Mkdir {
                code: dst_code.clone(),
                rel_path: dst_meta_rel.clone(),
            });
        }
        changes.append(&mut moves);
        changes.push(Change::RemoveEmptyDir {
            rel_path: src_rel.join(&src_meta_name),
        });
    }

    // 2. The open dir: loose documents, homed bulk, and bulk directories — everything
    //    that is not the meta dir or a child node, moved whole and never descended into.
    let child_paths: Vec<&Path> = src.children.iter().map(|c| c.path.as_path()).collect();
    for entry in std::fs::read_dir(&src.path)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == src_meta_name || child_paths.contains(&entry.path().as_path()) {
            continue;
        }
        changes.push(Change::Rename {
            from: src_rel.join(&name),
            to: dst_rel.join(swap_code_prefix(
                &name,
                src.code.as_str(),
                dst_code.as_str(),
            )),
        });
    }

    // 3. Child nodes: merged into their twin where the destination holds one, moved
    //    whole where it does not.
    for child in &src.children {
        let new_code = match child.ch {
            Some(ch) => {
                if !dst_code.is_compact() {
                    return Err(Error::usage(format!(
                        "{} is a triple node and cannot become a child of the \
                         definition-prefix node {} (§5.1)",
                        child.code.as_str(),
                        dst_code.as_str()
                    )));
                }
                child_code(Some(dst_code), ch)
            }
            None => Code::parse(&format!("{}_{}", dst_code.as_str(), child.label))?,
        };
        let twin = dst_existing.and_then(|d| d.children.iter().find(|c| c.code == new_code));
        if let Some(twin) = twin {
            if twin.label != child.label {
                // One code is one node (§5.3), so one of the two labels has to go. The
                // tree's own is kept and the other is named, never silently dropped.
                relabelled.push(json!({
                    "code": new_code.as_str(),
                    "kept": twin.label,
                    "dropped": child.label,
                }));
            }
            merge_into(
                root,
                child,
                &twin.code,
                &rel_path(root, &twin.path),
                Some(twin),
                changes,
                rules,
                relabelled,
            )?;
        } else {
            let new_dirname = match child.ch {
                Some(ch) => triple_dirname(Some(dst_code), ch, &child.label),
                None => def_dirname(Some(dst_code), &child.label),
            };
            let child_new_rel = dst_rel.join(&new_dirname);
            push_rename(changes, rel_path(root, &child.path), child_new_rel.clone());
            recode_contents(
                child,
                &child.code,
                &new_code,
                &child_new_rel,
                changes,
                rules,
            )?;
        }
    }

    // 4. What is left is an empty directory, and only because this plan emptied it.
    changes.push(Change::RemoveEmptyDir { rel_path: src_rel });
    Ok(())
}

/// `pan mv-file <file>… --to <code>` (§10.1, §7.2) — re-home files to another node,
/// rewriting the `[code]` prefix of each name that carries one. **Any** file: a record,
/// series or rule lands in the target's meta dir, a document or homed bulk loose in its
/// open node dir (§6.1, §6.5), and a name carrying no code at all (`IMG_1234.jpg`) moves
/// verbatim. Many sources build **one** plan — one token, one confirm — so a shell glob
/// re-homes a directory's worth of files in a single reviewed transaction.
///
/// It once refused everything without a `__` in its name, on the grounds that a document
/// is re-homed by its core. `tab move` does re-home a document, one slug at a time, and
/// it remains the record-level verb — it wakes Auspex, as a core's write does. This is the
/// structural half: it moves files, knows no core (I5), and is what a bulk migration has
/// to reach for, since nothing else moves a hundred thousand photos.
///
/// Where a name's code comes from is worth stating, because the two halves differ. A
/// `__`-named file **names its own code in its first segment**, and that is the prefix
/// replaced — including where it disagrees with the meta dir it was misfiled into, which
/// is the case §10.2 built this verb for. A document or a bulk file carries no `__`, so
/// its code is its *node's*, read off where it sits.
pub fn plan_mv_files(root: &Path, files: &[PathBuf], to_code: &Code) -> Result<(Plan, Value)> {
    if files.is_empty() {
        return Err(Error::usage("mv-file: name at least one file to re-home"));
    }
    let (_to_nn, to_path) = resolve_node(root, to_code)?;
    let to_meta = to_path.join(format!("{}__", to_code.as_str()));

    // One walk for every source: a loose file's home is the node whose open directory
    // holds it, and only the tree knows which that is.
    let tree = match build_tree(root, None)? {
        TreeRoot::Forest(nodes) => nodes,
        TreeRoot::Subtree(node) => vec![node],
    };

    let mut renames: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut moved = Vec::new();
    let mut needs_meta = false;

    for file in files {
        let file_abs = if file.is_absolute() {
            file.clone()
        } else {
            root.join(file)
        };
        if !file_abs.is_file() {
            return Err(Error::not_found(format!("no file at {}", file.display())));
        }
        let file_abs = in_root_spelling(root, file_abs);
        let basename = file_abs
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let here = home_code_of(&tree, file_abs.parent().unwrap_or(root));

        // What the file is decides both where it lands and whose code its name carries.
        // Every shape `classify` names carries the parsed code already, so nothing is
        // re-derived here (§5.2).
        let (into_meta, from_code) =
            match classify(&basename, false, here.as_ref().unwrap_or(to_code)) {
                FileClass::Annotation { code }
                | FileClass::Partitioned { code, .. }
                | FileClass::EntityNode { code, .. }
                | FileClass::NamedSeries { code, .. }
                | FileClass::DeterminedSeries { code, .. }
                | FileClass::Rule { code, .. } => (true, Some(code)),
                FileClass::Document { code, .. } => (false, Some(code)),
                // Homed bulk, or a name the tools own no shape for: its prefix, where it has
                // one, is the node's it sits at.
                _ => (false, here.clone()),
            };
        let new_basename = match &from_code {
            Some(from) => swap_code_prefix(&basename, from.as_str(), to_code.as_str()),
            None => basename.clone(),
        };
        let dest = if into_meta {
            to_meta.join(&new_basename)
        } else {
            to_path.join(&new_basename)
        };

        if dest == file_abs {
            return Err(Error::validation(format!(
                "{basename:?} is already at {} (§7.2)",
                to_code.as_str()
            )));
        }
        if dest.exists() {
            return Err(Error::validation(format!(
                "{} already holds a file named {new_basename:?} — re-home would overwrite it \
                 (§5.4)",
                to_code.as_str()
            )));
        }
        // Two sources landing on one name, which no amount of disk-checking would catch
        // because neither is there yet (§5.4).
        if let Some((other, _)) = renames.iter().find(|(_, to)| *to == dest) {
            return Err(Error::validation(format!(
                "{basename:?} and {:?} would both be re-homed to {new_basename:?} (§5.4)",
                other
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default()
            )));
        }
        needs_meta |= into_meta;
        renames.push((file_abs, dest));
    }

    let mut changes = Vec::new();
    // The target meta dir is minted lazily on first write; create it if this is the first.
    if needs_meta && !to_meta.is_dir() {
        changes.push(Change::Mkdir {
            code: to_code.clone(),
            rel_path: rel_path(root, &to_meta),
        });
    }
    for (from, to) in &renames {
        let (from, to) = (rel_path(root, from), rel_path(root, to));
        moved.push(json!({
            "file": from.to_string_lossy(),
            "to": to.to_string_lossy(),
        }));
        changes.push(Change::Rename { from, to });
    }

    Ok((Plan::new("mv-file", changes), Value::Array(moved)))
}

/// Re-express a path the hand named in the root's **own spelling**.
///
/// The same file has more than one absolute name — `/tmp/t/x` and `/private/tmp/t/x` name
/// one file on macOS — and a shell glob hands over whichever the working directory wore.
/// Two things break on the difference, both silently: the plan carries an absolute path
/// where every other change carries a root-relative one, and the directory holding the
/// file compares unequal to the node's, so the file's code goes unrecognized and its
/// prefix is never swapped. Canonicalizing *both* ends and rebuilding on `root` settles
/// it. A path outside the root, or one that will not canonicalize, is returned as it came.
fn in_root_spelling(root: &Path, path: PathBuf) -> PathBuf {
    let (Ok(root_real), Ok(path_real)) = (root.canonicalize(), path.canonicalize()) else {
        return path;
    };
    match path_real.strip_prefix(&root_real) {
        Ok(rel) => root.join(rel),
        Err(_) => path,
    }
}

/// The code of the node whose directory `dir` is (§5.2).
///
/// A **meta dir names its own code**, so it is read off the name — which is right for a
/// drifted one too: where a file sits is the whole of its scope (§9.1). An open node dir
/// is found in the tree by path. `None` where the directory belongs to no node, whose
/// files carry no code of the tree's to rewrite.
fn home_code_of(nodes: &[Node], dir: &Path) -> Option<Code> {
    let name = dir.file_name()?.to_string_lossy().into_owned();
    if let Some(code) = name.strip_suffix("__") {
        return Code::parse(code).ok();
    }
    for node in nodes {
        if node.path == dir {
            return Some(node.code.clone());
        }
        if let Some(found) = home_code_of(&node.children, dir) {
            return Some(found);
        }
    }
    None
}

/// `pan rename-prefix <old> <new> [code]` (§10.1, §10.2) — rewrite a code prefix over a
/// subtree. A **repair**: where a crashed rename or a hand's `mkdir` left files carrying
/// the wrong `[code]` prefix (a `csa__`-named file stranded inside a `cso__/` meta dir),
/// this rewrites every directory and file name under the scope whose code prefix is `old`
/// to `new`. Unlike `rename`, it does *not* touch the scope node's own directory — it
/// fixes contents, not identity. Scope defaults to the whole tree.
///
/// **A code-prefix hit cascades the rule headers naming it** (§10.2, §9.2), exactly as
/// `rename` and `mv` do: the walk renames child *directories* too, so a node's code really
/// does change and a `writes=core@home` grant naming it really is stale. Codes are not
/// refs, so no `core:slug` cascade — that is `rename-pattern`'s, for a *slug* hit.
///
/// **The walk is the node tree's, exactly as `rename`'s and `mv`'s are** — node dirs, their
/// meta dirs, and the loose files beside them (§6.3). It once recursed through every
/// directory it met, which put `.git`, `node_modules`, and `target` inside a project homed
/// at a node in scope: it renamed files in git object stores and build output, and (before
/// the pre-flight) clobbered whatever it landed on. A non-node directory now rides along
/// inside its parent untouched, which is what a bulk directory has always done under
/// `rename` and `mv` ([`recode_contents`]). Nothing here reads an ignore file to get that
/// — §13 and §18 leave no ignore file any say over the tree, and the tree bound needs
/// none. The cost is that a loose file in a non-node directory (or at the root) is out of
/// reach, which is the same constraint every other verb already carries.
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

    let mut changes = Vec::new();
    let mut rules = Vec::new();
    match build_tree(root, scope)? {
        // A scope node's own directory is never renamed — the repair fixes a node's
        // contents, not its identity — so the walk starts inside it.
        TreeRoot::Subtree(node) => {
            let node_rel = rel_path(root, &node.path);
            prefix_contents(&node, &node_rel, old, new, &mut changes, &mut rules)?;
        }
        // Tree-wide, the root stands in for the scope node and the spheres are its
        // contents, so a sphere's own dirname is fair game like any child node's.
        TreeRoot::Forest(nodes) => {
            for node in &nodes {
                let dirname = dir_name_of(node);
                let renamed = swap_prefix_run(&dirname, old, new);
                let node_rel = PathBuf::from(&renamed);
                push_rename(&mut changes, PathBuf::from(&dirname), node_rel.clone());
                prefix_contents(node, &node_rel, old, new, &mut changes, &mut rules)?;
            }
        }
    }

    if changes.is_empty() {
        return Err(Error::not_found(format!(
            "no name under the scope carries the code prefix {old:?}"
        )));
    }
    let count = changes.len();
    // The grants naming the codes this repair rewrote (§10.2, §9.2).
    changes.extend(header_cascade(
        root,
        &Code::parse(old)?,
        &Code::parse(new)?,
        &rules,
    )?);
    let plan = Plan::new("rename-prefix", changes);
    let record = json!({ "old": old, "new": new, "renamed": count });
    Ok((plan, record))
}

/// Rename every name *inside* one node whose code prefix carries `old` — its meta dirs
/// and their files, its loose files, its child node dirs — then recurse the children.
/// Top-down, so a renamed directory's children use the new path (like [`plan_recode`]).
/// `node_rel` is where the node lives after any ancestor rename, for the `Change`
/// from-paths; the node's own absolute path is read to enumerate its contents.
///
/// **A directory that is neither a meta dir, a child node, nor a name carrying the run is
/// passed over entirely** — homed bulk, a project's `.git`, a build tree — which is the
/// bound that keeps this walk the size of `rename`'s and `mv`'s (§6.3). What it now also
/// reaches is the **masked** directory: one carrying the dead prefix on its own name, which
/// the node walk cannot hand over because a child is read against its parent's code and the
/// drift *is* that the two disagree ([`cascade_masked`], D11).
fn prefix_contents(
    node: &Node,
    node_rel: &Path,
    old: &str,
    new: &str,
    changes: &mut Vec<Change>,
    rules: &mut Vec<(PathBuf, PathBuf)>,
) -> Result<()> {
    let child_paths: Vec<&Path> = node.children.iter().map(|c| c.path.as_path()).collect();
    for entry in std::fs::read_dir(&node.path)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !entry.file_type()?.is_dir() {
            // A loose document, or homed bulk beside it.
            push_rename(
                changes,
                node_rel.join(&name),
                node_rel.join(swap_prefix_run(&name, old, new)),
            );
            continue;
        }
        // A meta dir — the node's own, or a drifted one a crashed rename left behind,
        // which is exactly what this repair is for.
        if !name.ends_with("__") {
            if child_paths.contains(&entry.path().as_path()) {
                continue; // a child node: the recursion below, which knows its code
            }
            // A directory the parser cannot read as a node. Renamed if it carries the
            // run, and then walked — a masked directory hides a whole branch, not a name.
            let renamed = swap_prefix_run(&name, old, new);
            if renamed == name {
                continue;
            }
            let lands_at = node_rel.join(&renamed);
            push_rename(changes, node_rel.join(&name), lands_at.clone());
            cascade_masked(&entry.path(), &lands_at, old, new, changes)?;
            continue;
        }
        let meta_renamed = swap_prefix_run(&name, old, new);
        let meta_rel = node_rel.join(&meta_renamed);
        push_rename(changes, node_rel.join(&name), meta_rel.clone());
        for file in std::fs::read_dir(entry.path())? {
            let file = file?;
            let fname = file.file_name().to_string_lossy().into_owned();
            let renamed = swap_prefix_run(&fname, old, new);
            let lands_at = meta_rel.join(&renamed);
            push_rename(changes, meta_rel.join(&fname), lands_at.clone());
            // A **directory** inside a meta dir — D11's own three-defect example, and where
            // most of a recode's stranding turns out to live. It is walked whatever its own
            // name reads: a meta dir holds no bulk to mistake it for (§6.1), so the reason
            // to enter only what was renamed does not apply here, and a directory already
            // spelling the live code can still hold a whole branch spelling the dead one.
            if file.file_type()?.is_dir() {
                if !NOT_THE_TREES.contains(&fname.as_str()) {
                    cascade_masked(&file.path(), &lands_at, old, new, changes)?;
                }
                continue;
            }
            // A rule this repair moves, paired to where it lands (§9.2) — moved by its
            // own prefix or by its meta dir's, since a grant goes stale either way.
            // Recognized by the reserved kind segment rather than through `classify`,
            // which wants the *node's* code and this walk is precisely where a file's
            // prefix and its node disagree.
            if is_rule_name(&fname) && lands_at != node_rel.join(&name).join(&fname) {
                rules.push((file.path(), lands_at));
            }
        }
    }

    for child in &node.children {
        let dirname = dir_name_of(child);
        let child_rel = node_rel.join(swap_prefix_run(&dirname, old, new));
        push_rename(changes, node_rel.join(&dirname), child_rel.clone());
        prefix_contents(child, &child_rel, old, new, changes, rules)?;
    }
    Ok(())
}

/// Whether a filename wears the reserved `function` token in its kind slot — an Auspex
/// rule (§9.1). The kind is always the first `__`-segment after the code (§5.2).
fn is_rule_name(fname: &str) -> bool {
    fname
        .split_once("__")
        .and_then(|(_, rest)| rest.split("__").next())
        .is_some_and(|kind| kind == RESERVED_KIND_FUNCTION)
}

/// A node's own directory name.
fn dir_name_of(node: &Node) -> String {
    node.path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// `pan rename-pattern <from> <to> [code]` (§10.1, §5.4) — a literal substitution across
/// **record slugs and hand-named series names** in a scope, each hit cascading its
/// `core:slug` refs. The bulk typo fix: `rename-pattern johnn john` re-slugs every
/// `…__johnn.json` (and its refs) at once.
///
/// Scope is a record-identity substitution, so it touches files in meta dirs only — never
/// a node's own directory. A **node's** identity (a label, or a definition-prefix node's
/// definition, which *is* a slug) is one node's to rename: use `rename` / `rename --def`.
/// A code prefix is `rename-prefix`'s. Defaults to the whole tree.
pub fn plan_rename_pattern(
    root: &Path,
    from: &str,
    to: &str,
    scope: Option<&Code>,
    registry: &CoreRegistry,
) -> Result<(Plan, Value)> {
    if from.is_empty() {
        return Err(Error::usage("rename-pattern: the `from` literal is empty"));
    }
    if from == to {
        return Err(Error::usage("rename-pattern: `from` and `to` are the same"));
    }

    let nodes = match build_tree(root, scope)? {
        TreeRoot::Forest(nodes) => nodes,
        TreeRoot::Subtree(node) => vec![node],
    };

    // 1. Collect the record files whose slug/name carries the literal: the file rename,
    //    and (where the kind has an installed core) the identity ref to cascade.
    let mut hits: Vec<PatternHit> = Vec::new();
    for node in &nodes {
        collect_pattern_hits(root, node, from, to, registry, &mut hits)?;
    }
    if hits.is_empty() {
        return Err(Error::not_found(format!(
            "no record slug or series name under the scope carries {from:?}"
        )));
    }

    // 2. Refuse two hits colliding on one target file.
    for i in 0..hits.len() {
        for j in (i + 1)..hits.len() {
            if hits[i].new_rel == hits[j].new_rel {
                return Err(Error::validation(format!(
                    "rename-pattern would rename two records onto {} (§5.4)",
                    hits[i].new_rel.display()
                )));
            }
        }
    }

    // 3. File renames first; a map so a ref rewrite lands on a file this op also moved.
    let mut changes = Vec::new();
    let mut moved: HashMap<PathBuf, PathBuf> = HashMap::new();
    for hit in &hits {
        changes.push(Change::Rename {
            from: hit.old_rel.clone(),
            to: hit.new_rel.clone(),
        });
        moved.insert(hit.old_rel.clone(), hit.new_rel.clone());
    }

    // 4. One ref cascade per distinct (core, old-ident) — refs point at an identity, not a
    //    file, so identical (core, ident) hits share a walk. Remap a rewrite whose record
    //    this op also renamed to its new path.
    let mut done: Vec<(String, String)> = Vec::new();
    let mut ref_count = 0usize;
    for hit in &hits {
        let Some(core) = &hit.core else { continue };
        let key = (core.clone(), hit.old_ident.clone());
        if done.contains(&key) {
            continue;
        }
        done.push(key);

        let (Ok(from_ref), Ok(to_ref)) = (
            Ref::parse(&format!("{core}:{}", hit.old_ident)),
            Ref::parse(&format!("{core}:{}", hit.new_ident)),
        ) else {
            continue;
        };
        let own_kinds: Vec<&str> = registry
            .kinds_of(core)
            .unwrap_or(&[])
            .iter()
            .map(|(k, _)| k.as_str())
            .collect();
        let cascade = plan_cascade(root, &own_kinds, &from_ref, &to_ref)?;
        for rewrite in &cascade.rewrites {
            let rel = moved
                .get(&rewrite.rel_path)
                .cloned()
                .unwrap_or_else(|| rewrite.rel_path.clone());
            ref_count += rewrite.refs;
            changes.push(Change::RewriteRefs {
                rel_path: rel,
                is_series: rewrite.is_series,
                from: from_ref.to_token(),
                to: to_ref.to_token(),
            });
        }
    }

    let record = json!({ "from": from, "to": to, "renamed": hits.len(), "refs": ref_count });
    Ok((Plan::new("rename-pattern", changes), record))
}

/// One record whose slug or series name carries the pattern literal.
struct PatternHit {
    old_rel: PathBuf,
    new_rel: PathBuf,
    /// The owning core's name, where a kind maps to one — `None` means no installed core,
    /// so the file is renamed but nothing references it to cascade (§5.5).
    core: Option<String>,
    old_ident: String,
    new_ident: String,
}

/// Gather the pattern hits in one node's meta dir, then recurse its children. Only a
/// partitioned entity's slug and a hand-named series' name are record identities carried
/// in the filename (§5.2); an entity-as-node's slug is its *node's* definition and a
/// determined series has no name of its own, so neither is a pattern hit here.
fn collect_pattern_hits(
    root: &Path,
    node: &Node,
    from: &str,
    to: &str,
    registry: &CoreRegistry,
    hits: &mut Vec<PatternHit>,
) -> Result<()> {
    let meta = node.path.join(format!("{}__", node.code.as_str()));
    if meta.is_dir() {
        for entry in std::fs::read_dir(&meta)? {
            let fname = entry?.file_name().to_string_lossy().into_owned();
            let (kind, ident, is_series) = match classify(&fname, false, &node.code) {
                FileClass::Partitioned { kind, slug, .. } => (kind, slug, false),
                FileClass::NamedSeries { kind, name, .. } => (kind, name, true),
                _ => continue,
            };
            if !ident.contains(from) {
                continue;
            }
            let new_ident = ident.replace(from, to);
            let ext = if is_series { "jsonl" } else { "json" };
            let new_fname = format!("{}__{kind}__{new_ident}.{ext}", node.code.as_str());
            let core = registry.core_of_kind(&kind).map(|c| c.name.clone());
            hits.push(PatternHit {
                old_rel: rel_path(root, &meta.join(&fname)),
                new_rel: rel_path(root, &meta.join(&new_fname)),
                core,
                old_ident: ident,
                new_ident,
            });
        }
    }
    for child in &node.children {
        collect_pattern_hits(root, child, from, to, registry, hits)?;
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
    let mut rules = Vec::new();
    recode_contents(
        &node,
        old_code,
        new_code,
        new_top_rel,
        &mut changes,
        &mut rules,
    )?;
    // A label-only rename moves a directory and no code, so no grant went stale.
    if old_code.as_str() != new_code.as_str() {
        changes.extend(header_cascade(root, old_code, new_code, &rules)?);
    }
    Ok(changes)
}

/// The rule headers a recode invalidates (§9.2, §10.1) — the grant cascade, the twin of
/// the ref cascade one layer down.
///
/// A `writes=core@home` grant names a **node**, and a recode renames nodes; a grant left
/// pointing at a code that no longer exists is not merely stale but *silently wrong*, and
/// `writes=` is the whole guard (§9.5). So every rule in the tree is read — a rule may
/// grant writes at any node, not only the one it sits at (§9.1 scopes where it *runs*, not
/// where it may write) — and one whose header names the branch is rewritten.
///
/// `moving` carries the rules this operation is itself relocating, paired to where they
/// will be: a change must name the file's path *after* the renames, since that is when it
/// runs. Every other rule in the tree is read where it already sits.
fn header_cascade(
    root: &Path,
    old_code: &Code,
    new_code: &Code,
    moving: &[(PathBuf, PathBuf)],
) -> Result<Vec<Change>> {
    let mut changes = Vec::new();
    let mut push_if_stale = |read_from: &Path, rel: PathBuf| {
        let Ok(text) = std::fs::read_to_string(read_from) else {
            return; // not text is not a header; §9.2 leaves it to `aus` to report
        };
        if crate::rule::rewrite_writes_homes(&text, old_code, new_code).is_some() {
            changes.push(Change::RewriteHeader {
                rel_path: rel,
                from: old_code.clone(),
                to: new_code.clone(),
            });
        }
    };

    for (old_abs, new_rel) in moving {
        push_if_stale(old_abs, new_rel.clone());
    }
    // The rest of the tree, whose rule files this op leaves where they are. Filtered by
    // the paths already handled above rather than by directory: `rename-prefix` may be
    // scoped at the root, where "outside the branch" would exclude nothing and every
    // moving rule would be rewritten twice — the second time at a path that no longer
    // exists by the time the plan reaches it.
    let handled: Vec<&PathBuf> = moving.iter().map(|(old, _)| old).collect();
    let tops = match build_tree(root, None)? {
        TreeRoot::Forest(nodes) => nodes,
        TreeRoot::Subtree(node) => vec![node],
    };
    let mut found = Vec::new();
    for node in &tops {
        collect_rule_files(node, &mut found);
    }
    for abs in found {
        if handled.iter().any(|done| **done == abs) {
            continue;
        }
        let rel = rel_path(root, &abs);
        push_if_stale(&abs, rel);
    }
    Ok(changes)
}

/// Every rule file at or under `node` (§9.1).
///
/// This is the second walk in the workspace looking for [`FileClass::Rule`] — no `Store`
/// walk could ever yield one, since a rule belongs to no core's token set — so it is
/// built the same way Auspex's is: the tree walk plus a per-node meta-dir `read_dir`.
fn collect_rule_files(node: &Node, out: &mut Vec<PathBuf>) {
    let meta = node.path.join(format!("{}__", node.code.as_str()));
    if let Ok(entries) = std::fs::read_dir(&meta) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if matches!(classify(&name, false, &node.code), FileClass::Rule { .. }) {
                out.push(entry.path());
            }
        }
    }
    for child in &node.children {
        collect_rule_files(child, out);
    }
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
    rules: &mut Vec<(PathBuf, PathBuf)>,
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
            let entry = entry?;
            let fname = entry.file_name().to_string_lossy().into_owned();
            let renamed = recode_name(&fname, &node.code, &node_new_code, old_code, new_code);
            let lands_at = node_new_rel.join(&new_meta).join(&renamed);
            push_rename(
                changes,
                node_new_rel.join(&new_meta).join(&fname),
                lands_at.clone(),
            );
            // A **directory** inside a meta dir. §6.1 gives a meta dir records, series and
            // rules and nothing else, so the walk never looked inside one — and this is
            // where the bulk of a recode's stranding turned out to sit: whole branches
            // filed under a meta dir, every name in them carrying the dead code (D11).
            if entry.file_type()?.is_dir() {
                if !NOT_THE_TREES.contains(&fname.as_str()) {
                    cascade_masked(
                        &entry.path(),
                        &lands_at,
                        old_code.as_str(),
                        new_code.as_str(),
                        changes,
                    )?;
                }
                continue;
            }
            // A rule the recode moves: remembered with where it lands, so the grant
            // cascade can name the path the rewrite will actually find (§9.2, §10.1).
            if matches!(classify(&fname, false, &node.code), FileClass::Rule { .. }) {
                rules.push((meta_abs.join(&fname), lands_at));
            }
        }
    }

    // The open node dir: loose documents and homed bulk files, and the directories the
    // node walk cannot address (D11). A child directory whose name takes a char §5.1
    // rejects is no node's, so a recode passed it over as bulk — and it carried the dead
    // code on its own name and on every name beneath it. One `mv` stranded 320 entries
    // that way (§3.13). The meta dir and the real child nodes are handled above and by the
    // recursion below, which know their codes exactly.
    let child_paths: Vec<&Path> = node.children.iter().map(|c| c.path.as_path()).collect();
    for entry in std::fs::read_dir(&node.path)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_dir = entry.file_type()?.is_dir();
        if is_dir && (name == old_meta || child_paths.contains(&entry.path().as_path())) {
            continue;
        }
        let renamed = recode_name(&name, &node.code, &node_new_code, old_code, new_code);
        let lands_at = node_new_rel.join(&renamed);
        push_rename(changes, node_new_rel.join(&name), lands_at.clone());
        // Entered only where it was renamed: a directory carrying the branch's prefix is
        // the tree's and hides a whole branch, one that does not is bulk and rides along
        // inside its parent as it always has (§6.3).
        if is_dir && renamed != name {
            cascade_masked(
                &entry.path(),
                &lands_at,
                old_code.as_str(),
                new_code.as_str(),
                changes,
            )?;
        }
    }

    // Child node dirs, recursively.
    for child in &node.children {
        let old_dirname = dir_name_of(child);
        let new_dirname = match child.ch {
            Some(ch) => triple_dirname(Some(&node_new_code), ch, &child.label),
            None => def_dirname(Some(&node_new_code), &child.label),
        };
        let child_new_rel = node_new_rel.join(&new_dirname);
        push_rename(
            changes,
            node_new_rel.join(&old_dirname),
            child_new_rel.clone(),
        );
        recode_contents(child, old_code, new_code, &child_new_rel, changes, rules)?;
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

/// Directories no walk enters, however it reaches them (B2). Machine-made trees hold no
/// name of the tree's, and a rename inside a git object store or a build output destroys
/// rather than repairs — so a decoy `[old]…` name in one is still not the tree's to move.
const NOT_THE_TREES: [&str; 11] = [
    ".git",
    "node_modules",
    "target",
    ".venv",
    "venv",
    "__pycache__",
    "dist",
    "build",
    ".next",
    "DerivedData",
    ".cache",
];

/// Swap a leading **code-prefix run** for `new` — the boundary [`swap_code_prefix`] cannot
/// see, and the whole of D11.
///
/// A stranded name does not merely carry the dead code; it carries the dead code *and its
/// own remaining tokens*, with no `_` between them. `aook_251001_candela`'s child spells
/// `aook251001_b_board`, so an exact-code test matches nothing and the entry is passed
/// over — which is how one move left 320 entries behind (§3.13).
///
/// The test is therefore on the **head**: the segment before the first `_`, which in every
/// name the tree makes is a code — a node dir carries its parent's, a meta dir and a record
/// carry their own. The head must parse as a code and must open with `old`.
///
/// Demanding a head at all is what keeps `assimulation` and `asstderr` out. They are
/// ordinary words that open with `ass` and carry no `_`, and taking `startswith` for a
/// prefix test inflated the first stranding count from 2,460 to 5,446.
///
/// A head already opening with `new` is left alone where `new` **extends** `old`
/// (`aot` → `aott`): `aott_f_forge` reads equally as a stranded `aot` and as a correct
/// `aott`, the name cannot say which, and of the two only renaming a correct name is
/// unrecoverable. A stranded scan names what this declines; a wrong rename names nothing.
///
/// Everything after the run is carried through byte for byte, NFD included (D8) — the
/// destination is the stored name with a prefix replaced, never a spelling rebuilt.
///
/// The head is matched under NFC and its code parsed with combining marks allowed, so a
/// node whose char is `ö` reaches its own children: `assefp_ö_övning` is stored NFC and
/// every `assefpö_*` inside it NFD, and before that the head failed to parse and the whole
/// directory was passed over as bulk.
fn swap_prefix_run(name: &str, old: &str, new: &str) -> String {
    let Some((head, _)) = name.split_once('_') else {
        return name.to_string();
    };
    let Some(run) = nfc_prefix_len(head, old) else {
        return name.to_string();
    };
    if nfc_prefix_len(new, old).is_some() && nfc_prefix_len(head, new).is_some() {
        return name.to_string();
    }
    if Code::parse(head).is_err() {
        return name.to_string();
    }
    format!("{}{}", spell_like(new, &name[..run]), &name[run..])
}

/// A name's new spelling under a recode: **this node's code exactly** where that applies,
/// and the **branch's prefix run** where it does not.
///
/// One walk meets three shapes. A name built from the node it sits at
/// (`assedae__task.jsonl`) takes the exact swap. A name spelling an *ancestor*
/// (`asseda_e_föreläsning.pdf`, sitting at node `assedae`) and a name spelling a
/// *descendant* (`aook251001_b_board`, under a masked child of `aook`) are both invisible
/// to it and are what left thousands of entries behind — the run reaches either, because
/// every name under the branch opens with the branch's own code.
///
/// The exact test goes first because it is the one that survives a **definition-prefix**
/// code: `csa_john_appleseed` carries a `_`, so it is not a head and the run cannot see it.
fn recode_name(
    name: &str,
    node: &Code,
    node_new: &Code,
    branch: &Code,
    branch_new: &Code,
) -> String {
    let exact = swap_code_prefix(name, node.as_str(), node_new.as_str());
    if exact != name {
        return exact;
    }
    swap_prefix_run(name, branch.as_str(), branch_new.as_str())
}

/// Cascade a prefix run over a directory the node walk cannot address, and everything
/// under it (D11).
///
/// A **masked** directory is one the tree cannot read as a node: its name takes a char
/// §5.1 rejects (`aook_251001_candela`), or it still spells the parent code a recode
/// replaced — which is the drift `rename-prefix` exists to repair and, circularly, the very
/// thing that hides it from the parser that would find it. Either way `rename`, `mv` and
/// `rename-prefix` passed it over as bulk and left its whole interior carrying the dead
/// code.
///
/// The walk enters only what it renames. A directory whose own name carries the run is the
/// tree's; one that does not is bulk, and rides along inside its parent exactly as it
/// always has (§6.3). So the bound grows by the masked directories and by nothing else,
/// and [`NOT_THE_TREES`] holds either way.
///
/// **That bound is a measured choice, not a guess.** Entering every directory instead takes
/// the `ass` → `asd` recode from 2,460 stranded names to 80, and of those 80 exactly 7 are
/// the tree's: the rest are a game's `assult_rifle` assets, referenced by path from its
/// scene files, and a virtualenv an older bad rename already mangled into `assite-packages`.
/// Renaming those is destruction, and B2 was that bug once already. What this leaves behind
/// is a tree name under a bulk directory that carries no prefix of its own
/// (`…/_res_ska_sf0003_ö1/assea_…​.tex`) — visible in the dry-run, and one `mv-file` each.
///
// ponytail: enters only what it renames — a stranded name under an unprefixed bulk dir
// stays out of reach. Measured at 7 of 11,657 for `ass`. Widen only with a real name for
// what makes a directory the tree's; `startswith` is not it.
///
/// `dir_rel` is where the directory lives *after* the rename that reached it: the plan
/// stays top-down, so every `from` names a path that exists at the moment its own rename
/// runs.
fn cascade_masked(
    dir_abs: &Path,
    dir_rel: &Path,
    old: &str,
    new: &str,
    changes: &mut Vec<Change>,
) -> Result<()> {
    for entry in std::fs::read_dir(dir_abs)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if NOT_THE_TREES.contains(&name.as_str()) {
            continue;
        }
        let mut renamed = swap_prefix_run(&name, old, new);
        if renamed == name {
            renamed = swap_dir_code_dot(&name, dir_abs, old, new);
        }
        if renamed == name {
            continue;
        }
        let lands_at = dir_rel.join(&renamed);
        push_rename(changes, dir_rel.join(&name), lands_at.clone());
        // `file_type` reads the entry, not its target, so a symlink is renamed as the name
        // it is and never walked into — the tree does not follow a link out of itself.
        if entry.file_type()?.is_dir() {
            cascade_masked(&entry.path(), &lands_at, old, new, changes)?;
        }
    }
    Ok(())
}

/// A bare `[code].[ext]` name inside a **masked** directory, where the code is the one the
/// directory's own name spells.
///
/// [`swap_prefix_run`] cannot see these: `asseta.pdf` carries no `_`, so it has no head, and
/// that refusal is exactly what keeps `assets` and `assimulation` safe. But a masked walk
/// has no node code to test against either — the directory it is standing in was never a
/// node the parser would yield.
///
/// The directory's *name* is the missing code. `asset_a_after_action_review` spells `asset`
/// and takes the char `a`, so a file beside it named `asseta.*` carries that node's code and
/// nothing else could have produced it. `assedai2_doc` spells `assedai2` the same way. A
/// directory whose name yields no code — `appcat`, `cache`, `src` — yields no match, which
/// is what refuses `assessment-config.yaml` and a Haskell build cache's `assolver-plan`.
///
/// So the test is: the stem before the first `.` equals a code the containing directory's
/// name spells, and that code opens with `old`. A stem that merely *starts* like one is not
/// a hit — the equality is the whole guard, and it is what a run at a `.` could never be.
fn swap_dir_code_dot(name: &str, dir_abs: &Path, old: &str, new: &str) -> String {
    if name.contains('_') {
        return name.to_string();
    }
    let Some((stem, _)) = name.split_once('.') else {
        return name.to_string();
    };
    let Some(dirname) = dir_abs.file_name().and_then(|n| n.to_str()) else {
        return name.to_string();
    };
    let parts: Vec<&str> = dirname.split('_').collect();
    let mut codes = vec![parts[0].to_string()];
    // `{code}_{char}_{label}` and the label-less `{code}_{char}`: the char is exactly one
    // character (§5.1). A longer second segment is a label, not a char.
    if parts.len() >= 2 {
        let mut ch = parts[1].chars();
        if let (Some(c), None) = (ch.next(), ch.next())
            && (c.is_alphabetic() || c.is_ascii_digit())
        {
            codes.push(format!("{}{c}", parts[0]));
        }
    }
    let nfc_stem: String = stem.nfc().collect();
    if !codes
        .iter()
        .any(|c| c.nfc().collect::<String>() == nfc_stem)
    {
        return name.to_string();
    }
    let Some(run) = nfc_prefix_len(name, old) else {
        return name.to_string();
    };
    format!("{}{}", spell_like(new, &name[..run]), &name[run..])
}

/// Swap a leading `old_code` prefix (at a `_` or `.` boundary) for `new_code` in a file or
/// directory name. Records/series/rules/meta use `__`, a document a single `_`, and a bare
/// `[code].[ext]` an extension dot; in all three the code is exactly the leading `old_code`
/// followed by that boundary. A name not beginning with the code at a boundary (homed bulk,
/// a stray file) is returned unchanged.
///
/// The match is under NFC (see [`nfc_prefix_len`]) because a node's own directory and the
/// files inside it can disagree on spelling — the mint writes NFC, the filesystem writes
/// NFD — and a byte-exact compare silently strands every file on the other side of it.
///
/// The exact test, for a name whose node — and so whose code — the walk already knows.
/// [`swap_prefix_run`] is its twin for a name the walk had to find by its prefix alone.
fn swap_code_prefix(name: &str, old_code: &str, new_code: &str) -> String {
    let Some(run) = nfc_prefix_len(name, old_code) else {
        return name.to_string();
    };
    let rest = &name[run..];
    // `_` opens a label, `__` a file field, `.` an extension. All three are boundaries, and
    // the third is the one a bare `[code].[ext]` name needs: `asseta.aux` beside the node
    // `asseta` carries that code as surely as `asseta_notes.md` does, and only the exact
    // node code is trusted at a `.` — a *run* match there would rename `assets` (§5, the
    // `startswith` trap), which is destruction.
    //
    // A **digit** is the fourth, and it is a boundary for the same reason the others are:
    // no word continues a code with one. `asseff_t_tentamen` holds ninety exam PDFs named
    // `assefft250113l.pdf` — the node's code with a date run straight onto it — and they are
    // as much the node's as any `assefft_*`. `assets`, `assrc`, `assettings` and
    // `assimulation` all continue with a letter and none of them can ever match here.
    if rest.starts_with('_')
        || rest.starts_with('.')
        || rest.starts_with(|c: char| c.is_ascii_digit())
    {
        return format!("{}{rest}", spell_like(new_code, &name[..run]));
    }
    name.to_string()
}

/// `new` spelled in the normalization the run it replaces was written in.
///
/// The replacement is the one part of a rename that is *rebuilt* rather than carried, so it
/// is the one place a composed spelling can leak into a decomposed tree. Syncthing on macOS
/// requires NFD and silently refuses to back up what is not (D8), so a recode that quietly
/// recomposed a name would break the backup of every file it touched. A run carrying a
/// combining mark takes a decomposed replacement; an ASCII run leaves `new` as it is.
fn spell_like(new: &str, run_text: &str) -> String {
    if run_text == run_text.nfc().collect::<String>() {
        return new.to_string();
    }
    new.nfd().collect()
}

/// The byte length of the leading run of `name` that spells `code`, compared under NFC.
///
/// The tree holds both spellings of the same name: a `pan new` mint writes NFC and macOS
/// writes NFD (D8), and they sit in the same directory — `assefp_ö_övning` is NFC while
/// every `assefpö_*.pdf` inside it is NFD. A byte-exact `strip_prefix` misses across that
/// boundary, which is what left a recode's children stranded under a renamed parent.
///
/// Comparing whole-name-first is what keeps the run from splitting a composed character:
/// `asso\u{308}x` against `asso` normalizes to `assöx`, which does not open with `asso`, so
/// the `o` that belongs to `ö` is never taken as the end of the run.
///
/// The length is an offset into `name` **as stored**, so the caller replaces exactly that
/// run and carries every byte after it through untouched — NFD included.
fn nfc_prefix_len(name: &str, code: &str) -> Option<usize> {
    let target: String = code.nfc().collect();
    let whole: String = name.nfc().collect();
    if !whole.starts_with(&target) {
        return None;
    }
    let mut acc = String::new();
    for (i, ch) in name.char_indices() {
        acc.push(ch);
        if acc.nfc().collect::<String>() == target {
            return Some(i + ch.len_utf8());
        }
    }
    None
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
fn triple_dirname(parent: Option<&Code>, ch: char, label: &str) -> String {
    match parent {
        Some(p) => format!("{}_{ch}_{label}", p.as_str()),
        None => format!("{ch}_{label}"),
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
fn child_code(parent: Option<&Code>, ch: char) -> Code {
    let s = match parent {
        Some(p) => format!("{}{ch}", p.as_str()),
        None => ch.to_string(),
    };
    Code::parse(&s).expect("a parent code plus a normalized char is a valid code")
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
