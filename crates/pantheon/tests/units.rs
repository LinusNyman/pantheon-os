//! Property unit tests for the spine's contract surface (§5, §7). The JSON output
//! shapes are frozen separately by the snapshots in `contract.rs`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use pantheon::code::parse_node_dirname;
use pantheon::mint::NewSpec;
use pantheon::{
    Change, Code, CoreRegistry, DiscoveredCore, FindingCode, Key, Line, Plan, Ref, RefOutcome,
    SeriesRef, Severity, Shape, build_tree, normalize, plan_merge, plan_mv, plan_mv_files,
    plan_new, plan_rename, plan_rename_def, plan_rename_pattern, plan_rename_prefix, plan_rm,
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

    let misfiled = std::path::PathBuf::from("c_contextus/c_s_societas/cs__/csa__person__mara.json");
    let (plan, _) = plan_mv_files(&root, &[misfiled], &Code::parse("csa").unwrap()).unwrap();
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
fn rename_def_reslugs_the_entity_and_cascades_its_refs() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    mint(&root, "c", triple("s", "societas"));
    mint(&root, "cs", triple("a", "amicitia"));
    mint(&root, "cs", triple("b", "beata"));
    // A definition-prefix node under csa with an album person promoted to it.
    mint(
        &root,
        "csa",
        NewSpec::Def {
            definition: "john_appleseed",
        },
    );
    write_record(
        &root,
        "csa_john_appleseed",
        "csa_john_appleseed__person.json",
        r#"{"refs":[],"data":{}}"#,
    );
    // A record elsewhere in the tree that references album:john_appleseed.
    write_record(
        &root,
        "csb",
        "csb__person__mara.json",
        r#"{"refs":["album:john_appleseed"],"data":{}}"#,
    );

    let (plan, _) = plan_rename_def(
        &root,
        &Code::parse("csa_john_appleseed").unwrap(),
        "john_smith",
        &album_registry(),
    )
    .unwrap();
    plan.apply(&root).unwrap();

    // The node dir, meta dir, and entity file followed the new definition.
    assert!(
        root.join(
            "c_contextus/c_s_societas/cs_a_amicitia/csa_john_smith_/csa_john_smith__/\
             csa_john_smith__person.json"
        )
        .is_file()
    );
    // The external ref was rewritten to the new slug.
    let mara = std::fs::read_to_string(
        root.join("c_contextus/c_s_societas/cs_b_beata/csb__/csb__person__mara.json"),
    )
    .unwrap();
    assert!(mara.contains("album:john_smith"), "{mara}");
    assert!(!mara.contains("john_appleseed"), "{mara}");
}

#[test]
fn rename_prefix_repairs_a_drifted_code_prefix() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    mint(&root, "c", triple("s", "societas"));
    mint(&root, "cs", triple("o", "officium"));
    // A `csa`-prefixed file stranded in `cso`'s meta dir — what a crashed rename leaves.
    write_record(
        &root,
        "cso",
        "csa__person__x.json",
        r#"{"refs":[],"data":{}}"#,
    );

    let (plan, _) =
        plan_rename_prefix(&root, "csa", "cso", Some(&Code::parse("cso").unwrap())).unwrap();
    plan.apply(&root).unwrap();

    assert!(
        root.join("c_contextus/c_s_societas/cs_o_officium/cso__/cso__person__x.json")
            .is_file()
    );
    assert!(
        !root
            .join("c_contextus/c_s_societas/cs_o_officium/cso__/csa__person__x.json")
            .exists()
    );
}

#[test]
fn rename_pattern_reslugs_records_and_cascades_refs() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    mint(&root, "c", triple("s", "societas"));
    mint(&root, "cs", triple("a", "amicitia"));
    mint(&root, "cs", triple("b", "beata"));
    // A person with a typo'd slug, and a record elsewhere referencing it.
    write_record(
        &root,
        "csa",
        "csa__person__johnn.json",
        r#"{"refs":[],"data":{}}"#,
    );
    write_record(
        &root,
        "csb",
        "csb__person__mara.json",
        r#"{"refs":["album:johnn"],"data":{}}"#,
    );

    let (plan, _) = plan_rename_pattern(&root, "johnn", "john", None, &album_registry()).unwrap();
    plan.apply(&root).unwrap();

    // The slug'd file was renamed.
    assert!(
        root.join("c_contextus/c_s_societas/cs_a_amicitia/csa__/csa__person__john.json")
            .is_file()
    );
    assert!(
        !root
            .join("c_contextus/c_s_societas/cs_a_amicitia/csa__/csa__person__johnn.json")
            .exists()
    );
    // The ref followed the re-slug.
    let mara = std::fs::read_to_string(
        root.join("c_contextus/c_s_societas/cs_b_beata/csb__/csb__person__mara.json"),
    )
    .unwrap();
    assert!(
        mara.contains("album:john") && !mara.contains("johnn"),
        "{mara}"
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
    // **The genuine choice is enumerated, never made** (§10.2). No single legal
    // correction, so `fix` is absent; one candidate per holder that could take the
    // fuller name, and both findings carry the same list because the choice is between
    // the holders rather than a property of the file you are looking at.
    for dupe in &dupes {
        assert!(dupe.fix.is_none(), "no single answer here: {dupe:?}");
        assert_eq!(
            dupe.candidates,
            vec![
                "pan rename-pattern alex alex_csa csa".to_string(),
                "pan rename-pattern alex alex_cso cso".to_string(),
            ],
            "one command per holder, in tree order: {dupe:?}"
        );
    }
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

// ── G4: the grant cascade and the dead grant (§9.2, §10.1, §10.2) ────────────

/// Write a rule file at a node's meta dir. **`pan new` mints no meta dir** — one appears
/// on first write — so a fixture placing a rule has to `create_dir_all` it.
fn write_rule(root: &Path, code: &str, name: &str, contents: &str) {
    write_record(
        root,
        code,
        &format!("{code}__function__{name}.sh"),
        contents,
    );
}

/// **A recode re-homes the grants that named the branch** (§9.2, §10.1).
///
/// `writes=core@home` names a *node*, and a rename changes what the node is called. A
/// grant left behind points at a code nothing carries, and since the grant is the whole
/// guard (§9.5) it would go on authorizing nothing in silence — so the rename cascades it
/// exactly as it cascades a ref.
///
/// Both halves are proved at once: a rule **inside** the branch (whose own file moves) and
/// one **outside** it (which does not), because a rule may grant writes at any node, not
/// only the one it sits at.
#[test]
fn a_rename_cascades_the_writes_grants_that_named_the_branch() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    mint(&root, "c", triple("s", "societas"));
    mint(&root, "cs", triple("a", "amicitia"));
    mint(&root, "root", triple("a", "actio"));

    // Inside the branch: the rule's own file is renamed by the recode too.
    write_rule(
        &root,
        "csa",
        "nudge",
        "#!/bin/sh\n# auspex: watch=pensum writes=pensum@csa:add;annales@csa/hours:add \
         desc=csa in the prose stays\necho '{}'\n",
    );
    // Outside it: same grant, a file nothing moves.
    write_rule(
        &root,
        "a",
        "watcher",
        "#!/bin/sh\n# auspex: writes=pensum@csa:add;album@a:add\necho '{}'\n",
    );

    // `cs a amicitia` → `cs t amicitia`: the code goes `csa` → `cst` (§5.1).
    let (plan, _) =
        plan_rename(&root, &Code::parse("csa").unwrap(), Some("t"), None, None).unwrap();
    plan.apply(&root).unwrap();

    let inside = std::fs::read_to_string(
        root.join("c_contextus/c_s_societas/cs_t_amicitia/cst__/cst__function__nudge.sh"),
    )
    .expect("the rule moved with its node and kept its name");
    assert!(
        inside.contains("writes=pensum@cst:add;annales@cst/hours:add"),
        "the grant follows the node it named: {inside}"
    );
    assert!(
        inside.contains("desc=csa in the prose stays"),
        "only the grant went stale — a hand's sentence is not the tools' to edit: {inside}"
    );
    assert!(inside.contains("watch=pensum"), "{inside}");

    let outside =
        std::fs::read_to_string(root.join("a_actio/a__/a__function__watcher.sh")).unwrap();
    assert!(
        outside.contains("writes=pensum@cst:add;album@a:add"),
        "a grant is cascaded wherever the rule sits, and only the entry that moved: {outside}"
    );
}

