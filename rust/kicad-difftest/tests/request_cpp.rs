// SPDX-License-Identifier: GPL-3.0-or-later
use kicad_difftest::{process, request};
use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn request_boundary_matches_pinned_cpp_protocol() -> TestResult {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest
        .parent()
        .and_then(Path::parent)
        .ok_or("repo root")?;
    let temp = tempfile::tempdir()?;
    let executable = temp
        .path()
        .join(format!("request-fixture{}", std::env::consts::EXE_SUFFIX));
    let compiler = std::env::var_os("CXX").unwrap_or_else(|| "c++".into());
    let build = Command::new(compiler)
        .args(["-std=c++17", "-O0", "-ffp-contract=off"])
        .arg("-I")
        .arg(root.join("thirdparty/nlohmann_json"))
        .arg("-I")
        .arg(root.join("port/oracle"))
        .arg(manifest.join("tests/request_fixture.cpp"))
        .arg(root.join("port/oracle/protocol.cpp"))
        .arg("-o")
        .arg(&executable)
        .output()?;
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );

    let envelope = |token: &str| format!(r#"{{"unit":"u","case":"c","input":{{"n":{token}}}}}"#);
    let mut cases = Vec::new();
    for token in [
        "-0",
        "0",
        "-0.0",
        "0.0",
        "-0e0",
        "0e0",
        "18446744073709551615",
        "18446744073709551616",
        "-9223372036854775808",
        "-9223372036854775809",
        "1.7976931348623157e308",
        "1.7976931348623158e308",
        "1.7976931348623159e308",
        "-1.7976931348623158e308",
        "-1.7976931348623159e308",
        "2.2250738585072014e-308",
        "4.9406564584124654e-324",
        "2.4703282292062327e-324",
        "1e-1000",
        "-1e-1000",
        "0e1000",
        "-0e1000",
        "1e1000",
        "-1e1000",
        "1e2147483648",
        "0e2147483648",
        "1e-2147483648",
        "NaN",
        "01",
        r#""1e1000""#,
    ] {
        cases.push(envelope(token));
    }
    for count in [308, 309, 400] {
        for sign in ["", "-"] {
            cases.push(envelope(&format!("{sign}{}", "9".repeat(count))));
        }
    }
    for token in [
        format!("{}e-100", "9".repeat(400)),
        format!("{}e-1000", "9".repeat(400)),
        format!("1e{}", "9".repeat(400)),
        format!("1e-{}", "9".repeat(400)),
        format!("0e{}", "9".repeat(400)),
        format!("-0e{}", "9".repeat(400)),
    ] {
        cases.push(envelope(&token));
    }
    // Exercise both rounding boundaries using exact integer decimal expansions.
    // f64::MAX = 2^1024 - 2^971; the overflow midpoint is 2^1024 - 2^970.
    for (token, accepted) in [
        (
            concat!(
                "179769313486231580793728971405303415079934132710037826936173778980444968292764",
                "750946649017977587207096330286416692887910946555547851940402630657488671505820",
                "681908902000708383676273854845817711531764475730270069855571366959622842914819",
                "860834936475292719074168444365510704342711559699508093042880177904174497791"
            ),
            true,
        ),
        (
            concat!(
                "179769313486231580793728971405303415079934132710037826936173778980444968292764",
                "750946649017977587207096330286416692887910946555547851940402630657488671505820",
                "681908902000708383676273854845817711531764475730270069855571366959622842914819",
                "860834936475292719074168444365510704342711559699508093042880177904174497792"
            ),
            false,
        ),
        (
            concat!(
                "179769313486231580793728971405303415079934132710037826936173778980444968292764",
                "750946649017977587207096330286416692887910946555547851940402630657488671505820",
                "681908902000708383676273854845817711531764475730270069855571366959622842914819",
                "860834936475292719074168444365510704342711559699508093042880177904174497793"
            ),
            false,
        ),
    ] {
        assert_eq!(request::parse(token).is_ok(), accepted, "{token}");
        cases.push(envelope(token));
        cases.push(envelope(&format!("-{token}")));
    }
    for text in [
        r#"{"unit":"u","case":"c","input":{"a":[{"n":1e1000}]}}"#,
        r#"{"unit":"u","case":"c","input":{},"extra":1e1000}"#,
        r#"{"unit":1e1000,"case":"c","input":{}}"#,
        r#"{"unit":"u","case":1e1000,"input":{}}"#,
        r#"{"unit":"u","case":"c","input":{"a":1,"\u0061":2}}"#,
        r#"{"unit":"u","case":"c","input":{"a":[{"x":1,"x":2}]}}"#,
        r#"{"unit":"u","case":"c","input":{"s":"\"1e1000\"","1e1000":null}}"#,
    ] {
        cases.push(text.to_owned());
    }
    let output = process::run(
        &process::Program {
            executable,
            args: vec![],
        },
        format!("{}\n", cases.join("\n")).as_bytes(),
        Duration::from_secs(30),
    )?;
    let observations: Vec<Value> = output
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    assert_eq!(observations.len(), cases.len());
    for (text, observation) in cases.iter().zip(&observations) {
        let cpp = &observation["reply"];
        match request::parse(text) {
            Ok(_) => assert_eq!(cpp["error"]["kind"], "unknown_unit", "{text}: {cpp}"),
            Err(message) => assert_eq!(
                cpp,
                &json!({
                    "unit": null, "case": null,
                    "error": {"kind": "invalid_request", "message": message}
                }),
                "{text}"
            ),
        }
    }
    for (token, integer, unsigned, negative_zero) in [
        ("-0", true, false, false),
        ("0", true, true, false),
        ("-0.0", false, false, true),
        ("0.0", false, false, false),
        ("18446744073709551615", true, true, false),
        ("18446744073709551616", false, false, false),
        ("-9223372036854775809", false, false, false),
        ("-1e-1000", false, false, true),
    ] {
        let text = envelope(token);
        let position = cases
            .iter()
            .position(|case| case == &text)
            .ok_or("missing case")?;
        let observation = observations.get(position).ok_or("missing observation")?;
        assert_eq!(observation["integer"], integer, "{token}");
        assert_eq!(observation["unsigned"], unsigned, "{token}");
        assert_eq!(observation["negative_zero"], negative_zero, "{token}");
    }
    Ok(())
}
