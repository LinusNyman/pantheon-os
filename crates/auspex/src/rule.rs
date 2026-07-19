//! Running a rule (§9.3): context on stdin, proposals on stdout.
//!
//! A rule is a **pure function of the tree** and never writes. Auspex enforces that
//! rather than trusting it: the child runs with `PANTHEON_RULE=1`, under which every
//! core refuses every write verb (exit `6`), so a rule's only path into the tree is a
//! proposal Auspex applies on its behalf.
//!
//! ## The child is spawned directly
//!
//! No `sh -c` and no language table (§13, §9.1). The rule file *is* the program: its
//! shebang names the interpreter, which is why §9.1 can say the extension names the
//! language "or the shebang does" and `classify` can discard the extension entirely.
//! A rule without its exec bit therefore fails to spawn, and is reported like any
//! other erroring rule rather than taking the run down with it (§9.5).
//!
//! ## Two variables, both load-bearing
//!
//! `PANTHEON_RULE=1` is the enforcement above. **`PANTHEON_ROOT` is the subtler one:**
//! §9.3 has a rule read the tree through the core CLIs (`alb ls -H csa -f json`), and
//! those resolve their root from the environment (§6.2). The context JSON carries no
//! root, so this is the only channel — without it a rule under `aus -C /some/tree`
//! would quietly read whichever tree the ambient environment named.

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

/// How long a rule may run before Auspex kills it.
///
/// **The spec bounds this nowhere**, and the two failure modes pull opposite ways: no
/// limit lets one hung rule leave a detached process alive forever — and `aus run` is
/// spawned by *every* successful write (§9.4) — while a short one would kill the
/// legitimate slow rule §9.3 explicitly allows, the one that "may send mail or call an
/// API". Thirty seconds is long enough for a network round trip and short enough that
/// a wedged rule cannot accumulate across a session.
///
/// Hardcoded, and deliberately **not** a knob: no env var, no flag, no config (§18).
/// Tests reach the mechanism by calling [`evaluate`] with their own deadline.
pub(crate) const DEADLINE: Duration = Duration::from_secs(30);

/// What one rule produced.
pub(crate) enum Outcome {
    /// The rule ran and proposed these writes — possibly none.
    Proposed(Vec<Value>),
    /// The rule did not produce a usable answer. Reported against that rule and no
    /// other: "a rule that errors is skipped and reported; others are unaffected"
    /// (§9.5).
    Failed(String),
}

/// Run one rule to completion, or to `deadline`.
///
/// `context` is the JSON handed to the rule's stdin (§9.3). `root` is the tree `aus`
/// resolved, which the child needs to read it back through the core CLIs.
pub(crate) fn evaluate(path: &Path, root: &Path, context: &str, deadline: Duration) -> Outcome {
    tracing::debug!(rule = %path.display(), "running");

    let mut child = match Command::new(path)
        .env("PANTHEON_RULE", "1")
        .env("PANTHEON_ROOT", root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            // The common cause is a missing exec bit, which is worth naming: a rule is
            // a file a hand authored, and `chmod +x` is the fix.
            return Outcome::Failed(format!(
                "could not run {}: {e} — a rule is executed directly, so it needs its \
                 exec bit and a shebang (§9.1)",
                path.display()
            ));
        }
    };

    let mut stdin = child.stdin.take().expect("stdin was piped");
    let mut stdout = child.stdout.take().expect("stdout was piped");
    let mut stderr = child.stderr.take().expect("stderr was piped");

    // Three streams moved at once, because doing it in sequence deadlocks: a rule that
    // writes more than a pipe buffer before reading its context would block on stdout
    // while Auspex blocked on stdin. `thread::scope` borrows them without `'static`.
    let (out, err, waited) = std::thread::scope(|scope| {
        let writer = scope.spawn(move || {
            // A rule that never reads its stdin gives a broken pipe here, which is its
            // right — `sign`/`now` are offered, not required.
            let _ = stdin.write_all(context.as_bytes());
        });
        let reader = scope.spawn(move || {
            let mut buf = String::new();
            let _ = stdout.read_to_string(&mut buf);
            buf
        });
        let errors = scope.spawn(move || {
            let mut buf = String::new();
            let _ = stderr.read_to_string(&mut buf);
            buf
        });
        let waited = wait_until(&mut child, deadline);
        let _ = writer.join();
        (
            reader.join().unwrap_or_default(),
            errors.join().unwrap_or_default(),
            waited,
        )
    });

    // The rule's stderr is captured rather than inherited, so that a failure can quote
    // it — but a *succeeding* rule's diagnostics would then vanish. Trace them instead,
    // where `RUST_LOG=debug aus plan` reaches them and a detached hook discards them
    // with the rest of Auspex's stderr (§13).
    if !err.trim().is_empty() {
        tracing::debug!(rule = %path.display(), stderr = %err.trim(), "rule wrote to stderr");
    }

    match waited {
        Waited::TimedOut => Outcome::Failed(format!(
            "timed out after {}s and was killed (§9.4: a hook-woken rule should be cheap)",
            deadline.as_secs()
        )),
        Waited::Failed(e) => Outcome::Failed(format!("could not be waited on: {e}")),
        Waited::Exited(status) if !status.success() => {
            let code = status
                .code()
                .map_or_else(|| "a signal".to_string(), |c| format!("exit {c}"));
            Outcome::Failed(match tail(&err) {
                Some(said) => format!("{code}: {said}"),
                None => format!("{code}, saying nothing on stderr"),
            })
        }
        Waited::Exited(_) => parse(&out),
    }
}

