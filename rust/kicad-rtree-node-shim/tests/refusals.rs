// SPDX-License-Identifier: GPL-3.0-or-later
//! The adapter must refuse every fixture request with the recorded reply.  The same
//! fixture replies were produced by the live C++ oracle (see VALIDATION.md).

use serde_json::Value;
use std::io::Write;
use std::process::{Command, Stdio};

fn lines(text: &str) -> Vec<Value> {
    text.lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn refusals_match_the_recorded_replies() {
    let root = env!("CARGO_MANIFEST_DIR");
    let requests =
        std::fs::read_to_string(format!("{root}/fixtures/refusals.requests.jsonl")).unwrap();
    let expected =
        lines(&std::fs::read_to_string(format!("{root}/fixtures/refusals.replies.jsonl")).unwrap());
    let mut child = Command::new(env!("CARGO_BIN_EXE_kicad-rtree-node-shim"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(requests.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let actual = lines(&String::from_utf8(output.stdout).unwrap());
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(&expected) {
        assert_eq!(actual, expected);
    }
}

#[test]
fn malformed_transport_is_refused_with_null_identities() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_kicad-rtree-node-shim"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"{\"unit\":1e1000}\n{\"unit\":\"a\",\"unit\":\"b\"}\n[]\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    let replies = lines(&String::from_utf8(output.stdout).unwrap());
    assert_eq!(replies.len(), 3);
    for reply in &replies[..2] {
        assert_eq!(reply["unit"], Value::Null);
        assert_eq!(reply["error"]["message"], "invalid JSON or duplicate key");
    }
    assert_eq!(
        replies[2]["error"]["message"],
        "expected unit, case, input object"
    );
}
