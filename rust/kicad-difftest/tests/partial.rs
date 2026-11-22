// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit partial-unit reporting: real exact comparisons plus enumerated Rust
//! deferrals, without weakening the strict default or the golden check.
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::process::{Command, Output};
type TestResult = Result<(), Box<dyn std::error::Error>>;

fn python() -> Result<String, Box<dyn std::error::Error>> {
    for executable in ["python3", "python"] {
        if let Ok(output) = Command::new(executable)
            .args(["-c", "import sys; print(sys.executable)"])
            .output()
        {
            if output.status.success() {
                return Ok(String::from_utf8(output.stdout)?.trim().to_owned());
            }
        }
    }
    Err("Python 3 is required for fault-injection process tests".into())
}

/// Runs the harness with the peer modes and returns its exit and written report.
fn run(
    root: &Path,
    action: &str,
    oracle: &str,
    rust: &str,
    extra: &[&str],
) -> Result<(Output, Option<Value>), Box<dyn std::error::Error>> {
    let peer = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/peer.py");
    let python = python()?;
    let report = root.join("report.json");
    let _ = fs::remove_file(&report);
    let output = Command::new(env!("CARGO_BIN_EXE_kicad-difftest"))
        .args([action, "--root"])
        .arg(root)
        .args([
            "--unit",
            "test.unit",
            "--oracle",
            &python,
            "--rust",
            &python,
        ])
        .arg("--oracle-arg")
        .arg(&peer)
        .args(["--oracle-arg", oracle, "--rust-arg"])
        .arg(&peer)
        .args(["--rust-arg", rust, "--timeout-secs", "3", "--report"])
        .arg(&report)
        .args(extra)
        .output()?;
    let written = match fs::read(&report) {
        Ok(bytes) => Some(serde_json::from_slice(&bytes)?),
        Err(_) => None,
    };
    Ok((output, written))
}

fn setup(root: &Path) -> TestResult {
    fs::create_dir_all(root.join("port/cases"))?;
    fs::create_dir_all(root.join("port/goldens"))?;
    fs::write(
        root.join("port/cases/test.unit.jsonl"),
        "{\"unit\":\"test.unit\",\"case\":\"a\",\"input\":{\"n\":9007199254740993}}\n\
         {\"unit\":\"test.unit\",\"case\":\"b\",\"input\":{}}\n\
         {\"unit\":\"test.unit\",\"case\":\"c\",\"input\":{}}\n",
    )?;
    Ok(())
}

fn status(report: &Option<Value>) -> Option<&str> {
    report.as_ref()?.get("status")?.as_str()
}

