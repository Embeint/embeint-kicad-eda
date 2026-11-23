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

/// (case, outcome) for every reported case, in report order.
fn outcomes(report: &Value) -> Vec<(String, String)> {
    report["outcomes"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|o| {
                    let field = |k: &str| o[k].as_str().unwrap_or("?").to_owned();
                    (field("case"), field("outcome"))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn pairs(expected: &[(&str, &str)]) -> Vec<(String, String)> {
    expected
        .iter()
        .map(|(c, o)| ((*c).to_owned(), (*o).to_owned()))
        .collect()
}

/// Parses a reported reply line and returns its `result`, or Null.
fn result_of(line: &Value) -> Result<Value, Box<dyn std::error::Error>> {
    let text = line.as_str().ok_or("reply line must be a string")?;
    Ok(serde_json::from_str::<Value>(text)?["result"].clone())
}

/// The oracle peer's successful `n` field, kept as an exact token in the report line.
fn oracle_n(line: &Value) -> bool {
    line.as_str()
        .is_some_and(|l| l.contains("\"n\": 9007199254740993") && l.contains("\"result\""))
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
    assert_eq!(report["unprocessed"], 0);
    assert_eq!(report["golden_checked"], false);
    assert_eq!(report["golden"], json!({"status": "not_requested"}));
    assert_eq!(report["run_faults"], json!([]));
    assert_eq!(
        outcomes(&report),
        pairs(&[("a", "compared"), ("b", "deferred"), ("c", "compared")])
    );
    // The deferred case keeps its owner, symbol, the C++ success and the Rust reply.
    let b = &report["outcomes"][1];
    assert_eq!(
        (&b["upstream"], &b["unit"]),
        (&json!("f"), &json!("dependency"))
    );
    assert!(oracle_n(&b["oracle"]), "{}", b["oracle"]);
    assert!(result_of(&b["oracle"])?.is_object());
    assert!(b["rust"].as_str().is_some_and(|r| r.contains("unported")));
    // An accepted report keeps compared cases as IDs only.
    assert!(report["outcomes"][0].get("oracle").is_none());
    // stdout carries counts only; the report retains every case outcome.
    let stdout: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(stdout["status"], "partial");
    assert!(stdout.get("outcomes").is_none());

    // Strict default: the same replies fail. compare reports the deferral; check
    // without a golden reports the golden failure, which takes precedence, and
    // both keep the per-case outcomes.
    for (action, expected) in [("compare", "deferred"), ("check", "failed")] {
        let (output, report) = run(root, action, "ok", "defer-b", &[])?;
        assert!(!output.status.success(), "strict {action}");
        let report = report.ok_or("report")?;
        assert_eq!(report["status"], expected, "strict {action}");
        assert_eq!(report["mode"], "strict");
        assert_eq!(
            outcomes(&report),
            pairs(&[("a", "compared"), ("b", "deferred"), ("c", "compared")])
        );
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
    assert_eq!(
        outcomes(&report),
        pairs(&[("a", "compared"), ("b", "compared"), ("c", "compared")])
    );
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
    assert_eq!(
        outcomes(&report),
        pairs(&[("a", "deferred"), ("b", "deferred"), ("c", "deferred")])
    );
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
    const C: &str = "compared";
    const D: &str = "deferred";
    const F: &str = "failed";
    const U: &str = "unprocessed";
    // (oracle, Rust, outcomes of a/b/c, run-level faults). Every run fails, and
    // every case keeps its own outcome: one fault never erases the others.
    for (oracle, rust, expected, faults) in [
        // A mismatch among the successful cases, before or after other outcomes.
        ("ok", "changed-a,defer-b", [F, D, C], 0),
        ("ok", "changed-c,defer-b", [C, D, F], 0),
        // Non-deferral errors, from either side or both.
        ("ok", "error-a,defer-b", [F, D, C], 0),
        ("ok", "error", [F, F, F], 0),
        ("error", "error", [F, F, F], 0),
        ("error-b", "defer-b", [C, F, C], 0),
        // The oracle never defers: a deferral there has no reference outcome.
        ("defer-b", "defer-b", [C, F, C], 0),
        ("deferred", "ok", [F, F, F], 0),
        // Malformed deferral envelopes are failures, not deferrals.
        ("ok", "nounit-b", [C, F, C], 0),
        ("ok", "extra-b", [C, F, C], 0),
        ("ok", "badowner-b", [C, F, C], 0),
        ("ok", "emptyupstream-b", [C, F, C], 0),
        // Identity faults alongside a well-formed deferral.
        ("ok", "defer-b,missing", [C, D, F], 0),
        ("ok", "defer-b,duplicate", [C, D, C], 1),
        ("ok", "defer-b,reorder", [F, D, F], 0),
        ("ok", "defer-b,noisy", [F, F, F], 1),
        ("defer-b,missing", "ok", [C, F, F], 0),
        // Process faults before any output leave every case unprocessed and are
        // also run faults.
        ("ok", "defer-b,failure", [U, U, U], 1),
        ("ok", "defer-b,timeout", [U, U, U], 1),
        ("failure", "defer-b", [U, U, U], 1),
    ] {
        let label = format!("oracle {oracle} Rust {rust}");
        let (output, report) = run(temp.path(), "compare", oracle, rust, &["--partial"])?;
        assert!(!output.status.success(), "{label}");
        let report = report.ok_or_else(|| format!("{label}: missing report"))?;
        assert_eq!(report["status"], "failed", "{label}");
        assert_eq!(report["mode"], "partial", "{label}");
        assert_eq!(report["cases"], 3, "{label}");
        let expected_pairs = pairs(&[("a", expected[0]), ("b", expected[1]), ("c", expected[2])]);
        assert_eq!(outcomes(&report), expected_pairs, "{label}");
        let count = |name: &str| expected.iter().filter(|o| **o == name).count();
        assert_eq!(report["compared"], count(C), "{label}");
        assert_eq!(report["deferred"], count(D), "{label}");
        assert_eq!(report["failures"], count(F), "{label}");
        assert_eq!(report["unprocessed"], count(U), "{label}");
        assert_eq!(
            report["run_faults"].as_array().map(Vec::len),
            Some(faults),
            "{label}"
        );
        assert!(report["error"].as_str().is_some_and(|e| !e.is_empty()));
        for (index, entry) in report["outcomes"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            match expected[index] {
                F | U => assert!(entry["reason"].as_str().is_some_and(|r| !r.is_empty())),
                D => assert_eq!(entry["unit"], "dependency"),
                _ => assert!(entry.get("reason").is_none()),
            }
            // A failed run keeps every successful reference reply, compared cases
            // included, and never labels an oracle error as a reference result.
            if oracle == "ok" {
                assert!(oracle_n(&entry["oracle"]), "{label} case {index}");
            }
        }
    }
    // Strict mode also enumerates outcomes on failure: a mismatch after a deferral.
    let (output, report) = run(temp.path(), "compare", "ok", "changed-c,defer-b", &[])?;
    assert!(!output.status.success());
    let report = report.ok_or("missing report")?;
    assert_eq!(report["status"], "failed");
    assert_eq!(outcomes(&report), pairs(&[("a", C), ("b", D), ("c", F)]));
    Ok(())
}

#[test]
fn partial_check_retains_every_reference_reply_against_the_golden() -> TestResult {
    let temp = tempfile::tempdir()?;
    let root = temp.path();
    setup(root)?;
    // check needs the committed golden in both modes; the live outcomes remain.
    let (output, report) = run(root, "check", "ok", "defer-b", &["--partial"])?;
    assert!(!output.status.success());
    let report = report.ok_or("missing report")?;
    assert_eq!(report["status"], "failed");
    assert_eq!(report["golden_checked"], true);
    assert_eq!(report["golden"]["status"], "failed");
    assert!(report["golden"]["error"].is_string());
    assert_eq!(
        outcomes(&report),
        pairs(&[("a", "compared"), ("b", "deferred"), ("c", "compared")])
    );
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
    assert_eq!(report["golden"]["status"], "matched");
    assert_eq!(report["golden"]["mismatched_cases"], json!([]));
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
        let report = report.ok_or("missing report")?;
        assert_eq!(report["status"], "failed");
        assert_eq!(report["golden"]["status"], "failed");
        // The golden fault does not erase the live oracle/Rust outcomes.
        assert_eq!(
            outcomes(&report),
            pairs(&[("a", "compared"), ("b", "deferred"), ("c", "compared")])
        );
    }
    fs::write(&golden, format!("{a}\n{stale_b}\n{c}\n"))?;
    let (_, report) = run(root, "check", "ok", "defer-b", &["--partial"])?;
    let report = report.ok_or("missing report")?;
    let mismatched = &report["golden"]["mismatched_cases"];
    assert_eq!(mismatched.as_array().map(Vec::len), Some(1));
    assert_eq!(mismatched[0]["case"], "b");
    assert_eq!(mismatched[0]["golden"], json!(stale_b));
    assert_eq!(mismatched[0]["oracle"], json!(b));
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

#[test]
fn input_and_transport_faults_are_explicit() -> TestResult {
    let temp = tempfile::tempdir()?;
    let root = temp.path();
    setup(root)?;
    // A program that cannot start leaves every case unprocessed, and the other
    // side's successful reference replies are retained.
    let peer = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/peer.py");
    let report_path = root.join("transport.json");
    let output = Command::new(env!("CARGO_BIN_EXE_kicad-difftest"))
        .args(["compare", "--partial", "--root"])
        .arg(root)
        .args([
            "--unit",
            "test.unit",
            "--oracle",
            &python()?,
            "--oracle-arg",
        ])
        .arg(&peer)
        .args(["--oracle-arg", "ok", "--rust"])
        .arg(root.join("missing-shim"))
        .arg("--report")
        .arg(&report_path)
        .output()?;
    assert!(!output.status.success());
    let report: Value = serde_json::from_slice(&fs::read(&report_path)?)?;
    assert_eq!(report["status"], "failed");
    assert_eq!(report["unprocessed"], 3);
    assert_eq!(report["rust_output_sha256"], Value::Null);
    assert_eq!(report["rust_sha256"], Value::Null);
    assert!(report["oracle_output_sha256"].is_string());
    for entry in report["outcomes"].as_array().into_iter().flatten() {
        assert_eq!(entry["outcome"], "unprocessed");
        assert!(entry["reason"]
            .as_str()
            .is_some_and(|r| r.contains("cannot start")));
        assert!(oracle_n(&entry["oracle"]));
    }
    // An invalid corpus cannot be enumerated: the report says so and has no outcomes.
    let cases = root.join("port/cases/test.unit.jsonl");
    let text = fs::read_to_string(&cases)?;
    let first = text.lines().next().ok_or("empty corpus")?.to_owned();
    fs::write(&cases, format!("{text}{first}\n"))?;
    let (output, report) = run(root, "compare", "ok", "defer-b", &["--partial"])?;
    assert!(!output.status.success());
    let report = report.ok_or("missing report")?;
    assert_eq!(report["status"], "failed");
    assert!(report.get("outcomes").is_none());
    assert!(report["error"]
        .as_str()
        .is_some_and(|e| e.contains("duplicate case")));
    Ok(())
}

#[test]
fn replies_written_before_a_process_fault_are_kept_and_golden_checked() -> TestResult {
    let temp = tempfile::tempdir()?;
    let root = temp.path();
    setup(root)?;
    bless(root)?;
    let golden = root.join("port/goldens/test.unit.jsonl");
    let blessed = fs::read_to_string(&golden)?;
    // (oracle, Rust, outcomes, golden status, run faults, a fault text). The process
    // fault always fails the run, but recovered replies keep their outcomes and
    // their reference lines, and are compared with the golden.
    for (oracle, rust, expected, golden_status, faults, needle) in [
        (
            "exitafter",
            "defer-b",
            ["compared", "deferred", "compared"],
            "matched",
            1,
            "exiting after replies",
        ),
        (
            "sleepafter",
            "defer-b",
            ["compared", "deferred", "compared"],
            "matched",
            1,
            "timeout",
        ),
        (
            "exitafter,skip-c",
            "defer-b",
            ["compared", "deferred", "unprocessed"],
            "incomplete",
            1,
            "process failed",
        ),
        (
            "sleepafter,truncate",
            "defer-b",
            ["compared", "deferred", "failed"],
            "incomplete",
            1,
            "timeout",
        ),
        (
            "exitafter,garbage",
            "defer-b",
            ["compared", "deferred", "compared"],
            "matched",
            2,
            "beyond the 3 cases",
        ),
        (
            "ok",
            "defer-b,exitafter",
            ["compared", "deferred", "compared"],
            "matched",
            1,
            "process failed",
        ),
        (
            "ok",
            "defer-b,sleepafter,skip-c",
            ["compared", "deferred", "unprocessed"],
            "matched",
            1,
            "timeout",
        ),
        (
            "ok",
            "defer-b,exitafter,truncate",
            ["compared", "deferred", "failed"],
            "matched",
            1,
            "process failed",
        ),
        // Without a process fault, a missing reply is a failure of its case.
        (
            "skip-c",
            "defer-b",
            ["compared", "deferred", "failed"],
            "incomplete",
            0,
            "",
        ),
    ] {
        let label = format!("oracle {oracle} Rust {rust}");
        let (output, report) = run(root, "check", oracle, rust, &["--partial"])?;
        assert!(!output.status.success(), "{label}");
        let report = report.ok_or_else(|| format!("{label}: missing report"))?;
        assert_eq!(report["status"], "failed", "{label}");
        let expected_pairs = pairs(&[("a", expected[0]), ("b", expected[1]), ("c", expected[2])]);
        assert_eq!(outcomes(&report), expected_pairs, "{label}");
        assert_eq!(report["golden"]["status"], golden_status, "{label}");
        let run_faults = report["run_faults"].as_array().ok_or("run_faults")?;
        assert_eq!(run_faults.len(), faults, "{label}: {run_faults:?}");
        assert!(
            run_faults
                .iter()
                .any(|f| f.as_str().is_some_and(|f| f.contains(needle)))
                || faults == 0,
            "{label}: {run_faults:?}"
        );
        assert!(report["oracle_output_sha256"].is_string(), "{label}");
        assert!(report["rust_output_sha256"].is_string(), "{label}");
        for (index, entry) in report["outcomes"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            if expected[index] != "unprocessed" && expected[index] != "failed" || oracle == "ok" {
                assert!(oracle_n(&entry["oracle"]), "{label} case {index}");
            }
        }
        if oracle.contains("garbage") {
            assert_eq!(
                report["extra_replies"],
                json!([{"side": "oracle", "line": 4, "raw": "EXTRA_REFERENCE_EVIDENCE",
                    "truncated": false}])
            );
        } else {
            assert_eq!(report["extra_replies"], json!([]), "{label}");
        }
    }
    // A recovered reference reply that differs from the golden is reported as such.
    let lines: Vec<&str> = blessed.lines().collect();
    let stale = lines[1].replace("9007199254740993", "9007199254740992");
    fs::write(&golden, format!("{}\n{stale}\n{}\n", lines[0], lines[2]))?;
    let (output, report) = run(root, "check", "exitafter", "defer-b", &["--partial"])?;
    assert!(!output.status.success());
    let report = report.ok_or("missing report")?;
    assert_eq!(report["golden"]["status"], "failed");
    assert_eq!(report["golden"]["mismatched_cases"][0]["case"], "b");
    fs::write(&golden, &blessed)?;
    // Strict mode: a process fault fails even when every reply agrees.
    let (output, report) = run(root, "check", "exitafter", "ok", &[])?;
    assert!(!output.status.success());
    let report = report.ok_or("missing report")?;
    assert_eq!(report["status"], "failed");
    assert_eq!(report["compared"], 3);
    Ok(())
}

#[test]
fn a_failed_blessing_step_keeps_reference_evidence() -> TestResult {
    let temp = tempfile::tempdir()?;
    let root = temp.path();
    setup(root)?;
    let approval = root.join("approval.json");
    let a = approval.to_str().ok_or("path encoding")?;
    // propose without --provenance: the comparison is green, the action fails.
    let (output, report) = run(root, "propose", "ok", "ok", &["--approval", a])?;
    assert!(!output.status.success());
    let report = report.ok_or("missing report")?;
    assert_eq!(report["status"], "failed");
    assert!(report["run_faults"][0]
        .as_str()
        .is_some_and(|f| f.starts_with("propose: missing --provenance")));
    assert!(report["error"]
        .as_str()
        .is_some_and(|e| e.contains("missing --provenance")));
    assert_eq!(
        outcomes(&report),
        pairs(&[("a", "compared"), ("b", "compared"), ("c", "compared")])
    );
    for entry in report["outcomes"].as_array().into_iter().flatten() {
        assert!(oracle_n(&entry["oracle"]));
    }
    assert!(!approval.exists());
    assert!(!root.join("port/goldens/test.unit.jsonl").exists());
    Ok(())
}

/// The bytes the peer writes for `modes` on this corpus, hashed like the runner.
fn peer_stdout_sha256(root: &Path, modes: &str) -> Result<String, Box<dyn std::error::Error>> {
    use sha2::{Digest, Sha256};
    use std::io::Write;
    let peer = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/peer.py");
    let mut child = Command::new(python()?)
        .arg(&peer)
        .arg(modes)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .ok_or("stdin")?
        .write_all(&fs::read(root.join("port/cases/test.unit.jsonl"))?)?;
    let output = child.wait_with_output()?;
    Ok(format!("{:x}", Sha256::digest(&output.stdout)))
}

#[test]
fn invalid_utf8_reply_fails_its_case_and_later_replies_are_still_evaluated() -> TestResult {
    let temp = tempfile::tempdir()?;
    let root = temp.path();
    setup(root)?;
    bless(root)?;
    let cases = ["a", "b", "c"];
    for fault in ["badbyte", "badcont", "truncmb"] {
        for (index, case) in cases.iter().enumerate() {
            // Timeouts cost the full timeout, so they run at the middle position only;
            // the evaluator test covers every position with every fault.
            for (after, faults) in [("", 0), (",exitafter", 1), (",sleepafter", 1)] {
                if after == ",sleepafter" && index != 1 {
                    continue;
                }
                let bad = format!("{fault}-{case}{after}");
                for (oracle, rust, side) in [
                    (bad.as_str(), "defer-b", "oracle"),
                    ("ok", bad.as_str(), "rust"),
                ] {
                    let label = format!("oracle {oracle} Rust {rust}");
                    let (output, report) = run(root, "check", oracle, rust, &["--partial"])?;
                    assert!(!output.status.success(), "{label}");
                    let report = report.ok_or_else(|| format!("{label}: missing report"))?;
                    assert_eq!(report["status"], "failed", "{label}");
                    let mut expected = if side == "oracle" {
                        ["compared", "deferred", "compared"]
                    } else {
                        ["compared"; 3]
                    };
                    expected[index] = "failed";
                    assert_eq!(
                        outcomes(&report),
                        pairs(&[("a", expected[0]), ("b", expected[1]), ("c", expected[2])]),
                        "{label}"
                    );
                    let entry = &report["outcomes"][index];
                    assert!(
                        entry["reason"]
                            .as_str()
                            .is_some_and(|r| r.contains("not valid UTF-8")),
                        "{label}"
                    );
                    // The malformed line is kept as exact hex, never as decoded text.
                    let hex_key = format!("{side}_hex");
                    assert!(entry[&hex_key].is_string(), "{label}: {entry}");
                    assert!(entry.get(side).is_none(), "{label}");
                    // Later (and earlier) replies keep their real positions, and
                    // every recovered oracle result is golden-checked.
                    for (other, item) in report["outcomes"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .enumerate()
                    {
                        if other != index || side == "rust" {
                            assert!(oracle_n(&item["oracle"]), "{label} case {other}");
                        }
                    }
                    let golden = &report["golden"];
                    if side == "oracle" {
                        assert_eq!(golden["status"], "incomplete", "{label}");
                        assert_eq!(golden["unchecked_cases"], json!([case]), "{label}");
                    } else {
                        assert_eq!(golden["status"], "matched", "{label}");
                    }
                    assert_eq!(
                        report["run_faults"].as_array().map(Vec::len),
                        Some(faults),
                        "{label}"
                    );
                }
            }
        }
    }
    // The output hash covers the exact raw bytes, invalid ones included.
    let (_, report) = run(root, "check", "badbyte-b", "defer-b", &["--partial"])?;
    let report = report.ok_or("missing report")?;
    assert_eq!(
        report["oracle_output_sha256"],
        json!(peer_stdout_sha256(root, "badbyte-b")?)
    );
    let hex = report["outcomes"][1]["oracle_hex"].as_str().ok_or("hex")?;
    assert!(hex.starts_with("7b22ff22"), "{hex}");
    // CRLF line endings are accepted line terminators, as before.
    let (output, report) = run(root, "check", "crlf", "defer-b,crlf", &["--partial"])?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(status(&report), Some("partial"));
    // An invalid extra line is kept as exact bytes in extra_replies.
    let (output, report) = run(root, "check", "badextra", "defer-b", &["--partial"])?;
    assert!(!output.status.success());
    let report = report.ok_or("missing report")?;
    assert_eq!(
        report["extra_replies"],
        json!([{"side": "oracle", "line": 4, "raw_hex": "4558545241ff45564944454e4345",
            "truncated": false}])
    );
    assert_eq!(
        outcomes(&report),
        pairs(&[("a", "compared"), ("b", "deferred"), ("c", "compared")])
    );
    Ok(())
}

#[test]
fn large_stderr_is_bounded_on_clean_failed_and_timed_out_runs() -> TestResult {
    let temp = tempfile::tempdir()?;
    let root = temp.path();
    setup(root)?;
    // Clean run with 4 MiB of stderr on both sides: still green.
    let (output, report) = run(root, "compare", "stderrbig", "stderrbig", &[])?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(status(&report), Some("green"));
    let limit = kicad_difftest::process::STDERR_LIMIT;
    // exitafter also writes its 22-byte "exiting after replies\n" line.
    let written = |modes: &str| 4 * 1024 * 1024 + if modes.contains("exitafter") { 22 } else { 0 };
    for (oracle, rust, needle) in [
        ("stderrbig,exitafter", "ok", "process failed"),
        ("stderrbig,sleepafter", "ok", "timeout"),
        ("ok", "stderrbig,exitafter", "process failed"),
    ] {
        let (output, report) = run(root, "compare", oracle, rust, &[])?;
        assert!(!output.status.success());
        let report = report.ok_or("missing report")?;
        let faults = report["run_faults"].as_array().ok_or("run_faults")?;
        assert_eq!(faults.len(), 1, "{oracle} {rust}");
        let fault = faults[0].as_str().ok_or("fault")?;
        let omitted = written(if oracle == "ok" { rust } else { oracle }) - limit;
        assert!(fault.contains(needle), "{oracle} {rust}");
        assert!(
            fault.contains(&format!("[{omitted} more bytes omitted]")),
            "{oracle} {rust}: {}",
            &fault[fault.len().saturating_sub(200)..]
        );
        assert!(
            fault.len() < limit + 1024,
            "{oracle} {rust}: {}",
            fault.len()
        );
        // The replies written alongside the diagnostics are still compared.
        assert_eq!(report["compared"], 3, "{oracle} {rust}");
    }
    Ok(())
}
