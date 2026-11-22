// SPDX-License-Identifier: GPL-3.0-or-later
//! Per-case evaluation of one oracle and one Rust run.
//!
//! Every corpus case receives exactly one outcome, in corpus order, so a failed run
//! still reports what was compared, what Rust deferred (with the retained C++ reply)
//! and what failed or could not be evaluated, instead of stopping at the first fault.
//! Comparison stays exact; this module only refuses to discard the remaining cases.

use crate::process::Captured;
use crate::{parse_envelope, records, Deferral, Envelope, Record, Reply};

/// At most this many reply lines beyond the corpus are kept per program, each
/// truncated to `EXTRA_LINE_LIMIT` bytes; the fault message gives the full count.
pub const EXTRA_LINES_KEPT: usize = 16;
pub const EXTRA_LINE_LIMIT: usize = 4096;

/// What happened to one corpus case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Rust returned a result exactly equal to the oracle's result.
    Compared,
    /// Rust returned a well-formed deferral; the oracle returned a result.
    Deferred(Deferral),
    /// A definite fault for this case: a mismatch, an error reply on either side,
    /// a malformed or misplaced reply, or a missing reply.
    Failed(String),
    /// A run-level fault (process failure or timeout) prevented evaluation.
    Unprocessed(String),
}

impl Outcome {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Compared => "compared",
            Self::Deferred(_) => "deferred",
            Self::Failed(_) => "failed",
            Self::Unprocessed(_) => "unprocessed",
        }
    }
}

/// A raw stdout line kept as evidence: text when it is valid UTF-8, otherwise its
/// exact bytes. Bytes are never decoded lossily into something that could parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawLine {
    Text(String),
    Bytes(Vec<u8>),
}

impl RawLine {
    fn new(bytes: &[u8]) -> Self {
        match std::str::from_utf8(bytes) {
            Ok(text) => Self::Text(text.to_owned()),
            Err(_) => Self::Bytes(bytes.to_vec()),
        }
    }
}

/// Splits raw stdout into lines at LF bytes, exactly as `str::lines` would for
/// valid UTF-8: `\n` or `\r\n` ends a line, a final line without a terminator is
/// kept (a bare final `\r` stays part of it), and a trailing terminator adds no
/// empty line. Splitting on bytes first means one invalid line never hides or
/// shifts the lines after it.
pub fn split_lines(bytes: &[u8]) -> Vec<&[u8]> {
    let mut lines = Vec::new();
    let mut rest = bytes;
    while !rest.is_empty() {
        match rest.iter().position(|&b| b == b'\n') {
            Some(end) => {
                let (line, tail) = rest.split_at(end);
                lines.push(line.strip_suffix(b"\r").unwrap_or(line));
                rest = tail.get(1..).unwrap_or_default();
            }
            None => {
                lines.push(rest);
                break;
            }
        }
    }
    lines
}

/// One case's outcome with whatever evidence each side produced for it.
#[derive(Debug, Clone)]
pub struct CaseOutcome {
    pub case: String,
    pub outcome: Outcome,
    /// The oracle's successful reply for this case, when it produced one.
    pub oracle: Option<Record>,
    /// The oracle's raw reply line when it was not a usable result.
    pub oracle_raw: Option<RawLine>,
    /// The Rust raw reply line for this case, when one was present at its position.
    pub rust: Option<RawLine>,
}

/// A reply line beyond the corpus, kept as failure evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtraLine {
    pub side: &'static str,
    /// One-based stdout line number.
    pub line: usize,
    /// At most `EXTRA_LINE_LIMIT` bytes of the line (whole characters for text).
    pub raw: RawLine,
    pub truncated: bool,
}

#[derive(Debug, Clone)]
pub struct Evaluation {
    pub outcomes: Vec<CaseOutcome>,
    /// Faults not attributable to one case: process faults (start, exit, timeout,
    /// non-UTF-8 output) and reply lines beyond the corpus. Any fault fails the run.
    pub faults: Vec<String>,
    /// The first `EXTRA_LINES_KEPT` extra lines of each program.
    pub extra: Vec<ExtraLine>,
}

impl Evaluation {
    pub fn count(&self, name: &str) -> usize {
        self.outcomes
            .iter()
            .filter(|c| c.outcome.name() == name)
            .count()
    }
}

enum Slot {
    Reply(Reply),
    Fault {
        reason: String,
        raw: Option<RawLine>,
    },
    Unavailable(String),
}