/// A rule's stdout into proposals (§9.5 step 1).
///
/// Parsed rather than echoed. §9.3 describes `aus plan` as printing the rule's stdout,
/// but `aus`'s own stdout is contract JSON (I4) and has to stay valid — a rule emitting
/// garbage would otherwise corrupt the command's own output rather than being reported
/// as the rule bug it is.
fn parse(out: &str) -> Outcome {
    // "Empty or absent → no-op" (§9.5). A rule that proposes nothing is the ordinary
    // case, not a failure: most wakes find nothing to say.
    if out.trim().is_empty() {
        return Outcome::Proposed(Vec::new());
    }
    // Classified deliberately rather than with `?`: `From<serde_json::Error>` maps to a
    // bare runtime error and would lose which rule emitted this.
    let Ok(value) = serde_json::from_str::<Value>(out) else {
        return Outcome::Failed(format!(
            "did not emit JSON: {:?} (§9.3 wants {{\"writes\":[…]}} on stdout)",
            tail(out).unwrap_or_default()
        ));
    };
    let Some(object) = value.as_object() else {
        return Outcome::Failed(
            "emitted JSON that is not an object; §9.3 wants {\"writes\":[…]}".to_string(),
        );
    };
    match object.get("writes") {
        // An object with no `writes` proposed nothing, which reads the same as silence.
        None => Outcome::Proposed(Vec::new()),
        Some(Value::Array(writes)) => Outcome::Proposed(writes.clone()),
        Some(_) => Outcome::Failed("`writes` is not an array (§9.3)".to_string()),
    }
}

enum Waited {
    Exited(ExitStatus),
    TimedOut,
    Failed(String),
}

/// Poll for the child, killing it past `deadline`.
///
/// std has no `wait_timeout`, and this needs no crate for one — §13 makes the same
/// call about the detached spawn, which it describes as std calls "behind a
/// crate-shaped temptation".
fn wait_until(child: &mut Child, deadline: Duration) -> Waited {
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Waited::Exited(status),
            Ok(None) => {}
            Err(e) => return Waited::Failed(e.to_string()),
        }
        if start.elapsed() >= deadline {
            let _ = child.kill();
            // Reap it, so killing does not leave a zombie behind — and so the stdout
            // and stderr readers see EOF and their threads can join.
            let _ = child.wait();
            return Waited::TimedOut;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// A `/bin/sh` rule on disk, executable.
    fn script(name: &str, body: &str) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join(format!("aus-rule-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    /// The timeout, reached through [`evaluate`]'s deadline parameter rather than by
    /// waiting out [`DEADLINE`] — which is why the deadline is a parameter at all. It
    /// is not a knob on the CLI (§18); this is the only caller that ever varies it.
    #[test]
    fn a_rule_that_hangs_is_killed_and_reported() {
        let rule = script("hangs.sh", "#!/bin/sh\nsleep 30\n");
        let started = Instant::now();
        let outcome = evaluate(
            &rule,
            std::path::Path::new("/nonexistent-root"),
            "{}",
            Duration::from_millis(200),
        );
        let elapsed = started.elapsed();

        match outcome {
            Outcome::Failed(why) => assert!(
                why.contains("timed out"),
                "a killed rule says so rather than looking like a silent one: {why}"
            ),
            Outcome::Proposed(_) => panic!("a rule that sleeps 30s must not report proposals"),
        }
        assert!(
            elapsed < Duration::from_secs(5),
            "the kill happens near the deadline, not at the rule's own pace: {elapsed:?}"
        );
    }

    /// A rule that never reads its stdin must not wedge Auspex. The writer thread meets
    /// a broken pipe; that is the rule's right, and the run carries on.
    #[test]
    fn a_rule_that_ignores_its_context_still_proposes() {
        let rule = script(
            "ignores.sh",
            "#!/bin/sh\necho '{\"writes\":[{\"core\":\"pensum\"}]}'\n",
        );
        let outcome = evaluate(
            &rule,
            std::path::Path::new("/nonexistent-root"),
            // Larger than a pipe buffer, so a rule that never reads would block a
            // writer that was not on its own thread.
            &"x".repeat(200_000),
            Duration::from_secs(10),
        );
        match outcome {
            Outcome::Proposed(writes) => assert_eq!(writes.len(), 1),
            Outcome::Failed(why) => panic!("should have proposed, got: {why}"),
        }
    }
}

/// The last non-empty line of a stream, for a one-line report.
///
/// A traceback's last line is the one naming the error, and a per-rule report in a JSON
/// array is no place for the whole of one. The full text is not lost: it is traced at
/// debug above, which is where `RUST_LOG=debug` reaches it.
fn tail(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .next_back()
        .map(str::to_string)
}
