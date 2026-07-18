//! Property unit tests for the spine's contract surface (§5, §7). The JSON output
//! shapes are frozen separately by the snapshots in `contract.rs`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use pantheon::code::parse_node_dirname;
use pantheon::mint::NewSpec;
use pantheon::{
    Code, CoreRegistry, DiscoveredCore, FindingCode, Key, Line, Ref, RefOutcome, SeriesRef, Shape,
    build_tree, normalize, plan_new, resolve_all, resolve_code, validate, with_record_lock,
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

#[test]
fn validate_reports_a_cross_node_duplicate_softly() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    mint(&root, "c", triple("s", "societas"));
    mint(&root, "cs", triple("a", "amicitia"));
    mint(&root, "cs", triple("o", "officium"));
    for (code, dir) in [("csa", "csa"), ("cso", "cso")] {
        write_record(
            &root,
            code,
            &format!("{dir}__person__alex.json"),
            r#"{"refs":[],"data":{}}"#,
        );
    }

    let findings = validate(&root, &album_registry()).unwrap();
    let dupes: Vec<_> = findings
        .iter()
        .filter(|f| f.code == FindingCode::DuplicateSlug)
        .collect();
    // Every file holding the name is named — the fix is made at the source (§5.4).
    assert_eq!(dupes.len(), 2, "{findings:?}");
    assert!(
        dupes
            .iter()
            .all(|f| f.severity == pantheon::Severity::Warning)
    );
    assert!(dupes[0].msg.contains("album:alex"), "{:?}", dupes[0].msg);
    // Soft: a warning is not a validation failure (§5.4, §18).
    assert!(
        !findings
            .iter()
            .any(|f| f.severity == pantheon::Severity::Error),
        "a duplicate slug must never harden into an error: {findings:?}"
    );

    // One name at one node is not a duplicate, however many kinds the core has.
    let clean = fresh_root();
    mint(&clean, "root", triple("c", "contextus"));
    mint(&clean, "c", triple("s", "societas"));
    mint(&clean, "cs", triple("a", "amicitia"));
    write_record(
        &clean,
        "csa",
        "csa__person__alex.json",
        r#"{"refs":[],"data":{}}"#,
    );
    assert!(validate(&clean, &album_registry()).unwrap().is_empty());
}

