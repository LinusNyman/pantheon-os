//! xtask — dev tasks (§14). Run via `cargo xtask <task>`.
//!
//! `seed` mints a demo tree and fills it by **driving the real binaries** — the same
//! commands a hand would type (I8). It links no core and reaches for no library: it
//! shells out exactly as a lens does, so what it produces is what the contract
//! produces, and a seed that succeeded is itself a check that the CLIs still work
//! together (I4).

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "xtask", about = "dev tasks for PantheonOS")]
struct Cli {
    #[command(subcommand)]
    task: Task,
}

#[derive(Subcommand)]
enum Task {
    /// Mint a demo tree and fill it, so there is something for a screen to render.
    Seed {
        /// Where to mint it. Refuses a directory that already has contents.
        #[arg(long, default_value = "target/demo")]
        root: PathBuf,
        /// Mint into a directory that already holds a tree, adding to it.
        #[arg(long)]
        force: bool,
    },
    /// Widen every six-digit date in a tree to `YYYYMMDD` (format 1 → 2, §5.4, §15.5).
    ///
    /// **Dry-run unless `--apply`.** Prints one line per value it would change.
    MigrateDates {
        /// The tree to rewrite.
        #[arg(long)]
        root: PathBuf,
        /// Actually write. Without it, nothing on disk is touched.
        #[arg(long)]
        apply: bool,
    },
}

fn main() -> Result<()> {
    match Cli::parse().task {
        Task::Seed { root, force } => seed(&root, force),
        Task::MigrateDates { root, apply } => migrate_dates(&root, apply),
    }
}

// ── format 1 → 2: the date width (§5.4) ──────────────────────────────────────

/// Every record field that holds a day, across all seven cores (§8.1–§8.7).
///
/// Named explicitly rather than "any six-digit string", because a core's `data` also
/// carries values a hand chose — an Annales reading of `250601` is a *measurement*, and
/// widening it would corrupt the record it was meant to preserve. Only the keys the spec
/// gives a date shape are rewritten; `key` is handled separately, being the envelope's.
const DATE_FIELDS: [&str; 7] = ["done", "from", "to", "at", "until", "expires", "lapses"];

/// Widen a six-digit `YYMMDD` to `YYYYMMDD`, preserving any `Thhmm` tail.
///
/// **`20` is the right prefix and only here.** The spine refuses to guess a century on a
/// hand's input (§5.4) precisely so that the guess is made *once*, in a migration a hand
/// reads and approves, against a tree whose dates are all known to be this century. A
/// tree holding a genuine pre-2000 date must be corrected by hand first — which is why
/// this prints every change rather than reporting a count.
fn widen(value: &str) -> Option<String> {
    let (day, tail) = match value.split_once('T') {
        Some((day, time)) => (day, Some(time)),
        None => (value, None),
    };
    if day.len() != 6 || !day.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    // Reject an impossible month or day rather than widening a six-digit *name* that
    // merely looks dated — a slug is all-digits often enough to matter (§5.4).
    let month: u32 = day[2..4].parse().ok()?;
    let dom: u32 = day[4..6].parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&dom) {
        return None;
    }
    Some(match tail {
        Some(time) => format!("20{day}T{time}"),
        None => format!("20{day}"),
    })
}

fn migrate_dates(root: &Path, apply: bool) -> Result<()> {
    if !root.is_dir() {
        bail!("{} is not a directory", root.display());
    }
    let mut changes = 0usize;
    let mut files = 0usize;
    visit(root, &mut |path| {
        if !is_record_file(path) {
            return Ok(());
        }
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let mut out = String::with_capacity(text.len());
        let mut touched = false;
        // Line-oriented for both shapes: a `.jsonl` is one record per line, and an
        // entity `.json` is rewritten the same way so the file's own formatting,
        // key order and whitespace survive untouched (I6 — the file stays a thing a
        // hand can read, and a re-serialize would reflow it).
        for line in text.lines() {
            let rewritten = rewrite_line(line, path, &mut changes);
            touched |= rewritten != line;
            out.push_str(&rewritten);
            out.push('\n');
        }
        if touched {
            files += 1;
            if apply {
                std::fs::write(path, out).with_context(|| format!("writing {}", path.display()))?;
            }
        }
        Ok(())
    })?;

    println!();
    if apply {
        println!("applied {changes} date(s) across {files} file(s)");
    } else {
        println!("would change {changes} date(s) across {files} file(s) — re-run with --apply");
    }
    Ok(())
}