/// Aligns one program's output with the corpus by position, as the protocol
/// requires one reply per request in request order. A reply at the wrong position
/// is a fault for the case expected there; extra lines are run-level faults.
///
/// Output written before a nonzero exit or timeout is still parsed, so every
/// recoverable reply keeps its evidence; the process fault itself always fails
/// the run. Cases the process never answered are unprocessed, not failed.
fn slots(
    output: &Captured,
    unit: &str,
    cases: &[Record],
    label: &'static str,
    faults: &mut Vec<String>,
    extra: &mut Vec<ExtraLine>,
) -> Vec<Slot> {
    if let Some(fault) = &output.fault {
        faults.push(format!("{label}: {fault}"));
    }
    if !output.started {
        let reason = format!(
            "{label}: {}",
            output.fault.as_deref().unwrap_or("did not run")
        );
        return cases
            .iter()
            .map(|_| Slot::Unavailable(reason.clone()))
            .collect();
    }
    let mut lines = split_lines(&output.stdout).into_iter();
    let mut result = Vec::with_capacity(cases.len());
    for (index, case) in cases.iter().enumerate() {
        let Some(line) = lines.next() else {
            result.push(match &output.fault {
                Some(fault) => Slot::Unavailable(format!(
                    "{label}: no reply for case {} before the process fault: {fault}",
                    case.case
                )),
                None => Slot::Fault {
                    reason: format!(
                        "{label}: no reply for case {} (line {})",
                        case.case,
                        index + 1
                    ),
                    raw: None,
                },
            });
            continue;
        };
        let Ok(line) = std::str::from_utf8(line) else {
            // A malformed line fails its own case; the following lines keep their
            // positions and are evaluated normally.
            let offset = std::str::from_utf8(line)
                .err()
                .map_or(0, |e| e.valid_up_to());
            result.push(Slot::Fault {
                reason: format!(
                    "{label}: line {} is not valid UTF-8 (invalid byte at offset {offset})",
                    index + 1
                ),
                raw: Some(RawLine::Bytes(line.to_vec())),
            });
            continue;
        };
        let raw = Some(RawLine::Text(line.to_owned()));
        result.push(match parse_envelope(line, unit, Envelope::Partial, index) {
            Err(reason) => Slot::Fault {
                reason: format!("{label}: {reason}"),
                raw,
            },
            Ok(reply) if reply.record.case != case.case => Slot::Fault {
                reason: format!(
                    "{label}: case order/identity at {index}: {} != {}",
                    reply.record.case, case.case
                ),
                raw,
            },
            Ok(reply) => Slot::Reply(reply),
        });
    }
    let mut count = 0;
    for (offset, line) in lines.enumerate() {
        count += 1;
        if offset < EXTRA_LINES_KEPT {
            let kept = match std::str::from_utf8(line) {
                Ok(text) => {
                    let mut end = text.len().min(EXTRA_LINE_LIMIT);
                    while !text.is_char_boundary(end) {
                        end -= 1;
                    }
                    RawLine::Text(text.get(..end).unwrap_or_default().to_owned())
                }
                Err(_) => RawLine::new(line.get(..EXTRA_LINE_LIMIT).unwrap_or(line)),
            };
            let kept_len = match &kept {
                RawLine::Text(text) => text.len(),
                RawLine::Bytes(bytes) => bytes.len(),
            };
            extra.push(ExtraLine {
                side: label,
                line: cases.len() + offset + 1,
                raw: kept,
                truncated: kept_len < line.len(),
            });
        }
    }
    if count > 0 {
        faults.push(format!(
            "{label}: {count} reply lines beyond the {} cases (first {} kept in extra_replies)",
            cases.len(),
            count.min(EXTRA_LINES_KEPT)
        ));
    }
    result
}