/// A label-only rename moves a directory and no code, so **no grant went stale**.
#[test]
fn a_label_rename_leaves_every_grant_alone() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    write_rule(
        &root,
        "c",
        "nudge",
        "#!/bin/sh\n# auspex: writes=pensum@c:add\necho '{}'\n",
    );

    let (plan, _) = plan_rename(
        &root,
        &Code::parse("c").unwrap(),
        None,
        Some("communitas"),
        None,
    )
    .unwrap();
    plan.apply(&root).unwrap();

    let text =
        std::fs::read_to_string(root.join("c_communitas/c__/c__function__nudge.sh")).unwrap();
    assert!(text.contains("writes=pensum@c:add"), "{text}");
}

/// **A grant naming no node is reported, never honoured** (§9.2, §10.2).
///
/// The guard's own declaration having gone dead is exactly the failure nothing else would
/// surface: the rule runs, proposes, and every proposal is refused, with no error to read.
/// A warning, because the *tree* is consistent — and failing closed is the safe direction.
#[test]
fn validate_reports_a_grant_naming_no_node() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    write_rule(
        &root,
        "c",
        "nudge",
        "#!/bin/sh\n# auspex: writes=pensum@c:add;annales@zzz:add\necho '{}'\n",
    );

    let findings = validate(&root, &album_registry()).unwrap();
    let dead: Vec<_> = findings
        .iter()
        .filter(|f| f.code == FindingCode::DeadHeaderCode)
        .collect();
    assert_eq!(dead.len(), 1, "only the entry naming nothing: {findings:?}");
    assert_eq!(dead[0].severity, Severity::Warning);
    assert!(dead[0].msg.contains("zzz"), "{}", dead[0].msg);
    assert!(
        !findings
            .iter()
            .any(|f| f.severity == pantheon::Severity::Error),
        "a dead grant is not a broken tree: {findings:?}"
    );

    // And a rule whose grants all name live nodes says nothing.
    let clean = fresh_root();
    mint(&clean, "root", triple("c", "contextus"));
    write_rule(
        &clean,
        "c",
        "nudge",
        "#!/bin/sh\n# auspex: writes=pensum@c:add\necho '{}'\n",
    );
    assert!(
        !validate(&clean, &album_registry())
            .unwrap()
            .iter()
            .any(|f| f.code == FindingCode::DeadHeaderCode),
        "a live grant is not a finding"
    );
}

/// **`rename-prefix` cascades grants too** (§10.2, §9.2).
///
/// The repair renames child *directories*, so a node's code really does change and a
/// `writes=core@home` naming it really is stale — §10.2 says it cascades "exactly as
/// `rename` and `mv` do", and it does.
#[test]
fn rename_prefix_cascades_the_grants_the_repair_invalidates() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    mint(&root, "c", triple("s", "societas"));
    // A hand's `mkdir` left a child carrying the wrong code prefix: `cs` under `c`, when
    // its own name says `ct`. The repair rewrites the prefix over the scope.
    write_rule(
        &root,
        "cs",
        "nudge",
        "#!/bin/sh\n# auspex: writes=pensum@cs:add\necho '{}'\n",
    );

    let (plan, _) =
        plan_rename_prefix(&root, "cs", "ct", Some(&Code::parse("c").unwrap())).unwrap();
    plan.apply(&root).unwrap();

    let text =
        std::fs::read_to_string(root.join("c_contextus/c_s_societas/ct__/ct__function__nudge.sh"))
            .expect("the rule kept its name past the prefix rewrite");
    assert!(text.contains("writes=pensum@ct:add"), "{text}");
}

// ── the pre-flight: no plan may destroy what it did not name (§5.4, §10.1) ────

/// **A rename refuses to overwrite a file at its target** — the defect that made this
/// pre-flight necessary.
///
/// `std::fs::rename` replaces whatever sits at the destination, and a recode plans one
/// rename per file whose code prefix changes. A stray `csa`-prefixed file beside the
/// `cs`-prefixed one it will be renamed to was therefore destroyed silently, at exit `0`.
///
/// The load-bearing assertion is the second pair: **both files are still there**. The
/// refusal is only worth having because nothing moved.
#[test]
fn a_rename_refuses_to_overwrite_a_file_at_its_target() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    mint(&root, "c", triple("s", "societas"));
    mint(&root, "cs", triple("a", "amicitia"));
    let node = root.join("c_contextus/c_s_societas/cs_a_amicitia");
    std::fs::write(node.join("csa_notes.md"), "the real notes").unwrap();
    // A stray carrying the code the rename is about to produce.
    std::fs::write(node.join("cst_notes.md"), "a misfiled stray").unwrap();

    let (plan, _) =
        plan_rename(&root, &Code::parse("csa").unwrap(), Some("t"), None, None).unwrap();
    let err = plan.apply(&root).unwrap_err();
    assert_eq!(err.exit_code(), pantheon::ExitCode::Validation);
    assert!(err.to_string().contains("cst_notes.md"), "{err}");

    // Two files went in and two are still here, under the name they had.
    assert_eq!(
        std::fs::read_to_string(node.join("csa_notes.md")).unwrap(),
        "the real notes"
    );
    assert_eq!(
        std::fs::read_to_string(node.join("cst_notes.md")).unwrap(),
        "a misfiled stray"
    );
}

/// **The pre-flight sees past the renames the plan itself makes.**
///
/// A recode renames the branch's directory *first*, so the colliding file's planned
/// destination (`cs_t_.../csa_x.md` → `cs_t_.../cst_x.md`) names a directory that does not
/// exist yet, while the file it would destroy sits under the old one. Asking the disk
/// about the destination as written finds nothing and waves the plan through — which is
/// exactly how this shipped. Each virtual path is mapped back through the plan's own
/// renames before the tree is asked, and **every** collision is named at once, so one
/// dry-run answers for the whole plan.
#[test]
fn the_preflight_maps_a_path_back_through_the_plans_own_renames() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    mint(&root, "c", triple("s", "societas"));
    mint(&root, "cs", triple("a", "amicitia"));
    let node = root.join("c_contextus/c_s_societas/cs_a_amicitia");
    for (name, body) in [
        ("csa_one.md", "one"),
        ("cst_one.md", "blocker one"),
        ("csa_two.md", "two"),
        ("cst_two.md", "blocker two"),
    ] {
        std::fs::write(node.join(name), body).unwrap();
    }

    let (plan, _) =
        plan_rename(&root, &Code::parse("csa").unwrap(), Some("t"), None, None).unwrap();
    let err = plan.preflight(&root).unwrap_err();
    let msg = err.to_string();
    // Both, not the first — a dry-run that named one problem at a time would take four
    // rounds to clear a branch.
    assert!(msg.contains("cst_one.md"), "{msg}");
    assert!(msg.contains("cst_two.md"), "{msg}");
    assert!(msg.starts_with("2 of this plan's renames"), "{msg}");
}

