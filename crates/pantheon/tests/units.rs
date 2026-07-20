//! Property unit tests for the spine's contract surface (§5, §7). The JSON output
//! shapes are frozen separately by the snapshots in `contract.rs`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use pantheon::code::parse_node_dirname;
use pantheon::mint::NewSpec;
use pantheon::{
    Code, CoreRegistry, DiscoveredCore, FindingCode, Key, Line, Ref, RefOutcome, SeriesRef,
    Severity, Shape, build_tree, normalize, plan_mv, plan_mv_file, plan_new, plan_rename, plan_rm,
    resolve_all, resolve_code, validate, with_record_lock,
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
fn rm_removes_an_empty_node_and_refuses_a_full_one() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    mint(&root, "c", triple("s", "societas"));
    mint(&root, "cs", triple("a", "amicitia"));

    // A node with a child is refused (§10.1, exit 3).
    let err = plan_rm(&root, &Code::parse("cs").unwrap()).unwrap_err();
    assert_eq!(err.exit_code(), pantheon::ExitCode::Validation);

    // A node holding a record is refused too.
    write_record(
        &root,
        "csa",
        "csa__person__mara.json",
        r#"{"refs":[],"data":{}}"#,
    );
    assert!(plan_rm(&root, &Code::parse("csa").unwrap()).is_err());

    // Emptied, the leaf removes — meta scaffold and all — and the parent survives.
    std::fs::remove_file(
        root.join("c_contextus/c_s_societas/cs_a_amicitia/csa__/csa__person__mara.json"),
    )
    .unwrap();
    let (plan, _) = plan_rm(&root, &Code::parse("csa").unwrap()).unwrap();
    plan.apply(&root).unwrap();
    assert!(!root.join("c_contextus/c_s_societas/cs_a_amicitia").exists());
    assert!(root.join("c_contextus/c_s_societas").is_dir());
}

#[test]
fn rename_char_cascades_dirs_and_files_over_the_whole_branch() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    mint(&root, "c", triple("s", "societas"));
    mint(&root, "cs", triple("a", "amicitia"));
    write_record(
        &root,
        "cs",
        "cs__group__club.json",
        r#"{"refs":[],"data":{}}"#,
    );
    write_record(
        &root,
        "csa",
        "csa__person__mara.json",
        r#"{"refs":[],"data":{}}"#,
    );

    let (plan, _) = plan_rename(&root, &Code::parse("cs").unwrap(), Some("t"), None, None).unwrap();
    plan.apply(&root).unwrap();

    // Every level's dir, meta dir, and record file followed cs -> ct.
    assert!(
        root.join("c_contextus/c_t_societas/ct__/ct__group__club.json")
            .is_file()
    );
    assert!(
        root.join("c_contextus/c_t_societas/ct_a_amicitia/cta__/cta__person__mara.json")
            .is_file()
    );
    assert!(!root.join("c_contextus/c_s_societas").exists());
    // No error-severity finding: the tree is consistent after the cascade.
    assert!(
        !validate(&root, &album_registry())
            .unwrap()
            .iter()
            .any(|f| f.severity == Severity::Error)
    );
}

#[test]
fn rename_label_moves_only_the_node_dir() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    mint(&root, "c", triple("s", "societas"));
    mint(&root, "cs", triple("a", "amicitia"));

    let (plan, _) = plan_rename(
        &root,
        &Code::parse("cs").unwrap(),
        None,
        Some("guild"),
        None,
    )
    .unwrap();
    plan.apply(&root).unwrap();

    // The label changed; the code did not, so the child keeps its `cs` prefix.
    assert!(root.join("c_contextus/c_s_guild/cs_a_amicitia").is_dir());
    assert!(!root.join("c_contextus/c_s_societas").exists());
}