/// Writes the golden from a strict, fully agreeing run through the reviewed path.
fn bless(root: &Path) -> TestResult {
    let approval = root.join("approval.json");
    let provenance = root.join("provenance.json");
    let evidence = root.join("evidence.json");
    fs::write(
        &provenance,
        br#"{"upstream_commit":"pin","compiler":"test peer","flags":"test only"}"#,
    )?;
    let a = approval.to_str().ok_or("path encoding")?;
    let p = provenance.to_str().ok_or("path encoding")?;
    let e = evidence.to_str().ok_or("path encoding")?;
    let opts = ["--approval", a, "--provenance", p, "--evidence", e];
    assert!(run(root, "propose", "ok", "ok", &opts)?.0.status.success());
    let mut approved: Value = serde_json::from_slice(&fs::read(&approval)?)?;
    approved["reason"] = json!("synthetic baseline for partial tests");
    fs::write(&approval, serde_json::to_vec(&approved)?)?;
    let (output, _) = run(root, "bless", "ok", "ok", &opts)?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[test]
fn mixed_success_and_deferral_is_partial_only_when_requested() -> TestResult {
    let temp = tempfile::tempdir()?;
    let root = temp.path();
    setup(root)?;
    // The oracle ("ok") succeeds on b; only the Rust shim defers it.
    let (output, report) = run(root, "compare", "ok", "defer-b", &["--partial"])?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = report.ok_or("missing report")?;
    assert_eq!(report["status"], "partial");
    assert_eq!(report["mode"], "partial");
    assert_eq!(report["cases"], 3);
    assert_eq!(report["compared"], 2);
    assert_eq!(report["deferred"], 1);
    assert_eq!(report["failures"], 0);
    assert_eq!(report["compared_cases"], json!(["a", "c"]));
    assert_eq!(
        report["deferred_cases"],
        json!([{"case": "b", "upstream": "f", "unit": "dependency"}])
    );
    // stdout carries counts only; the report retains every case ID.
    let stdout: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(stdout["status"], "partial");
    assert!(stdout.get("compared_cases").is_none());

    // Strict default: the same replies fail as deferred in compare and check.
    for action in ["compare", "check"] {
        let (output, report) = run(root, action, "ok", "defer-b", &[])?;
        assert!(!output.status.success(), "strict {action}");
        assert_eq!(status(&report), Some("deferred"), "strict {action}");
        assert_eq!(report.ok_or("report")?["mode"], "strict");
    }
    Ok(())
}

#[test]
fn partial_without_deferrals_reports_green() -> TestResult {
    let temp = tempfile::tempdir()?;
    setup(temp.path())?;
    let (output, report) = run(temp.path(), "compare", "ok", "ok", &["--partial"])?;
    assert!(output.status.success());
    let report = report.ok_or("missing report")?;
    assert_eq!(report["status"], "green");
    assert_eq!(report["mode"], "partial");
    assert_eq!(report["compared_cases"], json!(["a", "b", "c"]));
    assert_eq!(report["deferred_cases"], json!([]));
    let (output, report) = run(temp.path(), "compare", "ok", "ok", &[])?;
    assert!(output.status.success());
    let report = report.ok_or("missing report")?;
    assert_eq!(report["status"], "green");
    assert_eq!(report["mode"], "strict");
    assert_eq!(report["compared"], 3);
    Ok(())
}

#[test]
fn all_deferred_is_not_a_successful_unit() -> TestResult {
    let temp = tempfile::tempdir()?;
    setup(temp.path())?;
    let (output, report) = run(temp.path(), "compare", "ok", "deferred", &["--partial"])?;
    assert!(!output.status.success());
    let report = report.ok_or("missing report")?;
    assert_eq!(report["status"], "deferred");
    assert_eq!(report["compared"], 0);
    assert_eq!(report["deferred"], 3);
    assert_eq!(report["compared_cases"], json!([]));
    assert!(report["error"]
        .as_str()
        .is_some_and(|e| e.starts_with("DEFERRED all 3 cases")));
    let explicit = run(
        temp.path(),
        "compare",
        "ok",
        "defer-a,defer-b,defer-c",
        &["--partial"],
    )?;
    assert!(!explicit.0.status.success());
    assert_eq!(status(&explicit.1), Some("deferred"));
    Ok(())
}

#[test]
fn failures_always_fail_in_partial_mode() -> TestResult {
    let temp = tempfile::tempdir()?;
    setup(temp.path())?;
    for (oracle, rust) in [
        // A mismatch among the successful cases.
        ("ok", "changed-a,defer-b"),
        ("ok", "changed-c,defer-b"),
        // Non-deferral errors, from either side or both.
        ("ok", "error-a,defer-b"),
        ("ok", "error"),
        ("error", "error"),
        ("error-b", "defer-b"),
        // The oracle never defers: a deferral there has no reference outcome.
        ("defer-b", "defer-b"),
        ("deferred", "ok"),
        // Malformed deferral envelopes are failures, not deferrals.
        ("ok", "nounit-b"),
        ("ok", "extra-b"),
        ("ok", "badowner-b"),
        ("ok", "emptyupstream-b"),
        // Identity and transport faults alongside a well-formed deferral.
        ("ok", "defer-b,missing"),
        ("ok", "defer-b,duplicate"),
        ("ok", "defer-b,reorder"),
        ("ok", "defer-b,noisy"),
        ("ok", "defer-b,failure"),
        ("ok", "defer-b,timeout"),
        ("defer-b,missing", "ok"),
        ("failure", "defer-b"),
    ] {
        let (output, report) = run(temp.path(), "compare", oracle, rust, &["--partial"])?;
        assert!(!output.status.success(), "oracle {oracle} Rust {rust}");
        let state = status(&report);
        assert!(
            matches!(state, Some("failed" | "deferred")),
            "oracle {oracle} Rust {rust}: {state:?}"
        );
        if oracle == "ok" {
            assert_eq!(state, Some("failed"), "oracle {oracle} Rust {rust}");
        }
    }
    Ok(())
}

#[test]
fn partial_check_retains_every_reference_reply_against_the_golden() -> TestResult {
    let temp = tempfile::tempdir()?;
    let root = temp.path();
    setup(root)?;
    // check needs the committed golden in both modes.
    let (output, report) = run(root, "check", "ok", "defer-b", &["--partial"])?;
    assert!(!output.status.success());
    assert_eq!(status(&report), Some("failed"));
    bless(root)?;
    let golden = root.join("port/goldens/test.unit.jsonl");
    let before = fs::read_to_string(&golden)?;
    let (output, report) = run(root, "check", "ok", "defer-b", &["--partial"])?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = report.ok_or("missing report")?;
    assert_eq!(report["status"], "partial");
    assert_eq!(report["golden_checked"], true);
    let lines: Vec<&str> = before.lines().collect();
    let [a, b, c] = lines[..] else {
        return Err("expected three golden lines".into());
    };
    // A stale golden fails even where Rust defers: the oracle reply for b is
    // compared with the golden. Missing, extra and reordered lines fail too.
    let stale_b = b.replace("9007199254740993", "9007199254740992");
    for damaged in [
        format!("{a}\n{stale_b}\n{c}\n"),
        format!("{a}\n{c}\n"),
        format!("{a}\n{b}\n{c}\n{a}\n"),
        format!("{a}\n{c}\n{b}\n"),
        String::new(),
    ] {
        fs::write(&golden, &damaged)?;
        let (output, report) = run(root, "check", "ok", "defer-b", &["--partial"])?;
        assert!(!output.status.success(), "{damaged}");
        assert_eq!(status(&report), Some("failed"));
    }
    fs::write(&golden, &before)?;
    // The oracle changing its reply for the deferred case is a golden failure.
    let (output, report) = run(root, "check", "changed-b", "defer-b", &["--partial"])?;
    assert!(!output.status.success());
    assert_eq!(status(&report), Some("failed"));
    assert_eq!(fs::read_to_string(&golden)?, before);
    Ok(())
}

#[test]
fn partial_never_blesses_and_is_an_explicit_single_flag() -> TestResult {
    let temp = tempfile::tempdir()?;
    let root = temp.path();
    setup(root)?;
    let approval = root.join("approval.json");
    let evidence = root.join("evidence.json");
    let provenance = root.join("provenance.json");
    fs::write(
        &provenance,
        br#"{"upstream_commit":"pin","compiler":"test peer","flags":"test only"}"#,
    )?;
    let a = approval.to_str().ok_or("path encoding")?;
    let p = provenance.to_str().ok_or("path encoding")?;
    let e = evidence.to_str().ok_or("path encoding")?;
    for action in ["propose", "bless"] {
        for rust in ["ok", "defer-b"] {
            let opts = [
                "--partial",
                "--approval",
                a,
                "--provenance",
                p,
                "--evidence",
                e,
            ];
            let (output, report) = run(root, action, "ok", rust, &opts)?;
            assert_eq!(output.status.code(), Some(2), "{action} {rust}");
            assert!(report.is_none());
            assert!(!approval.exists() && !evidence.exists());
        }
    }
    // Strict propose also refuses a Rust deferral; nothing is written.
    let opts = ["--approval", a, "--provenance", p, "--evidence", e];
    let (output, report) = run(root, "propose", "ok", "defer-b", &opts)?;
    assert!(!output.status.success());
    assert_eq!(status(&report), Some("deferred"));
    assert!(!approval.exists());
    assert!(!root.join("port/goldens/test.unit.jsonl").exists());
    let (output, _) = run(root, "compare", "ok", "ok", &["--partial", "--partial"])?;
    assert_eq!(output.status.code(), Some(2));
    Ok(())
}
