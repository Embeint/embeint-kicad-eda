// SPDX-License-Identifier: GPL-3.0-or-later
use std::path::Path;
use std::process::Command;

#[test]
fn corpus_regeneration_preserves_every_committed_byte() -> Result<(), Box<dyn std::error::Error>> {
    let scripts = Path::new(env!("CARGO_MANIFEST_DIR")).join("../kicad-resistors");
    let python = if cfg!(windows) { "python" } else { "python3" };
    let regeneration = Command::new(python)
        .arg(scripts.join("cases.py"))
        .arg("--check")
        .output()?;
    assert!(
        regeneration.status.success(),
        "{}",
        String::from_utf8_lossy(&regeneration.stderr)
    );
    assert!(String::from_utf8_lossy(&regeneration.stdout).contains(
        "1050 cases, SHA-256 58dd1186c9f10c8efb54bc90102c71508c8e852c0b8c08d437380b112fe3c597"
    ));
    let contracts = Command::new(python)
        .arg(scripts.join("test_cases.py"))
        .output()?;
    assert!(
        contracts.status.success(),
        "{}",
        String::from_utf8_lossy(&contracts.stderr)
    );
    Ok(())
}
