//! Auspex — the rules engine (§9). Reads the cores' readings for signs and turns
//! them into intentions (Pensum tasks); it proposes deeds, never does them. The
//! only reactive writer (I2).
//!
//! **Rules are files** (§9.1), discovered by walking the tree the way Pantheon
//! discovers nodes: no registry, no `rules.toml`. A rule is
//! `[code]__function__[name][.ext]` in a meta dir, and **where the file sits is the
//! whole of its scope** — this node and everything under it (§6.3).
//!
//! A rule is a pure function of the tree — context on stdin, proposals on stdout
//! (§9.3) — and Auspex checks every proposal against the rule's own `writes=` grant
//! before applying (§9.5). No daemon: it is woken by hooks and by a hand's own
//! `aus run` (§9.4).
//!
//! This crate carries the **read half** so far: discovery and the header. Executing a
//! rule is `plan`/`test`'s, and applying a proposal is `run`'s.
//!
//! ## Discovery never runs code
//!
//! The header is parsed out of the file's first line — or its second, when a shebang
//! takes the first — and **never by executing it** (§9.2). Rule files are code an LLM
//! may have authored, so a rule's capabilities must be readable before it is trusted
//! to run. That is also why there is no compiled form: a binary could declare its
//! header only by being run, which is the one thing discovery must not do.

use std::path::{Path, PathBuf};

use pantheon::Result;
use pantheon::classify::{self, FileClass};
use pantheon::code::Code;
use pantheon::tree::{Node, TreeRoot, build_tree};

mod cli;
// The screen rides the `tui` feature; drop it and the rules browser is a CLI (§14).
#[cfg(feature = "tui")]
mod screen;

pub use cli::run_cli;
#[cfg(feature = "tui")]
pub use screen::AuspexApp;

/// One discovered rule (§9.1).
///
/// `scope` is the meta dir the file sits in, never the code its *filename* carries:
/// §9.1 puts the whole of a rule's scope in its location, so a `mv` re-scopes it
/// completely and no header key narrows it. The two normally agree — re-scoping is a
/// move plus a prefix rewrite — and `declared` is how a reader learns they do not.
#[derive(Clone, Debug)]
pub(crate) struct Rule {
    pub scope: Code,
    pub name: String,
    pub path: PathBuf,
    /// The code the *filename* declares, kept only when it disagrees with `scope`.
    pub declared: Option<String>,
    pub header: Header,
}

impl Rule {
    /// The file's own name, which is how a hand refers to it (`touch`, `rm`, §9.1).
    pub fn file_name(&self) -> String {
        self.path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
    }
}

/// A rule's declaration (§9.2), read without executing it.
///
/// **`writes` is default-deny.** A rule declaring nothing is read-only: it may
/// propose, but nothing it proposes lands. Capabilities are kept here in their header
/// form — `core@home[/series]:verbs` — because that is what a hand reads before
/// granting, and what `ls` must show back unchanged. Parsing them into a structure is
/// the enforcing verb's job (§9.5), not the browser's.
#[derive(Clone, Debug, Default)]
pub(crate) struct Header {
    pub watch: Vec<String>,
    pub writes: Vec<String>,
    pub desc: Option<String>,
    /// Why the header did not parse, where it did not. A rule whose declaration is
    /// unreadable keeps its default-deny `writes` — the safe reading — and says so
    /// rather than being silently dropped from the listing.
    pub error: Option<String>,
}

/// Every rule in scope, in tree order (§9.1).
///
/// `at` is `aus run [scope]`'s argument: `None` walks the whole forest, `Some(code)`
/// the subtree at that node — which is [`build_tree`]'s own distinction, so a scope
/// costs nothing to resolve.
///
/// # Errors
/// If the root cannot be walked, or `at` names no node (exit `4`).
pub(crate) fn discover(root: &Path, at: Option<&Code>) -> Result<Vec<Rule>> {
    let nodes = match build_tree(root, at)? {
        TreeRoot::Forest(nodes) => nodes,
        TreeRoot::Subtree(node) => vec![node],
    };
    let mut out = Vec::new();
    for node in &nodes {
        collect(node, &mut out);
    }
    Ok(out)
}

