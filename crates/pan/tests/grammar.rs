//! `pan`'s share of the shared grammar (§7.3) — the parts it had been the exception to.
//!
//! Asserted structurally rather than snapshotted, like `doctor` beside it: what is
//! stable is the *rule*, not the prose that satisfies it.

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn pan(args: &[&str]) -> (i32, Value, String) {
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_pan"));
    let out = Command::new(bin)
        .args(args)
        .env_remove("PANTHEON_ROOT")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let value = serde_json::from_str(&stdout).unwrap_or(Value::Null);
    (
        out.status.code().unwrap_or(1),
        value,
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// **A bare short piped emits, rather than opening** (§7.3).
///
/// `pan` had been the one exception in the suite: it answered a pipe with prose where
/// every core answers with the contract. A screen has nothing to draw down a pipe, and
/// the TTY rule governs `pan` too.
#[test]
fn a_piped_bare_short_emits_the_surface_as_json() {
    let (code, value, _) = pan(&[]);
    assert_eq!(code, 0);
    assert_eq!(value["short"], "pan");
    assert!(
        value["verbs"].as_array().is_some_and(|v| !v.is_empty()),
        "the surface names its verbs: {value}"
    );
}

/// L3: `pan`'s errors follow the hand too (§7.3, I8) — it routes through the spine's
/// `emit_error` like a core. Down a pipe (`-f json`) it is the `{"error":{…}}` envelope;
/// on the human path (`-f table`) it is a plain `error: <msg>` line. Same exit either way.
#[test]
fn errors_follow_the_hand() {
    // `validate` with no root named is a usage error (exit 2, §6.2) — no fixture needed.
    let (json_code, _, json_err) = pan(&["validate", "-f", "json"]);
    assert_eq!(json_code, 2);
    let envelope: Value = serde_json::from_str(json_err.trim()).unwrap();
    assert_eq!(envelope["error"]["code"], 2);

    let (tty_code, _, tty_err) = pan(&["validate", "-f", "table"]);
    assert_eq!(tty_code, 2);
    assert!(
        tty_err.starts_with("error: "),
        "a human error line, got {tty_err:?}"
    );
    assert!(!tty_err.contains('{'), "no JSON envelope on the human path");
}

/// **A failure's hand is stderr's, not stdout's** (§7.3) — the two differ whenever
/// stdout alone is captured, and `pan cd` is where that is the *normal* case: the
/// shipped shim (`pan init`) runs it inside `$(…)`, so every failed jump has a pipe on
/// stdout and the hand's own terminal on stderr. Keyed off stdout, `a zzz` answered a
/// human with `{"error":{"code":4,…}}`.
///
/// Reachable only with a real pty, which is why the bug outlived a green suite.
#[cfg(unix)]
#[test]
fn a_failure_follows_stderr_when_only_stdout_is_captured() {
    use std::io::Read;
    use std::os::fd::{FromRawFd, OwnedFd};
    use std::process::Stdio;

    let (mut controller, mut device) = (0, 0);
    // SAFETY: `openpty` writes two fresh fds through the out-pointers and reads none of
    // the three optional arguments when they are null. Both fds are adopted by an
    // `OwnedFd` on the line after the check, so neither leaks.
    let (controller, device) = unsafe {
        let rc = libc::openpty(
            &raw mut controller,
            &raw mut device,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        assert_eq!(rc, 0, "openpty");
        (
            OwnedFd::from_raw_fd(controller),
            OwnedFd::from_raw_fd(device),
        )
    };

    // stdout piped, stderr on the terminal — the shim's own shape.
    let mut child = Command::new(PathBuf::from(env!("CARGO_BIN_EXE_pan")))
        .args(["cd", "zzz"])
        .env("PANTHEON_ROOT", env!("CARGO_MANIFEST_DIR"))
        .stdout(Stdio::piped())
        .stderr(Stdio::from(device))
        .spawn()
        .unwrap();

    // Drain the terminal *while the child runs*, never after it exits: the device end
    // closes with the child, and on Darwin that discards whatever the controller has
    // not yet read — a `read_to_end` after `wait` returns `Ok(0)` about half the time.
    // The read ends of its own accord when the child goes (EOF here, EIO on Linux).
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = std::fs::File::from(controller).read_to_end(&mut buf);
        buf
    });

    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(4), "no such node (§7.3)");
    let written = reader.join().unwrap();

    let line = String::from_utf8_lossy(&written);
    assert!(
        line.starts_with("error: "),
        "a hand at a terminal reads a human line, got {line:?}"
    );
    assert!(
        !line.contains('{'),
        "no JSON envelope at a terminal: {line:?}"
    );
}

