// SPDX-License-Identifier: GPL-3.0-or-later
use kicad_difftest::{
    compare, compare_partial, exact::parse, golden, identities, partial_replies, records,
};
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

#[test]
fn partial_replies_classify_only_well_formed_deferrals() -> Result<(), String> {
    let ok = |case: &str, value: &str| {
        format!("{{\"unit\":\"u\",\"case\":\"{case}\",\"result\":{value}}}\n")
    };
    let defer = |case: &str, error: &str| {
        format!("{{\"unit\":\"u\",\"case\":\"{case}\",\"error\":{error}}}\n")
    };
    let well_formed = r#"{"kind":"unported","upstream":"MD5_HASH::Hash","unit":"owner.unit"}"#;
    let oracle = records(
        &format!("{}{}{}", ok("a", "1"), ok("b", "2"), ok("c", "3")),
        "u",
        false,
    )?;
    let mixed = format!(
        "{}{}{}",
        ok("a", "1"),
        defer("b", well_formed),
        ok("c", "3")
    );
    // Strict parsing is unchanged: any deferral rejects the reply set.
    assert!(matches!(records(&mixed, "u", false), Err(e) if e.starts_with("DEFERRED")));
    let replies = partial_replies(&mixed, "u")?;
    let outcome = compare_partial(&oracle, &replies, "partial")?;
    assert_eq!(outcome.compared, ["a", "c"]);
    assert_eq!(outcome.deferred.len(), 1);
    assert_eq!(outcome.deferred[0].0, "b");
    assert_eq!(outcome.deferred[0].1.upstream, "MD5_HASH::Hash");
    assert_eq!(outcome.deferred[0].1.unit, "owner.unit");
    let alias = well_formed.replace("unported", "deferred");
    assert!(partial_replies(&defer("a", &alias), "u")?[0]
        .deferral
        .is_some());
    // A mismatch among the successful replies fails; so do identity faults.
    let changed = format!(
        "{}{}{}",
        ok("a", "1.0"),
        defer("b", well_formed),
        ok("c", "3")
    );
    assert!(compare_partial(&oracle, &partial_replies(&changed, "u")?, "p").is_err());
    let missing = format!("{}{}", ok("a", "1"), defer("b", well_formed));
    assert!(compare_partial(&oracle, &partial_replies(&missing, "u")?, "p").is_err());
    let reordered = format!(
        "{}{}{}",
        defer("b", well_formed),
        ok("a", "1"),
        ok("c", "3")
    );
    assert!(compare_partial(&oracle, &partial_replies(&reordered, "u")?, "p").is_err());
    assert!(partial_replies(&format!("{mixed}{}", defer("b", well_formed)), "u").is_err());
    // All-deferred compares nothing; the caller must refuse it.
    let all = format!(
        "{}{}{}",
        defer("a", well_formed),
        defer("b", well_formed),
        defer("c", well_formed)
    );
    let outcome = compare_partial(&oracle, &partial_replies(&all, "u")?, "p")?;
    assert!(outcome.compared.is_empty());
    assert_eq!(outcome.deferred.len(), 3);
    // Other error kinds and malformed deferrals are failures, never deferrals.
    for error in [
        r#"{"kind":"execution_error","message":"x"}"#,
        r#"{"kind":"invalid_input","message":"x"}"#,
        r#"{"kind":"unported","upstream":"f"}"#,
        r#"{"kind":"unported","unit":"owner"}"#,
        r#"{"kind":"unported","upstream":"f","unit":"owner","message":"x"}"#,
        r#"{"kind":"unported","upstream":"","unit":"owner"}"#,
        r#"{"kind":"unported","upstream":"f","unit":"../owner"}"#,
        r#"{"kind":"unported","upstream":"f","unit":1}"#,
        r#"{"kind":"unported","upstream":"f","unit":"owner","unit":"owner"}"#,
        r#""unported""#,
    ] {
        let result = partial_replies(&defer("a", error), "u");
        assert!(result.is_err(), "{error}");
    }
    let wrong_unit =
        r#"{"unit":"v","case":"a","error":{"kind":"unported","upstream":"f","unit":"o"}}"#;
    assert!(partial_replies(wrong_unit, "u").is_err());
    assert!(partial_replies("", "u").is_err());
    Ok(())
}
