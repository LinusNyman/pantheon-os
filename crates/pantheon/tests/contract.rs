//! The frozen contract (§7.2, BUILD-PLAN): insta snapshots of the file→core map,
//! name normalization, code parsing, the `schema` surface, the four verbs' JSON, and
//! the error envelope. Plan tokens are redacted (they hash a path); regenerate these
//! deliberately, never blindly.

// These tests build human-readable snapshot text; `push_str(&format!(...))` reads
// clearest here and the allocation is irrelevant in a test.
#![allow(clippy::format_push_string)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use pantheon::code::parse_node_dirname;
use pantheon::mint::NewSpec;
use pantheon::{
    Code, CoreRegistry, DiscoveredCore, Error, Shape, build_tree, classify, normalize, plan_new,
    resolve_all, resolve_code, validate,
};

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn fresh_root() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("pan-snap-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap()
}

fn mint(root: &Path, parent: &str, ch: &str, label: &str) {
    let (plan, _) = plan_new(root, parent, NewSpec::Triple { ch, label }).unwrap();
    plan.apply(root).unwrap();
}

// ── the file→core map (§5.2) ────────────────────────────────────────────────

#[test]
fn classify_map() {
    let node = Code::parse("csa").unwrap();
    let cases: &[(&str, bool)] = &[
        ("csa__", true),
        ("csa__.toml", false),
        ("csa_curriculum.toml", false),
        ("Cargo.toml", false),
        ("csa__person__alex.json", false),
        ("csa_john_appleseed__person.json", false),
        ("cso__log__meetings.jsonl", false),
        ("cso__task.jsonl", false),
        ("csa_doctors_appointment.md", false),
        ("csa__function__balance_check", false),
        ("csa__function__deploy.sh", false),
        ("csa_x_thing", true),
        ("csa_john_appleseed_", true),
        ("photo.png", false),
        ("stray_note.txt", false),
    ];
    let mut out = String::new();
    for &(name, is_dir) in cases {
        out.push_str(&format!(
            "{name}  (dir={is_dir})\n  => {:?}\n",
            classify(name, is_dir, &node)
        ));
    }
    insta::assert_snapshot!("classify_map", out);
}

// ── name normalization (§5.1) ───────────────────────────────────────────────

#[test]
fn normalize_table() {
    let inputs = [
        "Hello World",
        "café",
        "cafe\u{0301}",
        "Ångström",
        "a--b",
        "  x  ",
        "u.s.a!",
        "MixedCASE__Foo",
        "YouTube-Video",
        "",
        "-",
        "___",
    ];
    let mut out = String::new();
    for i in inputs {
        out.push_str(&format!("{i:?} => {:?}\n", normalize(i)));
    }
    insta::assert_snapshot!("normalize_table", out);
}

// ── addressing (§5.1) ───────────────────────────────────────────────────────

#[test]
fn code_and_dirname_parsing() {
    let mut out = String::from("tokenize_compact:\n");
    for code in ["a", "csa", "asdl01", "asdl0103"] {
        let toks = Code::parse(code).unwrap().tokenize_compact().unwrap();
        out.push_str(&format!("  {code} => {toks:?}\n"));
    }
    out.push_str("parse_node_dirname:\n");
    let cs = Code::parse("cs").unwrap();
    let csa = Code::parse("csa").unwrap();
    let cases: &[(Option<&Code>, &str)] = &[
        (None, "a_actio"),
        (Some(&cs), "cs_a_amicitia"),
        (Some(&csa), "csa_john_appleseed_"),
        (Some(&csa), "csa_x_thing"),
    ];
    for &(parent, dir) in cases {
        let nn = parse_node_dirname(parent, dir).unwrap();
        out.push_str(&format!(
            "  {dir} => code={} form={} char={:?} label={}\n",
            nn.code.as_str(),
            nn.form.as_str(),
            nn.ch.as_ref().map(pantheon::CharToken::as_code_str),
            nn.label
        ));
    }
    insta::assert_snapshot!("code_and_dirname_parsing", out);
}

