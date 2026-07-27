//! The rule declaration header (§9.2) — read here so it is read once.
//!
//! A rule is Auspex's to *run*; its header is a **structural** fact about the tree, and
//! two things outside Auspex need it. `pan`'s node cascade must rewrite the
//! `writes=core@home` grants a recode invalidates (§10.1), and `pan validate` must report
//! a grant naming a node that is not there (§10.2). Both are the spine's work, and the
//! spine cannot ask Auspex anything (I5, hub-and-spoke) — so the grammar lives here and
//! Auspex reads it from the hub like everyone else.
//!
//! Nothing here *enforces* a grant. Parsing a capability into the thing a proposal is
//! checked against is the enforcing verb's job (§9.5) and stays in `auspex`; this module
//! knows only that a header has fields, that one of them names nodes, and how to rewrite
//! those names in place.

use std::path::Path;

use crate::code::Code;

/// A rule's declaration (§9.2), read without executing it.
///
/// **`writes` is default-deny.** A rule declaring nothing is read-only: it may propose,
/// but nothing it proposes lands. Capabilities are kept in their header form —
/// `core@home[/series]:verbs` — because that is what a hand reads before granting, and
/// what `ls` must show back unchanged.
#[derive(Clone, Debug, Default)]
pub struct Header {
    pub watch: Vec<String>,
    pub writes: Vec<String>,
    pub desc: Option<String>,
    /// Why the header did not parse, where it did not. A rule whose declaration is
    /// unreadable keeps its default-deny `writes` — the safe reading — and says so
    /// rather than being silently dropped from the listing.
    pub error: Option<String>,
}

/// The `auspex:` comment header, from the first line — or the second, when a shebang
/// takes the first, **and no further** (§9.2).
///
/// A file with no header is a legal rule: it declares nothing, so it is read-only by the
/// default-deny rule and proposes into the void until a grant is written.
#[must_use]
pub fn read_header(path: &Path) -> Header {
    let Ok(text) = std::fs::read_to_string(path) else {
        // §9.2: a rule is always text. One that is not is malformed, not missing.
        return Header {
            error: Some("not readable as UTF-8 text (§9.2)".to_string()),
            ..Header::default()
        };
    };
    match header_line(&text) {
        Some((_, line)) => parse_header(line),
        None => Header::default(),
    }
}

/// Which line of a rule file carries its declaration, and that line (§9.2).
///
/// The first, unless a shebang takes it — and then the second, **and no further**. A
/// header further down is not a header, which is what keeps this a fixed read rather than
/// a scan of the whole file.
#[must_use]
pub fn header_line(text: &str) -> Option<(usize, &str)> {
    let mut lines = text.lines();
    let first = lines.next()?;
    if first.starts_with("#!") {
        lines.next().map(|second| (1, second))
    } else {
        Some((0, first))
    }
}

