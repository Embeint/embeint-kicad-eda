// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kicad_difftest::evaluate::{check_golden, evaluate, Evaluation, GoldenCheck, Outcome, RawLine};
use kicad_difftest::process::Program;
use kicad_difftest::{golden, process, records, valid_unit, Record};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

const HELP: &str =
    "kicad-difftest <compare|check|propose|bless> --unit ID --oracle PATH --rust PATH
  [--root PATH] [--oracle-arg ARG ...] [--rust-arg ARG ...] [--timeout-secs N]
  [--report PATH] [--partial] [--approval PATH --provenance PATH] [--evidence PATH]
compare: live oracle/Rust agreement; check: also require committed golden.
propose: write approval skeleton after live agreement; bless: verify filled approval,
rerun live comparison, write new evidence then golden.
--partial (compare/check only, for units declared partial): report well-formed Rust
unported/deferred replies instead of failing on them; every other case is still
compared exactly, every oracle reply must succeed (and match the golden for check),
and at least one real exact comparison is required. See kicad-difftest/README.md.";

struct Options {
    action: String,
    values: BTreeMap<String, String>,
    oracle_args: Vec<String>,
    rust_args: Vec<String>,
    /// Explicit partial-unit mode; never a default and never valid for blessing.
    partial: bool,
}

impl Options {
    fn get(&self, key: &str) -> Result<&str, String> {
        self.values
            .get(key)
            .map(String::as_str)
            .ok_or_else(|| format!("missing --{key}"))
    }
    fn path(&self, key: &str) -> Result<PathBuf, String> {
        Ok(PathBuf::from(self.get(key)?))
    }
}

fn options() -> Result<Options, String> {
    let mut args = std::env::args().skip(1);
    let action = args.next().ok_or(HELP)?;
    if !["compare", "check", "propose", "bless"].contains(&action.as_str()) {
        return Err(HELP.into());
    }
    let mut opts = Options {
        action,
        values: BTreeMap::new(),
        oracle_args: Vec::new(),
        rust_args: Vec::new(),
        partial: false,
    };
    while let Some(key) = args.next() {
        if key == "--partial" {
            if opts.partial {
                return Err("unknown or repeated option --partial".into());
            }
            opts.partial = true;
            continue;
        }
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {key}"))?;
        match key.as_str() {
            "--oracle-arg" => opts.oracle_args.push(value),
            "--rust-arg" => opts.rust_args.push(value),
            _ => {
                let key = key.strip_prefix("--").ok_or(HELP)?;
                if ![
                    "unit",
                    "oracle",
                    "rust",
                    "root",
                    "timeout-secs",
                    "report",
                    "approval",
                    "provenance",
                    "evidence",
                ]
                .contains(&key)
                    || opts.values.insert(key.into(), value).is_some()
                {
                    return Err(format!("unknown or repeated option --{key}"));
                }
            }
        }
    }
    if opts.partial && !matches!(opts.action.as_str(), "compare" | "check") {
        return Err(
            "--partial applies only to compare and check; golden blessing requires a zero-deferral comparison"
                .into(),
        );
    }
    Ok(opts)
}

fn file_hash(path: &std::path::Path) -> Result<String, String> {
    Ok(golden::hash(
        &std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?,
    ))
}