// ── the schema surface (§7.2) ───────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
struct Person {
    /// Away periods, accumulated in the record (I1).
    away: Vec<String>,
}

struct Album;
impl pantheon::Core for Album {
    type Record = Person;
    const NAME: &'static str = "album";
    fn kinds() -> &'static [(&'static str, Shape)] {
        &[("person", Shape::Partitioned)]
    }
    fn validate(_record: &Person) -> pantheon::Result<()> {
        Ok(())
    }
}

#[test]
fn core_schema_surface() {
    let schema = pantheon::schema::<Album>(1);
    insta::assert_snapshot!(
        "core_schema_surface",
        pretty(&serde_json::to_value(&schema).unwrap())
    );
}

// ── the four verbs' JSON (§5.5) ─────────────────────────────────────────────

#[test]
fn verb_new_dry_run_and_created() {
    let root = fresh_root();
    let (plan, node) = plan_new(
        &root,
        "root",
        NewSpec::Triple {
            ch: "a",
            label: "actio",
        },
    )
    .unwrap();
    let mut dry = plan.to_json();
    dry["token"] = json!("[redacted]");
    insta::assert_snapshot!("verb_new_dry_run", pretty(&dry));
    insta::assert_snapshot!("verb_new_created", pretty(&json!({ "created": [node] })));
}

#[test]
fn verb_tree() {
    let root = fresh_root();
    mint(&root, "root", "a", "actio");
    mint(&root, "a", "c", "cura");
    let (plan, _) = plan_new(
        &root,
        "ac",
        NewSpec::Def {
            definition: "John Appleseed",
        },
    )
    .unwrap();
    plan.apply(&root).unwrap();
    let tree = build_tree(&root, None).unwrap();
    insta::assert_snapshot!("verb_tree", pretty(&tree.to_json()));
}

#[test]
fn verb_resolve() {
    let root = fresh_root();
    mint(&root, "root", "c", "contextus");
    mint(&root, "c", "s", "societas");
    mint(&root, "cs", "a", "amicitia");
    let node = resolve_code(&root, &Code::parse("csa").unwrap()).unwrap();
    let meta = node.join("csa__");
    std::fs::create_dir_all(&meta).unwrap();
    std::fs::write(
        meta.join("csa__person__alex.json"),
        r#"{"refs":[],"data":{}}"#,
    )
    .unwrap();

    let reg = CoreRegistry::from_cores(vec![DiscoveredCore {
        name: "album".to_string(),
        short: "alb".to_string(),
        kinds: vec![("person".to_string(), Shape::Partitioned)],
        format_version: 1,
    }]);
    let refs = [
        pantheon::Ref::parse("album:alex").unwrap(),
        pantheon::Ref::parse("mappa:home").unwrap(),
    ];
    let outcomes = resolve_all(&root, &reg, &refs).unwrap();
    insta::assert_snapshot!(
        "verb_resolve",
        pretty(&pantheon::resolve::outcomes_json(&outcomes))
    );
}

#[test]
fn verb_validate_clean() {
    let root = fresh_root();
    mint(&root, "root", "a", "actio");
    let reg = CoreRegistry::from_cores(vec![]);
    let findings = validate(&root, &reg).unwrap();
    insta::assert_snapshot!(
        "verb_validate_clean",
        pretty(&pantheon::validate::findings_json(&findings))
    );
}

// ── the error envelope (§7.3) ───────────────────────────────────────────────

#[test]
fn error_envelope_per_code() {
    let errors = [
        Error::runtime("io failed"),
        Error::usage("bad flag"),
        Error::validation("slug collision"),
        Error::not_found("no such node"),
        Error::write_refused("under a rule"),
    ];
    let mut out = String::new();
    for e in &errors {
        out.push_str(&format!(
            "exit {} => {}\n",
            e.exit_code().as_u8(),
            e.to_error_json()
        ));
    }
    insta::assert_snapshot!("error_envelope_per_code", out);
}