#[test]
fn a_change_body_names_a_series_only_when_there_is_one() {
    let base = pantheon::RecordChange {
        verb: "add",
        core: "annales".to_string(),
        home: "ecv".to_string(),
        kind: "log".to_string(),
        series: Some("weight".to_string()),
        key: "260718".to_string(),
        before: None,
        after: Some(serde_json::json!({"values": ["78.4"]})),
        cascade: None,
    };

    // The plan token is redacted in every snapshot, so it is pinned here instead:
    // this is the exact byte string a Series change has always hashed. An edit to
    // `body()` that reorders, adds, or renames a key breaks *this* rather than
    // silently invalidating every token a hand is holding (§7.3).
    let series_body = r#"{"after":{"values":["78.4"]},"before":null,"core":"annales","home":"ecv","key":"260718","kind":"log","series":"weight","verb":"add"}"#;
    let token_of = |c: &pantheon::RecordChange| c.to_json()["change"].to_string();
    assert_eq!(token_of(&base), series_body);

    // A partitioned core keeps no series, so the key is absent rather than hollow —
    // and the cascade appears only on the verb that has one.
    let entity = pantheon::RecordChange {
        core: "album".to_string(),
        kind: "person".to_string(),
        series: None,
        key: "mara".to_string(),
        ..base.clone()
    };
    let body = token_of(&entity);
    assert!(!body.contains("series"), "{body}");
    assert!(!body.contains("cascade"), "{body}");

    let renamed = pantheon::RecordChange {
        verb: "rename",
        cascade: Some(serde_json::json!([{"path": "x", "refs": 1}])),
        ..entity.clone()
    };
    assert!(token_of(&renamed).contains("cascade"));
    // And the cascade is part of the identity it guards: a tree that grew a fourth
    // ref since the review must not pass the token check (§7.3).
    assert_ne!(entity.token(), renamed.token());
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

// ── the rename cascade (§5.4, step 3) ───────────────────────────────────────

const OWN: &[&str] = &["person", "organization", "group"];

fn r(token: &str) -> Ref {
    Ref::parse(token).unwrap()
}

#[test]
fn the_cascade_rewrites_refs_across_cores_and_shapes() {
    let root = societas_root();
    let store = pantheon::Store::<Reg>::new(&root);
    store
        .write_entity(
            &addr("csa", "person", "mara"),
            vec![r("album:johnn"), r("album:book_club")],
            &Agent::default(),
        )
        .unwrap();
    // Another core's series, pointing at the same person (§5.4).
    write_record(
        &root,
        "cso",
        "cso__log__standups.jsonl",
        "{\"key\":\"260718\",\"refs\":[\"album:johnn\"],\"data\":{\"values\":[\"ok\"]}}\n\
         {\"key\":\"260719\",\"refs\":[],\"data\":{\"values\":[\"none\"]}}\n",
    );

    let plan = pantheon::plan_cascade(&root, OWN, &r("album:johnn"), &r("album:john")).unwrap();
    assert_eq!(plan.totals(), (2, 2), "two refs, in two files");
    plan.apply(&root).unwrap();

    // The entity's ref moved; its sibling ref did not.
    let (_, mara) = store.get_entity("mara", None, None).unwrap();
    let tokens: Vec<String> = mara.refs.iter().map(pantheon::Ref::to_token).collect();
    assert_eq!(tokens, vec!["album:john", "album:book_club"]);

    // And so did the other core's line — without disturbing the one beside it.
    let series = std::fs::read_to_string(
        resolve_code(&root, &code("cso"))
            .unwrap()
            .join("cso__")
            .join("cso__log__standups.jsonl"),
    )
    .unwrap();
    assert!(series.contains(r#""refs":["album:john"]"#), "{series}");
    assert!(
        series.contains(r#"{"key":"260719","refs":[],"data":{"values":["none"]}}"#),
        "the untouched line is carried through verbatim: {series}"
    );
}

#[test]
fn the_cascade_leaves_the_data_half_alone() {
    let root = societas_root();
    // A record whose `data` a core owns and the spine must not touch (I5) — note the
    // key order, which a parse-and-reserialize round trip would sort.
    write_record(
        &root,
        "csa",
        "csa__person__mara.json",
        "{\n  \"refs\": [\n    \"album:johnn\"\n  ],\n  \"data\": {\n    \"zeta\": \"1\",\n    \"alpha\": \"2\"\n  }\n}\n",
    );
    pantheon::plan_cascade(&root, OWN, &r("album:johnn"), &r("album:john"))
        .unwrap()
        .apply(&root)
        .unwrap();

    let after = std::fs::read_to_string(
        resolve_code(&root, &code("csa"))
            .unwrap()
            .join("csa__")
            .join("csa__person__mara.json"),
    )
    .unwrap();
    assert!(after.contains("album:john"));
    let zeta = after.find("zeta").expect("zeta survives");
    let alpha = after.find("alpha").expect("alpha survives");
    assert!(
        zeta < alpha,
        "data key order is the core's, not ours: {after}"
    );
}

#[test]
fn the_cascade_refuses_an_occupied_slug() {
    let root = societas_root();
    let store = pantheon::Store::<Reg>::new(&root);
    for (home, slug) in [("csa", "johnn"), ("cso", "john")] {
        store
            .write_entity(&addr(home, "person", slug), vec![], &Agent::default())
            .unwrap();
    }
    // Tree-wide and hard, unlike `add`'s cross-node warning (§7.2 vs §18).
    let err = pantheon::plan_cascade(&root, OWN, &r("album:johnn"), &r("album:john")).unwrap_err();
    assert_eq!(err.exit_code(), pantheon::ExitCode::Validation);

    // A free name is fine, and a name held by *another* core's record is not ours.
    assert!(pantheon::plan_cascade(&root, OWN, &r("album:johnn"), &r("album:jon")).is_ok());
    write_record(
        &root,
        "csa",
        "csa__location__jon.json",
        r#"{"refs":[],"data":{}}"#,
    );
    assert!(pantheon::plan_cascade(&root, OWN, &r("album:johnn"), &r("album:jon")).is_ok());
}

#[test]
fn a_cascade_with_nothing_to_rewrite_is_a_clean_no_op() {
    let root = societas_root();
    let plan = pantheon::plan_cascade(&root, OWN, &r("album:nobody"), &r("album:someone")).unwrap();
    assert_eq!(plan.totals(), (0, 0));
    assert_eq!(plan.to_json(), serde_json::json!([]));
    plan.apply(&root).unwrap();
}

#[test]
fn a_cascade_plan_is_stable_so_its_token_is() {
    let root = societas_root();
    let store = pantheon::Store::<Reg>::new(&root);
    for slug in ["a", "b", "c", "d"] {
        store
            .write_entity(
                &addr("csa", "person", slug),
                vec![r("album:johnn")],
                &Agent::default(),
            )
            .unwrap();
    }
    let once = pantheon::plan_cascade(&root, OWN, &r("album:johnn"), &r("album:john")).unwrap();
    let twice = pantheon::plan_cascade(&root, OWN, &r("album:johnn"), &r("album:john")).unwrap();
    assert_eq!(
        once.to_json(),
        twice.to_json(),
        "readdir order must not leak"
    );
    assert_eq!(once.totals(), (4, 4));
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

// ── the nameless series (§7.1, step 4) ──────────────────────────────────────

/// A stand-in for Pensum: one token, filed as a **nameless** series.
#[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema, Default, Debug)]
struct Doing {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    done: Option<String>,
}

struct Nameless;
impl pantheon::Core for Nameless {
    type Record = Doing;
    const NAME: &'static str = "pensum";
    fn kinds() -> &'static [(&'static str, Shape)] {
        &[("task", Shape::Series { named: false })]
    }
    fn validate(_record: &Doing) -> pantheon::Result<()> {
        Ok(())
    }
}

/// A stand-in for Annales: one token, hand-named.
struct Named;
impl pantheon::Core for Named {
    type Record = Doing;
    const NAME: &'static str = "annales";
    fn kinds() -> &'static [(&'static str, Shape)] {
        &[("log", Shape::Series { named: true })]
    }
    fn validate(_record: &Doing) -> pantheon::Result<()> {
        Ok(())
    }
}

fn line(key: &str) -> Line<Doing> {
    Line {
        key: Key::parse(key).unwrap(),
        refs: vec![],
        data: Doing::default(),
    }
}

#[test]
fn the_walk_sees_a_nameless_series() {
    let root = societas_root();
    write_record(&root, "csa", "csa__task.jsonl", "");
    // A hand-named series of another core at the same node is not ours (§5.0).
    write_record(&root, "csa", "csa__log__weight.jsonl", "");

    let found = pantheon::Store::<Nameless>::new(&root)
        .find_series(None, None, None)
        .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].kind, "task");
    // Nameless is the whole point: there is no name slot to have filled.
    assert_eq!(found[0].name, None);
    assert_eq!(found[0].label(), "task");
}