/// **`rename-prefix` refuses a collision across two nodes** — the second half of the same
/// defect (three files in, three out).
#[test]
fn rename_prefix_refuses_a_collision_across_two_nodes() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    mint(&root, "c", triple("s", "societas"));
    mint(&root, "cs", triple("a", "amicitia"));
    mint(&root, "cs", triple("o", "officium"));
    let amicitia = root.join("c_contextus/c_s_societas/cs_a_amicitia");
    let officium = root.join("c_contextus/c_s_societas/cs_o_officium");
    std::fs::write(amicitia.join("csa_x.md"), "amicitia's own").unwrap();
    std::fs::write(amicitia.join("cso_x.md"), "a stray from officium").unwrap();
    std::fs::write(officium.join("cso_x.md"), "officium's own").unwrap();

    let (plan, _) = plan_rename_prefix(&root, "csa", "cso", None).unwrap();
    assert!(plan.apply(&root).is_err());
    assert_eq!(
        std::fs::read_to_string(amicitia.join("csa_x.md")).unwrap(),
        "amicitia's own"
    );
    assert_eq!(
        std::fs::read_to_string(amicitia.join("cso_x.md")).unwrap(),
        "a stray from officium"
    );
    assert_eq!(
        std::fs::read_to_string(officium.join("cso_x.md")).unwrap(),
        "officium's own"
    );
}

/// **A directory at a rename target stops the plan before anything moves** (§10.1).
///
/// The old asymmetry: a non-empty directory at a target aborted the apply partway with
/// `ENOTEMPTY` — the node dir renamed, its child did not, and the repair was a hand's —
/// while a *file* at a target was silently clobbered. Both ends are one answer now.
#[test]
fn a_directory_at_a_rename_target_is_refused_before_anything_moves() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    mint(&root, "c", triple("s", "societas"));
    mint(&root, "cs", triple("a", "amicitia"));
    mint(&root, "csa", triple("p", "proiectum"));
    // A stray directory *inside* the branch, squatting on the name the recode is about to
    // give the child. Not a sibling of the renamed node, so §5.3's own collision check —
    // which asks the parent — cannot see it.
    let blocker = root.join("c_contextus/c_s_societas/cs_a_amicitia/cst_p_proiectum/blocker");
    std::fs::create_dir_all(&blocker).unwrap();
    std::fs::write(blocker.join("b.txt"), "blocks").unwrap();

    let (plan, _) =
        plan_rename(&root, &Code::parse("csa").unwrap(), Some("t"), None, None).unwrap();
    assert!(plan.apply(&root).is_err());
    assert!(
        root.join("c_contextus/c_s_societas/cs_a_amicitia").is_dir(),
        "the node dir must not have moved"
    );
    assert!(blocker.join("b.txt").is_file());
}

// ── the walk: rename-prefix goes where rename and mv go, and no further ───────

/// **`rename-prefix` does not descend into a non-node directory** (§6.3).
///
/// It once walked every directory under its scope, which put a project's `.git`,
/// `node_modules` and `target` inside it: it renamed files in git object stores and build
/// output. `rename` and `mv` have always left a bulk directory to ride along inside its
/// parent, and this now matches them — the bound is the node tree, and it needs no ignore
/// file to find it (§13, §18).
#[test]
fn rename_prefix_does_not_descend_into_a_non_node_dir() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    mint(&root, "c", triple("s", "societas"));
    mint(&root, "cs", triple("a", "amicitia"));
    // A project homed at the node, with the three directories a repo grows.
    let proj = root.join("c_contextus/c_s_societas/cs_a_amicitia/project");
    for sub in [".git/refs", "node_modules/pkg", "target/demo"] {
        std::fs::create_dir_all(proj.join(sub)).unwrap();
    }
    std::fs::write(proj.join(".git/refs/csa_ref"), "git").unwrap();
    std::fs::write(proj.join("node_modules/pkg/csa_mod.js"), "nm").unwrap();
    std::fs::write(proj.join("target/demo/csa_art.md"), "tgt").unwrap();
    // …and one real record at the same node, so the plan is not empty for the wrong reason.
    write_record(
        &root,
        "csa",
        "csa__person__mara.json",
        r#"{"refs":[],"data":{}}"#,
    );

    let (plan, _) = plan_rename_prefix(&root, "csa", "cso", None).unwrap();
    let paths: Vec<String> = plan
        .to_json()
        .get("changes")
        .and_then(|c| c.as_array())
        .map(|changes| {
            changes
                .iter()
                .filter_map(|c| {
                    c.get("from")
                        .and_then(|f| f.as_str())
                        .map(ToOwned::to_owned)
                })
                .collect()
        })
        .unwrap_or_default();
    for machine_made in [".git", "node_modules", "target"] {
        assert!(
            !paths.iter().any(|p| p.contains(machine_made)),
            "{machine_made} is not the tree's to rename: {paths:?}"
        );
    }
    assert!(
        paths.iter().any(|p| p.ends_with("csa__person__mara.json")),
        "the record at the node is still repaired: {paths:?}"
    );
}

// ── the masked directory: what the parser cannot read, the walk still repairs (D11) ──

