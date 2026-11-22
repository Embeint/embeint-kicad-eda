// SPDX-License-Identifier: GPL-3.0-or-later
use kicad_difftest::{compare, exact, records, request};
use serde_json::Value;

fn invalid(text: &str) {
    assert_eq!(
        request::parse(text),
        Err("invalid JSON or duplicate key".to_owned()),
        "{text}"
    );
}

#[test]
fn overflow_is_a_parse_failure_everywhere() {
    for token in ["1e1000".to_owned(), "-1e1000".to_owned(), "9".repeat(400)] {
        for text in [
            token.clone(),
            format!(r#"{{"unit":"u","case":"c","input":{{"n":{token}}}}}"#),
            format!(r#"{{"unit":"u","case":"c","input":{{"nested":[{{"n":{token}}}]}}}}"#),
            format!(r#"{{"unit":"u","case":"c","input":{{}},"extra":[{token}]}}"#),
            format!(r#"{{"unit":{token},"case":"c","input":{{}}}}"#),
            format!(r#"{{"unit":"u","case":{token},"input":{{}}}}"#),
        ] {
            invalid(&text);
        }
    }
}

#[test]
fn finite_fallback_and_underflow_remain_parseable() -> Result<(), String> {
    for token in [
        "18446744073709551616",
        "-9223372036854775809",
        "1.7976931348623157e308",
        "1.7976931348623158e308",
        "-1.7976931348623158e308",
        "2.2250738585072014e-308",
        "4.9406564584124654e-324",
        "2.4703282292062327e-324",
        "1e-1000",
        "-1e-1000",
        "0e1000",
        "-0e1000",
        "1E+2",
    ] {
        let value = request::parse(&format!("[{token}]"))?;
        assert!(value[0].is_number(), "{token}");
    }
    let value = request::parse("18446744073709551616")?;
    assert_eq!(value.as_u64(), None);
    let value = request::parse("-9223372036854775809")?;
    assert_eq!(value.as_i64(), None);
    for token in ["1.7976931348623159e308", "-1.7976931348623159e308", "1e309"] {
        invalid(token);
    }
    // Large integers use nlohmann's floating fallback, rather than an i64/u64 guard.
    assert!(request::parse(&"9".repeat(308)).is_ok());
    invalid(&"9".repeat(309));
    Ok(())
}

#[test]
fn signed_zero_and_full_integer_domain_survive_validation() -> Result<(), String> {
    for token in [
        "-0",
        "0",
        "-0.0",
        "0.0",
        "-0e+0",
        "0e+0",
        "-9223372036854775808",
        "9223372036854775808",
        "18446744073709551615",
    ] {
        let value = request::parse(&format!(r#"{{"n":{token}}}"#))?;
        // Preserve exactly what the adapters' existing Value deserialization returns.
        let original: Value =
            serde_json::from_str(&format!(r#"{{"n":{token}}}"#)).map_err(|e| e.to_string())?;
        assert_eq!(value, original);
        assert_eq!(value["n"].to_string(), token, "{token}");
    }
    let max = request::parse("18446744073709551615")?;
    assert_eq!(max.as_u64(), Some(u64::MAX));
    assert_eq!(max.as_i64(), None);
    assert_eq!(request::parse("-0")?.as_i64(), Some(0));
    assert_eq!(request::parse("-0.0")?.as_i64(), None);
    Ok(())
}

#[test]
fn syntax_duplicates_and_nesting_are_checked_before_value_conversion() -> Result<(), String> {
    for text in [
        "",
        "NaN",
        "Infinity",
        "01",
        "[1,]",
        "{} {}",
        r#"{"a":1,"a":2}"#,
        r#"{"a":[{"x":1,"\u0078":2}]}"#,
    ] {
        invalid(text);
    }
    let nested = format!("{}0{}", "[".repeat(127), "]".repeat(127));
    assert!(request::parse(&nested).is_ok());
    let too_deep = format!("{}0{}", "[".repeat(129), "]".repeat(129));
    assert!(exact::parse(&too_deep).is_err());
    invalid(&too_deep);
    let text = r#"{"1e1000":"1e1000","n":"9999999999999999999999999999999999999999","x":[true,false,null],"escaped":"\"1e1000\""}"#;
    let value = request::parse(text)?;
    assert_eq!(value["1e1000"], "1e1000");
    assert_eq!(value["escaped"], "\"1e1000\"");
    // Envelope validation belongs to the adapter, after this parsing boundary.
    assert_eq!(request::parse("[]")?, serde_json::json!([]));
    Ok(())
}

#[test]
fn comparator_still_accepts_and_distinguishes_arbitrary_precision_tokens() -> Result<(), String> {
    for (a, b) in [
        ("1e1000".to_owned(), "1e1001".to_owned()),
        ("9".repeat(400), format!("{}8", "9".repeat(399))),
        ("-0".to_owned(), "0".to_owned()),
        ("-0.0".to_owned(), "0.0".to_owned()),
    ] {
        assert_ne!(exact::parse(&a)?, exact::parse(&b)?);
        let line = |token: &str| format!(r#"{{"unit":"u","case":"c","result":{token}}}"#);
        let old = records(&line(&a), "u", false)?;
        compare(&old, &records(&line(&a), "u", false)?, "same")?;
        assert!(compare(&old, &records(&line(&b), "u", false)?, "changed").is_err());
    }
    Ok(())
}
