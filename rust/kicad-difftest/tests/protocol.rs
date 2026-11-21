// SPDX-License-Identifier: GPL-3.0-or-later
use kicad_difftest::{compare, exact::parse, golden, identities, records};
use serde_json::json;

#[test]
fn numeric_tokens_never_round_or_normalize() -> Result<(), String> {
    for (a, b) in [
        ("9007199254740992", "9007199254740993"),
        (
            "184467440737095516160000000000000000000",
            "184467440737095516160000000000000000001",
        ),
        ("1.00000000000000001", "1.00000000000000002"),
        ("1", "1.0"),
        ("1e0", "1E+0"),
        ("-0", "0"),
        ("-0.0", "0.0"),
        ("1e99999", "1e99998"),
    ] {
        assert_ne!(parse(a)?, parse(b)?, "{a} vs {b}");
        assert_eq!(parse(a)?, parse(a)?);
    }
    assert_eq!(
        parse(r#"{"a": 1, "b": "\u0061"}"#)?,
        parse(r#"{"b":"a","a":1}"#)?
    );
    for invalid in [
        "NaN",
        "Infinity",
        "-Infinity",
        r#"{"x":1,"x":1}"#,
        r#"{"x":{"a":1,"a":2}}"#,
    ] {
        assert!(parse(invalid).is_err(), "{invalid}");
    }
    assert_ne!(
        parse(r#"{"$float":"nan"}"#)?,
        parse(r#"{"$float":"+inf"}"#)?
    );
    Ok(())
}

#[test]
fn inventory_status_and_values_are_independent_checks() -> Result<(), String> {
    let requests = records("{\"unit\":\"u\",\"case\":\"a\",\"input\":{}}\n{\"unit\":\"u\",\"case\":\"b\",\"input\":{}}\n", "u", true)?;
    let a = "{\"unit\":\"u\",\"case\":\"a\",\"result\":1}\n";
    let b = "{\"unit\":\"u\",\"case\":\"b\",\"result\":2}\n";
    let replies = records(&format!("{a}{b}"), "u", false)?;
    identities(&requests, &replies, "replies")?;
    assert!(identities(&requests, &records(a, "u", false)?, "missing").is_err());
    assert!(identities(
        &requests,
        &records(&format!("{b}{a}"), "u", false)?,
        "order"
    )
    .is_err());
    assert!(records(&format!("{a}{a}"), "u", false).is_err());
    assert!(records(&a.replace("\"u\"", "\"v\""), "u", false).is_err());
    assert!(records("", "u", false).is_err());
    assert!(records("\n", "u", false).is_err());
    assert!(compare(
        &replies,
        &records(&format!("{}{b}", a.replace(":1", ":1.0")), "u", false)?,
        "changed"
    )
    .is_err());
    let error = r#"{"unit":"u","case":"a","error":{"kind":"invalid_input","detail":"bad"}}"#;
    assert!(matches!(records(error, "u", false), Err(e) if e.starts_with("FAILURE")));
    for kind in ["unported", "deferred"] {
        let deferred = records(&error.replace("invalid_input", kind), "u", false);
        assert!(matches!(deferred, Err(e) if e.starts_with("DEFERRED")));
    }
    Ok(())
}

#[test]
fn blessing_requires_exact_case_values_reason_and_provenance() -> Result<(), String> {
    let old_text = "{\"unit\":\"u\",\"case\":\"a\",\"result\":9007199254740992}\n";
    let new_text = "{\"unit\":\"u\",\"case\":\"a\",\"result\":9007199254740993}\n";
    let old = records(old_text, "u", false)?;
    let new = records(new_text, "u", false)?;
    let (proposal, text) = golden::proposal(
        "u",
        b"cases",
        old_text,
        &old,
        &new,
        json!({"source_revision":"pin"}),
    );
    assert_eq!(text, new_text);
    assert_eq!(proposal["changes"][0]["old"], old_text.trim_end());
    assert_eq!(proposal["changes"][0]["new"], new_text.trim_end());
    assert!(golden::approve(&proposal, &proposal).is_err());
    let mut approval = proposal.clone();
    approval["reason"] = json!("match pinned C++ boundary result");
    golden::approve(&proposal, &approval)?;
    for key in [
        "changes",
        "cases_sha256",
        "old_sha256",
        "new_sha256",
        "oracle",
    ] {
        let mut stale = approval.clone();
        stale[key] = json!(null);
        assert!(golden::approve(&proposal, &stale).is_err(), "{key}");
    }
    let (baseline, _) = golden::proposal(
        "u",
        b"cases",
        "",
        &[],
        &new,
        json!({"source_revision":"pin"}),
    );
    assert_eq!(baseline["changes"][0]["old"], json!(null));
    assert_eq!(baseline["changes"][0]["new"], new_text.trim_end());
    assert_eq!(baseline["oracle"]["source_revision"], "pin");
    let (removed, _) = golden::proposal("u", b"new cases", old_text, &old, &[], json!({}));
    assert_eq!(removed["changes"][0]["new"], json!(null));
    Ok(())
}
