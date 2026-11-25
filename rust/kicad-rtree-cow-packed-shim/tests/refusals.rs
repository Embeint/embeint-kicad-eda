// SPDX-License-Identifier: GPL-3.0-or-later
use std::io::Write;
use std::process::{Command, Stdio};
#[test]
fn exact_refusal_controls() -> Result<(), Box<dyn std::error::Error>> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_kicad-rtree-cow-packed-shim"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    // Write in parallel with the child's stdout drain so fixture growth cannot
    // deadlock a pipe. The host runner is deliberately not a portable crate.
    let mut input = child.stdin.take().ok_or("missing stdin")?;
    let writer = std::thread::spawn(move || {
        input.write_all(include_bytes!("../fixtures/refusals.requests.jsonl"))
    });
    let output = child.wait_with_output()?;
    writer.join().map_err(|_| "writer failed")??;
    assert!(output.status.success());
    let actual = String::from_utf8(output.stdout)?;
    let want = include_str!("../fixtures/refusals.replies.jsonl");
    let a = actual
        .lines()
        .map(serde_json::from_str::<serde_json::Value>)
        .collect::<Result<Vec<_>, _>>()?;
    let w = want
        .lines()
        .map(serde_json::from_str::<serde_json::Value>)
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(a, w);
    Ok(())
}
