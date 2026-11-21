// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod exact;
pub mod golden;
pub mod process;

use exact::Exact;
use std::collections::BTreeSet;

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

/// Validates identities and status separately from result agreement.
pub fn records(text: &str, unit: &str, requests: bool) -> Result<Vec<Record>, String> {
    valid_unit(unit)?;
    let mut seen = BTreeSet::new();
    let mut records = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let context = || format!("{unit} line {}", index + 1);
        let value = exact::parse(line).map_err(|e| format!("{}: {e}", context()))?;
        let object = value.object()?;
        let field = |key: &str| {
            object
                .get(key)
                .ok_or_else(|| format!("{}: missing {key}", context()))
        };
        if field("unit")?.string()? != unit {
            return Err(format!("{}: wrong unit identity", context()));
        }
        let case = field("case")?.string()?.to_owned();
        if !seen.insert(case.clone()) {
            return Err(format!("{}: duplicate case {case}", context()));
        }
        if object.len() != 3 {
            return Err(format!(
                "{}: expected exactly three envelope fields",
                context()
            ));
        }
        if requests {
            field("input")?.object()?;
        } else if let Some(error) = object.get("error") {
            let error = error.object()?;
            let kind = error
                .get("kind")
                .ok_or_else(|| format!("{}: error missing kind", context()))?
                .string()?;
            if matches!(kind, "unported" | "deferred") {
                return Err(format!("DEFERRED {} case {case}: {line}", context()));
            }
            return Err(format!("FAILURE {} case {case}: {line}", context()));
        } else {
            field("result")?;
        }
        records.push(Record {
            case,
            raw: line.to_owned(),
            value,
        });
    }
    if records.is_empty() {
        return Err(format!("{unit}: empty case set is not a pass"));
    }
    Ok(records)
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
