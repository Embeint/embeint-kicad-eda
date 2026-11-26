// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
use kicad_arc_construction_shim::dispatch;
use std::io::{self, BufRead, Write};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut stdout = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        writeln!(stdout, "{}", dispatch(&line?))?;
        stdout.flush()?;
    }
    Ok(())
}
