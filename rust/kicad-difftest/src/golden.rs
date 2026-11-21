// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{compare, records, Record};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    file.write_all(bytes).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())
}

pub fn json_bytes(value: &Value) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub fn old(path: &Path, unit: &str) -> Result<(String, Vec<Record>), String> {
    match fs::read_to_string(path) {
        Ok(text) => {
            let values = records(&text, unit, false)?;
            Ok((text, values))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((String::new(), Vec::new())),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// Keeps the original text for unchanged cases; evidence lists additions,
/// removals, value changes and order changes with both original reply lines.
pub fn proposal(
    unit: &str,
    input: &[u8],
    old_text: &str,
    old: &[Record],
    new: &[Record],
    provenance: Value,
) -> (Value, String) {
    let old_by_case: BTreeMap<_, _> = old
        .iter()
        .enumerate()
        .map(|(i, r)| (r.case.as_str(), (i, r)))
        .collect();
    let mut changes = Vec::new();
    let mut text = String::new();
    for (index, record) in new.iter().enumerate() {
        let before = old_by_case.get(record.case.as_str());
        let line = match before {
            Some((_, before)) if before.value == record.value => &before.raw,
            _ => &record.raw,
        };
        text.push_str(line);
        text.push('\n');
        if !matches!(before, Some((i, b)) if *i == index && b.value == record.value) {
            changes.push(
                json!({"case": record.case, "old_index": before.map(|(i, _)| i),
                "new_index": index, "old": before.map(|(_, b)| &b.raw), "new": line}),
            );
        }
    }
    for (index, before) in old.iter().enumerate() {
        if !new.iter().any(|r| r.case == before.case) {
            changes.push(json!({"case": before.case, "old_index": index,
                "new_index": null, "old": before.raw, "new": null}));
        }
    }
    (
        json!({"format": 1, "unit": unit, "reason": "", "changes": changes,
        "cases_sha256": hash(input), "old_sha256": hash(old_text.as_bytes()),
        "new_sha256": hash(text.as_bytes()), "oracle": provenance}),
        text,
    )
}

pub fn approve(proposal: &Value, approval: &Value) -> Result<(), String> {
    let reason = approval
        .get("reason")
        .and_then(Value::as_str)
        .filter(|r| !r.trim().is_empty())
        .ok_or("blessing requires a nonempty reason")?;
    let mut expected = proposal.clone();
    expected["reason"] = Value::String(reason.into());
    if &expected != approval {
        return Err("approval does not exactly match current cases, old/new values and oracle provenance; propose again".into());
    }
    Ok(())
}

pub fn check(path: &Path, unit: &str, actual: &[Record]) -> Result<(), String> {
    compare(&records(&read(path)?, unit, false)?, actual, "golden")
}