/// The `from` paths of a plan's renames, in the order they would be applied.
fn plan_froms(plan: &Plan) -> Vec<String> {
    plan.changes
        .iter()
        .filter_map(|c| match c {
            Change::Rename { from, .. } => Some(from.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect()
}

/// **`rename-prefix` repairs the children the parser cannot see** (D11).
///
/// The shape a half-finished recode leaves: the node dir wears its new code and every
/// child still spells the old one. Those children are exactly what the repair is for, and
/// exactly what the node walk refuses to hand it — a child is read against its parent's
/// code, and the drift *is* that the two disagree. Measured on this fixture in Phase 4,
/// `rename-prefix aom aotm aotm` planned **one** change, the loose file, and left all
/// three directories carrying the dead prefix.
#[test]
fn rename_prefix_repairs_children_the_parser_cannot_read() {
    let root = fresh_root();
    mint(&root, "root", triple("a", "actio"));
    mint(&root, "a", triple("o", "opus"));
    mint(&root, "ao", triple("t", "tutela"));
    mint(&root, "aot", triple("m", "millesimals"));
    let node = root.join("a_actio/a_o_opus/ao_t_tutela/aot_m_millesimals");
    for child in ["aom_b_bokklubb", "aom_g_gfx", "aom_o_ordforandeposter"] {
        std::fs::create_dir_all(node.join(child)).unwrap();
    }
    std::fs::write(node.join("aom_idea.md"), "idea").unwrap();

    let (plan, _) =
        plan_rename_prefix(&root, "aom", "aotm", Some(&Code::parse("aotm").unwrap())).unwrap();
    let froms = plan_froms(&plan);
    assert_eq!(froms.len(), 4, "three dirs and one file: {froms:?}");
    plan.apply(&root).unwrap();

    for child in ["aotm_b_bokklubb", "aotm_g_gfx", "aotm_o_ordforandeposter"] {
        assert!(node.join(child).is_dir(), "{child} was not repaired");
    }
    assert!(node.join("aotm_idea.md").is_file());
}

/// **A scope whose own directory name opens with the old prefix still resolves** (D11).
///
/// `aott` lives at `aot_t_tenet_industries`, so the scope's own name carries `aot_` — and
/// its drifted children carry it too. The repair works inside the scope, so the scope's
/// own directory is never renamed however its name reads; before this it planned nothing
/// at all and refused with `no name under the scope carries the code prefix`.
#[test]
fn rename_prefix_takes_a_scope_whose_own_name_opens_with_the_old_prefix() {
    let root = fresh_root();
    mint(&root, "root", triple("a", "actio"));
    mint(&root, "a", triple("o", "opus"));
    mint(&root, "ao", triple("t", "tutela"));
    mint(&root, "aot", triple("t", "tenet_industries"));
    let node = root.join("a_actio/a_o_opus/ao_t_tutela/aot_t_tenet_industries");
    std::fs::create_dir_all(node.join("aot_f_forge")).unwrap();
    // A sibling that is already right. `aott_g_grid` reads equally as a stranded `aot`
    // and as a correct `aott`, and only one of the two readings can be undone.
    std::fs::create_dir_all(node.join("aott_g_grid")).unwrap();

    let (plan, _) =
        plan_rename_prefix(&root, "aot", "aott", Some(&Code::parse("aott").unwrap())).unwrap();
    plan.apply(&root).unwrap();

    assert!(node.is_dir(), "the scope root is never renamed");
    assert!(node.join("aott_f_forge").is_dir());
    assert!(
        node.join("aott_g_grid").is_dir(),
        "a name already carrying the new prefix is left alone"
    );
}

/// **A masked directory is descended into, and every byte outside the prefix survives**
/// (D11 criteria 1 and 3; D8).
///
/// `aotm_251001_candela` takes a six-digit char, which fails §5.1, so no walk can address
/// it — and it is three levels of masking deep. The names inside are NFD and must stay
/// NFD: the destination is the stored bytes with a prefix run swapped, never a spelling
/// rebuilt from a literal (§3.13's near miss, caught by the Phase 4 preflight).
#[test]
fn rename_prefix_descends_a_masked_dir_and_keeps_its_nfd() {
    let root = fresh_root();
    mint(&root, "root", triple("a", "actio"));
    mint(&root, "a", triple("o", "opus"));
    mint(&root, "ao", triple("t", "tutela"));
    mint(&root, "aot", triple("m", "millesimals"));
    let node = root.join("a_actio/a_o_opus/ao_t_tutela/aot_m_millesimals");
    // NFD: `a` + COMBINING RING ABOVE, never the precomposed `å`.
    let nfd = "a\u{030a}rskursma\u{0308}rke";
    let deep = node
        .join("aom_251001_candela")
        .join("aom251001_p_photos")
        .join(format!("aom251001p_{nfd}"));
    std::fs::create_dir_all(&deep).unwrap();
    std::fs::write(deep.join(format!("aom251001p_{nfd}_note.md")), "n").unwrap();

    let (plan, _) =
        plan_rename_prefix(&root, "aom", "aotm", Some(&Code::parse("aotm").unwrap())).unwrap();
    plan.apply(&root).unwrap();

    let landed = node
        .join("aotm_251001_candela")
        .join("aotm251001_p_photos")
        .join(format!("aotm251001p_{nfd}"));
    assert!(landed.is_dir(), "the masked interior was not reached");
    assert!(landed.join(format!("aotm251001p_{nfd}_note.md")).is_file());
    // The bytes, not just the name: a normalizing filesystem would answer `is_dir` to
    // either spelling, so the directory's own entry is read back and compared.
    let read_back: Vec<String> = std::fs::read_dir(landed.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        read_back.contains(&format!("aotm251001p_{nfd}")),
        "NFD was not preserved: {read_back:?}"
    );
}

/// **A code parses when its char arrives decomposed.** On a decomposed filesystem `ö` is
/// `o` + U+0308, and a combining mark is neither alphabetic nor numeric — so the parser
/// refused the whole code, and every gate standing on it shut.
#[test]
fn a_code_parses_when_its_char_arrives_decomposed() {
    assert!(Code::parse("assefpo\u{308}").is_ok(), "NFD code refused");
    assert!(Code::parse("assefpö").is_ok(), "NFC code refused");
    // A mark is part of the letter before it; a symbol is still not a code character.
    assert!(Code::parse("assefp-ö").is_err());
}

/// **A recode reaches the children of a node whose char is not ASCII.**
///
/// The mint wrote NFC and the filesystem writes NFD, so a node directory and the files
/// inside it disagree on spelling: `ass_ö_övning` is composed, every `assö_*` in it is
/// decomposed. Both gates then shut — the exact node-code test compared bytes and missed,
/// and the prefix run failed because the head would not parse — so the directory was
/// renamed and its whole interior left carrying the dead code. Measured at 100 stranded
/// entries on the live `ass` → `asd`.
///
/// The composed node is built **by hand** here. D8 closed the write side, so the mint no
/// longer produces this mismatch — but every directory minted before it did is still in the
/// tree, so the mismatch has to be constructed to stay tested. It is the fixture, not an
/// incidental of how the fixture was made.
#[test]
fn a_recode_reaches_children_of_a_node_whose_char_is_not_ascii() {
    let root = fresh_root();
    mint(&root, "root", triple("a", "actio"));
    mint(&root, "a", triple("s", "scientia"));
    mint(&root, "as", triple("s", "studium"));
    // NFC: U+00F6 LATIN SMALL LETTER O WITH DIAERESIS, the spelling the old mint wrote.
    let nfc_node = "ass_\u{f6}_\u{f6}vning";
    let node = root
        .join("a_actio/a_s_scientia/as_s_studium")
        .join(nfc_node);
    std::fs::create_dir_all(&node).unwrap();
    // NFD: `o` + COMBINING DIAERESIS — the spelling the filesystem hands back.
    std::fs::write(node.join("asso\u{308}_1.pdf"), "x").unwrap();

    let (plan, _) =
        plan_rename(&root, &Code::parse("ass").unwrap(), Some("d"), None, None).unwrap();
    plan.apply(&root).unwrap();

    let parent = root.join("a_actio/a_s_scientia/as_d_studium");
    let nodes_back: Vec<String> = std::fs::read_dir(&parent)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    // The recode substitutes the code run and carries every other byte through, so a node
    // that was composed stays composed — `pan` does not re-spell what it did not mint (D8).
    assert!(
        nodes_back.contains(&"asd_\u{f6}_\u{f6}vning".to_string()),
        "the non-ASCII node was not renamed, or its spelling was rewritten: {nodes_back:?}"
    );
    let landed = parent.join("asd_\u{f6}_\u{f6}vning");
    // Read the entry back rather than asking `is_file`: a normalizing filesystem answers
    // to either spelling, so only the stored bytes prove the tail was carried through.
    let read_back: Vec<String> = std::fs::read_dir(&landed)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        read_back.contains(&"asdo\u{308}_1.pdf".to_string()),
        "the child was stranded or its NFD lost: {read_back:?}"
    );
}

/// **The mint writes the tree's spelling** (D8, the write side).
///
/// `pan new` composed every name it minted, which is what put NFC node directories over the
/// NFD children macOS writes beneath them — the mismatch the test above now has to build by
/// hand. Syncthing on macOS treats NFD as canonical: given a composed name it either
/// renames it back (`autoNormalize=true`, which silently reverted Phase 1 twice) or refuses
/// to index it at all (`autoNormalize=false`, which dropped 1,367 files out of the backup).
///
/// Read the entry back rather than asking `is_dir`: APFS answers to either spelling, so
/// only the stored bytes prove which one was written.
#[test]
fn the_mint_writes_the_trees_spelling() {
    let root = fresh_root();
    mint(&root, "root", triple("a", "actio"));
    mint(&root, "a", triple("s", "scientia"));
    mint(&root, "as", triple("s", "studium"));
    mint(&root, "ass", triple("ö", "övning"));

    let held: Vec<String> = std::fs::read_dir(root.join("a_actio/a_s_scientia/as_s_studium"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        held.contains(&"ass_o\u{308}_o\u{308}vning".to_string()),
        "the mint did not write the decomposed name (D8): {held:?}"
    );
    assert!(
        !held.contains(&"ass_\u{f6}_\u{f6}vning".to_string()),
        "the mint still writes NFC (D8): {held:?}"
    );
}

/// **A typed label is minted decomposed; an untouched one is carried through** (D8).
///
/// The two halves of a rename have different authorities over spelling. A label arriving on
/// the command line is a fresh mint and is written in the tree's spelling. A label nobody
/// typed is not `pan`'s to re-spell — it is carried byte for byte, which is what keeps a
/// bare `--char` recode the pure prefix substitution that `ass` → `asd` measured at 3,361
/// renames with the whole tail intact.
#[test]
fn a_typed_label_is_minted_decomposed_and_an_untouched_one_is_carried() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    mint(&root, "c", triple("s", "societas"));
    let parent = root.join("c_contextus/c_s_societas");
    // Composed, built by hand as the old mint would have left it.
    let branch = parent.join("cs_t_tr\u{e4}ning");
    std::fs::create_dir_all(branch.join("cst__")).unwrap();

    let entries = |dir: &Path| -> Vec<String> {
        std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect()
    };

    // No label typed: the recode substitutes the code run and touches nothing else.
    let (plan, _) =
        plan_rename(&root, &Code::parse("cst").unwrap(), Some("u"), None, None).unwrap();
    plan.apply(&root).unwrap();
    let held = entries(&parent);
    assert!(
        held.contains(&"cs_u_tr\u{e4}ning".to_string()),
        "a carried label was re-spelled: {held:?}"
    );

    // A typed label is a mint, so it lands in the tree's spelling. It has to be a
    // *different* label: the no-op guard compares the typed token against the one on disk
    // and refuses before spelling is ever reached, so `--label` cannot be used to decompose
    // a composed name in place. Left as it is — under D8 a composed name is not a defect,
    // so there is nothing that needs that repair.
    let (plan, _) = plan_rename(
        &root,
        &Code::parse("csu").unwrap(),
        None,
        Some("övning"),
        None,
    )
    .unwrap();
    plan.apply(&root).unwrap();
    let held = entries(&parent);
    assert!(
        held.contains(&"cs_u_o\u{308}vning".to_string()),
        "a typed label was not decomposed: {held:?}"
    );
}

/// **A real violation is still reported, and its fix does not recompose the name** (D8).
///
/// Loosening `non_normalized_name` to ignore Unicode form must not loosen it to ignore
/// everything: case, punctuation, spacing and `_` runs are still §5.1 violations. And the
/// fix `pan validate` prints is the one a hand will run, so it has to be safe to run —
/// suggesting the composed spelling is what made the old finding a trap.
#[test]
fn a_real_violation_is_still_reported_and_its_fix_does_not_recompose() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    mint(&root, "c", triple("s", "societas"));
    // Uppercase is a genuine violation; the diaeresis is not. Composed, so that a fix
    // built by recomposing and a fix built by decomposing are visibly different strings.
    let branch = root.join("c_contextus/c_s_societas/cs_t_Tr\u{e4}ning");
    std::fs::create_dir_all(branch.join("cst__")).unwrap();

    let findings = validate(&root, &album_registry()).unwrap();
    let finding = findings
        .iter()
        .find(|f| f.code == FindingCode::NonNormalizedName)
        .expect("an uppercase label is still a violation (§5.1)");
    let fix = finding.fix.as_deref().expect("the finding carries a fix");
    assert!(
        fix.contains("tra\u{308}ning"),
        "the fix is not in the tree's spelling: {fix:?}"
    );
    assert!(
        !fix.contains("tr\u{e4}ning"),
        "the fix recomposes the name and would drop it out of the backup: {fix:?}"
    );
}

/// **A bare `[code].[ext]` name follows its node; a word that merely opens with the code
/// does not.**
///
/// `ass.aux` beside the node `ass` carries that code as surely as `ass_notes.md` does, and
/// nothing but the extension dot says so. The dot is trusted only against the node's *own*
/// code — a run match there would rename `assets`, which is destruction and was B2 once.
#[test]
fn a_bare_code_dot_ext_follows_its_node_but_a_word_does_not() {
    let root = fresh_root();
    mint(&root, "root", triple("a", "actio"));
    mint(&root, "a", triple("s", "scientia"));
    mint(&root, "as", triple("s", "studium"));
    let node = root.join("a_actio/a_s_scientia/as_s_studium");
    std::fs::write(node.join("ass.aux"), "x").unwrap();
    std::fs::create_dir(node.join("assets")).unwrap();
    std::fs::write(node.join("assets.json"), "x").unwrap();
    std::fs::write(node.join("assimulation.md"), "x").unwrap();

    let (plan, _) =
        plan_rename(&root, &Code::parse("ass").unwrap(), Some("d"), None, None).unwrap();
    plan.apply(&root).unwrap();

    let landed = root.join("a_actio/a_s_scientia/as_d_studium");
    assert!(
        landed.join("asd.aux").is_file(),
        "[code].[ext] not followed"
    );
    assert!(landed.join("assets").is_dir(), "`assets` was renamed");
    assert!(
        landed.join("assets.json").is_file(),
        "`assets.json` renamed"
    );
    assert!(
        landed.join("assimulation.md").is_file(),
        "`assimulation.md` was renamed"
    );
}

/// **A digit continues a code; a letter starts a word.**
///
/// `asseff_t_tentamen` holds ninety exam PDFs named `assefft250113l.pdf` — the node's code
/// with a date run straight onto it, no separator. They are the node's files and a recode
/// has to take them. Nothing that merely opens with the code can follow it with a digit,
/// which is what keeps `assets.json`, `assrc` and `assettings.json` out at the same gate.
#[test]
fn a_digit_continues_a_code_but_a_letter_starts_a_word() {
    let root = fresh_root();
    mint(&root, "root", triple("a", "actio"));
    mint(&root, "a", triple("s", "scientia"));
    mint(&root, "as", triple("s", "studium"));
    let node = root.join("a_actio/a_s_scientia/as_s_studium");
    std::fs::write(node.join("ass250113l.pdf"), "x").unwrap();
    std::fs::write(node.join("assrc"), "x").unwrap();
    std::fs::write(node.join("assettings.json"), "x").unwrap();

    let (plan, _) =
        plan_rename(&root, &Code::parse("ass").unwrap(), Some("d"), None, None).unwrap();
    plan.apply(&root).unwrap();

    let landed = root.join("a_actio/a_s_scientia/as_d_studium");
    assert!(
        landed.join("asd250113l.pdf").is_file(),
        "a dated code was not followed"
    );
    assert!(landed.join("assrc").is_file(), "`assrc` was renamed");
    assert!(
        landed.join("assettings.json").is_file(),
        "`assettings.json` was renamed"
    );
}

/// **Inside a masked directory, a bare `[code].[ext]` is read against the directory's own
/// name** — the only code available where the parser never yielded a node.
///
/// `asst_a_review` spells `asst` and takes the char `a`, so `assta.pdf` beside it carries
/// that code and nothing else could have written it. `assets.json` in the same directory
/// spells nothing the directory does, and stays. Equality against the directory's code is
/// the whole guard: a *run* at a `.` would take both.
#[test]
fn a_masked_bare_code_dot_ext_is_read_against_its_directory() {
    let root = fresh_root();
    mint(&root, "root", triple("a", "actio"));
    mint(&root, "a", triple("s", "scientia"));
    mint(&root, "as", triple("s", "studium"));
    let node = root.join("a_actio/a_s_scientia/as_s_studium");
    // A six-digit char is no char §5.1 will read, so this directory is masked and its
    // interior is reached only by the cascade.
    let masked = node.join("ass_251001_candela").join("asst_a_review");
    std::fs::create_dir_all(&masked).unwrap();
    std::fs::write(masked.join("assta.pdf"), "x").unwrap();
    std::fs::write(masked.join("assets.json"), "x").unwrap();
    std::fs::write(masked.join("assessment-config.yaml"), "x").unwrap();

    let (plan, _) =
        plan_rename(&root, &Code::parse("ass").unwrap(), Some("d"), None, None).unwrap();
    plan.apply(&root).unwrap();

    let landed = root
        .join("a_actio/a_s_scientia/as_d_studium")
        .join("asd_251001_candela")
        .join("asdt_a_review");
    assert!(landed.is_dir(), "the masked interior was not reached");
    assert!(
        landed.join("asdta.pdf").is_file(),
        "the directory's own code was not followed"
    );
    assert!(
        landed.join("assets.json").is_file(),
        "`assets.json` was renamed — a run at a `.` got through"
    );
    assert!(
        landed.join("assessment-config.yaml").is_file(),
        "`assessment-config.yaml` was renamed"
    );
}

/// **A word that merely opens with the prefix is not a prefix hit** (D11's measurement
/// trap), and a machine-made tree inside a masked directory is still out of bounds (B2).
///
/// `assimulation` and `asstderr` are ordinary words beginning with `ass`; taking
/// `startswith` for a prefix test inflated the first stranding count from 2,460 to 5,446.
/// A tree name always carries a `_` after its code, so the head before the first `_` is
/// the whole test. And a repo homed *inside* a masked directory is reached by the walk for
/// the first time here — its `.git` and `node_modules` must stay untouched.
#[test]
fn rename_prefix_skips_a_word_that_opens_with_the_prefix_and_prunes_the_machine_made() {
    let root = fresh_root();
    mint(&root, "root", triple("a", "actio"));
    mint(&root, "a", triple("s", "studium"));
    mint(&root, "as", triple("s", "scholae"));
    let node = root.join("a_actio/a_s_studium/as_s_scholae");
    // A masked child — the walk enters this one.
    let masked = node.join("ass_251001_kurs");
    std::fs::create_dir_all(masked.join(".git/refs")).unwrap();
    std::fs::create_dir_all(masked.join("node_modules/pkg")).unwrap();
    std::fs::write(masked.join(".git/refs/ass_ref"), "git").unwrap();
    std::fs::write(masked.join("node_modules/pkg/ass_mod.js"), "nm").unwrap();
    // Ordinary words, and one real stranded name to keep the plan non-empty.
    std::fs::write(masked.join("assimulation.py"), "sim").unwrap();
    std::fs::create_dir_all(masked.join("asstderr")).unwrap();
    std::fs::write(masked.join("ass251001_note.md"), "n").unwrap();

    let (plan, _) =
        plan_rename_prefix(&root, "ass", "asd", Some(&Code::parse("ass").unwrap())).unwrap();
    let froms = plan_froms(&plan);
    for untouched in [".git", "node_modules", "assimulation", "asstderr"] {
        assert!(
            !froms.iter().any(|p| p.contains(untouched)),
            "{untouched} is not a prefix hit: {froms:?}"
        );
    }
    plan.apply(&root).unwrap();
    assert!(node.join("asd_251001_kurs/asd251001_note.md").is_file());
    assert!(node.join("asd_251001_kurs/assimulation.py").is_file());
    assert!(node.join("asd_251001_kurs/asstderr").is_dir());
    assert!(node.join("asd_251001_kurs/.git/refs/ass_ref").is_file());
}

/// **`mv` recodes the interior of a directory it cannot resolve** (D11 criterion 4).
///
/// The kthis case: `pan mv aook --to aot` cascaded every resolvable descendant and left
/// **320** entries inside directories whose names fail §5.1 — masking turned into a
/// stranding, one move at a time.
#[test]
fn mv_recodes_the_interior_of_a_dir_it_cannot_resolve() {
    let root = fresh_root();
    mint(&root, "root", triple("a", "actio"));
    mint(&root, "a", triple("o", "opus"));
    mint(&root, "ao", triple("o", "officium"));
    mint(&root, "ao", triple("t", "tutela"));
    mint(&root, "aoo", triple("k", "kth"));
    let old = root.join("a_actio/a_o_opus/ao_o_officium/aoo_k_kth");
    let masked = old.join("aook_251001_candela");
    std::fs::create_dir_all(masked.join("aook251001_b_board")).unwrap();
    std::fs::write(masked.join("aook251001_b_board/aook251001b_note.md"), "n").unwrap();

    let (plan, _) = plan_mv(&root, &Code::parse("aook").unwrap(), "aot").unwrap();
    plan.apply(&root).unwrap();

    let landed = root.join("a_actio/a_o_opus/ao_t_tutela/aot_k_kth/aotk_251001_candela");
    assert!(landed.is_dir(), "the masked child kept the dead prefix");
    assert!(
        landed
            .join("aotk251001_b_board/aotk251001b_note.md")
            .is_file(),
        "the masked interior was stranded"
    );
}

// ── mv-file: any file, and many at once (§7.2, §6.5) ─────────────────────────

/// **`mv-file` re-homes a document and leaves an uncoded name alone.**
///
/// It once refused everything without a `__` in its name, which left nothing at all for
/// bulk — the overwhelming majority of what a migration moves. A name that carries the
/// source node's code takes the target's; one that carries no code (a camera's
/// `IMG_1234.jpg`) is nobody's to rename and moves verbatim. A record still lands in the
/// meta dir, a document and bulk loose in the open node dir (§6.1, §6.5).
#[test]
fn mv_file_rehomes_a_document_and_leaves_an_uncoded_name_alone() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    mint(&root, "c", triple("s", "societas"));
    mint(&root, "cs", triple("a", "amicitia"));
    mint(&root, "cs", triple("o", "officium"));
    let from = root.join("c_contextus/c_s_societas/cs_a_amicitia");
    std::fs::write(from.join("csa_notes.md"), "a document").unwrap();
    std::fs::write(from.join("IMG_1234.jpg"), "a photo").unwrap();
    write_record(
        &root,
        "csa",
        "csa__person__mara.json",
        r#"{"refs":[],"data":{}}"#,
    );

    let (plan, _) = plan_mv_files(
        &root,
        &[
            from.join("csa_notes.md"),
            from.join("IMG_1234.jpg"),
            from.join("csa__/csa__person__mara.json"),
        ],
        &Code::parse("cso").unwrap(),
    )
    .unwrap();
    plan.apply(&root).unwrap();

    let to = root.join("c_contextus/c_s_societas/cs_o_officium");
    assert!(
        to.join("cso_notes.md").is_file(),
        "the document took the code"
    );
    assert!(to.join("IMG_1234.jpg").is_file(), "an uncoded name is kept");
    assert!(
        to.join("cso__/cso__person__mara.json").is_file(),
        "a record still lands in the meta dir"
    );
    assert!(!from.join("csa_notes.md").exists());
}