fn run(opts: &Options) -> Result<Value, String> {
    let unit = opts.get("unit")?;
    valid_unit(unit)?;
    let root = opts.values.get("root").map_or(".", String::as_str);
    let root = PathBuf::from(root);
    let case_path = root.join("port/cases").join(format!("{unit}.jsonl"));
    let golden_path = root.join("port/goldens").join(format!("{unit}.jsonl"));
    let input = golden::read(&case_path)?;
    let cases = records(&input, unit, true)?;
    let timeout: u64 = opts.values.get("timeout-secs").map_or(Ok(60), |s| {
        s.parse().map_err(|e| format!("invalid timeout: {e}"))
    })?;
    if timeout == 0 {
        return Err("timeout must be positive".into());
    }
    let oracle = Program {
        executable: opts.path("oracle")?,
        args: opts.oracle_args.clone(),
    };
    let rust = Program {
        executable: opts.path("rust")?,
        args: opts.rust_args.clone(),
    };
    // Both programs always run, and each run's output is kept however it ended,
    // so one side's fault never hides the other's evidence or its own replies.
    let oracle_out = process::capture(&oracle, input.as_bytes(), Duration::from_secs(timeout));
    let rust_out = process::capture(&rust, input.as_bytes(), Duration::from_secs(timeout));
    let mut eval = evaluate(&cases, unit, &oracle_out, &rust_out);
    // Every available oracle reply, including those of Rust-deferred cases, is
    // compared with the golden.
    let golden_check =
        (opts.action == "check").then(|| check_golden(golden::read(&golden_path), unit, &eval));
    if status(&eval, golden_check.as_ref(), opts.partial) == "green"
        && matches!(opts.action.as_str(), "propose" | "bless")
    {
        // A green evaluation has a successful oracle record for every case.
        let oracle_records: Vec<Record> = eval
            .outcomes
            .iter()
            .filter_map(|c| c.oracle.clone())
            .collect();
        if let Err(error) = bless(
            opts,
            unit,
            &input,
            &golden_path,
            &oracle,
            &rust,
            timeout,
            &oracle_records,
        ) {
            eval.faults.push(format!("{}: {error}", opts.action));
        }
    }
    // The report is built only from the final outcome, so a failed blessing step
    // keeps the full reference evidence of a failed run.
    let mut report = report(
        opts,
        unit,
        &input,
        &eval,
        golden_check.as_ref(),
        [&oracle_out, &rust_out],
    );
    report["oracle_sha256"] = json!(file_hash(&oracle.executable).ok());
    report["rust_sha256"] = json!(file_hash(&rust.executable).ok());
    Ok(report)
}