/// Rewrite one JSON line's date-bearing values, reporting each.
///
/// A textual pass over `"field":"value"` rather than a parse-and-re-emit, for the reason
/// above: re-serializing would reformat every record in the tree, and a migration that
/// churns files it did not need to touch is one nobody can review.
fn rewrite_line(line: &str, path: &Path, changes: &mut usize) -> String {
    let mut out = line.to_string();
    for field in DATE_FIELDS.iter().chain(std::iter::once(&"key")) {
        let needle = format!("\"{field}\":");
        let mut from = 0usize;
        while let Some(at) = out[from..].find(&needle) {
            let start = from + at + needle.len();
            let rest = out[start..].trim_start();
            let pad = out[start..].len() - rest.len();
            let Some(inner) = rest.strip_prefix('"') else {
                from = start;
                continue;
            };
            let Some(end) = inner.find('"') else {
                from = start;
                continue;
            };
            let value = &inner[..end];
            match widen(value) {
                Some(wide) => {
                    println!("{}: {field} {value} → {wide}", path.display());
                    *changes += 1;
                    let lo = start + pad + 1;
                    let hi = lo + value.len();
                    out.replace_range(lo..hi, &wide);
                    from = lo + wide.len();
                }
                None => from = start + pad + 1 + end,
            }
        }
    }
    out
}

/// Whether this file is one of the tree's own records (§5.2, §6.1).
///
/// **This is the whole safety of the migration, and a bare extension test is not enough.**
/// A node may home a project, so the tree holds a `package-lock.json` for every dependency
/// a hand ever installed — in one live tree, 23,995 `.json` files of which 148 were
/// records. An npm lockfile carries `"from"` keys, so a walk filtering on extension alone
/// would rewrite dates inside dependency metadata. This is the same trap `rename-prefix`
/// fell into (Wave 8), and the same answer: bound the walk to the node tree.
///
/// The test is §5.2's own rule — a record sits in its node's meta dir and its filename
/// **begins with that dir's name** (`csa__/csa__person__alex.json`, `e__/e__task.jsonl`).
/// A `__pycache__` or any other coincidentally `__`-suffixed directory fails it, because
/// nothing inside is named after the directory.
fn is_record_file(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if ext != "json" && ext != "jsonl" {
        return false;
    }
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let Some(dir) = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
    else {
        return false;
    };
    dir.ends_with("__") && name.starts_with(dir)
}

/// Walk every file under `root`. No ignore file governs the tree (§13, §18), so this
/// descends into everything and lets [`is_record_file`] decide at the leaf — never a
/// hardcoded skip list, which would be an ignore file spelled differently.
fn visit(dir: &Path, f: &mut impl FnMut(&Path) -> Result<()>) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            visit(&path, f)?;
        } else {
            f(&path)?;
        }
    }
    Ok(())
}

/// The nodes the demo tree carries.
///
/// Shaped after the reference tree of §2 — Actio / Contextus / Ego — which is *an
/// illustration, never a shape the tools impose*. A seeded tree is a demo, not a
/// template: the ontology is emergent and wholly the user's (I7, §5.0).
const NODES: &[(&str, &str, &str)] = &[
    ("root", "a", "actio"),
    ("a", "c", "cura"),
    ("a", "o", "opus"),
    ("root", "c", "contextus"),
    ("c", "s", "societas"),
    ("cs", "a", "amicitia"),
    ("root", "e", "ego"),
    ("e", "c", "corpus"),
    ("ec", "v", "valetudo"),
];

fn seed(root: &Path, force: bool) -> Result<()> {
    if root.exists() && !force {
        let occupied = std::fs::read_dir(root)
            .with_context(|| format!("reading {}", root.display()))?
            .next()
            .is_some();
        if occupied {
            bail!(
                "{} already has contents — pass --force to add to it, or name an empty \
                 directory. A seed that overwrote a tree would be the one destructive \
                 dev task in the repo.",
                root.display()
            );
        }
    }
    std::fs::create_dir_all(root)?;
    let root = root.canonicalize()?;

    for (parent, ch, label) in NODES {
        run(&root, "pan", &["new", parent, ch, label, "-y"])?;
    }

    // Tasks at two nodes, so an Agenda has something cross-node to show (P§3).
    for (home, task) in [
        ("ac", "renew_passport"),
        ("ac", "call_the_dentist"),
        ("ao", "write_the_release_notes"),
    ] {
        run(&root, "pen", &[home, task, "-y"])?;
    }
    // One already done, so `list` and `list --all` differ visibly.
    run(
        &root,
        "pen",
        &[
            "edit",
            "-H",
            "ac",
            "renew_passport",
            "--done",
            "260701",
            "-y",
        ],
    )?;

    // People, and a ref between them — the edge, never a nesting (I9).
    run(&root, "alb", &["add", "-H", "csa", "alex", "-y"])?;
    run(
        &root,
        "alb",
        &["add", "-H", "csa", "mara", "-r", "album:alex", "-y"],
    )?;

    // A hand-named log, minted explicitly (§7.3) — a plain `add` would refuse.
    run(
        &root,
        "ann",
        &["ecv", "weight", "78.4", "-c", "-a", "260701", "-y"],
    )?;
    run(
        &root,
        "ann",
        &["ecv", "weight", "78.1", "-a", "260708", "-y"],
    )?;
    run(
        &root,
        "ann",
        &["ecv", "weight", "77.9", "-a", "260715", "-y"],
    )?;

    println!("seeded {}", root.display());
    println!();
    println!("  export PANTHEON_ROOT={}", root.display());
    println!("  atr            # the mosaic");
    println!("  pen list       # the same tasks, as a table");
    println!("  atr | jq       # the same figures, as JSON");
    Ok(())
}