#[test]
fn mv_rehomes_and_cascades_and_refuses_a_cycle() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    mint(&root, "c", triple("s", "societas"));
    mint(&root, "cs", triple("a", "amicitia"));
    mint(&root, "root", triple("d", "disciplina"));
    write_record(
        &root,
        "csa",
        "csa__person__mara.json",
        r#"{"refs":[],"data":{}}"#,
    );

    // A node cannot move into its own descendant.
    assert!(plan_mv(&root, &Code::parse("c").unwrap(), "cs").is_err());

    let (plan, _) = plan_mv(&root, &Code::parse("cs").unwrap(), "d").unwrap();
    plan.apply(&root).unwrap();
    assert!(
        root.join("d_disciplina/d_s_societas/ds_a_amicitia/dsa__/dsa__person__mara.json")
            .is_file()
    );
    assert!(!root.join("c_contextus/c_s_societas").exists());
}

#[test]
fn mv_file_rehomes_a_misfiled_record() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    mint(&root, "c", triple("s", "societas"));
    mint(&root, "cs", triple("a", "amicitia"));
    // A `csa` record misfiled in `cs`'s meta dir (§10.2's misfile case).
    write_record(
        &root,
        "cs",
        "csa__person__mara.json",
        r#"{"refs":[],"data":{}}"#,
    );

    let misfiled = std::path::Path::new("c_contextus/c_s_societas/cs__/csa__person__mara.json");
    let (plan, _) = plan_mv_file(&root, misfiled, &Code::parse("csa").unwrap()).unwrap();
    plan.apply(&root).unwrap();

    assert!(
        root.join("c_contextus/c_s_societas/cs_a_amicitia/csa__/csa__person__mara.json")
            .is_file()
    );
    assert!(
        !root
            .join("c_contextus/c_s_societas/cs__/csa__person__mara.json")
            .exists()
    );
}

#[test]
fn rename_char_refuses_a_sibling_collision() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    mint(&root, "c", triple("s", "societas"));
    mint(&root, "c", triple("t", "tempus"));
    // Renaming cs's char to `t` would collide with the existing sibling ct (§5.3).
    assert!(plan_rename(&root, &Code::parse("cs").unwrap(), Some("t"), None, None).is_err());
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

/// A determined series **whose determinant is a slug** carries a name in its filename
/// and is still not a ref target (§5.4, §7.1) — its name slot holds its determinant, not
/// an identity. `classify` cannot see the difference: `crp__balance__checking.jsonl` has
/// three segments, so it is structurally a `NamedSeries` and rightly says so — **only the
/// registry's `named` bit tells the two apart**. So the reader must ask the registry, or
/// `rationes:checking` resolves ambiguously between a holding and its own balance file,
/// and `pan validate` calls the pair a duplicate slug. Pensum's determined series is
/// nameless and Fasti's `event` is hand-named, so Rationes is the first shape that reaches
/// this at all.
#[test]
fn a_determined_series_is_never_a_ref_target_even_when_it_carries_a_name() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    mint(&root, "c", triple("r", "res"));
    mint(&root, "cr", triple("p", "pecunia"));
    write_record(
        &root,
        "crp",
        "crp__account__checking.json",
        r#"{"refs":[],"data":{"currency":"usd"}}"#,
    );
    write_record(
        &root,
        "crp",
        "crp__balance__checking.jsonl",
        "{\"key\":\"260718\",\"refs\":[],\"data\":{\"amount\":4200.0}}\n",
    );

    let reg = CoreRegistry::from_cores(vec![DiscoveredCore {
        name: "rationes".to_string(),
        short: "rat".to_string(),
        kinds: vec![
            ("account".to_string(), Shape::Partitioned),
            ("balance".to_string(), Shape::Series { named: false }),
        ],
        format_version: 1,
    }]);

    let outcomes = resolve_all(&root, &reg, &[Ref::parse("rationes:checking").unwrap()]).unwrap();
    match &outcomes[0] {
        RefOutcome::Resolved(r) => assert_eq!(
            r.kind, "account",
            "the holding is the record; its balance file is reached through it"
        ),
        other => panic!("expected the holding alone, got {other:?}"),
    }

    // The same index feeds `validate`, so the spurious duplicate dies with it.
    let findings = validate(&root, &reg).unwrap();
    assert!(
        !findings
            .iter()
            .any(|f| f.code == FindingCode::DuplicateSlug),
        "a holding and its own balance series are one record, not two names: {findings:?}"
    );
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
fn a_non_normalized_label_carries_its_normalizing_fix() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    // A child node whose label is not in normal form. Minting normalizes (§5.1), so this
    // is written by hand — the way a stray `mkdir` would leave it (I8).
    std::fs::create_dir_all(root.join("c_contextus").join("c_x_Bad_Label")).unwrap();

    let findings = validate(&root, &album_registry()).unwrap();
    let finding = findings
        .iter()
        .find(|f| f.code == FindingCode::NonNormalizedName)
        .expect("the non-normalized label is reported");
    // §10.2: the single legal correction, surfaced as the command that applies it — the
    // normal form of the label, at the node's own code.
    assert_eq!(
        finding.fix.as_deref(),
        Some("pan rename cx --label bad_label"),
        "{finding:?}"
    );
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