/// Writes a proposal or, after verifying its approval, evidence and then the golden.
#[allow(clippy::too_many_arguments)]
fn bless(
    opts: &Options,
    unit: &str,
    input: &str,
    golden_path: &std::path::Path,
    oracle: &Program,
    rust: &Program,
    timeout: u64,
    oracle_records: &[Record],
) -> Result<(), String> {
    match opts.action.as_str() {
        "propose" | "bless" => {
            let provenance_text = golden::read(&opts.path("provenance")?)?;
            let supplied_provenance = kicad_difftest::exact::parse(&provenance_text)?;
            let mut provenance_program = oracle.clone();
            provenance_program.args.push("--provenance".into());
            let live_provenance =
                process::run(&provenance_program, &[], Duration::from_secs(timeout))?;
            if supplied_provenance != kicad_difftest::exact::parse(&live_provenance)? {
                return Err("supplied provenance differs from the live oracle --provenance".into());
            }
            let provenance: Value =
                serde_json::from_str(&provenance_text).map_err(|e| e.to_string())?;
            if !matches!(&provenance, Value::Object(fields) if !fields.is_empty()) {
                return Err("oracle provenance must be a nonempty JSON object recording source revision, compiler and flags".into());
            }
            let provenance = json!({"provenance": provenance,
                "oracle_sha256": file_hash(&oracle.executable)?, "oracle_args": oracle.args,
                "rust_sha256": file_hash(&rust.executable)?, "rust_args": rust.args});
            let (old_text, old) = golden::old(golden_path, unit)?;
            let (proposal, new_text) = golden::proposal(
                unit,
                input.as_bytes(),
                &old_text,
                &old,
                oracle_records,
                provenance,
            );
            let approval_path = opts.path("approval")?;
            if opts.action == "propose" {
                golden::write_new(&approval_path, &golden::json_bytes(&proposal)?)?;
            } else {
                let approval_text = golden::read(&approval_path)?;
                kicad_difftest::exact::parse(&approval_text)?;
                let approval: Value =
                    serde_json::from_str(&approval_text).map_err(|e| e.to_string())?;
                golden::approve(&proposal, &approval)?;
                // Evidence must be durable before the golden can change. A failed
                // golden replacement leaves evidence to inspect, never the reverse.
                golden::write_new(&opts.path("evidence")?, &golden::json_bytes(&approval)?)?;
                let parent = golden_path.parent().ok_or("golden has no parent")?;
                let mut temporary =
                    tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
                use std::io::Write;
                temporary
                    .write_all(new_text.as_bytes())
                    .map_err(|e| e.to_string())?;
                temporary.as_file().sync_all().map_err(|e| e.to_string())?;
                temporary.persist(golden_path).map_err(|e| e.to_string())?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Builds the run report from the per-case evaluation.
///
/// Every corpus case appears once in `outcomes`, in corpus order. Deferred, failed
/// and unprocessed cases carry the oracle and Rust reply lines that were available.
/// Compared cases carry the oracle line only when the run is not accepted, so a
/// failed run keeps every recoverable reference result while an accepted report
/// stays proportional to its exceptions (the golden holds the agreed results).
fn report(
    opts: &Options,
    unit: &str,
    input: &str,
    eval: &Evaluation,
    golden_check: Option<&GoldenCheck>,
    outputs: [&process::Captured; 2],
) -> Value {
    let compared = eval.count("compared");
    let deferred = eval.count("deferred");
    let failures = eval.count("failed");
    let unprocessed = eval.count("unprocessed");
    let status = status(eval, golden_check, opts.partial);
    let accepted_run = status == "green" || (status == "partial" && opts.partial);
    let outcomes: Vec<Value> = eval
        .outcomes
        .iter()
        .map(|c| {
            let mut entry = json!({"case": c.case, "outcome": c.outcome.name()});
            match &c.outcome {
                Outcome::Compared => {}
                Outcome::Deferred(d) => {
                    entry["upstream"] = json!(d.upstream);
                    entry["unit"] = json!(d.unit);
                }
                Outcome::Failed(reason) | Outcome::Unprocessed(reason) => {
                    entry["reason"] = json!(reason);
                }
            }
            if c.outcome != Outcome::Compared || !accepted_run {
                match (&c.oracle, &c.oracle_raw) {
                    (Some(record), _) => entry["oracle"] = json!(record.raw),
                    (None, Some(raw)) => put_raw(&mut entry, "oracle", raw),
                    (None, None) => {}
                }
            }
            if c.outcome != Outcome::Compared {
                if let Some(raw) = &c.rust {
                    put_raw(&mut entry, "rust", raw);
                }
            }
            entry
        })
        .collect();
    let extra: Vec<Value> = eval
        .extra
        .iter()
        .map(|e| {
            let mut entry = json!({"side": e.side, "line": e.line, "truncated": e.truncated});
            put_raw(&mut entry, "raw", &e.raw);
            entry
        })
        .collect();
    let golden = match golden_check {
        None => json!({"status": "not_requested"}),
        Some(g) => json!({"status": g.status(), "error": g.error,
            "mismatched_cases": g.mismatched.iter()
                .map(|(case, golden, oracle)| json!({"case": case, "golden": golden, "oracle": oracle}))
                .collect::<Vec<_>>(),
            "unchecked_cases": g.unchecked}),
    };
    let mut report = json!({"unit": unit, "status": status, "action": opts.action,
        "mode": if opts.partial { "partial" } else { "strict" },
        "cases": eval.outcomes.len(), "compared": compared, "deferred": deferred,
        "failures": failures, "unprocessed": unprocessed,
        "golden_checked": golden_check.is_some(), "golden": golden,
        "run_faults": eval.faults, "extra_replies": extra, "outcomes": outcomes,
        "cases_sha256": golden::hash(input.as_bytes()),
        "oracle_output_sha256": output_hash(outputs[0]),
        "rust_output_sha256": output_hash(outputs[1])});
    if !accepted_run {
        report["error"] = json!(first_error(status, eval, golden_check));
    }
    report
}

/// Stores a raw line under `key` when it is valid UTF-8, otherwise its exact bytes
/// as lowercase hex under `<key>_hex`; invalid bytes are never decoded lossily.
fn put_raw(entry: &mut Value, key: &str, raw: &RawLine) {
    match raw {
        RawLine::Text(text) => entry[key] = json!(text),
        RawLine::Bytes(bytes) => {
            let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
            entry[format!("{key}_hex")] = json!(hex);
        }
    }
}

/// The SHA-256 of every captured stdout byte, or null if the program never ran.
fn output_hash(output: &process::Captured) -> Value {
    json!(output.started.then(|| golden::hash(&output.stdout)))
}

/// Failures take precedence; all-deferred proves no successful operation; a
/// deferral is acceptable only in explicit partial mode.
fn status(eval: &Evaluation, golden_check: Option<&GoldenCheck>, partial: bool) -> &'static str {
    let golden_ok = golden_check.is_none_or(|g| g.status() == "matched");
    if eval.count("failed") > 0
        || eval.count("unprocessed") > 0
        || !eval.faults.is_empty()
        || !golden_ok
    {
        "failed"
    } else if eval.count("compared") == 0 {
        "deferred"
    } else if eval.count("deferred") > 0 {
        if partial {
            "partial"
        } else {
            "deferred"
        }
    } else {
        "green"
    }
}

/// A one-line diagnostic for stderr; the report holds every outcome.
fn first_error(status: &str, eval: &Evaluation, golden_check: Option<&GoldenCheck>) -> String {
    let case_fault = eval.outcomes.iter().find_map(|c| match &c.outcome {
        Outcome::Failed(reason) | Outcome::Unprocessed(reason) => {
            Some(format!("FAILURE case {}: {reason}", c.case))
        }
        _ => None,
    });
    let golden = golden_check.and_then(|g| {
        g.error.clone().or_else(|| {
            g.mismatched
                .first()
                .map(|(case, old, new)| {
                    format!("golden: changed case {case}\nold: {old}\nnew: {new}")
                })
                .or_else(|| {
                    g.unchecked
                        .first()
                        .map(|case| format!("golden: case {case} unchecked"))
                })
        })
    });
    let deferral = eval.outcomes.iter().find_map(|c| match &c.outcome {
        Outcome::Deferred(d) => Some(format!(
            "DEFERRED case {} ({} owned by {}); rerun with --partial only for a unit declared partial",
            c.case, d.upstream, d.unit
        )),
        _ => None,
    });
    let message = case_fault
        .or_else(|| eval.faults.first().map(|f| format!("FAILURE {f}")))
        .or(golden.map(|g| format!("FAILURE {g}")));
    match (status, message) {
        (_, Some(message)) => message,
        ("deferred", None) if eval.count("compared") == 0 => format!(
            "DEFERRED all {} cases deferred; a partial unit needs at least one successful exact comparison",
            eval.outcomes.len()
        ),
        (_, None) => deferral.unwrap_or_else(|| format!("{status} run")),
    }
}

/// Only a strict green run, or a partial run with at least one exact comparison
/// requested explicitly with --partial, is accepted.
fn accepted(report: &Value, partial: bool) -> bool {
    match report.get("status").and_then(Value::as_str) {
        Some("green") => true,
        Some("partial") => partial,
        _ => false,
    }
}

fn main() -> std::process::ExitCode {
    if matches!(std::env::args().nth(1).as_deref(), Some("--help" | "-h")) {
        println!("{HELP}");
        return std::process::ExitCode::SUCCESS;
    }
    let opts = match options() {
        Ok(opts) => opts,
        Err(error) => {
            eprintln!("{error}");
            return std::process::ExitCode::from(2);
        }
    };
    let result = run(&opts);
    let report = match &result {
        Ok(report) => report.clone(),
        Err(error) => {
            json!({"status": if error.starts_with("DEFERRED") { "deferred" } else { "failed" },
            "unit": opts.values.get("unit"), "action": opts.action,
            "mode": if opts.partial { "partial" } else { "strict" }, "error": error})
        }
    };
    if let Some(path) = opts.values.get("report") {
        let written = golden::json_bytes(&report)
            .and_then(|bytes| std::fs::write(path, bytes).map_err(|e| e.to_string()));
        if let Err(error) = written {
            eprintln!("cannot write report: {error}");
            return std::process::ExitCode::from(1);
        }
    }
    if accepted(&report, opts.partial) {
        // The --report file keeps every case outcome; stdout carries the counts only.
        let mut summary = report;
        if let Some(fields) = summary.as_object_mut() {
            fields.remove("outcomes");
        }
        summary["golden"] = summary["golden"]["status"].clone();
        println!("{summary}");
        return std::process::ExitCode::SUCCESS;
    }
    match result {
        Ok(report) => eprintln!("{}", report.get("error").unwrap_or(&report)),
        Err(error) => eprintln!("{error}"),
    }
    std::process::ExitCode::from(1)
}
