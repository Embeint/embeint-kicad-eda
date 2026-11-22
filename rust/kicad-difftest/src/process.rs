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

/// Diagnostics copied into fault messages are bounded; protocol stdout is not
/// truncated, because every reply is compared exactly.
pub const STDERR_LIMIT: usize = 64 * 1024;

/// Everything one program run produced, kept independently of how it ended.
#[derive(Debug, Clone, Default)]
pub struct Captured {
    /// False when the program never ran (setup or start failure); no output exists.
    pub started: bool,
    /// Every stdout byte the program wrote, including output flushed before a
    /// nonzero exit or a timeout kill.
    pub stdout: Vec<u8>,
    /// At most `STDERR_LIMIT` bytes of stderr, lossily decoded for diagnostics,
    /// followed by a note of how many bytes were not read.
    pub stderr: String,
    /// The total stderr size from file metadata, when available.
    pub stderr_total: Option<u64>,
    /// Why the run cannot be trusted: start failure, timeout, wait failure,
    /// nonzero exit or non-UTF-8 stdout. Any fault fails the run.
    pub fault: Option<String>,
}

impl Captured {
    /// The longest valid UTF-8 prefix of stdout. Invalid bytes are already a fault.
    pub fn text(&self) -> &str {
        match std::str::from_utf8(&self.stdout) {
            Ok(text) => text,
            Err(error) => self
                .stdout
                .get(..error.valid_up_to())
                .and_then(|bytes| std::str::from_utf8(bytes).ok())
                .unwrap_or(""),
        }
    }
}

fn io(error: std::io::Error) -> String {
    error.to_string()
}

fn read_bytes(file: &mut File) -> Result<Vec<u8>, String> {
    file.seek(SeekFrom::Start(0)).map_err(io)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(io)?;
    Ok(bytes)
}

/// Bytes read from a reader under a hard limit, kept even if reading failed.
#[derive(Debug, Default)]
pub struct Bounded {
    pub bytes: Vec<u8>,
    pub error: Option<String>,
}

/// Reads at most `limit` bytes. Only `limit` bytes are ever requested from
/// `reader` (`Read::take`), so a huge stream is never loaded; bytes read before
/// an error are kept.
pub fn read_bounded<R: Read>(reader: R, limit: usize) -> Bounded {
    let mut bounded = Bounded::default();
    let limit = u64::try_from(limit).unwrap_or(u64::MAX);
    if let Err(error) = reader.take(limit).read_to_end(&mut bounded.bytes) {
        bounded.error = Some(error.to_string());
    }
    bounded
}

/// Reads at most `limit` bytes of a captured diagnostic file, taking its total
/// size from metadata rather than from reading. Returns the lossy text with a
/// note of omitted bytes (or of a read error), and the total size when known.
pub fn read_diagnostic(file: &mut File, limit: usize) -> (String, Option<u64>) {
    let total = file.metadata().ok().map(|m| m.len());
    let bounded = match file.seek(SeekFrom::Start(0)) {
        Ok(_) => read_bounded(&mut *file, limit),
        Err(error) => Bounded {
            bytes: Vec::new(),
            error: Some(io(error)),
        },
    };
    let mut text = String::from_utf8_lossy(&bounded.bytes).into_owned();
    let read = u64::try_from(bounded.bytes.len()).unwrap_or(u64::MAX);
    match total {
        Some(total) if total > read => {
            text.push_str(&format!(" [{} more bytes omitted]", total - read));
        }
        None if bounded.bytes.len() >= limit => text.push_str(" [total size unknown]"),
        _ => {}
    }
    if let Some(error) = bounded.error {
        text.push_str(&format!(" [read error: {error}]"));
    }
    (text, total)
}

/// Runs `program` with `input` on stdin and captures its output.
///
/// File-backed stdio avoids pipe deadlocks for large batches, without trusting
/// the child to read stdin before writing stdout or stderr. A timeout still kills
/// and reaps the child; whatever it wrote before that remains in `stdout`.
pub fn capture(program: &Program, input: &[u8], timeout: Duration) -> Captured {
    let setup = || -> Result<(File, File, File), String> {
        let mut stdin = tempfile::tempfile().map_err(io)?;
        stdin.write_all(input).map_err(io)?;
        stdin.seek(SeekFrom::Start(0)).map_err(io)?;
        Ok((
            stdin,
            tempfile::tempfile().map_err(io)?,
            tempfile::tempfile().map_err(io)?,
        ))
    };
    let not_started = |fault: String| Captured {
        fault: Some(fault),
        ..Captured::default()
    };
    let (stdin, mut stdout, mut stderr) = match setup() {
        Ok(files) => files,
        Err(error) => return not_started(error),
    };
    let child = (|| {
        Command::new(&program.executable)
            .args(&program.args)
            .stdin(Stdio::from(stdin))
            .stdout(Stdio::from(stdout.try_clone()?))
            .stderr(Stdio::from(stderr.try_clone()?))
            .spawn()
    })();
    let mut child = match child {
        Ok(child) => child,
        Err(e) => {
            return not_started(format!(
                "cannot start {}: {e}",
                program.executable.display()
            ))
        }
    };
    let start = Instant::now();
    let mut fault = loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break None,
            Ok(Some(status)) => {
                break Some(format!(
                    "{}: process failed ({status})",
                    program.executable.display()
                ))
            }
            Ok(None) if start.elapsed() < timeout => {
                std::thread::sleep(Duration::from_millis(10));
            }
            result => {
                let _ = child.kill();
                let _ = child.wait();
                break Some(format!(
                    "{}: timeout or wait failure: {result:?}",
                    program.executable.display()
                ));
            }
        }
    };
    let (stderr_text, stderr_total) = read_diagnostic(&mut stderr, STDERR_LIMIT);
    let stdout_bytes = match read_bytes(&mut stdout) {
        Ok(bytes) => bytes,
        Err(error) => {
            fault.get_or_insert(format!("stdout unreadable: {error}"));
            Vec::new()
        }
    };
    if let Some(message) = &mut fault {
        message.push_str(": ");
        message.push_str(&stderr_text);
    } else if let Err(error) = std::str::from_utf8(&stdout_bytes) {
        fault = Some(format!(
            "{}: stdout is not valid UTF-8 after byte {}",
            program.executable.display(),
            error.valid_up_to()
        ));
    }
    Captured {
        started: true,
        stdout: stdout_bytes,
        stderr: stderr_text,
        stderr_total,
        fault,
    }
}

/// Runs `program` and returns its stdout only if it started, exited zero within
/// `timeout` and wrote valid UTF-8; otherwise the fault, with bounded stderr.
pub fn run(program: &Program, input: &[u8], timeout: Duration) -> Result<String, String> {
    let captured = capture(program, input, timeout);
    match captured.fault {
        Some(fault) => Err(fault),
        None => Ok(captured.text().to_owned()),
    }
}