fn pensum_registry() -> CoreRegistry {
    CoreRegistry::from_cores(vec![DiscoveredCore {
        name: "pensum".to_string(),
        short: "pen".to_string(),
        kinds: vec![("task".to_string(), Shape::Series { named: false })],
        format_version: 1,
    }])
}

#[test]
fn a_name_keyed_line_resolves_but_a_date_keyed_one_does_not() {
    let root = societas_root();
    // The last line is date-keyed: the key's own shape is the second gate, so a
    // sample registers nothing even inside a series whose lines are targets.
    write_record(
        &root,
        "csa",
        "csa__task.jsonl",
        "{\"key\":\"reach_out_to_alex\",\"refs\":[],\"data\":{}}\n\
         {\"key\":\"file_taxes\",\"refs\":[],\"data\":{}}\n\
         {\"key\":\"260718\",\"refs\":[],\"data\":{}}\n",
    );

    let reg = pensum_registry();
    let want = [
        Ref::parse("pensum:reach_out_to_alex").unwrap(),
        Ref::parse("pensum:file_taxes").unwrap(),
        Ref::parse("pensum:260718").unwrap(),
        Ref::parse("pensum:never_written").unwrap(),
    ];
    let out = resolve_all(&root, &reg, &want).unwrap();

    let RefOutcome::Resolved(one) = &out[0] else {
        panic!("a task is reached by its key (§5.4): {:?}", out[0])
    };
    assert_eq!(one.home.as_str(), "csa");
    assert_eq!(one.kind, "task");
    // The resolution points at the series file the line lives in (I3).
    assert!(one.rel_path.ends_with("csa__task.jsonl"));

    assert!(matches!(out[1], RefOutcome::Resolved(_)));
    assert!(
        matches!(out[2], RefOutcome::Unresolved(_)),
        "a date-keyed line is a sample, never a target (I1)"
    );
    assert!(matches!(out[3], RefOutcome::Unresolved(_)));
}

#[test]
fn a_ref_to_a_task_no_longer_dangles() {
    let root = societas_root();
    write_record(
        &root,
        "csa",
        "csa__task.jsonl",
        "{\"key\":\"reach_out_to_alex\",\"refs\":[],\"data\":{}}\n",
    );
    // Another record pointing at the task — the edge §8.5 says a task is reached by.
    write_record(
        &root,
        "cso",
        "cso__task.jsonl",
        "{\"key\":\"chase_it_up\",\"refs\":[\"pensum:reach_out_to_alex\"],\"data\":{}}\n",
    );
    let findings = validate(&root, &pensum_registry()).unwrap();
    let dangling: Vec<_> = findings
        .iter()
        .filter(|f| f.code == FindingCode::DanglingRef)
        .collect();
    assert!(dangling.is_empty(), "{dangling:?}");
}

