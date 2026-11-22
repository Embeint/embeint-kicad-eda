// SPDX-License-Identifier: GPL-3.0-or-later
use kicad_difftest::evaluate::{
    check_golden, evaluate, Evaluation, Outcome, EXTRA_LINES_KEPT, EXTRA_LINE_LIMIT,
};
use kicad_difftest::process::{read_bounded, read_diagnostic, Captured};

/// Output of a program that ran, optionally ending in a process fault.
fn out(text: &str, fault: Option<&str>) -> Captured {
    Captured {
        started: true,
        stdout: text.as_bytes().to_vec(),
        fault: fault.map(str::to_owned),
        ..Captured::default()
    }
}

/// A program that never started.
fn never(fault: &str) -> Captured {
    Captured {
        fault: Some(fault.to_owned()),
        ..Captured::default()
    }
}
use kicad_difftest::{compare, exact::parse, golden, identities, partial_replies, records};
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
    let cases = records(
        "{\"unit\":\"u\",\"case\":\"a\",\"input\":{}}\n\
         {\"unit\":\"u\",\"case\":\"b\",\"input\":{}}\n\
         {\"unit\":\"u\",\"case\":\"c\",\"input\":{}}\n",
        "u",
        true,
    )?;
    let oracle_text = format!("{}{}{}", ok("a", "1"), ok("b", "2"), ok("c", "3"));
    let oracle = out(&oracle_text, None);
    let run = |rust: &str| evaluate(&cases, "u", &oracle, &out(rust, None));
    let names = |eval: &Evaluation| -> Vec<(String, &'static str)> {
        eval.outcomes
            .iter()
            .map(|c| (c.case.clone(), c.outcome.name()))
            .collect()
    };
    let expect = |pairs: &[(&str, &'static str)]| -> Vec<(String, &'static str)> {
        pairs.iter().map(|(c, n)| ((*c).to_owned(), *n)).collect()
    };
    let mixed = format!(
        "{}{}{}",
        ok("a", "1"),
        defer("b", well_formed),
        ok("c", "3")
    );
    // Strict parsing is unchanged: any deferral rejects the reply set.
    assert!(matches!(records(&mixed, "u", false), Err(e) if e.starts_with("DEFERRED")));
    assert!(partial_replies(&mixed, "u")?[1].deferral.is_some());
    let eval = run(&mixed);
    assert_eq!(
        names(&eval),
        expect(&[("a", "compared"), ("b", "deferred"), ("c", "compared")])
    );
    assert!(eval.faults.is_empty());
    let Outcome::Deferred(deferral) = &eval.outcomes[1].outcome else {
        return Err("b must be deferred".into());
    };
    assert_eq!(deferral.upstream, "MD5_HASH::Hash");
    assert_eq!(deferral.unit, "owner.unit");
    // The deferred case keeps the oracle's real reference result.
    assert_eq!(
        eval.outcomes[1].oracle.as_ref().map(|r| r.raw.as_str()),
        Some(ok("b", "2").trim_end())
    );
    let alias = well_formed.replace("unported", "deferred");
    assert!(partial_replies(&defer("a", &alias), "u")?[0]
        .deferral
        .is_some());
    // A mismatch after a success and a deferral keeps both earlier outcomes.
    let changed = format!(
        "{}{}{}",
        ok("a", "1"),
        defer("b", well_formed),
        ok("c", "3.0")
    );
    assert_eq!(
        names(&run(&changed)),
        expect(&[("a", "compared"), ("b", "deferred"), ("c", "failed")])
    );
    // A missing reply fails only its own case; a misplaced one fails its position.
    let missing = format!("{}{}", ok("a", "1"), defer("b", well_formed));
    assert_eq!(
        names(&run(&missing)),
        expect(&[("a", "compared"), ("b", "deferred"), ("c", "failed")])
    );
    let reordered = format!(
        "{}{}{}",
        defer("b", well_formed),
        ok("a", "1"),
        ok("c", "3")
    );
    assert_eq!(
        names(&run(&reordered)),
        expect(&[("a", "failed"), ("b", "failed"), ("c", "compared")])
    );
    let extra = run(&format!("{mixed}{}", defer("b", well_formed)));
    assert_eq!(extra.faults.len(), 1);
    assert!(partial_replies(&format!("{mixed}{}", defer("b", well_formed)), "u").is_err());
    // A Rust process failure leaves every case unprocessed with its reference kept;
    // an oracle failure leaves no reference, and an oracle deferral is a failure.
    let crashed = evaluate(&cases, "u", &oracle, &never("cannot start"));
    assert_eq!(crashed.count("unprocessed"), 3);
    assert!(crashed.outcomes.iter().all(|c| c.oracle.is_some()));
    let no_oracle = evaluate(&cases, "u", &never("cannot start"), &out(&mixed, None));
    assert_eq!(no_oracle.count("unprocessed"), 3);
    let oracle_defers = evaluate(
        &cases,
        "u",
        &out(
            &format!(
                "{}{}{}",
                ok("a", "1"),
                defer("b", well_formed),
                ok("c", "3")
            ),
            None,
        ),
        &out(&mixed, None),
    );
    assert_eq!(
        names(&oracle_defers),
        expect(&[("a", "compared"), ("b", "failed"), ("c", "compared")])
    );
    // All-deferred compares nothing; the caller must refuse it.
    let all = format!(
        "{}{}{}",
        defer("a", well_formed),
        defer("b", well_formed),
        defer("c", well_formed)
    );
    let eval = run(&all);
    assert_eq!(eval.count("compared"), 0);
    assert_eq!(eval.count("deferred"), 3);
    // The golden is compared with every available oracle result, deferred or not.
    let eval = run(&mixed);
    assert_eq!(
        check_golden(Ok(oracle_text.clone()), "u", &eval).status(),
        "matched"
    );
    let stale = oracle_text.replace("\"result\":2", "\"result\":2.0");
    let check = check_golden(Ok(stale), "u", &eval);
    assert_eq!(check.status(), "failed");
    assert_eq!(check.mismatched.len(), 1);
    assert_eq!(check.mismatched[0].0, "b");
    assert_eq!(
        check_golden(Err("missing".into()), "u", &eval).status(),
        "failed"
    );
    assert_eq!(
        check_golden(Ok(oracle_text.clone()), "u", &crashed).status(),
        "matched"
    );
    assert_eq!(
        check_golden(Ok(oracle_text.clone()), "u", &no_oracle).status(),
        "incomplete"
    );
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

#[test]
fn output_before_a_process_fault_is_evaluated_and_the_fault_still_fails() -> Result<(), String> {
    let line = |case: &str, value: &str| {
        format!("{{\"unit\":\"u\",\"case\":\"{case}\",\"result\":{value}}}\n")
    };
    let cases = records(
        "{\"unit\":\"u\",\"case\":\"a\",\"input\":{}}\n\
         {\"unit\":\"u\",\"case\":\"b\",\"input\":{}}\n\
         {\"unit\":\"u\",\"case\":\"c\",\"input\":{}}\n",
        "u",
        true,
    )?;
    let all = format!("{}{}{}", line("a", "1"), line("b", "2"), line("c", "3"));
    let names = |eval: &Evaluation| -> Vec<&'static str> {
        eval.outcomes.iter().map(|c| c.outcome.name()).collect()
    };
    let rust = out(&all, None);
    // Three complete replies, then exit 7 or a timeout: every reply is compared and
    // golden-checked, and the process fault is still a run fault.
    for fault in ["oracle: process failed (exit status: 7)", "timeout"] {
        let eval = evaluate(&cases, "u", &out(&all, Some(fault)), &rust);
        assert_eq!(names(&eval), ["compared", "compared", "compared"]);
        assert_eq!(eval.faults.len(), 1);
        assert!(eval.faults[0].contains(fault));
        assert!(eval.outcomes.iter().all(|c| c.oracle.is_some()));
        assert_eq!(
            check_golden(Ok(all.clone()), "u", &eval).status(),
            "matched"
        );
        let stale = all.replace("\"result\":2", "\"result\":20");
        let check = check_golden(Ok(stale), "u", &eval);
        assert_eq!(check.status(), "failed");
        assert_eq!(check.mismatched[0].0, "b");
    }
    // Output stopping early: answered cases keep their outcomes, the rest are
    // unprocessed and their golden lines unchecked.
    let two = format!("{}{}", line("a", "1"), line("b", "2"));
    let eval = evaluate(&cases, "u", &out(&two, Some("timeout")), &rust);
    assert_eq!(names(&eval), ["compared", "compared", "unprocessed"]);
    let check = check_golden(Ok(all.clone()), "u", &eval);
    assert_eq!(check.status(), "incomplete");
    assert_eq!(check.unchecked, ["c"]);
    // A truncated final line is a malformed reply; nothing after it is invented.
    let truncated = format!("{}{{\"unit\":\"u\",\"case\":\"b\",\"res", line("a", "1"));
    let eval = evaluate(&cases, "u", &out(&truncated, Some("timeout")), &rust);
    assert_eq!(names(&eval), ["compared", "failed", "unprocessed"]);
    assert!(eval.outcomes[1]
        .oracle_raw
        .as_deref()
        .is_some_and(|r| r.ends_with("\"res")));
    // The same output without a fault but with a missing final reply fails that case.
    let eval = evaluate(&cases, "u", &out(&two, None), &rust);
    assert_eq!(names(&eval), ["compared", "compared", "failed"]);
    assert!(eval.faults.is_empty());
    // Equivalent preservation for Rust: replies before its fault are compared.
    let eval = evaluate(&cases, "u", &out(&all, None), &out(&two, Some("exit 3")));
    assert_eq!(names(&eval), ["compared", "compared", "unprocessed"]);
    assert_eq!(eval.faults.len(), 1);
    // Malformed lines among recovered output fail only their own case.
    let malformed = format!("{}not json\n{}", line("a", "1"), line("c", "3"));
    let eval = evaluate(&cases, "u", &out(&malformed, Some("exit 1")), &rust);
    assert_eq!(names(&eval), ["compared", "failed", "compared"]);
    Ok(())
}

