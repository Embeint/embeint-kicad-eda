// SPDX-License-Identifier: GPL-3.0-or-later
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

fn cli(
    root: &Path,
    action: &str,
    oracle: &str,
    rust: &str,
    extra: &[&str],
) -> Result<Output, Box<dyn std::error::Error>> {
    let peer = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/peer.py");
    let python = python()?;
    Ok(Command::new(env!("CARGO_BIN_EXE_kicad-difftest"))
        .args([action, "--root"])
        .arg(root)
        .args([
            "--unit",
            "test.unit",
            "--oracle",
            &python,
            "--rust",
            &python,
            "--oracle-arg",
        ])
        .arg(&peer)
        .args(["--oracle-arg", oracle, "--rust-arg"])
        .arg(&peer)
        .args(["--rust-arg", rust, "--timeout-secs", "1"])
        .args(extra)
        .output()?)
}

fn setup(root: &Path) -> TestResult {
    fs::create_dir_all(root.join("port/cases"))?;
    fs::create_dir_all(root.join("port/goldens"))?;
    fs::write(root.join("port/cases/test.unit.jsonl"),
        "{\"unit\":\"test.unit\",\"case\":\"a\",\"input\":{\"n\":9007199254740993}}\r\n{\"unit\":\"test.unit\",\"case\":\"b\",\"input\":{}}\r\n")?;
    Ok(())
}

#[test]
fn identical_input_bytes_and_process_faults() -> TestResult {
    let temp = tempfile::tempdir()?;
    setup(temp.path())?;
    let ok = cli(temp.path(), "compare", "ok", "ok", &[])?;
    assert!(
        ok.status.success(),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );
    for mode in [
        "failure",
        "timeout",
        "missing",
        "duplicate",
        "reorder",
        "changed",
        "deferred",
        "error",
        "noisy",
    ] {
        let output = cli(temp.path(), "compare", mode, "ok", &[])?;
        assert!(!output.status.success(), "oracle {mode}");
        let output = cli(temp.path(), "compare", "ok", mode, &[])?;
        assert!(!output.status.success(), "Rust {mode}");
    }
    assert!(!cli(temp.path(), "compare", "deferred", "deferred", &[])?
        .status
        .success());
    assert!(!cli(temp.path(), "compare", "error", "error", &[])?
        .status
        .success());
    assert!(!cli(temp.path(), "check", "ok", "ok", &[])?.status.success());
    Ok(())
}

#[test]
fn baseline_blessing_is_live_reviewed_and_evidenced() -> TestResult {
    let temp = tempfile::tempdir()?;
    let root = temp.path();
    setup(root)?;
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
    let correct_provenance = fs::read(&provenance)?;
    fs::write(&provenance, br#"{"unrelated":"oracle"}"#)?;
    assert!(!cli(root, "propose", "ok", "ok", &opts)?.status.success());
    assert!(!approval.exists());
    fs::write(&provenance, correct_provenance)?;
    assert!(cli(root, "propose", "ok", "ok", &opts)?.status.success());
    assert!(!cli(root, "bless", "ok", "ok", &opts)?.status.success());
    assert!(!root.join("port/goldens/test.unit.jsonl").exists());
    let mut approved: Value = serde_json::from_slice(&fs::read(&approval)?)?;
    approved["reason"] = json!("first baseline against pinned oracle; synthetic test");
    fs::write(&approval, serde_json::to_vec(&approved)?)?;
    assert!(!cli(root, "bless", "failure", "ok", &opts)?.status.success());
    assert!(!evidence.exists());
    let blessed = cli(root, "bless", "ok", "ok", &opts)?;
    assert!(
        blessed.status.success(),
        "{}",
        String::from_utf8_lossy(&blessed.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(&evidence)?)?,
        approved
    );
    assert!(cli(root, "check", "ok", "ok", &[])?.status.success());
    let before = fs::read(root.join("port/goldens/test.unit.jsonl"))?;
    let text = String::from_utf8(before.clone())?;
    let first = text.lines().next().ok_or("missing first golden")?;
    for damaged in [
        format!("{first}\n"),
        format!("{text}{first}\n"),
        text.replace("9007199254740993", "9007199254740992"),
    ] {
        fs::write(root.join("port/goldens/test.unit.jsonl"), damaged)?;
        assert!(!cli(root, "check", "ok", "ok", &[])?.status.success());
    }
    fs::write(root.join("port/goldens/test.unit.jsonl"), &before)?;
    assert!(!cli(root, "bless", "ok", "ok", &opts)?.status.success());
    assert_eq!(fs::read(root.join("port/goldens/test.unit.jsonl"))?, before);
    Ok(())
}