/// Evaluates every case. The oracle must return a result for a case: any oracle
/// error, including `unported`, fails that case, so a Rust deferral is only ever
/// recorded against a real reference result. Deferral acceptance is the caller's
/// policy; this function never turns an error into a deferral.
pub fn evaluate(cases: &[Record], unit: &str, oracle: &Captured, rust: &Captured) -> Evaluation {
    let mut faults = Vec::new();
    let mut extra = Vec::new();
    let oracle_slots = slots(oracle, unit, cases, "oracle", &mut faults, &mut extra);
    let rust_slots = slots(rust, unit, cases, "Rust", &mut faults, &mut extra);
    let mut outcomes = Vec::with_capacity(cases.len());
    for ((case, o), r) in cases.iter().zip(oracle_slots).zip(rust_slots) {
        let rust_raw = match &r {
            Slot::Reply(reply) => Some(RawLine::Text(reply.record.raw.clone())),
            Slot::Fault { raw, .. } => raw.clone(),
            Slot::Unavailable(_) => None,
        };
        // Partial envelope parsing already turned every non-deferral error reply into
        // a fault; a well-formed oracle deferral is a fault here as well.
        let (oracle_record, oracle_raw, oracle_problem) = match o {
            Slot::Reply(reply) if reply.deferral.is_some() => (
                None,
                Some(RawLine::Text(reply.record.raw)),
                Some(Outcome::Failed(
                    "oracle: a reference deferral is a failure; the oracle must return a result"
                        .into(),
                )),
            ),
            Slot::Reply(reply) => (Some(reply.record), None, None),
            Slot::Fault { reason, raw } => (None, raw, Some(Outcome::Failed(reason))),
            Slot::Unavailable(reason) => (None, None, Some(Outcome::Unprocessed(reason))),
        };
        let outcome = match (oracle_problem, &r, &oracle_record) {
            (Some(failed @ Outcome::Failed(_)), _, _) => failed,
            (_, Slot::Fault { reason, .. }, _) => Outcome::Failed(reason.clone()),
            (_, Slot::Unavailable(reason), _) => Outcome::Unprocessed(reason.clone()),
            (Some(unprocessed), Slot::Reply(_), _) => unprocessed,
            (None, Slot::Reply(reply), Some(expected)) => match &reply.deferral {
                Some(deferral) => Outcome::Deferred(deferral.clone()),
                None if expected.value != reply.record.value => {
                    Outcome::Failed("oracle vs Rust: changed result".into())
                }
                None => Outcome::Compared,
            },
            (None, Slot::Reply(_), None) => Outcome::Unprocessed("oracle: no result".into()),
        };
        outcomes.push(CaseOutcome {
            case: case.case.clone(),
            outcome,
            oracle: oracle_record,
            oracle_raw,
            rust: rust_raw,
        });
    }
    Evaluation {
        outcomes,
        faults,
        extra,
    }
}

/// The golden comparison of every oracle result, kept per case.
#[derive(Debug, Clone, Default)]
pub struct GoldenCheck {
    /// The golden could not be read, parsed or aligned with the corpus.
    pub error: Option<String>,
    /// (case, golden line, oracle line) for each differing reference result.
    pub mismatched: Vec<(String, String, String)>,
    /// Cases whose oracle result was unavailable, so the golden could not be checked.
    pub unchecked: Vec<String>,
}

impl GoldenCheck {
    pub fn status(&self) -> &'static str {
        if self.error.is_some() || !self.mismatched.is_empty() {
            "failed"
        } else if !self.unchecked.is_empty() {
            "incomplete"
        } else {
            "matched"
        }
    }
}

/// Compares every available oracle result, including Rust-deferred cases, with
/// the committed golden. The golden itself is parsed strictly.
pub fn check_golden(text: Result<String, String>, unit: &str, eval: &Evaluation) -> GoldenCheck {
    let mut check = GoldenCheck::default();
    let golden = match text.and_then(|text| records(&text, unit, false)) {
        Ok(golden) => golden,
        Err(error) => {
            check.error = Some(format!("golden: {error}"));
            return check;
        }
    };
    if golden.len() != eval.outcomes.len() {
        check.error = Some(format!(
            "golden: case count {} != {}",
            golden.len(),
            eval.outcomes.len()
        ));
        return check;
    }
    for (index, (expected, case)) in golden.iter().zip(&eval.outcomes).enumerate() {
        if expected.case != case.case {
            check.error = Some(format!(
                "golden: case order/identity at {index}: {} != {}",
                expected.case, case.case
            ));
            return check;
        }
    }
    for (expected, case) in golden.iter().zip(&eval.outcomes) {
        match &case.oracle {
            Some(actual) if actual.value == expected.value => {}
            Some(actual) => {
                check
                    .mismatched
                    .push((case.case.clone(), expected.raw.clone(), actual.raw.clone()))
            }
            None => check.unchecked.push(case.case.clone()),
        }
    }
    check
}
