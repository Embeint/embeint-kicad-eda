// SPDX-License-Identifier: GPL-3.0-or-later
//! Native runner for `embeint-layout`: one JSON request per input line, one JSON response per
//! output line.  The process I/O lives here so the layout crate stays portable.
//!
//!   embeint-layout < requests.jsonl              responses, in order
//!   embeint-layout --bench N < requests.jsonl    per request, the best of 5 rounds of N calls,
//!                                                as {"ns_per_call": ...} lines

use std::io::{self, BufRead, BufWriter, Write};
use std::time::Instant;

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let iterations = match args.as_slice() {
        [] => None,
        [flag, count] if flag == "--bench" => match count.parse::<u32>() {
            Ok(count) if count > 0 => Some(count),
            _ => return usage(),
        },
        _ => return usage(),
    };

    let stdin = io::stdin();
    let mut out = BufWriter::new(io::stdout().lock());

    for line in stdin.lock().lines() {
        let line = line?;

        match iterations {
            None => writeln!(out, "{}", embeint_layout::layout_box_json(&line))?,
            Some(count) => {
                let best = (0..5)
                    .map(|_| {
                        let start = Instant::now();

                        for _ in 0..count {
                            std::hint::black_box(embeint_layout::layout_box_json(
                                std::hint::black_box(&line),
                            ));
                        }

                        start.elapsed().as_nanos() as f64 / f64::from(count)
                    })
                    .fold(f64::INFINITY, f64::min);
                writeln!(out, "{{\"ns_per_call\": {best:.1}}}")?;
            }
        }
    }

    out.flush()
}

fn usage() -> io::Result<()> {
    eprintln!("usage: embeint-layout [--bench N] < requests.jsonl");
    std::process::exit(2);
}