#[test]
fn one_task_key_at_two_nodes_is_the_soft_duplicate_finding() {
    let root = societas_root();
    for code in ["csa", "cso"] {
        write_record(
            &root,
            code,
            &format!("{code}__task.jsonl"),
            "{\"key\":\"file_taxes\",\"refs\":[],\"data\":{}}\n",
        );
    }
    let findings = validate(&root, &pensum_registry()).unwrap();
    let duplicates: Vec<_> = findings
        .iter()
        .filter(|f| f.code == FindingCode::DuplicateSlug)
        .collect();
    // Both files are named, because the fix is made at the source (§5.4).
    assert_eq!(duplicates.len(), 2, "{findings:?}");
    assert!(duplicates.iter().all(|f| f.severity == Severity::Warning));
}

// ── the record lock under contention (§6.4, step 4) ─────────────────────────

#[test]
fn concurrent_writers_all_land_on_one_nameless_series() {
    // The file a detached hook and a hand contend for (§6.4, §16 step 4). Eight
    // writers, none of which finds the file there to begin with, so every one of
    // them is racing to mint it as well as to append.
    let root = societas_root();
    let home = Code::parse("csa").unwrap();
    let store = pantheon::Store::<Nameless>::new(&root);
    let sref = SeriesRef {
        home: home.clone(),
        kind: "task".to_string(),
        name: None,
        path: store.series_path(&home, "task", None).unwrap(),
    };
    assert!(!sref.path.exists(), "the race starts with no file at all");

    std::thread::scope(|scope| {
        for w in 0..8 {
            let store = &store;
            let sref = &sref;
            scope.spawn(move || {
                for i in 0..20 {
                    store
                        .write_line(sref, &line(&format!("task_{w}_{i}")))
                        .unwrap();
                }
            });
        }
    });

    let lines = std::fs::read_to_string(&sref.path).unwrap();
    let keys: HashSet<&str> = lines.lines().filter(|l| !l.trim().is_empty()).collect();
    // 160 distinct lines, none lost. A writer that read bytes another had already
    // replaced would drop that other's work silently — which is the whole reason
    // the lock re-checks the inode after acquiring (lock.rs).
    assert_eq!(keys.len(), 160, "every writer's lines must survive");
}

#[test]
fn a_writer_whose_file_was_renamed_underneath_retries() {
    // The inode re-check, deterministically. B locks the file A is about to swap;
    // when A's temp-and-rename lands, B is holding a lock on an inode the path no
    // longer names, and must re-open rather than write into the void (§6.4).
    let root = fresh_root();
    let path = root.join("contended.jsonl");
    std::fs::write(&path, "").unwrap();

    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();

    std::thread::scope(|scope| {
        let p = path.as_path();
        scope.spawn(move || {
            with_record_lock(p, |prev| {
                // Inside A's lock: let B start, and hold until it is queued.
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                let mut out = prev.unwrap_or_default().to_vec();
                out.extend_from_slice(b"written_by_a\n");
                Ok(out)
            })
            .unwrap();
        });

        entered_rx.recv().unwrap();
        scope.spawn(move || {
            with_record_lock(p, |prev| {
                let mut out = prev.unwrap_or_default().to_vec();
                out.extend_from_slice(b"written_by_b\n");
                Ok(out)
            })
            .unwrap();
        });

        // B is now blocked on A's lock. Letting A finish swaps the inode under it.
        std::thread::sleep(std::time::Duration::from_millis(50));
        release_tx.send(()).unwrap();
    });

    let out = std::fs::read_to_string(&path).unwrap();
    assert!(
        out.contains("written_by_a"),
        "A's write must survive B: {out:?}"
    );
    assert!(out.contains("written_by_b"), "B's write must land: {out:?}");
}