/// **Many sources make one plan** — one token, one confirm, so a shell glob is a single
/// reviewed transaction — and two of them landing on one name is refused before any move.
#[test]
fn mv_file_takes_many_sources_and_refuses_two_onto_one_name() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    mint(&root, "c", triple("s", "societas"));
    mint(&root, "cs", triple("a", "amicitia"));
    mint(&root, "cs", triple("o", "officium"));
    let from = root.join("c_contextus/c_s_societas/cs_a_amicitia");
    std::fs::write(from.join("csa_one.md"), "one").unwrap();
    std::fs::write(from.join("csa_two.md"), "two").unwrap();

    let (plan, _) = plan_mv_files(
        &root,
        &[from.join("csa_one.md"), from.join("csa_two.md")],
        &Code::parse("cso").unwrap(),
    )
    .unwrap();
    assert_eq!(plan.changes.len(), 2, "two files, one plan");
    plan.apply(&root).unwrap();

    // A record and a document that would land on the same name: neither is there yet, so
    // no disk check could see it — the plan has to refuse itself (§5.4).
    let other = root.join("c_contextus/c_s_societas/cs_o_officium");
    std::fs::write(other.join("cso_same.md"), "already here").unwrap();
    std::fs::write(from.join("csa_same.md"), "collides").unwrap();
    mint(&root, "cs", triple("b", "beata"));
    let third = root.join("c_contextus/c_s_societas/cs_b_beata");
    std::fs::write(third.join("csb_same.md"), "collides too").unwrap();
    let err = plan_mv_files(
        &root,
        &[from.join("csa_same.md"), third.join("csb_same.md")],
        &Code::parse("csa").unwrap(),
    )
    .unwrap_err();
    assert_eq!(err.exit_code(), pantheon::ExitCode::Validation);
}