/// Recurse a node and its descendants, reading each meta dir.
///
/// The tree walk visits node dirs only — `build_tree` skips anything ending `__` — so
/// the meta dir is opened here by the same one-line formula every other walk uses.
/// A rule belongs to no core's token set, so no `Store` walk could ever yield one:
/// this is the only walk in the workspace that looks for [`FileClass::Rule`].
fn collect(node: &Node, out: &mut Vec<Rule>) {
    let meta = node.path.join(format!("{}__", node.code.as_str()));
    if let Ok(entries) = std::fs::read_dir(&meta) {
        let mut found: Vec<Rule> = entries
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .filter_map(|entry| {
                let file_name = entry.file_name();
                let name = file_name.to_string_lossy();
                match classify::classify(&name, false, &node.code) {
                    FileClass::Rule { code, name } => {
                        Some(rule_at(node, &code, name, entry.path()))
                    }
                    _ => None,
                }
            })
            .collect();
        // `read_dir` order is the filesystem's, which differs between machines and
        // would leave a snapshot of `ls` unfreezable.
        found.sort_by(|a, b| a.name.cmp(&b.name));
        out.append(&mut found);
    }
    for child in &node.children {
        collect(child, out);
    }
}

fn rule_at(node: &Node, declared: &Code, name: String, path: PathBuf) -> Rule {
    let mismatch = declared.as_str() != node.code.as_str();
    Rule {
        scope: node.code.clone(),
        name,
        declared: mismatch.then(|| declared.as_str().to_string()),
        header: read_header(&path),
        path,
    }
}

/// The `auspex:` comment header, from the first line — or the second, when a shebang
/// takes the first, **and no further** (§9.2).
///
/// A file with no header is a legal rule: it declares nothing, so it is read-only by
/// the default-deny rule and proposes into the void until a grant is written.
fn read_header(path: &Path) -> Header {
    let Ok(text) = std::fs::read_to_string(path) else {
        // §9.2: a rule is always text. One that is not is malformed, not missing.
        return Header {
            error: Some("not readable as UTF-8 text (§9.2)".to_string()),
            ..Header::default()
        };
    };
    let mut lines = text.lines();
    let Some(first) = lines.next() else {
        return Header::default();
    };
    let candidate = if first.starts_with("#!") {
        lines.next().unwrap_or_default()
    } else {
        first
    };
    parse_header(candidate)
}

/// One header line into its three keys. `#` for Python/shell/Ruby, `//` for JS/Rust
/// (§9.2) — the comment leader is the language's, and Auspex reads both.
///
/// **`desc=` takes the rest of the line and so must come last.** §9.2 calls it a
/// "one-line human description", which a whitespace-separated field cannot hold: the
/// alternative is quoting, and a header a hand must escape to write is worse than one
/// with an ordering rule. `watch` and `writes` are single tokens by construction —
/// comma- and semicolon-separated — so nothing else wants the space.
fn parse_header(line: &str) -> Header {
    let body = line.trim_start();
    let body = body
        .strip_prefix("//")
        .or_else(|| body.strip_prefix('#'))
        .map(str::trim_start);
    let Some(body) = body.and_then(|b| b.strip_prefix("auspex:")) else {
        // Not a declaration — a plain comment, or code. Read-only by default-deny.
        return Header::default();
    };

    let mut header = Header::default();
    let fields = match body.split_once("desc=") {
        Some((before, rest)) => {
            let rest = rest.trim();
            header.desc = (!rest.is_empty()).then(|| rest.to_string());
            before
        }
        None => body,
    };
    for field in fields.split_whitespace() {
        let Some((key, value)) = field.split_once('=') else {
            header.error = Some(format!("{field:?} is not a key=value field (§9.2)"));
            continue;
        };
        match key {
            "watch" => header.watch = split_list(value, ','),
            "writes" => header.writes = split_list(value, ';'),
            _ => header.error = Some(format!("unknown header key {key:?} (§9.2)")),
        }
    }
    header
}

fn split_list(value: &str, sep: char) -> Vec<String> {
    value
        .split(sep)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}