#[test]
fn the_cascade_refuses_renaming_a_task_onto_an_occupied_key() {
    let root = societas_root();
    write_record(
        &root,
        "csa",
        "csa__task.jsonl",
        "{\"key\":\"reach_out_to_alex\",\"refs\":[],\"data\":{}}\n\
         {\"key\":\"call_alex\",\"refs\":[],\"data\":{}}\n",
    );
    let own = ["task"];
    let from = Ref::parse("pensum:reach_out_to_alex").unwrap();

    // Onto a key its own file already holds.
    let err = pantheon::plan_cascade(&root, &own, &from, &Ref::parse("pensum:call_alex").unwrap())
        .unwrap_err();
    assert_eq!(err.exit_code(), pantheon::ExitCode::Validation);

    // And onto one another node holds — the check is tree-wide (§7.2).
    write_record(
        &root,
        "cso",
        "cso__task.jsonl",
        "{\"key\":\"ring_alex\",\"refs\":[],\"data\":{}}\n",
    );
    assert!(
        pantheon::plan_cascade(&root, &own, &from, &Ref::parse("pensum:ring_alex").unwrap())
            .is_err()
    );

    // A free name still plans cleanly.
    let clean = pantheon::plan_cascade(
        &root,
        &own,
        &from,
        &Ref::parse("pensum:email_alex").unwrap(),
    )
    .unwrap();
    assert!(clean.rewrites.is_empty());
}

#[test]
fn a_task_key_is_not_an_identity_to_a_core_that_does_not_own_the_token() {
    // `own_kinds` is the calling core's, so Album renaming an entity never trips
    // over a Pensum task that happens to share the name (I5, §5.4).
    let root = societas_root();
    write_record(
        &root,
        "csa",
        "csa__task.jsonl",
        "{\"key\":\"mara\",\"refs\":[],\"data\":{}}\n",
    );
    let plan = pantheon::plan_cascade(
        &root,
        &["person"],
        &Ref::parse("album:maara").unwrap(),
        &Ref::parse("album:mara").unwrap(),
    );
    assert!(plan.is_ok(), "another core's key is not ours to guard");
}

#[test]
fn a_date_keyed_line_never_blocks_a_rename() {
    // A sample is not an identity (I1), so it cannot occupy a name.
    let root = societas_root();
    write_record(
        &root,
        "csa",
        "csa__task.jsonl",
        "{\"key\":\"260718\",\"refs\":[],\"data\":{}}\n",
    );
    assert!(
        pantheon::plan_cascade(
            &root,
            &["task"],
            &Ref::parse("pensum:a").unwrap(),
            &Ref::parse("pensum:260718").unwrap(),
        )
        .is_ok()
    );
}

// ── reaching, re-keying, and re-homing a line (§5.4, §7.2) ──────────────────

/// A tree with one task at `csa` and two at `cso`, all through the store.
fn tasked_root() -> (PathBuf, pantheon::Store<Nameless>) {
    let root = societas_root();
    let store = pantheon::Store::<Nameless>::new(&root);
    for (code, keys) in [
        ("csa", &["reach_out_to_alex"][..]),
        ("cso", &["file_taxes", "book_flights"][..]),
    ] {
        let home = Code::parse(code).unwrap();
        let sref = SeriesRef {
            home: home.clone(),
            kind: "task".to_string(),
            name: None,
            path: store.series_path(&home, "task", None).unwrap(),
        };
        for key in keys {
            store.write_line(&sref, &line(key)).unwrap();
        }
    }
    (root, store)
}

#[test]
fn a_key_is_reached_tree_wide_and_ambiguity_is_listed_not_guessed() {
    let (root, store) = tasked_root();
    let (sref, found) = store
        .locate_line(&Key::parse("file_taxes").unwrap(), None, None)
        .unwrap();
    assert_eq!(sref.home.as_str(), "cso");
    assert_eq!(found.key.as_str(), "file_taxes");

    let missing = store
        .locate_line(&Key::parse("never_written").unwrap(), None, None)
        .unwrap_err();
    assert_eq!(missing.exit_code(), pantheon::ExitCode::NotFound);

    // The same key at two nodes: listed with its homes, never guessed (§7.3).
    let home = Code::parse("csa").unwrap();
    let sref = SeriesRef {
        home: home.clone(),
        kind: "task".to_string(),
        name: None,
        path: store.series_path(&home, "task", None).unwrap(),
    };
    store.write_line(&sref, &line("file_taxes")).unwrap();
    let ambiguous = store
        .locate_line(&Key::parse("file_taxes").unwrap(), None, None)
        .unwrap_err();
    assert_eq!(ambiguous.exit_code(), pantheon::ExitCode::Usage);

    // And that second one is the soft cross-node duplicate `add` warns on (§5.4).
    let elsewhere = store
        .duplicate_keys_elsewhere(&home, &Key::parse("file_taxes").unwrap(), None)
        .unwrap();
    assert_eq!(elsewhere.len(), 1);
    assert_eq!(elsewhere[0].home.as_str(), "cso");
    drop(root);
}

