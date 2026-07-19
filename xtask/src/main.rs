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
}

fn main() -> Result<()> {
    match Cli::parse().task {
        Task::Seed { root, force } => seed(&root, force),
    }
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
