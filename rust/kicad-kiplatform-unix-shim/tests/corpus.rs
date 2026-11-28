// SPDX-License-Identifier: GPL-3.0-or-later
//! Rust-only checks of the committed corpus and refusal controls. The exact C++ comparison is
//! the live difftest; these keep the Rust side's counts explicit, check that every control is
//! refused with its recorded message, and check decoded values of a few cases against what
//! the pinned C++ computes by hand.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

use std::process::Command;

use kicad_kiplatform_unix_shim::dispatch_with;
use serde_json::Value;

const CASES: &str =
    include_str!("../../../port/cases/libs.kiplatform.os.unix.application.ui.p001.jsonl");
const CONTROLS: &str =
    include_str!("../../../port/oracle/fixtures/kiplatform-unix-ui-p001-controls.jsonl");
const EXPECTED: &str = include_str!("../expected-replies.json");

fn terminate(code: i32) -> Result<i32, String> {
    let status = Command::new(env!("CARGO_BIN_EXE_kicad-kiplatform-unix-shim"))
        .arg("--terminate-child")
        .arg(code.to_string())
        .status()
        .map_err(|e| e.to_string())?;
    status.code().ok_or_else(|| "child did not exit".into())
}

fn replies() -> Vec<(Value, Value)> {
    CASES
        .lines()
        .map(|line| {
            let request: Value = serde_json::from_str(line).unwrap_or(Value::Null);
            (request, dispatch_with(line, Some(&terminate)))
        })
        .collect()
}

fn units(text: &str) -> Value {
    Value::from(text.chars().map(|c| c as i32).collect::<Vec<_>>())
}

#[test]
fn every_case_succeeds_or_defers_as_expected() {
    let expected: Value = serde_json::from_str(EXPECTED).unwrap_or(Value::Null);
    let mut deferred = 0u64;
    let mut cases = 0u64;
    for (request, reply) in replies() {
        cases += 1;
        assert_eq!(reply["case"], request["case"]);
        if reply["error"]["kind"] == "unported" {
            deferred += 1;
            let upstream = reply["error"]["upstream"].as_str().unwrap_or_default();
            assert!(
                upstream == "KIPLATFORM::IO::MAPPED_FILE::readIntoBuffer"
                    || upstream == "wxFileName::MakeAbsolute",
                "{reply}"
            );
            continue;
        }
        assert!(reply.get("result").is_some(), "{reply}");
    }
    assert_eq!(Some(cases), expected["cases"].as_u64());
    assert_eq!(Some(deferred), expected["deferred"].as_u64());
    assert_eq!(Some(cases - deferred), expected["compared"].as_u64());
}

#[test]
fn every_control_is_refused_with_its_message() {
    let expected: Value = serde_json::from_str(EXPECTED).unwrap_or(Value::Null);
    let mut count = 0u64;
    for line in CONTROLS.lines() {
        let control: Value = serde_json::from_str(line).unwrap_or(Value::Null);
        let request = control["request"].to_string();
        let reply = dispatch_with(&request, Some(&terminate));
        assert_eq!(reply["error"], control["error"], "{request}");
        count += 1;
    }
    assert_eq!(Some(count), expected["controls"].as_u64());
}

/// The first case of `op` whose input satisfies `pick`.
fn find<'a>(
    replies: &'a [(Value, Value)],
    op: &str,
    pick: impl Fn(&Value) -> bool,
) -> Option<&'a Value> {
    replies
        .iter()
        .find(|(request, _)| request["input"]["op"] == op && pick(&request["input"]))
        .map(|(_, reply)| &reply["result"])
}

fn proxy_list(input: &Value) -> Option<String> {
    let entry = input["host"]["replies"]["g_proxy_resolver_lookup"][0]["list"][0].as_str()?;
    let bytes: Vec<u8> = (0..entry.len() / 2)
        .filter_map(|i| u8::from_str_radix(entry.get(2 * i..2 * i + 2)?, 16).ok())
        .collect();
    String::from_utf8(bytes).ok()
}

#[test]
fn decoded_values_follow_the_pinned_bodies() {
    let all = replies();
    // GetSystemProxyConfig: scheme://host:port and the user information split at the colon.
    let auth = find(&all, "env_get_system_proxy_config", |i| {
        proxy_list(i).as_deref() == Some("http://user:secret@proxy:3128")
    });
    assert_eq!(auth.map(|r| &r["host"]), Some(&units("http://proxy:3128")));
    assert_eq!(auth.map(|r| &r["username"]), Some(&units("user")));
    assert_eq!(auth.map(|r| &r["password"]), Some(&units("secret")));
    // wx 3.2.4's ParseH16 skips the first character after '[', so "[::1]" is not an IPv6
    // literal: it becomes an escaped reg-name and the port has no digits.
    let v6 = find(&all, "env_get_system_proxy_config", |i| {
        proxy_list(i).as_deref() == Some("http://[::1]:8080") && i["host"].get("libc").is_none()
    });
    assert_eq!(v6.map(|r| &r["host"]), Some(&units("http://%5b:")));
    // The same skip makes "[2001:db8::1]" fail as IPv6 after "::" (ParseH16 skips the "1"),
    // so the escaped reg-name stops at the first colon and "db8" is not a port.
    let real_v6 = find(&all, "env_get_system_proxy_config", |i| {
        proxy_list(i).as_deref() == Some("http://[2001:db8::1]:443")
    });
    assert_eq!(real_v6.map(|r| &r["host"]), Some(&units("http://%5b2001:")));
    // IsFileHidden: the name part, not the path, starts with a dot.
    for (path, hidden) in [(".x", true), ("a/.x", true), ("x", false), (".a/b", false)] {
        let result = find(&all, "io_is_file_hidden", |i| {
            i["args"]["path"] == units(path)
        });
        if let Some(result) = result {
            assert_eq!(result["returned"], hidden, "{path}");
        }
    }
}