/// Run one binary out of `target/debug`, so a seed exercises what was just built
/// rather than whatever happens to be installed.
fn run(root: &Path, short: &str, args: &[&str]) -> Result<()> {
    let bin = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask sits beside the workspace root")
        .join("target/debug")
        .join(short);
    anyhow::ensure!(
        bin.exists(),
        "{} is not built — run `cargo build --workspace --bins` first",
        bin.display()
    );
    let out = Command::new(&bin)
        .args(args)
        .env("PANTHEON_ROOT", root)
        .output()
        .with_context(|| format!("running {short}"))?;
    if !out.status.success() {
        bail!(
            "{short} {} failed:\n{}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The guard that keeps the migration inside the tree (§5.2).
    ///
    /// **The negative cases are the load-bearing ones.** One live tree held 23,995 `.json`
    /// files of which 148 were records; the rest were dependency lockfiles, and an npm
    /// lockfile carries `"from"` keys a date pass would happily rewrite.
    #[test]
    fn only_a_node_s_own_records_are_migrated() {
        let yes = [
            "tree/c_contextus/cs__/cs__task.jsonl",
            "tree/e_ego/e__/e__person__alex.json",
            "tree/a/csa__/csa__balance__checking.jsonl",
        ];
        for p in yes {
            assert!(is_record_file(Path::new(p)), "{p} is a record");
        }
        let no = [
            // A project homed at a node brings its whole dependency tree with it.
            "tree/a_actio/proj/node_modules/foo/package-lock.json",
            "tree/a_actio/proj/package.json",
            // `__pycache__` ends in `__` but nothing inside is named after it.
            "tree/a_actio/proj/__pycache__/cache.json",
            // A meta dir holds a rule and an annotation too — neither is JSON.
            "tree/c_contextus/cs__/cs__function__stale.py",
            // A loose document lives in the open node dir, not the meta dir.
            "tree/e_ego/e_note.md",
        ];
        for p in no {
            assert!(!is_record_file(Path::new(p)), "{p} is not a record");
        }
    }

    /// A six-digit day widens; anything else is left exactly as it was.
    #[test]
    fn only_a_six_digit_day_widens() {
        assert_eq!(widen("260719").as_deref(), Some("20260719"));
        assert_eq!(widen("260719T1400").as_deref(), Some("20260719T1400"));
        // Already migrated — idempotence is what lets a hand re-run after a failure.
        assert_eq!(widen("20260719"), None);
        // A name-keyed line (a Pensum task) is never a date, whatever it spells.
        assert_eq!(widen("buy_milk"), None);
        // Six digits that cannot be a day: a slug is all-digits often enough to matter.
        assert_eq!(widen("999999"), None);
        assert_eq!(widen("123456"), None);
        assert_eq!(widen("250001"), None);
        assert_eq!(widen("250132"), None);
    }

    /// The rewrite touches the dated fields and leaves a core's own numbers alone —
    /// an Annales reading of `250601` is a *measurement*, not a date (§8.6).
    #[test]
    fn a_reading_s_own_value_is_never_widened() {
        let mut n = 0;
        let line = r#"{"key":"260601","refs":[],"data":{"values":["250601","4.0"]}}"#;
        let out = rewrite_line(line, Path::new("t"), &mut n);
        assert_eq!(
            out,
            r#"{"key":"20260601","refs":[],"data":{"values":["250601","4.0"]}}"#
        );
        assert_eq!(n, 1, "the key alone moved");
    }
}
