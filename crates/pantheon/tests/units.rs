//! Property unit tests for the spine's contract surface (§5, §7). The JSON output
//! shapes are frozen separately by the snapshots in `contract.rs`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use pantheon::code::parse_node_dirname;
use pantheon::mint::NewSpec;
use pantheon::{
    Code, CoreRegistry, DiscoveredCore, FindingCode, Ref, RefOutcome, Shape, build_tree, normalize,
    plan_new, resolve_all, resolve_code, validate, with_record_lock,
};

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn fresh_root() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("pan-it-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn mint(root: &Path, parent: &str, spec: NewSpec) {
    let (plan, _) = plan_new(root, parent, spec).unwrap();
    plan.apply(root).unwrap();
}

fn triple<'a>(ch: &'a str, label: &'a str) -> NewSpec<'a> {
    NewSpec::Triple { ch, label }
}

fn write_record(root: &Path, code: &str, filename: &str, contents: &str) {
    let node = resolve_code(root, &Code::parse(code).unwrap()).unwrap();
    let meta = node.join(format!("{code}__"));
    std::fs::create_dir_all(&meta).unwrap();
    std::fs::write(meta.join(filename), contents).unwrap();
}

fn album_registry() -> CoreRegistry {
    CoreRegistry::from_cores(vec![DiscoveredCore {
        name: "album".to_string(),
        short: "alb".to_string(),
        kinds: vec![("person".to_string(), Shape::Partitioned)],
        format_version: 1,
    }])
}

#[test]
fn normalize_is_idempotent_total_and_correct() {
    let cases = [
        "Hello World",
        "a--b__c",
        "  __x__  ",
        "Ångström",
        "café",
        "u.s.a!",
        "MixedCASE",
        "YouTube-Video",
    ];
    for c in cases {
        if let Some(n) = normalize(c) {
            assert_eq!(
                normalize(&n).as_deref(),
                Some(n.as_str()),
                "idempotent: {c:?}"
            );
            assert!(!n.contains("__"), "never contains __: {c:?} -> {n:?}");
        }
    }
    assert_eq!(normalize(""), None);
    assert_eq!(normalize("-"), None);
    assert_eq!(normalize("___"), None);
    assert_eq!(normalize("Hello World").as_deref(), Some("hello_world"));
    assert_eq!(normalize("a--b__c").as_deref(), Some("a_b_c"));
    assert_eq!(normalize("YouTube-Video").as_deref(), Some("youtube_video"));
    // Composed vs decomposed acute agree after NFC (§5.1).
    assert_eq!(normalize("cafe\u{0301}"), normalize("caf\u{00e9}"));
}

#[test]
fn code_tokenizes_reparents_and_rejects() {
    let csa = Code::parse("csa").unwrap();
    assert!(csa.is_compact());
    assert_eq!(csa.tokenize_compact().unwrap().len(), 3);
    assert_eq!(csa.parent_compact().unwrap().as_str(), "cs");

    // A numeric level is two digits: asdl01 -> a,s,d,l,01.
    let numeric = Code::parse("asdl01").unwrap();
    assert_eq!(numeric.tokenize_compact().unwrap().len(), 5);

    // Definition-prefix codes carry `_`, do not tokenize, and have no string parent.
    let def = Code::parse("csa_john_appleseed").unwrap();
    assert!(!def.is_compact());
    assert!(def.tokenize_compact().is_err());
    assert!(def.parent_compact().is_none());

    assert!(
        Code::parse("0a").is_err(),
        "a code never opens with a digit"
    );
    assert!(Code::parse("a__b").is_err(), "a code never contains __");
    assert!(Code::parse("_a").is_err(), "a code never has a leading _");
}

#[test]
fn node_dirname_tells_the_two_forms_apart() {
    let sphere = parse_node_dirname(None, "a_actio").unwrap();
    assert_eq!(sphere.code.as_str(), "a");
    assert_eq!(sphere.label, "actio");
    assert!(sphere.ch.is_some());

    let cs = Code::parse("cs").unwrap();
    let triple = parse_node_dirname(Some(&cs), "cs_a_amicitia").unwrap();
    assert_eq!(triple.code.as_str(), "csa");
    assert_eq!(triple.label, "amicitia");

    let csa = Code::parse("csa").unwrap();
    let def = parse_node_dirname(Some(&csa), "csa_john_appleseed_").unwrap();
    assert_eq!(def.code.as_str(), "csa_john_appleseed");
    assert!(def.ch.is_none());
    assert_eq!(def.label, "john_appleseed");
}

