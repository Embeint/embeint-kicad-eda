// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
//! Reads JSONL requests on stdin and writes one reply per line. With no argument the
//! libstdc++ sentry is the reference runtime's; `--sentry-flush=when-good` selects the GCC 15
//! and later destructor, for comparison against an oracle on such a runtime.
use kicad_wx_stl_compat::ostream::SentryFlush;
use kicad_wx_stl_compat_shim::dispatch_with;
use std::io::{self, BufRead, Write};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flush = match args.as_slice() {
        [] => SentryFlush::Unconditional,
        [arg] if arg == "--sentry-flush=unconditional" => SentryFlush::Unconditional,
        [arg] if arg == "--sentry-flush=when-good" => SentryFlush::WhenGood,
        _ => {
            eprintln!("usage: kicad-wx-stl-compat-shim [--sentry-flush=unconditional|when-good]");
            std::process::exit(2);
        }
    };
    let mut stdout = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        writeln!(stdout, "{}", dispatch_with(&line?, flush))?;
        stdout.flush()?;
    }
    Ok(())
}