#[test]
fn renaming_a_line_moves_its_key_and_leaves_its_neighbours_verbatim() {
    let (root, store) = tasked_root();
    let home = Code::parse("cso").unwrap();
    let path = store.series_path(&home, "task", None).unwrap();
    let sref = SeriesRef {
        home: home.clone(),
        kind: "task".to_string(),
        name: None,
        path: path.clone(),
    };
    let before: Vec<String> = std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();

    store
        .rename_line(
            &sref,
            &Key::parse("file_taxes").unwrap(),
            &Key::parse("do_the_taxes").unwrap(),
        )
        .unwrap();

    let after: Vec<String> = std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    assert!(after[0].contains("do_the_taxes"));
    assert_eq!(after[1], before[1], "an untouched line is carried verbatim");

    // Onto a key the same file already holds: exit 3, and nothing is written.
    let err = store
        .rename_line(
            &sref,
            &Key::parse("do_the_taxes").unwrap(),
            &Key::parse("book_flights").unwrap(),
        )
        .unwrap_err();
    assert_eq!(err.exit_code(), pantheon::ExitCode::Validation);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap().lines().count(),
        2,
        "a refused rename writes nothing"
    );
    drop(root);
}

#[test]
fn moving_a_line_lands_it_at_the_destination_and_drops_the_source() {
    let (root, store) = tasked_root();
    let from_home = Code::parse("cso").unwrap();
    let to_home = Code::parse("csa").unwrap();
    let source = SeriesRef {
        home: from_home.clone(),
        kind: "task".to_string(),
        name: None,
        path: store.series_path(&from_home, "task", None).unwrap(),
    };
    let key = Key::parse("file_taxes").unwrap();

    let dest = store.move_line(&source, &to_home, &key).unwrap();
    assert_eq!(dest.home.as_str(), "csa");
    assert!(dest.path.ends_with("csa__task.jsonl"));

    // Exactly one home holds it now — the move is not a copy.
    let found = store.find_line(&key, None, None).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0.home.as_str(), "csa");
    // The line the source keeps is untouched.
    let left = store.read_series(&source).unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].key.as_str(), "book_flights");
    drop(root);
}

#[test]
fn moving_onto_an_occupied_key_is_refused_rather_than_clobbering() {
    let (root, store) = tasked_root();
    let from_home = Code::parse("cso").unwrap();
    let to_home = Code::parse("csa").unwrap();
    let source = SeriesRef {
        home: from_home.clone(),
        kind: "task".to_string(),
        name: None,
        path: store.series_path(&from_home, "task", None).unwrap(),
    };
    // Give the destination a task of the same name first.
    let dest = SeriesRef {
        home: to_home.clone(),
        kind: "task".to_string(),
        name: None,
        path: store.series_path(&to_home, "task", None).unwrap(),
    };
    store.write_line(&dest, &line("file_taxes")).unwrap();

    let err = store
        .move_line(&source, &to_home, &Key::parse("file_taxes").unwrap())
        .unwrap_err();
    assert_eq!(err.exit_code(), pantheon::ExitCode::Validation);
    // Both sides intact: a refused move loses nothing.
    assert_eq!(store.read_series(&source).unwrap().len(), 2);
    assert_eq!(store.read_series(&dest).unwrap().len(), 2);
    drop(root);
}