/// One header line into its three keys. `#` for Python/shell/Ruby, `//` for JS/Rust
/// (§9.2) — the comment leader is the language's, and both are read.
///
/// **`desc=` takes the rest of the line and so must come last.** §9.2 calls it a
/// "one-line human description", which a whitespace-separated field cannot hold: the
/// alternative is quoting, and a header a hand must escape to write is worse than one
/// with an ordering rule. `watch` and `writes` are single tokens by construction —
/// comma- and semicolon-separated — so nothing else wants the space.
#[must_use]
pub fn parse_header(line: &str) -> Header {
    let Some(body) = declaration_body(line) else {
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

/// The `auspex:` body of a header line, past its comment leader.
fn declaration_body(line: &str) -> Option<&str> {
    let body = line.trim_start();
    let body = body
        .strip_prefix("//")
        .or_else(|| body.strip_prefix('#'))
        .map(str::trim_start)?;
    body.strip_prefix("auspex:")
}

fn split_list(value: &str, sep: char) -> Vec<String> {
    value
        .split(sep)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// The node a capability names — the `home` of `core@home[/series]:verbs` (§9.2).
///
/// `None` where the entry will not parse that far, which is the same fail-closed reading
/// the enforcing check makes: an unreadable grant is not a grant, and a cascade that
/// guessed at one would rewrite something nobody declared.
#[must_use]
pub fn capability_home(entry: &str) -> Option<&str> {
    let (loc, _verbs) = entry.split_once(':')?;
    let (_core, home_series) = loc.split_once('@')?;
    let home = match home_series.split_once('/') {
        Some((home, _series)) => home,
        None => home_series,
    };
    (!home.is_empty()).then_some(home)
}

/// Whether a recode of `branch` moves the node a grant names, and what it becomes
/// (§10.1, §5.3).
///
/// A descendant's code carries its ancestor's as a plain string prefix — a triple child
/// concatenates its char, a definition-prefix child appends past a `_` — which is exactly
/// what the recode itself relies on. A *sibling* that merely shared the prefix would be
/// `code_collision` (§5.3), so in a tree that validates, prefix means descendant.
#[must_use]
pub fn recoded_home(home: &str, branch: &Code, to: &Code) -> Option<String> {
    let old = branch.as_str();
    let tail = home.strip_prefix(old)?;
    Some(format!("{}{tail}", to.as_str()))
}

/// Rewrite the homes a rule's `writes=` grant names, for a branch that was recoded
/// (§10.1, §9.2).
///
/// Returns the file's new text, or `None` where the header names nothing in the branch —
/// so a caller can tell "nothing to do" from "rewritten", and a plan carries a change only
/// where there is one.
///
/// **Only the `writes=` field is touched.** `watch=` names *cores*, not nodes, and `desc=`
/// is a hand's sentence; rewriting either would be editing a rule's prose. The rest of the
/// line — its leader, its field order, its spacing, the description — is preserved
/// verbatim, for the same reason frontmatter is never re-serialized (§6.6): a hand wrote
/// it and only the part that went stale is the tools' to change.
#[must_use]
pub fn rewrite_writes_homes(text: &str, branch: &Code, to: &Code) -> Option<String> {
    let (index, line) = header_line(text)?;
    let body = declaration_body(line)?;
    // `desc=` runs to end of line, so the fields are only what precedes it.
    let fields_end = body.find("desc=").unwrap_or(body.len());
    let fields = &body[..fields_end];

    let mut rewritten = line.to_string();
    let mut touched = false;
    for field in fields.split_whitespace() {
        let Some(value) = field.strip_prefix("writes=") else {
            continue;
        };
        let mut entries: Vec<String> = Vec::new();
        for entry in value.split(';') {
            let recoded = capability_home(entry)
                .and_then(|home| recoded_home(home, branch, to).map(|new| (home, new)));
            match recoded {
                Some((home, new)) => {
                    touched = true;
                    // The home sits between `@` and the `/` or `:` that ends it, so
                    // replacing that one occurrence leaves core, series and verbs alone.
                    entries.push(entry.replacen(&format!("@{home}"), &format!("@{new}"), 1));
                }
                None => entries.push(entry.to_string()),
            }
        }
        if touched {
            rewritten = rewritten.replacen(field, &format!("writes={}", entries.join(";")), 1);
        }
    }
    if !touched {
        return None;
    }

    let mut lines: Vec<&str> = text.lines().collect();
    lines[index] = &rewritten;
    let mut out = lines.join("\n");
    if text.ends_with('\n') {
        out.push('\n');
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(s: &str) -> Code {
        Code::parse(s).unwrap()
    }

    #[test]
    fn a_header_reads_its_three_fields_and_desc_takes_the_rest() {
        let h = parse_header("# auspex: watch=pensum,album writes=pensum@csa:add desc=a note here");
        assert_eq!(h.watch, ["pensum", "album"]);
        assert_eq!(h.writes, ["pensum@csa:add"]);
        assert_eq!(h.desc.as_deref(), Some("a note here"));
        assert!(h.error.is_none());
        // The `//` leader reads identically (§9.2).
        assert_eq!(
            parse_header("// auspex: writes=pensum@csa:add")
                .writes
                .len(),
            1
        );
        // Not a declaration at all — read-only by default-deny.
        assert!(parse_header("# an ordinary comment").writes.is_empty());
    }

    /// A recode rewrites the homes a grant names — its own node and every descendant —
    /// and touches nothing else on the line (§10.1, §9.2).
    #[test]
    fn a_recode_rewrites_only_the_homes_in_writes() {
        let text = "#!/usr/bin/env python3\n\
                    # auspex: watch=pensum writes=pensum@csa:add;annales@csao/hours:add;album@x:add desc=csa stays in the prose\n\
                    print('hi')\n";
        let out = rewrite_writes_homes(text, &code("csa"), &code("cso")).expect("a home moved");
        assert!(
            out.contains("writes=pensum@cso:add;annales@csoo/hours:add;album@x:add"),
            "{out}"
        );
        // The description said `csa` and still does: only the grant went stale.
        assert!(out.contains("desc=csa stays in the prose"), "{out}");
        assert!(out.contains("watch=pensum"), "{out}");
        assert!(out.starts_with("#!/usr/bin/env python3\n"), "{out}");
        assert!(out.ends_with("print('hi')\n"), "{out}");
    }

    /// A header naming nothing in the branch is left alone — a plan carries a change only
    /// where there is one.
    #[test]
    fn a_header_outside_the_branch_is_not_rewritten() {
        let text = "# auspex: writes=pensum@x:add\n";
        assert!(rewrite_writes_homes(text, &code("csa"), &code("cso")).is_none());
        // And a rule with no header at all is simply not a grant.
        assert!(rewrite_writes_homes("print('hi')\n", &code("csa"), &code("cso")).is_none());
    }

    #[test]
    fn a_capability_names_its_home_or_nothing() {
        assert_eq!(capability_home("pensum@csa:add"), Some("csa"));
        assert_eq!(capability_home("annales@csa/hours:add"), Some("csa"));
        // Fail closed: an entry that will not parse names no node to rewrite.
        assert_eq!(capability_home("pensum@csa"), None);
        assert_eq!(capability_home("pensum:add"), None);
    }
}
