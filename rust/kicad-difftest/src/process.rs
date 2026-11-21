// SPDX-License-Identifier: GPL-3.0-or-later
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct Program {
    pub executable: PathBuf,
    pub args: Vec<String>,
}

fn io(error: std::io::Error) -> String {
    error.to_string()
}

fn read(mut file: File) -> Result<String, String> {
    file.seek(SeekFrom::Start(0)).map_err(io)?;
    let mut text = String::new();
    file.read_to_string(&mut text).map_err(io)?;
    Ok(text)
}

/// File-backed stdio avoids pipe deadlocks for large batches, without trusting
/// the child to read stdin before writing stdout or stderr.
pub fn run(program: &Program, input: &[u8], timeout: Duration) -> Result<String, String> {
    let mut stdin = tempfile::tempfile().map_err(io)?;
    stdin.write_all(input).map_err(io)?;
    stdin.seek(SeekFrom::Start(0)).map_err(io)?;
    let stdout = tempfile::tempfile().map_err(io)?;
    let stderr = tempfile::tempfile().map_err(io)?;
    let mut child = Command::new(&program.executable)
        .args(&program.args)
        .stdin(Stdio::from(stdin))
        .stdout(Stdio::from(stdout.try_clone().map_err(io)?))
        .stderr(Stdio::from(stderr.try_clone().map_err(io)?))
        .spawn()
        .map_err(|e| format!("cannot start {}: {e}", program.executable.display()))?;
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() < timeout => {
                std::thread::sleep(Duration::from_millis(10));
            }
            result => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "{}: timeout or wait failure: {result:?}",
                    program.executable.display()
                ));
            }
        }
    };
    if !status.success() {
        return Err(format!(
            "{}: process failed ({status}): {}",
            program.executable.display(),
            read(stderr)?
        ));
    }
    read(stdout)
}