// ── merge: the verb `mv` cannot be (§10.1, §5.3) ──────────────────────────────

/// **`merge` unions two branches**: a child both sides hold is merged into the one already
/// there, a child only the source holds is moved whole, and the source dissolves.
///
/// This is what `mv` cannot do. `mv` refuses a code collision (§5.3) and is right to — a
/// silent merge would be worse — but that left no verb at all for the case a tree
/// assembled from two trees is full of.
#[test]
fn merge_unions_two_branches_and_dissolves_the_source() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    mint(&root, "c", triple("s", "societas"));
    mint(&root, "cs", triple("a", "amicitia"));
    mint(&root, "cs", triple("o", "officium"));
    // A child both sides hold under the same char, spelled with different labels…
    mint(&root, "csa", triple("g", "grex"));
    mint(&root, "cso", triple("g", "globus"));
    // …and one only the source holds, carrying a bulk directory of its own.
    mint(&root, "csa", triple("h", "hospes"));
    write_record(
        &root,
        "csag",
        "csag__person__mara.json",
        r#"{"refs":[],"data":{}}"#,
    );
    let bulk = root.join("c_contextus/c_s_societas/cs_a_amicitia/csa_h_hospes/project/.git");
    std::fs::create_dir_all(&bulk).unwrap();
    std::fs::write(bulk.join("HEAD"), "ref: refs/heads/main").unwrap();

    let (plan, record) = plan_merge(
        &root,
        &Code::parse("csa").unwrap(),
        &Code::parse("cso").unwrap(),
    )
    .unwrap();
    plan.apply(&root).unwrap();

    let dst = root.join("c_contextus/c_s_societas/cs_o_officium");
    // The twin was merged into, not renamed onto: `globus` kept its directory and the
    // source's record arrived inside it, recoded.
    assert!(
        dst.join("cso_g_globus/csog__/csog__person__mara.json")
            .is_file()
    );
    // The child with no twin moved whole, bulk and all, undescended.
    assert!(dst.join("cso_h_hospes/project/.git/HEAD").is_file());
    // And the source is gone.
    assert!(!root.join("c_contextus/c_s_societas/cs_a_amicitia").exists());
    // The label that had to go is reported, never silently dropped: one code is one node.
    let dropped = record["relabelled"][0].clone();
    assert_eq!(dropped["code"], "csog");
    assert_eq!(dropped["kept"], "globus");
    assert_eq!(dropped["dropped"], "grex");
}

