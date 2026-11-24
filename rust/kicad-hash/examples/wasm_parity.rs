// SPDX-License-Identifier: GPL-3.0-or-later
//! Run every corpus case of `libs.kimath.include.application.pure.p001` through the shim's pure
//! operations, with no host I/O, and fold the complete replies into one FNV-1a digest.
//!
//! Native: `cargo run -p kicad-hash --example wasm_parity` prints the case count and digest.
//! wasm: build with `KICAD_HASH_EXPECTED_DIGEST=<native digest>` for `wasm32-unknown-unknown`
//! and run `node rust/kicad-hash/wasm-test.mjs PATH`; `main` returns non-zero unless the wasm
//! results reproduce the native digest exactly (every value, NaN bits included).
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../../kicad-hash-shim/src/operations.rs"]
#[allow(dead_code)]
mod operations;

use serde_json::Value;

const CASES: &str =
    include_str!("../../../port/cases/libs.kimath.include.application.pure.p001.jsonl");

/// 64-bit FNV-1a.
fn fnv1a(hash: u64, bytes: &[u8]) -> u64 {
    bytes.iter().fold(hash, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn main() -> Result<(), String> {
    let mut digest = 0xcbf2_9ce4_8422_2325_u64;
    let mut count = 0_u64;
    for line in CASES.lines() {
        let request: Value = serde_json::from_str(line).map_err(|error| error.to_string())?;
        if request["unit"] != operations::UNIT {
            return Err("corpus unit mismatch".into());
        }
        let reply = match operations::run(&request["input"]) {
            Ok(result) => serde_json::json!({"case": request["case"], "result": result}),
            Err(message) => serde_json::json!({"case": request["case"], "error": message}),
        };
        digest = fnv1a(digest, reply.to_string().as_bytes());
        digest = fnv1a(digest, b"\n");
        count += 1;
    }
    let text = format!("{count} cases, digest {digest:016x}");
    match option_env!("KICAD_HASH_EXPECTED_DIGEST") {
        Some(expected) if expected == format!("{digest:016x}") => Ok(()),
        Some(expected) => Err(format!("wasm/native mismatch: {text}, expected {expected}")),
        None => {
            println!("{text}");
            Ok(())
        }
    }
}
