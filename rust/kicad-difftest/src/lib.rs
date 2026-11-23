// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod evaluate;
pub mod exact;
pub mod golden;
pub mod process;
pub mod request;

use exact::Exact;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
pub struct Record {
    pub case: String,
    pub raw: String,
    pub value: Exact,
}

pub fn valid_unit(unit: &str) -> Result<(), String> {
    if unit.is_empty()
        || unit.starts_with('.')
        || !unit
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    {
        return Err(format!("invalid unit id {unit:?}"));
    }
    Ok(())
}

/// A Rust deferral: the owning unit and upstream C++ symbol of an `Unported` call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deferral {
    pub upstream: String,
    pub unit: String,
}

/// One reply envelope with its validated identity. `deferral` is set only for a
/// well-formed `unported`/`deferred` error parsed by [`partial_replies`].
#[derive(Debug, Clone)]
pub struct Reply {
    pub record: Record,
    pub deferral: Option<Deferral>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Envelope {
    Request,
    /// Every error envelope fails; deferrals report as `DEFERRED`.
    Strict,
    /// Well-formed deferrals are retained; every other error still fails.
    Partial,
}

/// Validates identities and status separately from result agreement.
pub fn records(text: &str, unit: &str, requests: bool) -> Result<Vec<Record>, String> {
    let kind = if requests {
        Envelope::Request
    } else {
        Envelope::Strict
    };
    Ok(envelopes(text, unit, kind)?
        .into_iter()
        .map(|reply| reply.record)
        .collect())
}

/// Like strict reply parsing, but retains each well-formed deferral envelope
/// (exactly `kind`, `upstream` and `unit`, with a valid owning unit id) instead
/// of rejecting it. Every other error kind, and any malformed deferral, fails.
/// This only classifies replies; it never treats a deferral as a result.
pub fn partial_replies(text: &str, unit: &str) -> Result<Vec<Reply>, String> {
    envelopes(text, unit, Envelope::Partial)
}

fn deferral(error: &BTreeMap<String, Exact>) -> Result<Deferral, String> {
    if error.len() != 3 {
        return Err("expected exactly kind, upstream and unit".into());
    }
    let field = |key: &str| {
        error
            .get(key)
            .ok_or_else(|| format!("missing {key}"))?
            .string()
            .map(str::to_owned)
    };
    let owner = field("unit")?;
    valid_unit(&owner)?;
    Ok(Deferral {
        upstream: field("upstream")?,
        unit: owner,
    })
}

fn envelopes(text: &str, unit: &str, kind: Envelope) -> Result<Vec<Reply>, String> {
    valid_unit(unit)?;
    let mut seen = BTreeSet::new();
    let mut replies = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let reply = parse_envelope(line, unit, kind, index)?;
        let case = &reply.record.case;
        if !seen.insert(case.clone()) {
            return Err(format!("{unit} line {}: duplicate case {case}", index + 1));
        }
        replies.push(reply);
    }
    if replies.is_empty() {
        return Err(format!("{unit}: empty case set is not a pass"));
    }
    Ok(replies)
}

/// Validates one envelope line (zero-based `index`) with its identity and status.
/// Set-level checks (duplicates, emptiness, order) belong to the caller.
pub(crate) fn parse_envelope(
    line: &str,
    unit: &str,
    kind: Envelope,
    index: usize,
) -> Result<Reply, String> {
    let context = || format!("{unit} line {}", index + 1);
    let value = exact::parse(line).map_err(|e| format!("{}: {e}", context()))?;
    let object = value.object().map_err(|e| format!("{}: {e}", context()))?;
    let field = |key: &str| {
        object
            .get(key)
            .ok_or_else(|| format!("{}: missing {key}", context()))
    };
    if field("unit")?.string()? != unit {
        return Err(format!("{}: wrong unit identity", context()));
    }
    let case = field("case")?.string()?.to_owned();
    if object.len() != 3 {
        return Err(format!(
            "{}: expected exactly three envelope fields",
            context()
        ));
    }
    let mut deferred = None;
    if kind == Envelope::Request {
        field("input")?.object()?;
    } else if let Some(error) = object.get("error") {
        let error = error
            .object()
            .map_err(|e| format!("FAILURE {} case {case}: {e}: {line}", context()))?;
        let error_kind = error
            .get("kind")
            .ok_or_else(|| {
                format!(
                    "FAILURE {} case {case}: error missing kind: {line}",
                    context()
                )
            })?
            .string()
            .map_err(|e| format!("FAILURE {} case {case}: error kind {e}: {line}", context()))?;
        if !matches!(error_kind, "unported" | "deferred") {
            return Err(format!("FAILURE {} case {case}: {line}", context()));
        }
        if kind == Envelope::Strict {
            return Err(format!("DEFERRED {} case {case}: {line}", context()));
        }
        deferred = Some(deferral(error).map_err(|e| {
            format!(
                "FAILURE {} case {case}: invalid deferral envelope ({e}): {line}",
                context()
            )
        })?);
    } else {
        field("result")?;
    }
    Ok(Reply {
        record: Record {
            case,
            raw: line.to_owned(),
            value,
        },
        deferral: deferred,
    })
}

pub fn identities(expected: &[Record], actual: &[Record], label: &str) -> Result<(), String> {
    if expected.len() != actual.len() {
        return Err(format!(
            "{label}: case count {} != {}",
            actual.len(),
            expected.len()
        ));
    }
    for (index, (a, b)) in expected.iter().zip(actual).enumerate() {
        if a.case != b.case {
            return Err(format!(
                "{label}: case order/identity at {index}: {} != {}",
                b.case, a.case
            ));
        }
    }
    Ok(())
}

pub fn compare(expected: &[Record], actual: &[Record], label: &str) -> Result<(), String> {
    identities(expected, actual, label)?;
    for (a, b) in expected.iter().zip(actual) {
        if a.value != b.value {
            return Err(format!(
                "{label}: changed case {}\nold: {}\nnew: {}",
                a.case, a.raw, b.raw
            ));
        }
    }
    Ok(())
}