/// **A merge refuses every file collision at once and moves nothing.**
///
/// Two records of one name, and the two annotation files, are genuine decisions: what a
/// merged `[code]__.toml` should say is not the tool's to invent. So they are listed and
/// the hand decides — which is the whole design of this verb, and it is [`Plan::preflight`]
/// that delivers it rather than anything merge-specific.
#[test]
fn merge_refuses_every_file_collision_at_once() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    mint(&root, "c", triple("s", "societas"));
    mint(&root, "cs", triple("a", "amicitia"));
    mint(&root, "cs", triple("o", "officium"));
    for (code, slug) in [
        ("csa", "mara"),
        ("cso", "mara"),
        ("csa", "jon"),
        ("cso", "jon"),
    ] {
        write_record(
            &root,
            code,
            &format!("{code}__person__{slug}.json"),
            r#"{"refs":[],"data":{}}"#,
        );
    }

    let (plan, _) = plan_merge(
        &root,
        &Code::parse("csa").unwrap(),
        &Code::parse("cso").unwrap(),
    )
    .unwrap();
    let err = plan.apply(&root).unwrap_err();
    let msg = err.to_string();
    assert!(msg.starts_with("2 of this plan's renames"), "{msg}");
    assert!(msg.contains("mara") && msg.contains("jon"), "{msg}");
    assert!(
        root.join("c_contextus/c_s_societas/cs_a_amicitia/csa__/csa__person__mara.json")
            .is_file(),
        "nothing moved"
    );
}

/// **A merge into a node's own descendant is refused** — it would move the destination
/// into itself (§10.1, the guard `mv` already carries).
#[test]
fn merge_refuses_a_node_into_its_own_descendant() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    mint(&root, "c", triple("s", "societas"));
    mint(&root, "cs", triple("a", "amicitia"));
    let err = plan_merge(
        &root,
        &Code::parse("cs").unwrap(),
        &Code::parse("csa").unwrap(),
    )
    .unwrap_err();
    assert_eq!(err.exit_code(), pantheon::ExitCode::Validation);
}

// ── annotations: the key set is open, and a field is annotation (§5.2, §18) ───

