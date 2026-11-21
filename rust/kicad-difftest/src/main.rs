// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kicad_difftest::process::Program;
use kicad_difftest::{compare, golden, identities, process, records, valid_unit};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

const HELP: &str =
    "kicad-difftest <compare|check|propose|bless> --unit ID --oracle PATH --rust PATH
  [--root PATH] [--oracle-arg ARG ...] [--rust-arg ARG ...] [--timeout-secs N]
  [--report PATH] [--approval PATH --provenance PATH] [--evidence PATH]
compare: live oracle/Rust agreement; check: also require committed golden.
propose: write approval skeleton after live agreement; bless: verify filled approval,
rerun live comparison, write new evidence then golden. See kicad-difftest/README.md.";

struct Options {
    action: String,
    values: BTreeMap<String, String>,
    oracle_args: Vec<String>,
    rust_args: Vec<String>,
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
    };
    while let Some(key) = args.next() {
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
    let oracle_text = process::run(&oracle, input.as_bytes(), Duration::from_secs(timeout))?;
    let oracle_records = records(&oracle_text, unit, false)?;
    identities(&cases, &oracle_records, "oracle")?;
    let rust_text = process::run(&rust, input.as_bytes(), Duration::from_secs(timeout))?;
    let rust_records = records(&rust_text, unit, false)?;
    identities(&cases, &rust_records, "Rust")?;
    compare(&oracle_records, &rust_records, "oracle vs Rust")?;
    match opts.action.as_str() {
        "check" => golden::check(&golden_path, unit, &oracle_records)?,
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
            let (old_text, old) = golden::old(&golden_path, unit)?;
            let (proposal, new_text) = golden::proposal(
                unit,
                input.as_bytes(),
                &old_text,
                &old,
                &oracle_records,
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
                temporary.persist(&golden_path).map_err(|e| e.to_string())?;
            }
        }
        _ => {}
    }
    Ok(
        json!({"unit": unit, "status": "green", "action": opts.action,
        "cases": cases.len(), "deferred": 0, "cases_sha256": golden::hash(input.as_bytes()),
        "oracle_output_sha256": golden::hash(oracle_text.as_bytes()),
        "rust_output_sha256": golden::hash(rust_text.as_bytes()),
        "oracle_sha256": file_hash(&oracle.executable)?, "rust_sha256": file_hash(&rust.executable)?}),
    )
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
            "unit": opts.values.get("unit"), "action": opts.action, "error": error})
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
    match result {
        Ok(_) => {
            println!("{report}");
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::from(1)
        }
    }
}
