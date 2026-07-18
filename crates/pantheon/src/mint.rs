//! Node minting (§5.5 `pan new`, §5.1). Build the planned transaction that mints a
//! node — the collision check, the code construction, and the normalization — as a
//! spine operation. `pan new` refuses to mint a code collision (§5.3); the check is a
//! glob of the parent's children.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::code::{Code, CodeForm};
use crate::plan::{Change, Plan};
use crate::tree::{child_node_names, resolve_code};
use crate::{Error, Result, name};

/// What to mint: a triple (a defining char plus a label) or a definition-prefix node
/// (a definition that doubles as the label).
#[derive(Clone, Copy)]
pub enum NewSpec<'a> {
    Triple { ch: &'a str, label: &'a str },
    Def { definition: &'a str },
}

/// Plan the mint of one node under `parent` (`"root"` mints a sphere, §5.5). Returns
/// the plan and the node's contract JSON, the `created` entry emitted on apply.
pub fn plan_new(root: &Path, parent: &str, spec: NewSpec) -> Result<(Plan, Value)> {
    let (parent_code, parent_path) = resolve_parent(root, parent)?;

    let (dirname, new_code, char_value, label, form) = match spec {
        NewSpec::Triple { ch, label } => {
            if parent_code.as_ref().is_some_and(|p| !p.is_compact()) {
                return Err(Error::usage(
                    "a definition-prefix node's children must themselves be definition-prefix; \
                     use --def (§5.1)",
                ));
            }
            let ch = normalize_char(ch)?;
            let label = name::normalize_token(label, "label")?;
            let dirname = match &parent_code {
                Some(p) => format!("{}_{ch}_{label}", p.as_str()),
                None => format!("{ch}_{label}"),
            };
            let code_str = match &parent_code {
                Some(p) => format!("{}{ch}", p.as_str()),
                None => ch.clone(),
            };
            (
                dirname,
                Code::parse(&code_str)?,
                Value::String(ch),
                label,
                CodeForm::Triple,
            )
        }
        NewSpec::Def { definition } => {
            let def = name::normalize_token(definition, "definition")?;
            let dirname = match &parent_code {
                Some(p) => format!("{}_{def}_", p.as_str()),
                None => format!("{def}_"),
            };
            let code_str = match &parent_code {
                Some(p) => format!("{}_{def}", p.as_str()),
                None => def.clone(),
            };
            (
                dirname,
                Code::parse(&code_str)?,
                Value::Null,
                def,
                CodeForm::DefinitionPrefix,
            )
        }
    };

    let siblings = child_node_names(&parent_path, parent_code.as_ref())?;
    check_no_collision(&siblings, &new_code)?;

    let base = parent_path.strip_prefix(root).unwrap_or(Path::new(""));
    let rel_path = base.join(&dirname);

    let plan = Plan::new(
        "new",
        vec![Change::Mkdir {
            code: new_code.clone(),
            rel_path: rel_path.clone(),
        }],
    );
    let node = json!({
        "code": new_code.as_str(),
        "char": char_value,
        "label": label,
        "form": form.as_str(),
        "parent": parent,
        "path": rel_path.to_string_lossy(),
    });
    Ok((plan, node))
}

fn resolve_parent(root: &Path, parent: &str) -> Result<(Option<Code>, PathBuf)> {
    if parent == "root" {
        return Ok((None, root.to_path_buf()));
    }
    let code = Code::parse(parent)?;
    let path = resolve_code(root, &code)?;
    Ok((Some(code), path))
}

/// A defining char is one alphabetic character or two ASCII digits (§5.1),
/// normalized on the way in.
fn normalize_char(ch: &str) -> Result<String> {
    let ch = name::normalize_token(ch, "char")?;
    let two_digits = ch.len() == 2 && ch.bytes().all(|b| b.is_ascii_digit());
    let one_alpha = ch.chars().count() == 1 && ch.chars().all(char::is_alphabetic);
    if two_digits || one_alpha {
        Ok(ch)
    } else {
        Err(Error::usage(format!(
            "char {ch:?} must be one letter or two digits (§5.1)"
        )))
    }
}

fn check_no_collision(siblings: &[crate::code::NodeName], new_code: &Code) -> Result<()> {
    let nc = new_code.as_str();
    for s in siblings {
        let sc = s.code.as_str();
        if sc == nc {
            return Err(Error::validation(format!(
                "code {nc:?} already exists (§5.3)"
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

/// Whether `longer` is `shorter` plus a `_`-joined continuation — the prefix-shadow
/// a compact walk cannot disambiguate (§5.3).
fn prefix_shadows(longer: &str, shorter: &str) -> bool {
    longer.len() > shorter.len()
        && longer.starts_with(shorter)
        && longer.as_bytes().get(shorter.len()) == Some(&b'_')
}
