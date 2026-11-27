// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
use kicad_font_shim::{dispatch_with, Options};
use kicad_wx_stl_compat::ostream::SentryFlush;
use std::io::{self, BufRead, Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut options = Options::default();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            // Local diagnostics against a GCC 15+ runtime; the reference is the default.
            "--sentry-flush=when-good" => options.sentry_flush = SentryFlush::WhenGood,
            "--sentry-flush=unconditional" => options.sentry_flush = SentryFlush::Unconditional,
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }
    let mut stdout = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        writeln!(stdout, "{}", dispatch_with(&line?, options))?;
        stdout.flush()?;
    }
    Ok(())
}