/// **An unknown annotation key lands in `[fields]`**, and the hand's own TOML survives.
///
/// The key set was closed at four, so placement rule 4 — "fields, not nodes" — had nowhere
/// to land: the only home for a warrant or a role was `keywords`, documented as search
/// hints for an LLM, which would have made the field indistinguishable from one. The four
/// typed keys keep their shapes; everything else is a field, namespaced so it can never
/// shadow one. A field is annotation and never behaviour — nothing reads one to decide
/// anything (§18).
#[test]
fn an_unknown_annotation_key_lands_in_fields() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    mint(&root, "c", triple("s", "societas"));
    let code = Code::parse("cs").unwrap();

    pantheon::set_annotations(
        &root,
        &code,
        &[("deity".to_string(), "Mercurius".to_string())],
    )
    .unwrap();
    // A hand's comment, written into the file between the two calls.
    let path = root.join("c_contextus/c_s_societas/cs__/cs__.toml");
    let mut text = std::fs::read_to_string(&path).unwrap();
    text.push_str("\n# why Mercurius and not Minerva\n");
    std::fs::write(&path, text).unwrap();

    pantheon::set_annotations(
        &root,
        &code,
        &[
            ("warrant".to_string(), "negotium".to_string()),
            ("role".to_string(), "contractor".to_string()),
        ],
    )
    .unwrap();

    let ann = pantheon::read_annotations(&root, &code).unwrap();
    assert_eq!(
        ann.deity.as_deref(),
        Some("Mercurius"),
        "the typed key is untouched"
    );
    assert_eq!(
        ann.fields.get("warrant").map(String::as_str),
        Some("negotium")
    );
    assert_eq!(
        ann.fields.get("role").map(String::as_str),
        Some("contractor")
    );

    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains("[fields]"),
        "namespaced, never top-level: {text}"
    );
    assert!(
        text.contains("# why Mercurius and not Minerva"),
        "toml_edit keeps a hand's comment (§6.6): {text}"
    );
}

/// **A merge cascades the grants its recode invalidates** (§9.2, §10.1).
///
/// `writes=core@home` names a *node*, and a merge changes what the branch's nodes are
/// called just as surely as a `rename` does. A grant left pointing at the dissolved code
/// is not merely stale but silently wrong: the grant is the whole guard (§9.5), so one
/// naming nothing authorizes nothing, and every proposal under it is refused with no
/// error anywhere to read.
#[test]
fn merge_cascades_the_grants_the_recode_invalidates() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    mint(&root, "c", triple("s", "societas"));
    mint(&root, "cs", triple("a", "amicitia"));
    mint(&root, "cs", triple("o", "officium"));
    write_rule(
        &root,
        "csa",
        "nudge",
        "#!/bin/sh\n# auspex: writes=pensum@csa:add\necho '{}'\n",
    );

    let (plan, _) = plan_merge(
        &root,
        &Code::parse("csa").unwrap(),
        &Code::parse("cso").unwrap(),
    )
    .unwrap();
    plan.apply(&root).unwrap();

    let text = std::fs::read_to_string(
        root.join("c_contextus/c_s_societas/cs_o_officium/cso__/cso__function__nudge.sh"),
    )
    .expect("the rule moved with the branch it sat in");
    assert!(text.contains("writes=pensum@cso:add"), "{text}");
}

/// **A rename between two spellings of one name is not a collision** (§5.1, §5.4).
///
/// APFS and HFS+ compare names case- and normalization-insensitively, so `träning` in NFD
/// and `träning` in NFC are byte-different paths naming **one file**. The pre-flight asked
/// whether the destination merely *existed*, so a normalizing rename collided with itself
/// — the message printed one path twice, and there was nothing to move out of the way.
///
/// What made that severe is *which* rename it refused: normal form was lowercase and NFC
/// (§5.1), so this was precisely the repair `pan validate` prescribed as
/// `pan rename <code> --label <normalized>`. The tool diagnosed a fault and then refused
/// its own fix, and a tree carrying 59 of them could not be repaired at all.
///
/// **D8 moved the destination but not the guard.** A name is written to disk decomposed
/// now, so this rename lands the label back in the spelling it already had — a rename of a
/// path onto *itself*, which is the sharpest form of the self-collision B8 fixed rather
/// than a weaker one. What it is no longer is a repair anyone asked for: a decomposed label
/// is not a finding, asserted below both before and after.
///
/// On a case-sensitive filesystem the destination genuinely does not exist and this passes
/// for the ordinary reason; it is the mac case that needs the identity check. The
/// lowercasing half of the original scenario is covered by `a_case_only_rename_applies`,
/// which is ASCII throughout and so untouched by D8.
#[test]
fn a_normalizing_rename_is_not_a_collision_with_itself() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    mint(&root, "c", triple("s", "societas"));
    // Built by hand, not minted: `mint` normalizes, and the fixture *is* the un-normalized
    // name. "tra" + U+0308 COMBINING DIAERESIS + "ning" — NFD.
    let nfd = "tra\u{0308}ning";
    let branch = root.join(format!("c_contextus/c_s_societas/cs_t_{nfd}"));
    std::fs::create_dir_all(branch.join("cst__")).unwrap();
    assert!(
        validate(&root, &album_registry())
            .unwrap()
            .iter()
            .all(|f| f.code != FindingCode::NonNormalizedName),
        "a decomposed label was a finding before the rename (D8)"
    );

    let (plan, _) = plan_rename(
        &root,
        &Code::parse("cst").unwrap(),
        None,
        Some("träning"),
        None,
    )
    .unwrap();
    plan.apply(&root).expect("a normalizing rename must apply");

    let held: Vec<String> = std::fs::read_dir(root.join("c_contextus/c_s_societas"))
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .filter(|n| n.starts_with("cs_t_"))
        .collect();
    assert_eq!(
        held,
        vec![format!("cs_t_{nfd}")],
        "one dir, and in the spelling the tree already held (D8)"
    );
    assert!(
        validate(&root, &album_registry())
            .unwrap()
            .iter()
            .all(|f| f.code != FindingCode::NonNormalizedName),
        "a decomposed label is not a finding (D8)"
    );
}

/// **A case-only rename applies too** — the same root cause, and the one every
/// normalization to lowercase meets, since `pan` lowercases every typed token (§5.1).
#[test]
fn a_case_only_rename_applies() {
    let root = fresh_root();
    mint(&root, "root", triple("c", "contextus"));
    mint(&root, "c", triple("s", "societas"));
    let branch = root.join("c_contextus/c_s_societas/cs_a_Amicitia");
    std::fs::create_dir_all(branch.join("csa__")).unwrap();

    let (plan, _) = plan_rename(
        &root,
        &Code::parse("csa").unwrap(),
        None,
        Some("amicitia"),
        None,
    )
    .unwrap();
    plan.apply(&root).expect("a case-only rename must apply");

    assert!(
        root.join("c_contextus/c_s_societas/cs_a_amicitia/csa__")
            .is_dir()
    );
    // The identity check must not have cost the guard: a *different* file at the
    // destination is still refused, which
    // `a_rename_refuses_to_overwrite_a_file_at_its_target` asserts in full.
    let node = root.join("c_contextus/c_s_societas/cs_a_amicitia");
    std::fs::write(node.join("csa_notes.md"), "the real notes").unwrap();
    std::fs::write(node.join("cst_notes.md"), "a different file").unwrap();
    let (plan, _) =
        plan_rename(&root, &Code::parse("csa").unwrap(), Some("t"), None, None).unwrap();
    assert!(
        plan.apply(&root).is_err(),
        "identity is not a licence to clobber"
    );
    assert_eq!(
        std::fs::read_to_string(node.join("cst_notes.md")).unwrap(),
        "a different file"
    );
}