#[test]
fn the_keyed_target_probes_its_leading_token_for_a_node_code() {
    let (root, store) = tasked_root();
    let elsewhere = root.join("cs_something_else");
    std::fs::create_dir_all(&elsewhere).unwrap();

    let resolve = |positionals: &[&str], home: Option<&str>| {
        let owned: Vec<String> = positionals.iter().map(|s| (*s).to_string()).collect();
        pantheon::contract::resolve_register_target(
            &store,
            &pantheon::RegisterQuery {
                kind: "task",
                home,
                positionals: &owned,
                // A locus that is not one of the codes under test, so an accidental
                // fallback cannot be mistaken for a correct probe.
                pwd: Some(
                    root.join("c_contextus/c_s_societas/cs_o_officium")
                        .as_path(),
                ),
            },
        )
    };

    // `csa` is a code and something follows it: home, then key.
    let t = resolve(&["csa", "reach_out_to_alex"], None).unwrap();
    assert_eq!(t.home.as_str(), "csa");
    assert_eq!(t.key.as_str(), "reach_out_to_alex");
    assert!(t.values.is_empty());
    assert!(t.existing.is_some(), "csa already holds a task series");

    // Not a code: key, then the trailing value stream. Home falls to the locus.
    let t = resolve(&["reach_out_to_alex", "call re: the contract"], None).unwrap();
    assert_eq!(t.home.as_str(), "cso");
    assert_eq!(t.key.as_str(), "reach_out_to_alex");
    assert_eq!(t.values, ["call re: the contract"]);

    // Three tokens are unambiguous.
    let t = resolve(&["csa", "reach_out_to_alex", "text"], None).unwrap();
    assert_eq!(
        (t.home.as_str(), t.key.as_str()),
        ("csa", "reach_out_to_alex")
    );
    assert_eq!(t.values, ["text"]);

    // A lone token is the key — a home with the name missing addresses nothing.
    let t = resolve(&["csa"], None).unwrap();
    assert_eq!((t.home.as_str(), t.key.as_str()), ("cso", "csa"));

    // `-H` short-circuits the probe, so a task really named `csa` is reachable.
    let t = resolve(&["csa", "text"], Some("csa")).unwrap();
    assert_eq!((t.home.as_str(), t.key.as_str()), ("csa", "csa"));
    assert_eq!(t.values, ["text"]);

    // A key is normalized on the way in, never hand-typed as a slug (§5.4).
    let t = resolve(&["Reach Out To Alex"], None).unwrap();
    assert_eq!(t.key.as_str(), "reach_out_to_alex");

    // Naming nothing is a usage error.
    assert!(matches!(
        resolve(&[], None),
        Err(e) if e.exit_code() == pantheon::ExitCode::Usage
    ));

    // At a node with no task file yet, the target resolves and `existing` is None —
    // that is the write that mints it (§8.5).
    let t = resolve(&["cs", "first_task"], None).unwrap();
    assert_eq!(t.home.as_str(), "cs");
    assert!(t.existing.is_none());
    drop(elsewhere);
}

// ── the document fence (§6.6) ──────────────────────────────────────────────────

