// SPDX-License-Identifier: GPL-3.0-or-later
//! JSONL adapter for `libs.kimath.src.geometry.poisson_disk.application.pure.p001`. It mirrors
//! `port/oracle/shims/kimath_src_poisson_disk.cpp` field for field, including validation order
//! and refusal messages: `tile` runs `POISSON_DISK::ToroidalUnitTile` within the caller's storage
//! limits, `rng` makes direct draws from the Boost.Random port it uses. IEEE values travel as
//! exact bit strings. See README.md for the operation schema.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

use kicad_boost_random::{Mt19937, UniformIntDistribution, UniformRealDistribution};
use kicad_math::poisson_disk::{toroidal_unit_tile, TileError, TileLimits, UNIT};
use kicad_math::Vector2D;
use serde_json::{json, Value};

/// Harness bound on the storage limits a request may give (the adapter's memory and reply size).
pub const MAX_LIMIT: i64 = 4_194_304;
/// Harness bound on direct draws per request, and on draw specifications.
pub const MAX_DRAWS: i64 = 65_536;
const MAX_SPECS: usize = 64;
/// Samples reported at each end of a digested tile.
const EDGE_SAMPLES: usize = 8;

type Result<T> = std::result::Result<T, String>;

fn keys(v: &Value, fields: &[&str]) -> Result<()> {
    let o = v.as_object().ok_or("expected object")?;
    if o.len() != fields.len() || fields.iter().any(|f| !o.contains_key(*f)) {
        return Err("unexpected or missing field".into());
    }
    Ok(())
}

fn text<'a>(v: &'a Value, name: &str) -> Result<&'a str> {
    v.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("expected string: {name}"))
}

/// `oracle::Integer`: a JSON integer (integer `-0` included) in `[min, max]`.
fn integer(v: &Value, name: &str, min: i64, max: i64) -> Result<i64> {
    v.get(name)
        .and_then(Value::as_i64)
        .filter(|n| *n >= min && *n <= max)
        .ok_or_else(|| format!("integer out of range: {name}"))
}

/// Exactly 16 lower-case hex digits: IEEE binary64 bits.
fn read_bits(v: &Value, name: &str) -> Result<f64> {
    let t = text(v, name)?;
    if t.len() != 16
        || !t
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(format!("expected bits: {name}"));
    }
    u64::from_str_radix(t, 16)
        .map(f64::from_bits)
        .map_err(|_| format!("expected bits: {name}"))
}

/// A canonical unsigned decimal string within `u64`.
fn read_decimal(v: &Value, name: &str) -> Result<u64> {
    let t = text(v, name)?;
    if t.is_empty() || (t.len() > 1 && t.starts_with('0')) || !t.bytes().all(|c| c.is_ascii_digit())
    {
        return Err(format!("expected decimal: {name}"));
    }
    t.parse::<u64>()
        .map_err(|_| format!("expected decimal: {name}"))
}

/// A non-negative JSON integer (not `-0`) within `u32`.
fn read_seed(v: &Value) -> Result<u32> {
    v.get("seed")
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| "seed out of range".to_string())
}

fn bits(v: f64) -> Value {
    json!(format!("{:016x}", v.to_bits()))
}

fn point(p: &Vector2D) -> Value {
    json!([bits(p.x), bits(p.y)])
}

