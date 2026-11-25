// SPDX-License-Identifier: GPL-3.0-or-later
//! Rust-only checks of the committed corpus and refusal controls. The exact C++
//! comparison is the live difftest; these keep the Rust side's counts explicit.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kicad_core::version_compare::{parse_version_string, VersionError};
use kicad_core_shim::dispatch;
use serde_json::Value;

const CASES: &str = include_str!("../../../port/cases/libs.core.application.pure.p001.jsonl");
const CONTROLS: &str = include_str!("../../../port/oracle/fixtures/core-p001-controls.jsonl");

#[test]
fn corpus_success_and_deferral_counts_are_explicit() {
    let (mut results, mut deferred, mut refused) = (0, 0, Vec::new());
    for line in CASES.lines() {
        let reply = dispatch(line);
        if reply.get("result").is_some() {
            results += 1;
        } else if reply["error"]["kind"] == "unported" {
            assert_eq!(reply["error"]["upstream"], "LATENCY_PROBE::delta_ms");
            assert_eq!(reply["error"]["unit"], kicad_core::HEADER_UNIT);
            deferred += 1;
        } else {
            refused.push(reply);
        }
    }
    assert!(refused.is_empty(), "{refused:?}");
    assert_eq!((results, deferred), (6191, 25));
}

#[test]
fn controls_are_refused_with_the_recorded_message() -> Result<(), serde_json::Error> {
    let mut count = 0;
    for line in CONTROLS.lines() {
        let control: Value = serde_json::from_str(line)?;
        let reply = dispatch(&control["request"].to_string());
        assert_eq!(
            reply["error"], control["error"],
            "{}",
            control["request"]["case"]
        );
        count += 1;
    }
    assert_eq!(count, 112);
    Ok(())
}

/// The C++ adapter's terminating-domain predicate equals the Rust loop's progress
/// check on every string of length <= 6 over the relevant alphabet.
#[test]
fn version_termination_predicate_matches_parser() {
    const ALPHABET: &[u8] = b"0-.a";
    let predicate = |text: &[u8]| {
        text.iter().enumerate().all(|(i, c)| {
            c.is_ascii_digit()
                || *c == b'.'
                || (*c == b'-' && text.get(i + 1).is_some_and(u8::is_ascii_digit))
        })
    };
    let mut text = Vec::new();
    for length in 0..=6u32 {
        for code in 0..4usize.pow(length) {
            text.clear();
            let mut rest = code;
            for _ in 0..length {
                text.push(ALPHABET[rest % 4]);
                rest /= 4;
            }
            let parsed = parse_version_string(&text);
            assert_eq!(
                predicate(&text),
                !matches!(parsed, Err(VersionError::NoProgress { .. })),
                "{:?}",
                String::from_utf8_lossy(&text)
            );
        }
    }
}