/// A hand-written note carries no fence, and Tabella handles every loose document in
/// place (§8.7) — so no fence is an empty envelope over a whole-file body, not an error.
#[test]
fn a_document_without_a_fence_is_all_body() {
    let doc = pantheon::document::parse("just prose\nand more\n").unwrap();
    assert_eq!(doc.frontmatter, pantheon::Frontmatter::default());
    assert_eq!(doc.body, "just prose\nand more\n");

    // A `+++` that is not the first line is body, not a fence.
    let doc = pantheon::document::parse("prose\n+++\ntype = \"x\"\n+++\n").unwrap();
    assert_eq!(doc.frontmatter.r#type, None);
    assert!(doc.body.starts_with("prose\n+++"));
}

#[test]
fn the_fence_carries_type_and_tags_over_an_opaque_body() {
    let text =
        "+++\ntype = \"principium\"\ntags = [\"mores\", \"vocatio\"]\n+++\n\nProse starts here.\n";
    let doc = pantheon::document::parse(text).unwrap();
    assert_eq!(doc.frontmatter.r#type.as_deref(), Some("principium"));
    assert_eq!(doc.frontmatter.tags, ["mores", "vocatio"]);
    // The blank line after the closing fence is the fence's, not the body's.
    assert_eq!(doc.body, "Prose starts here.\n");
}

/// An opening fence with no closing one is malformed: exit `3`, not a silent
/// reinterpretation of the whole file as body.
#[test]
fn an_unterminated_fence_is_a_validation_failure() {
    for text in ["+++\ntype = \"x\"\n", "+++\n", "+++"] {
        let err = pantheon::document::parse(text).unwrap_err();
        assert_eq!(err.exit_code(), pantheon::ExitCode::Validation, "{text:?}");
    }
}

#[test]
fn the_fence_scanner_accepts_crlf() {
    let text = "+++\r\ntype = \"nota\"\r\n+++\r\n\r\nProse.\r\n";
    let doc = pantheon::document::parse(text).unwrap();
    assert_eq!(doc.frontmatter.r#type.as_deref(), Some("nota"));
    assert_eq!(doc.body, "Prose.\r\n");
}

/// The fold reads frontmatter only (§7.2) — so TOML *below* the closing fence is
/// prose and must not leak into the envelope.
#[test]
fn read_frontmatter_stops_at_the_closing_fence() {
    let dir = fresh_root();
    let path = dir.join("note.md");
    std::fs::write(
        &path,
        "+++\ntype = \"right\"\n+++\n\ntype = \"wrong\"\ntags = [\"leaked\"]\n",
    )
    .unwrap();

    let fm = pantheon::read_frontmatter(&path).unwrap();
    assert_eq!(fm.r#type.as_deref(), Some("right"));
    assert!(fm.tags.is_empty(), "body TOML must not reach the envelope");

    // A fence-less file yields the empty envelope rather than an error.
    let bare = dir.join("bare.md");
    std::fs::write(&bare, "no fence here\n").unwrap();
    assert_eq!(
        pantheon::read_frontmatter(&bare).unwrap(),
        pantheon::Frontmatter::default()
    );
}

/// §6.6: all TOML is `toml_edit`'s, so comments and key ordering survive a rewrite by
/// code or LLM (I6, I8). This is the claim that forbids a serde round-trip here — and
/// the reason [`pantheon::Document`] carries `front_raw` rather than reconstructing
/// the fence from its two known fields.
#[test]
fn a_frontmatter_rewrite_preserves_comments_order_and_unread_keys() {
    let text = "+++\n# why this note exists\ntags = [\"mores\"]\nauthor = \"a hand\"\ntype = \"principium\"\n+++\n\nProse.\n";
    let mut doc = pantheon::document::parse(text).unwrap();
    doc.frontmatter.tags.push("vocatio".into());

    let out = doc.to_text().unwrap();
    assert!(
        out.contains("# why this note exists"),
        "comment lost:\n{out}"
    );
    assert!(
        out.contains("author = \"a hand\""),
        "a key Tabella does not read was dropped:\n{out}"
    );
    assert!(
        out.find("tags").unwrap() < out.find("type").unwrap(),
        "key order lost:\n{out}"
    );
    assert!(out.contains("vocatio"), "edit not applied:\n{out}");
    // Reparsing is stable, and the body came through untouched.
    assert_eq!(pantheon::document::parse(&out).unwrap().body, "Prose.\n");
}

/// A rewrite must not convert a CRLF file's line endings (§6.6, I6).
#[test]
fn a_rewrite_keeps_the_files_own_line_endings() {
    let doc = pantheon::document::parse("+++\r\ntype = \"nota\"\r\n+++\r\n\r\nProse.\r\n").unwrap();
    assert!(doc.crlf);
    let out = doc.to_text().unwrap();
    assert!(
        out.starts_with("+++\r\n"),
        "fence converted to LF:\n{out:?}"
    );
    assert!(out.ends_with("Prose.\r\n"), "body altered:\n{out:?}");
}