#[test]
fn ref_roundtrips_as_a_bare_string() {
    let r = Ref::parse("album:alex").unwrap();
    assert_eq!(serde_json::to_string(&r).unwrap(), "\"album:alex\"");
    let back: Ref = serde_json::from_str("\"album:alex\"").unwrap();
    assert_eq!(back, r);
}

#[test]
fn plan_token_is_deterministic_and_change_sensitive() {
    let root = fresh_root();
    let (a1, _) = plan_new(&root, "root", triple("a", "actio")).unwrap();
    let (a2, _) = plan_new(&root, "root", triple("a", "actio")).unwrap();
    assert_eq!(a1.token(), a2.token());

    let (b, _) = plan_new(&root, "root", triple("b", "bonum")).unwrap();
    assert_ne!(a1.token(), b.token());
    assert!(a1.check_token(&a1.token()).is_ok());
    assert!(a1.check_token("stale").is_err());
}

#[test]
fn new_refuses_a_code_collision() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    // A second sphere reusing char `c` collides (§5.3).
    assert!(plan_new(&root, "root", triple("c", "corpus")).is_err());
}

#[test]
fn record_lock_reads_prev_then_writes() {
    let root = fresh_root();
    let path = root.join("rec.json");
    with_record_lock(&path, |prev| {
        assert!(prev.is_none());
        Ok(b"first".to_vec())
    })
    .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"first");
    with_record_lock(&path, |prev| {
        assert_eq!(prev, Some(&b"first"[..]));
        Ok(b"second".to_vec())
    })
    .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"second");
}

#[test]
fn resolve_lists_ambiguous_and_reports_unresolved() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    mint(&root, "c", triple("s", "societas"));
    mint(&root, "cs", triple("a", "amicitia"));
    mint(&root, "cs", triple("b", "beata"));
    write_record(
        &root,
        "csa",
        "csa__person__alex.json",
        r#"{"refs":[],"data":{}}"#,
    );
    write_record(
        &root,
        "csb",
        "csb__person__alex.json",
        r#"{"refs":[],"data":{}}"#,
    );

    let reg = album_registry();
    let refs = [
        Ref::parse("album:alex").unwrap(),
        Ref::parse("album:nobody").unwrap(),
    ];
    let outcomes = resolve_all(&root, &reg, &refs).unwrap();
    assert!(matches!(outcomes[0], RefOutcome::Ambiguous(_)));
    assert!(matches!(outcomes[1], RefOutcome::Unresolved(_)));
}

#[test]
fn resolve_finds_a_unique_entity() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    mint(&root, "c", triple("s", "societas"));
    mint(&root, "cs", triple("a", "amicitia"));
    write_record(
        &root,
        "csa",
        "csa__person__alex.json",
        r#"{"refs":[],"data":{}}"#,
    );

    let reg = album_registry();
    let outcomes = resolve_all(&root, &reg, &[Ref::parse("album:alex").unwrap()]).unwrap();
    match &outcomes[0] {
        RefOutcome::Resolved(r) => {
            assert_eq!(r.home.as_str(), "csa");
            assert_eq!(r.kind, "person");
        }
        other => panic!("expected Resolved, got {other:?}"),
    }
}

#[test]
fn validate_flags_a_dangling_ref() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    mint(&root, "c", triple("s", "societas"));
    mint(&root, "cs", triple("a", "amicitia"));
    write_record(
        &root,
        "csa",
        "csa__person__alex.json",
        r#"{"refs":["album:ghost"],"data":{}}"#,
    );

    let reg = album_registry();
    let findings = validate(&root, &reg).unwrap();
    assert!(
        findings.iter().any(|f| f.code == FindingCode::DanglingRef),
        "{findings:?}"
    );
}

#[test]
fn a_clean_minted_tree_validates() {
    let root = fresh_root();
    mint(&root, "root", triple("a", "actio"));
    mint(&root, "a", triple("c", "cura"));
    let reg = CoreRegistry::from_cores(vec![]);
    assert!(validate(&root, &reg).unwrap().is_empty());
    // And it reads back as a forest.
    assert!(matches!(
        build_tree(&root, None).unwrap(),
        pantheon::TreeRoot::Forest(_)
    ));
}

#[test]
fn root_flag_beats_env() {
    let want = Path::new("/tmp/pan-root-flag");
    assert_eq!(
        pantheon::resolve_root(Some(want)).unwrap(),
        PathBuf::from(want)
    );
}

#[test]
fn a_stale_plan_token_is_a_validation_error() {
    let root = fresh_root();
    let (plan, _) = plan_new(&root, "root", triple("a", "actio")).unwrap();
    let err = plan.check_token("deadbeef").unwrap_err();
    assert_eq!(err.exit_code(), pantheon::ExitCode::Validation);
}
