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

// ── the partitioned register (§6.1, step 3) ─────────────────────────────────

/// A stand-in for Album: three tokens, all partitioned, one flat record.
#[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema, Default, Debug)]
struct Agent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    closeness: Option<String>,
}

struct Reg;
impl pantheon::Core for Reg {
    type Record = Agent;
    const NAME: &'static str = "album";
    fn kinds() -> &'static [(&'static str, Shape)] {
        &[
            ("person", Shape::Partitioned),
            ("organization", Shape::Partitioned),
            ("group", Shape::Partitioned),
        ]
    }
    fn validate(_record: &Agent) -> pantheon::Result<()> {
        Ok(())
    }
}

/// `c` → `cs` → `csa`, plus a `cso` sibling and a definition-prefix node under `csa`.
fn societas_root() -> PathBuf {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    mint(&root, "c", triple("s", "societas"));
    mint(&root, "cs", triple("a", "amicitia"));
    mint(&root, "cs", triple("o", "officium"));
    mint(
        &root,
        "csa",
        NewSpec::Def {
            definition: "john_appleseed",
        },
    );
    root
}

fn code(s: &str) -> Code {
    Code::parse(s).unwrap()
}

fn addr(home: &str, kind: &str, slug: &str) -> pantheon::EntityAddr {
    pantheon::EntityAddr {
        home: code(home),
        kind: kind.to_string(),
        slug: slug.to_string(),
    }
}

fn file_name_of(path: &Path) -> String {
    path.file_name().unwrap().to_string_lossy().into_owned()
}

#[test]
fn an_entity_writes_and_reads_back_by_slug() {
    let root = societas_root();
    let store = pantheon::Store::<Reg>::new(&root);
    let record = Agent {
        closeness: Some("close".to_string()),
    };
    let written = store
        .write_entity(&addr("csa", "person", "mara"), vec![], &record)
        .unwrap();
    assert_eq!(file_name_of(&written.path), "csa__person__mara.json");
    assert_eq!(written.form, pantheon::EntityForm::Partitioned);

    let (eref, entity) = store.get_entity("mara", None, None).unwrap();
    assert_eq!(eref.home.as_str(), "csa");
    assert_eq!(eref.kind, "person");
    assert_eq!(entity.data.closeness.as_deref(), Some("close"));
}

#[test]
fn an_entity_at_its_own_node_drops_the_slug_segment() {
    let root = societas_root();
    let store = pantheon::Store::<Reg>::new(&root);
    // The slug *is* the node's definition, so the filename carries only the kind (§5.2).
    let written = store
        .write_entity(
            &addr("csa_john_appleseed", "person", "john_appleseed"),
            vec![],
            &Agent::default(),
        )
        .unwrap();
    assert_eq!(
        file_name_of(&written.path),
        "csa_john_appleseed__person.json"
    );
    assert_eq!(written.form, pantheon::EntityForm::AsNode);

    // And the walk supplies the slug the filename does not carry.
    let (eref, _) = store.get_entity("john_appleseed", None, None).unwrap();
    assert_eq!(eref.slug, "john_appleseed");
    assert_eq!(eref.form, pantheon::EntityForm::AsNode);

    // A *different* slug at that same node is an ordinary partitioned file.
    let other = store
        .write_entity(
            &addr("csa_john_appleseed", "person", "someone_else"),
            vec![],
            &Agent::default(),
        )
        .unwrap();
    assert_eq!(
        file_name_of(&other.path),
        "csa_john_appleseed__person__someone_else.json"
    );
}

#[test]
fn a_slug_another_kind_holds_is_taken_at_that_node() {
    let root = societas_root();
    let store = pantheon::Store::<Reg>::new(&root);
    store
        .write_entity(
            &addr("csa", "group", "book_club"),
            vec![],
            &Agent::default(),
        )
        .unwrap();
    // Two files, one ref — the filesystem would permit what the ref namespace does not.
    let taken = store.slug_taken_at(&code("csa"), "book_club").unwrap();
    assert_eq!(taken.unwrap().kind, "group");
    assert!(store.slug_taken_at(&code("csa"), "mara").unwrap().is_none());
    // Another node is another matter — that check is a walk, so it stays soft.
    assert!(
        store
            .slug_taken_at(&code("cso"), "book_club")
            .unwrap()
            .is_none()
    );
}

#[test]
fn a_cross_node_duplicate_stays_soft_but_is_reported() {
    let root = societas_root();
    let store = pantheon::Store::<Reg>::new(&root);
    for home in ["csa", "cso"] {
        store
            .write_entity(&addr(home, "person", "alex"), vec![], &Agent::default())
            .unwrap();
    }
    // Both were written: nothing hard refused them (§5.4, §18).
    assert_eq!(
        store.find_entities(None, None, Some("alex")).unwrap().len(),
        2
    );
    let elsewhere = store
        .duplicate_slugs_elsewhere(&code("csa"), "alex")
        .unwrap();
    assert_eq!(elsewhere.len(), 1);
    assert_eq!(elsewhere[0].home.as_str(), "cso");
    // But a resolve meeting two lists them rather than guessing (§7.3).
    let err = store.get_entity("alex", None, None).unwrap_err();
    assert_eq!(err.exit_code(), pantheon::ExitCode::Usage);
    // Scoped to one node it is unambiguous again.
    assert!(store.get_entity("alex", None, Some(&code("cso"))).is_ok());
}

#[test]
fn relocate_is_the_one_primitive_behind_move_kind_and_rename() {
    let root = societas_root();
    let store = pantheon::Store::<Reg>::new(&root);
    let e = store
        .write_entity(&addr("csa", "person", "johnn"), vec![], &Agent::default())
        .unwrap();

    // rename: a new slug.
    let e = store
        .relocate_entity(&e, &addr("csa", "person", "john"))
        .unwrap();
    assert_eq!(file_name_of(&e.path), "csa__person__john.json");
    // edit -k: a new kind, same slug.
    let e = store
        .relocate_entity(&e, &addr("csa", "organization", "john"))
        .unwrap();
    assert_eq!(file_name_of(&e.path), "csa__organization__john.json");
    // move: a new home.
    let e = store
        .relocate_entity(&e, &addr("cso", "organization", "john"))
        .unwrap();
    assert_eq!(e.home.as_str(), "cso");
    assert!(e.path.exists());
    assert_eq!(
        store
            .find_entities(Some(&code("csa")), None, None)
            .unwrap()
            .len(),
        0
    );

    // And it refuses to clobber an occupied path rather than overwrite it.
    store
        .write_entity(&addr("cso", "person", "mara"), vec![], &Agent::default())
        .unwrap();
    let err = store
        .relocate_entity(&e, &addr("cso", "person", "mara"))
        .unwrap_err();
    assert_eq!(err.exit_code(), pantheon::ExitCode::Validation);
}

#[test]
fn the_entity_walk_counts_only_this_cores_kinds() {
    let root = societas_root();
    let store = pantheon::Store::<Reg>::new(&root);
    store
        .write_entity(&addr("csa", "person", "mara"), vec![], &Agent::default())
        .unwrap();
    // Another core's entity, and this core's own series, at the same node (§5.0).
    write_record(&root, "csa", "csa__location__home.json", "{\"data\":{}}\n");
    write_record(&root, "csa", "csa__log__weight.jsonl", "");
    let found = store.find_entities(None, None, None).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].slug, "mara");
}
