// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

//! JSONL adapter for unit `libs.kiplatform.os.unix.application.ui.p001`. The operation and host
//! script schema is in README.md and mirrors `port/oracle/shims/kiplatform_unix_ui_p001.cpp`
//! field for field, including validation order and refusal messages. The scripted host
//! ([`host::ScriptedHost`]) answers every boundary call of `kicad_kiplatform_unix` as the
//! adapter's `--wrap` fakes answer the pinned C++.

pub mod host;
pub mod json;
pub mod operations;

use serde_json::{json, Value};

pub use kicad_kiplatform_unix::UNIT;

pub use operations::Terminate;

fn keys(value: &Value, fields: &[&str]) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == fields.len() && fields.iter().all(|f| object.contains_key(*f))
    })
}

/// Answer one protocol line; `TerminateWithCode` is refused (no child process).
pub fn dispatch(line: &str) -> Value {
    dispatch_with(line, None)
}

/// Answer one protocol line, running `TerminateWithCode` through `terminate`.
pub fn dispatch_with(line: &str, terminate: Terminate<'_>) -> Value {
    let mut unit = Value::Null;
    let mut case = Value::Null;
    let failure = |unit: Value, case: Value, kind: &str, message: &str| json!({"case":case,"error":{"kind":kind,"message":message},"unit":unit});
    let request = match kicad_difftest::request::parse(line) {
        Ok(request) => request,
        Err(_) => {
            return failure(
                unit,
                case,
                "invalid_request",
                "invalid JSON or duplicate key",
            )
        }
    };
    if let Some(value) = request.get("unit").filter(|v| v.is_string()) {
        unit = value.clone();
    }
    if let Some(value) = request.get("case").filter(|v| v.is_string()) {
        case = value.clone();
    }
    if !keys(&request, &["unit", "case", "input"])
        || unit.as_str().is_none_or(str::is_empty)
        || case.as_str().is_none_or(str::is_empty)
        || !request["input"].is_object()
    {
        return failure(
            unit,
            case,
            "invalid_request",
            "expected unit, case, input object",
        );
    }
    if unit != UNIT {
        return failure(unit, case, "unknown_unit", "unit has no registered shim");
    }
    match operations::run(&request["input"], terminate) {
        Ok(result) => json!({"case":case,"result":result,"unit":unit}),
        Err(refusal) => {
            if let Some((upstream, owner)) = operations::as_unported(&refusal) {
                return json!({"case":case,"error":{"kind":"unported","upstream":upstream,"unit":owner},"unit":unit});
            }
            match refusal {
                json::Refusal::Input(message) => failure(unit, case, "invalid_input", &message),
                json::Refusal::Execution(message) => {
                    failure(unit, case, "execution_error", &message)
                }
            }
        }
    }
}

/// The child side of `TerminateWithCode`: run the body with a host whose `_exit` ends this
/// process with the status.
pub fn terminate_child(code: i32) -> ! {
    match host::ScriptedHost::new(&json!({})) {
        Ok(mut host) => {
            host.exit = std::process::exit;
            kicad_kiplatform_unix::app::terminate_with_code(&mut host, code)
        }
        Err(_) => std::process::exit(255),
    }
}