#[test]
fn extra_reply_lines_are_kept_bounded_as_evidence() -> Result<(), String> {
    let line = |case: &str| format!("{{\"unit\":\"u\",\"case\":\"{case}\",\"result\":1}}\n");
    let cases = records("{\"unit\":\"u\",\"case\":\"a\",\"input\":{}}\n", "u", true)?;
    let long = "x".repeat(EXTRA_LINE_LIMIT + 10);
    let mut oracle = format!("{}EXTRA_REFERENCE_EVIDENCE\n{long}\n", line("a"));
    for index in 0..EXTRA_LINES_KEPT {
        oracle.push_str(&format!("more {index}\n"));
    }
    let eval = evaluate(&cases, "u", &out(&oracle, None), &out(&line("a"), None));
    assert_eq!(eval.count("compared"), 1);
    assert_eq!(eval.faults.len(), 1);
    assert!(eval.faults[0].contains(&format!("{} reply lines", EXTRA_LINES_KEPT + 2)));
    assert_eq!(eval.extra.len(), EXTRA_LINES_KEPT);
    assert_eq!(eval.extra[0].raw, "EXTRA_REFERENCE_EVIDENCE");
    assert_eq!((eval.extra[0].side, eval.extra[0].line), ("oracle", 2));
    assert!(!eval.extra[0].truncated);
    assert_eq!(eval.extra[1].raw.len(), EXTRA_LINE_LIMIT);
    assert!(eval.extra[1].truncated);
    // Multi-byte characters are never split by the bound.
    let wide = "\u{e9}".repeat(EXTRA_LINE_LIMIT);
    let eval = evaluate(
        &cases,
        "u",
        &out(&line("a"), None),
        &out(&format!("{}{wide}\n", line("a")), None),
    );
    assert_eq!(eval.extra[0].side, "Rust");
    assert!(eval.extra[0].raw.len() <= EXTRA_LINE_LIMIT && eval.extra[0].truncated);
    Ok(())
}

