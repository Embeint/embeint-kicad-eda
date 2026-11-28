// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::process::Command;

/// Run this program as the `TerminateWithCode` child and return the exit status it ended with.
fn terminate_in_child(exe: &PathBuf, code: i32) -> Result<i32, String> {
    let status = Command::new(exe)
        .arg("--terminate-child")
        .arg(code.to_string())
        .status()
        .map_err(|e| e.to_string())?;
    status.code().ok_or_else(|| "child did not exit".to_owned())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--terminate-child") {
        let code: i32 = args.get(2).ok_or("missing code")?.parse()?;
        kicad_kiplatform_unix_shim::terminate_child(code);
    }
    let exe = std::env::current_exe()?;
    let terminate = |code: i32| terminate_in_child(&exe, code);
    let mut stdout = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        writeln!(
            stdout,
            "{}",
            kicad_kiplatform_unix_shim::dispatch_with(&line?, Some(&terminate))
        )?;
        stdout.flush()?;
    }
    Ok(())
}