#[test]
fn a_name_filter_never_matches_a_nameless_series() {
    let root = societas_root();
    write_record(&root, "csa", "csa__task.jsonl", "");
    let store = pantheon::Store::<Nameless>::new(&root);
    assert!(
        store
            .find_series(None, None, Some("task"))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        store.find_series(None, Some("task"), None).unwrap().len(),
        1
    );
}

#[test]
fn the_first_task_mints_the_series_and_a_named_one_still_refuses() {
    let root = societas_root();
    let home = Code::parse("csa").unwrap();

    // Nameless: minted by its determinant — the node's first task — not by `-c`
    // (§7.3, §18). The file does not exist until this write.
    let store = pantheon::Store::<Nameless>::new(&root);
    let sref = SeriesRef {
        home: home.clone(),
        kind: "task".to_string(),
        name: None,
        path: store.series_path(&home, "task", None).unwrap(),
    };
    assert!(!sref.path.exists());
    store.write_line(&sref, &line("reach_out_to_alex")).unwrap();
    assert!(sref.path.exists());
    assert!(sref.path.ends_with("csa__task.jsonl"));

    // Hand-named: still a not-found, because a typo must not conjure a log (§7.3).
    let named = pantheon::Store::<Named>::new(&root);
    let missing = SeriesRef {
        home: home.clone(),
        kind: "log".to_string(),
        name: Some("wieght".to_string()),
        path: named.series_path(&home, "log", Some("wieght")).unwrap(),
    };
    let err = named.write_line(&missing, &line("260718")).unwrap_err();
    assert_eq!(err.exit_code(), pantheon::ExitCode::NotFound);
    assert!(!missing.path.exists(), "a refused write mints nothing");
}

#[test]
fn a_name_keyed_line_is_its_own_present() {
    let root = societas_root();
    let home = Code::parse("csa").unwrap();
    let store = pantheon::Store::<Nameless>::new(&root);
    let sref = SeriesRef {
        home: home.clone(),
        kind: "task".to_string(),
        name: None,
        path: store.series_path(&home, "task", None).unwrap(),
    };
    for key in ["reach_out_to_alex", "file_taxes", "book_flights"] {
        store.write_line(&sref, &line(key)).unwrap();
    }

    // Every task survives the fold: a task is a record, not a sample (I1, §5.4).
    let folded = store.fold(None, None).unwrap();
    let mut keys: Vec<&str> = folded.iter().map(|p| p.line.key.as_str()).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["book_flights", "file_taxes", "reach_out_to_alex"]);
    assert!(folded.iter().all(|p| p.name.is_none()));
}

#[test]
fn a_date_keyed_series_still_folds_to_its_latest() {
    let root = societas_root();
    let home = Code::parse("csa").unwrap();
    let store = pantheon::Store::<Named>::new(&root);
    let sref = store.create_series(&home, "log", "weight").unwrap();
    for key in ["260718", "260720", "260719"] {
        store.write_line(&sref, &line(key)).unwrap();
    }
    let folded = store.fold(None, None).unwrap();
    assert_eq!(folded.len(), 1, "a sampled series folds to one present");
    assert_eq!(folded[0].line.key.as_str(), "260720");
    assert_eq!(folded[0].name.as_deref(), Some("weight"));
}