/// An endless reader that counts every byte it hands out.
struct Endless {
    served: std::rc::Rc<std::cell::Cell<usize>>,
}

impl std::io::Read for Endless {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        buf.fill(b'e');
        self.served.set(self.served.get() + buf.len());
        Ok(buf.len())
    }
}

#[test]
fn diagnostics_are_read_bounded_not_truncated_after_reading() -> Result<(), String> {
    use std::io::Write;
    // The read itself is bounded: an endless reader would never finish a full read.
    let served = std::rc::Rc::new(std::cell::Cell::new(0));
    let bounded = read_bounded(
        Endless {
            served: served.clone(),
        },
        1000,
    );
    assert_eq!(bounded.bytes.len(), 1000);
    assert!(bounded.error.is_none());
    assert!(served.get() <= 1000, "served {}", served.get());
    // File diagnostics: totals come from metadata, at, below and above the limit.
    let limit = kicad_difftest::process::STDERR_LIMIT;
    for size in [0, 1, limit - 1, limit, limit + 1, 3 * limit + 7] {
        let mut file = tempfile::tempfile().map_err(|e| e.to_string())?;
        file.write_all(&vec![b'x'; size])
            .map_err(|e| e.to_string())?;
        let (text, total) = read_diagnostic(&mut file, limit);
        assert_eq!(total, Some(size as u64), "{size}");
        let kept = size.min(limit);
        assert!(text.starts_with(&"x".repeat(kept)), "{size}");
        if size > limit {
            assert_eq!(
                &text[kept..],
                format!(" [{} more bytes omitted]", size - limit),
                "{size}"
            );
        } else {
            assert_eq!(text.len(), size, "{size}");
        }
    }
    Ok(())
}