/// The seven placement rules (§2), emitted so a human and an LLM file alike (§5.5, I8).
#[test]
fn constitution_emits_the_seven_rules() {
    let (code, value, stderr) = pan(&["constitution"]);
    assert_eq!(code, 0, "{stderr}");
    let rules = value["rules"].as_array().expect("rules is an array");
    assert_eq!(rules.len(), 7, "§2 states seven placement rules");
    // Numbered, so a caller can cite one.
    for (i, rule) in rules.iter().enumerate() {
        assert_eq!(rule["n"], i + 1);
        assert!(rule["name"].as_str().is_some_and(|s| !s.is_empty()));
        assert!(rule["rule"].as_str().is_some_and(|s| !s.is_empty()));
    }
    // With no node named there is no node half — null, not an empty object: the
    // question was not asked.
    assert_eq!(value["node"], Value::Null);
}

/// **`pan <code>` no longer swallows the rest of the line.**
///
/// It was a clap `external_subcommand`, which hands the subcommand every remaining word
/// as opaque text — so `pan csa -f table` parsed no `-f` at all and silently emitted
/// JSON. A universal flag quietly dropped is worse than one refused (§7.3). The implicit
/// verb is now inserted by a pre-pass, exactly as a core inserts `add`.
#[test]
fn a_trailing_flag_after_a_code_is_not_swallowed() {
    // `-f json` after the code must be *parsed*, not absorbed. Without a root this
    // fails at root resolution (exit 2) rather than at flag parsing — which is itself
    // the proof the flag was understood and the code reached its verb.
    let (code, _, stderr) = pan(&["csa", "-f", "json"]);
    assert_eq!(code, 2, "no root named is a usage error (§6.2): {stderr}");
    assert!(
        stderr.contains("PANTHEON_ROOT") || stderr.contains("root"),
        "it should fail on the root, not on the flag: {stderr}"
    );
}

/// A verb still wins over a node code — the ambiguity rule the pre-pass must preserve
/// (§7.3). `doctor` is a verb, never a code to look up.
#[test]
fn a_verb_still_wins_over_a_code() {
    let (code, value, _) = pan(&["doctor"]);
    assert_eq!(code, 0);
    assert!(
        value["apps"].is_array(),
        "`doctor` ran as a verb rather than being looked up as a code: {value}"
    );
}

/// **`merge` is a verb, so it wins over a node code** — the pre-pass must not insert the
/// implicit `lookup` in front of it (§7.3, §5.5).
///
/// A verb only wins if [`with_lookup_verb`] knows it is one, and `pan`'s verb set is a
/// hand-kept list. The tell that it was missed is the error: a code lookup fails at code
/// parsing, a verb at the root it was given none of.
#[test]
fn merge_is_a_verb_and_wins_over_a_code() {
    let (code, _, stderr) = pan(&["merge", "csa", "--into", "cso"]);
    assert_eq!(code, 2, "no root named is a usage error (§6.2): {stderr}");
    assert!(
        stderr.contains("PANTHEON_ROOT") || stderr.contains("root"),
        "`merge` ran as a verb rather than being looked up as a code: {stderr}"
    );
    // …and it is named on the surface a pipe gets, so a hand and an LLM find it (I8).
    let (_, value, _) = pan(&[]);
    let verbs = value["verbs"].as_array().cloned().unwrap_or_default();
    assert!(verbs.iter().any(|v| v == "merge"), "{value}");
}

/// **`annotate` takes a key it has never heard of** and answers with it in `fields`
/// (§5.2). The set was closed at four, which left placement rule 4 — "fields, not nodes" —
/// with nowhere to record a field at all.
#[test]
fn annotate_takes_an_unknown_key_as_a_field() {
    let root = std::env::temp_dir().join(format!("pan-grammar-fields-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let at = root.to_string_lossy().into_owned();

    assert_eq!(
        pan(&["-C", &at, "new", "root", "c", "contextus", "-y"]).0,
        0
    );
    let (code, value, stderr) = pan(&["-C", &at, "annotate", "c", "--set", "warrant=negotium"]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(value["fields"]["warrant"], "negotium", "{value}");
    // The typed four keep their own shapes beside it.
    let (_, value, _) = pan(&["-C", &at, "annotate", "c", "--set", "deity=Mercurius"]);
    assert_eq!(value["deity"], "Mercurius", "{value}");
    assert_eq!(value["fields"]["warrant"], "negotium", "{value}");

    let _ = std::fs::remove_dir_all(&root);
}
