// SPDX-License-Identifier: GPL-3.0-or-later
//! Complete-process replay of the literal private-marker-key envelopes recorded
//! from the pinned C++ adapter (`port/oracle/test_resistor_private_keys.py`).
use kicad_difftest::exact::{self, Exact};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

type TestResult = Result<(), Box<dyn std::error::Error>>;
const CASES: usize = 889;

fn fixture(name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../port/oracle/fixtures")
        .join(name);
    let text = std::fs::read_to_string(path)?;
    if text.contains('\r') || !text.ends_with('\n') {
        return Err(format!("{name} must be LF-terminated JSONL without CR").into());
    }
    Ok(text)
}

#[test]
fn private_marker_keys_match_recorded_cpp_complete_replies() -> TestResult {
    let requests = fixture("resistor-private-keys.requests.jsonl")?;
    let replies = fixture("resistor-private-keys.replies.jsonl")?;
    let mut child = Command::new(env!("CARGO_BIN_EXE_kicad-resistors-shim"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdin = child.stdin.take().ok_or("shim stdin")?;
    let writer = {
        let input = requests.clone();
        std::thread::spawn(move || stdin.write_all(input.as_bytes()))
    };
    let output = child.wait_with_output()?;
    writer.join().map_err(|_| "stdin writer panicked")??;
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    let actual = String::from_utf8(output.stdout)?;
    let requests: Vec<_> = requests.lines().collect();
    let expected: Vec<_> = replies.lines().collect();
    let actual: Vec<_> = actual.lines().collect();
    assert_eq!(requests.len(), CASES);
    assert_eq!(expected.len(), CASES);
    assert_eq!(actual.len(), CASES);
    let mut results = 0;
    for ((request, want), got) in requests.iter().zip(&expected).zip(&actual) {
        // Exact trees keep number tokens and types; key order is not significant.
        let want = exact::parse(want)?;
        assert_eq!(exact::parse(got)?, want, "{request}");
        results += usize::from(want.object()?.contains_key("result"));
    }
    assert_eq!(results, 64);
    Ok(())
}

#[test]
fn private_marker_key_objects_stay_ordinary_objects() -> TestResult {
    // Spot checks that the recorded replies are the refusals, not coerced values.
    let replies = fixture("resistor-private-keys.replies.jsonl")?;
    let error = |line: &str| -> Result<Option<String>, Box<dyn std::error::Error>> {
        let reply = exact::parse(line)?;
        Ok(match reply.object()?.get("error") {
            Some(error) => Some(error.object()?["message"].string()?.to_owned()),
            None => None,
        })
    };
    let find = |case: &str| {
        replies
            .lines()
            .find(|line| line.contains(&format!("\"case\":\"{case}\"")))
            .ok_or(format!("missing {case}"))
    };
    assert_eq!(
        error(find("pk-057-collector-target-0")?)?.as_deref(),
        Some("expected u64 IEEE754 bits")
    );
    assert_eq!(
        error(find("pk-001-format-bits-0")?)?.as_deref(),
        Some("expected u64 IEEE754 bits")
    );
    assert_eq!(
        error(find("pk-256-input-root-31")?)?.as_deref(),
        Some("missing field")
    );
    let unit = exact::parse(find("pk-unit-27")?)?;
    assert_eq!(unit.object()?["unit"], Exact::Scalar("null".into()));
    Ok(())
}