/// FNV-1a 64 over each sample's x then y bits, little-endian bytes, in result order.
pub fn digest(samples: &[Vector2D]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for p in samples {
        for coordinate in [p.x, p.y] {
            for byte in coordinate.to_bits().to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
    }
    format!("{hash:016x}")
}

fn tile(input: &Value) -> Result<Value> {
    keys(
        input,
        &[
            "op",
            "min_dist",
            "seed",
            "max_grid_cells",
            "max_samples",
            "emit",
        ],
    )?;
    let min_dist = read_bits(input, "min_dist")?;
    let seed = read_seed(input)?;
    let max_grid_cells = integer(input, "max_grid_cells", 0, MAX_LIMIT)? as u64;
    let max_samples = integer(input, "max_samples", 0, MAX_LIMIT)? as u64;
    let emit = text(input, "emit")?;
    if emit != "samples" && emit != "digest" {
        return Err("unknown emit".into());
    }
    let limits = TileLimits {
        max_grid_cells,
        max_samples,
    };
    let samples = toroidal_unit_tile(min_dist, seed, limits).map_err(|e| match e {
        TileError::UndefinedGridSize { .. } => "undefined grid size".to_string(),
        TileError::GridLimit { .. } => "grid limit".to_string(),
        TileError::SampleLimit { .. } => "sample limit".to_string(),
        // Unreachable through this adapter: the limits are bounded above and the host
        // allocates at most MAX_LIMIT elements per vector.
        TileError::InvalidLimits(_) => "invalid limits".to_string(),
        TileError::Allocation { .. } => "allocation failed".to_string(),
        TileError::BrokenInvariant => "broken invariant".to_string(),
    })?;
    let mut out = serde_json::Map::new();
    out.insert("count".into(), json!(samples.len()));
    if emit == "samples" {
        out.insert(
            "samples".into(),
            Value::Array(samples.iter().map(point).collect()),
        );
    } else {
        let edge = EDGE_SAMPLES.min(samples.len());
        let first: Vec<Value> = samples.iter().take(edge).map(point).collect();
        let last: Vec<Value> = samples
            .iter()
            .skip(samples.len() - edge)
            .map(point)
            .collect();
        out.insert("digest".into(), json!(digest(&samples)));
        out.insert("first".into(), Value::Array(first));
        out.insert("last".into(), Value::Array(last));
    }
    Ok(Value::Object(out))
}

enum Draw {
    U32,
    Int(UniformIntDistribution),
    Real(UniformRealDistribution),
}

fn rng(input: &Value) -> Result<Value> {
    keys(input, &["op", "seed", "draws"])?;
    let mut engine = Mt19937::new(read_seed(input)?);
    let specs = input
        .get("draws")
        .and_then(Value::as_array)
        .filter(|s| !s.is_empty() && s.len() <= MAX_SPECS)
        .ok_or("draws out of range")?;
    let mut total: i64 = 0;
    let mut draws: Vec<(Draw, i64)> = Vec::new();
    for spec in specs {
        if !spec.is_object() || spec.get("kind").is_none() || spec.get("count").is_none() {
            return Err("expected draw".into());
        }
        let kind = text(spec, "kind")?;
        match kind {
            "u32" => keys(spec, &["kind", "count"])?,
            "int" | "real" => keys(spec, &["kind", "count", "min", "max"])?,
            _ => return Err("unknown draw".into()),
        }
        let count = integer(spec, "count", 1, MAX_DRAWS)?;
        total += count;
        let draw = match kind {
            "int" => {
                let lo = read_decimal(spec, "min")?;
                let hi = read_decimal(spec, "max")?;
                Draw::Int(UniformIntDistribution::new(lo, hi).ok_or("invalid int range")?)
            }
            "real" => {
                let lo = read_bits(spec, "min")?;
                let hi = read_bits(spec, "max")?;
                Draw::Real(UniformRealDistribution::new(lo, hi).map_err(|_| "invalid real range")?)
            }
            _ => Draw::U32,
        };
        draws.push((draw, count));
    }
    if total > MAX_DRAWS {
        return Err("draws out of range".into());
    }
    let mut out = Vec::with_capacity(draws.len());
    for (draw, count) in &draws {
        let values: Vec<Value> = (0..*count)
            .map(|_| match draw {
                Draw::U32 => json!(engine.next_u32()),
                Draw::Int(d) => json!(d.sample(&mut engine).to_string()),
                Draw::Real(d) => bits(d.sample(&mut engine)),
            })
            .collect();
        out.push(Value::Array(values));
    }
    Ok(Value::Array(out))
}

fn run(input: &Value) -> Result<Value> {
    match text(input, "op")? {
        "tile" => tile(input),
        "rng" => rng(input),
        _ => Err("unknown operation".into()),
    }
}

/// Answer one protocol line.
pub fn dispatch(line: &str) -> Value {
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
    if keys(&request, &["unit", "case", "input"]).is_err()
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
    match run(&request["input"]) {
        Ok(result) => json!({"case":case,"result":result,"unit":unit}),
        Err(message) => failure(unit, case, "invalid_input", &message),
    }
}
