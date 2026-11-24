// SPDX-License-Identifier: GPL-3.0-or-later
//! Native Rust counterpart of primitive_probe.cpp for the same inputs: f64 sin,
//! cos and sin_cos (glibc libm on x86_64-unknown-linux-gnu), the same fixed-seed
//! sweep and the same primitive mixed-NaN arithmetic. Observations only; any
//! difference from the C++ probe is reported by run_probe.py, never accepted.
//! This is a native host probe, not portable port code.
//!
//! Usage: rust_probe --sweep-count N --sweep-out FILE

use std::fmt::Write as _;
use std::hint::black_box;
use std::io::Write as _;

/// Same table as LIBM_INPUTS in probe_common.h (checked by the tooling tests).
const LIBM_INPUTS: [u64; 37] = [
    0x0000000000000000,
    0x8000000000000000,
    0x0000000000000001,
    0x800000000000beef,
    0x000fffffffffffff,
    0x0010000000000000,
    0x01a56e1fc2f8f359,
    0x3e40000000000000,
    0x3e45798ee2308c3a,
    0x3fe0000000000000,
    0x3fe921fb54442d18,
    0x3ff0000000000000,
    0x3ff921fb54442d18,
    0x4000000000000000,
    0x400921fb54442d18,
    0x4012d97c7f3321d2,
    0x401921fb54442d18,
    0x4024000000000000,
    0x4059000000000000,
    0x40f86a0000000000,
    0x419921fb38000000,
    0x41cdcd6500000000,
    0x4330000000000000,
    0x4480f0cf064dd592,
    0x44b52d02c7e14af6,
    0x7e37e43c8800759c,
    0x7fefffffffffffff,
    0xbff0000000000000,
    0xc00921fb54442d18,
    0x7ff0000000000000,
    0xfff0000000000000,
    0x7ff8000000000000,
    0xfff8000000000000,
    0x7ff8000000000123,
    0x7ff0000000000001,
    0x7ff4000000000abc,
    0xfff0000000000042,
];

/// Same table as NAN_OPERANDS in probe_common.h.
const NAN_OPERANDS: [u64; 7] = [
    0x7ff8000000000001,
    0x7ff8000000000002,
    0xfff8000000000003,
    0x7ff0000000000004,
    0x7ff4000000000005,
    0x3ff8000000000000,
    0x7ff0000000000000,
];

const SWEEP_SEED: u64 = 0x4b69436164526566;
const TWO_PI: f64 = 6.283185307179586;
const MAX_SWEEP: u64 = 2_000_000;

struct SplitMix(u64);

impl SplitMix {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }
}

/// Same arithmetic, in the same order, as probe::SweepAngle.
fn sweep_angle(rng: &mut SplitMix, index: u64) -> f64 {
    let x = rng.next();
    match index % 4 {
        0 => {
            let u = (x >> 11) as f64 * (1.0 / 9007199254740992.0);
            (u * 2.0 - 1.0) * TWO_PI
        }
        1 => {
            let u = (x >> 11) as f64 * (1.0 / 9007199254740992.0);
            (u * 2.0 - 1.0) * 10000.0
        }
        2 => (x % 3600000) as f64 / 10000.0 * (3.141592653589793 / 180.0),
        _ => {
            let exponent = 1023 - 30 + ((x >> 52) % 61);
            let sign = (x >> 63) << 63;
            f64::from_bits(sign | (exponent << 52) | (x & 0x000fffffffffffff))
        }
    }
}

#[inline(never)]
fn add(a: f64, b: f64) -> f64 {
    a + b
}
#[inline(never)]
fn sub(a: f64, b: f64) -> f64 {
    a - b
}
#[inline(never)]
fn mul(a: f64, b: f64) -> f64 {
    a * b
}
#[inline(never)]
fn div(a: f64, b: f64) -> f64 {
    a / b
}

fn hex(value: f64) -> String {
    format!("{:016x}", value.to_bits())
}

fn usage() -> std::process::ExitCode {
    eprintln!("usage: rust_probe --sweep-count 1..2000000 --sweep-out FILE");
    std::process::ExitCode::from(2)
}

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut count = 0u64;
    let mut path = None;
    let mut pairs = args.chunks_exact(2);
    for pair in &mut pairs {
        match pair[0].as_str() {
            "--sweep-count" => count = pair[1].parse().unwrap_or(0),
            "--sweep-out" => path = Some(pair[1].clone()),
            _ => return usage(),
        }
    }
    let Some(path) = path else { return usage() };
    if !pairs.remainder().is_empty() || count == 0 || count > MAX_SWEEP {
        return usage();
    }

    let mut out = String::from("{\"schema\":1,\"probe\":\"rust\",\"libm_values\":[");
    for (i, bits) in LIBM_INPUTS.iter().enumerate() {
        let x = black_box(f64::from_bits(*bits));
        let (s, c) = x.sin_cos();
        let _ = write!(
            out,
            "{}{{\"x\":\"{:016x}\",\"sin\":\"{}\",\"cos\":\"{}\",\"sin_cos_sin\":\"{}\",\"sin_cos_cos\":\"{}\"}}",
            if i == 0 { "" } else { "," },
            bits,
            hex(x.sin()),
            hex(x.cos()),
            hex(s),
            hex(c)
        );
    }
    out.push_str("],\"nan_arithmetic\":[");
    let mut first = true;
    for a in NAN_OPERANDS {
        for b in NAN_OPERANDS {
            let x = black_box(f64::from_bits(a));
            let y = black_box(f64::from_bits(b));
            let _ = write!(
                out,
                "{}{{\"a\":\"{:016x}\",\"b\":\"{:016x}\",\"add\":\"{}\",\"sub\":\"{}\",\"mul\":\"{}\",\"div\":\"{}\"}}",
                if first { "" } else { "," },
                a,
                b,
                hex(add(x, y)),
                hex(sub(x, y)),
                hex(mul(x, y)),
                hex(div(x, y))
            );
            first = false;
        }
    }

    let mut bytes = Vec::with_capacity((count as usize).saturating_mul(24));
    let mut rng = SplitMix(SWEEP_SEED);
    for index in 0..count {
        let x = sweep_angle(&mut rng, index);
        for value in [x.to_bits(), x.sin().to_bits(), x.cos().to_bits()] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    if let Err(error) = std::fs::write(&path, &bytes) {
        eprintln!("{path}: {error}");
        return std::process::ExitCode::from(1);
    }
    let _ = write!(
        out,
        "],\"sweep\":{{\"seed\":\"{SWEEP_SEED:016x}\",\"count\":{count},\"record\":\"x,sin,cos as little-endian u64\"}}}}\n"
    );
    match std::io::stdout().write_all(out.as_bytes()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(_) => std::process::ExitCode::from(1),
    }
}
