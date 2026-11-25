// SPDX-License-Identifier: GPL-3.0-or-later
//! Port of `libs/core/profile.cpp`: `GetRunningMicroSecs` and `LATENCY_PROBE::to_string`.
//!
//! Portable code reads no clock. The host supplies the raw platform reading through
//! [`RunningClock`]; this module performs the reference's arithmetic on it. The
//! pinned Linux build compiles the `HAVE_CLOCK_GETTIME` body
//! (`clock_gettime( CLOCK_MONOTONIC )`); the `_WIN32` and `HAVE_GETTIMEOFDAY_FUNC`
//! bodies are ported as the other [`ClockReading`] forms.
//!
//! `PROF_TIMER`, `PROF_COUNTER`, `SCOPED_PROF_TIMER` and every inline `LATENCY_PROBE`
//! member (constructor, `Reset`, `Checkpoint`, `AddTimer`, `delta_ms`) are header
//! bodies owned by [`crate::HEADER_UNIT`]. `to_string`'s calls to `delta_ms` are typed
//! deferrals to that unit.

use core::fmt::Write as _;

/// `struct timespec` as filled in by `clock_gettime` (64-bit `time_t` and `long`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timespec {
    pub tv_sec: i64,
    pub tv_nsec: i64,
}

/// One raw reading of the platform clock that `GetRunningMicroSecs` uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockReading {
    /// `_WIN32`: `GetSystemTimeAsFileTime` (100 ns units).
    FileTime { high: u32, low: u32 },
    /// `HAVE_CLOCK_GETTIME`: `clock_gettime( CLOCK_MONOTONIC, &now )`.
    Monotonic(Timespec),
    /// `HAVE_GETTIMEOFDAY_FUNC`: `gettimeofday( &tv, 0 )`.
    TimeOfDay { tv_sec: i64, tv_usec: i64 },
}

/// Host-supplied clock. An implementation performs exactly one platform read per
/// call; on the pinned Linux profile that is `clock_gettime( CLOCK_MONOTONIC )`.
pub trait RunningClock {
    fn read_running_clock(&mut self) -> ClockReading;
}

/// Errors from profile arithmetic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileError {
    /// Signed 64-bit overflow, undefined in the reference.
    Overflow,
    /// A call into a body owned by another unit that is not ported yet.
    Unported {
        upstream: &'static str,
        unit: &'static str,
    },
}

/// `_WIN32` body: `( UINT64( high ) << 32 ) + low`, divided by 10, as `int64_t`.
#[must_use]
pub fn micro_secs_from_file_time(high: u32, low: u32) -> i64 {
    let t = ((u64::from(high) << 32) + u64::from(low)) / 10;
    // t <= (2^64 - 1) / 10 < 2^63, so the conversion is exact.
    i64::try_from(t).unwrap_or(i64::MAX)
}

/// `HAVE_CLOCK_GETTIME` body: `(int64_t) now.tv_sec * 1000000 + now.tv_nsec / 1000`.
///
/// # Errors
/// [`ProfileError::Overflow`] where the reference's signed arithmetic overflows.
pub fn micro_secs_from_timespec(now: Timespec) -> Result<i64, ProfileError> {
    now.tv_sec
        .checked_mul(1_000_000)
        .and_then(|usecs| usecs.checked_add(now.tv_nsec / 1000))
        .ok_or(ProfileError::Overflow)
}

/// `HAVE_GETTIMEOFDAY_FUNC` body: `(int64_t) tv.tv_sec * 1000000 + tv.tv_usec`.
///
/// # Errors
/// [`ProfileError::Overflow`] where the reference's signed arithmetic overflows.
pub fn micro_secs_from_timeval(tv_sec: i64, tv_usec: i64) -> Result<i64, ProfileError> {
    tv_sec
        .checked_mul(1_000_000)
        .and_then(|usecs| usecs.checked_add(tv_usec))
        .ok_or(ProfileError::Overflow)
}

/// `GetRunningMicroSecs`: one host clock read converted with the matching body.
///
/// # Errors
/// [`ProfileError::Overflow`] where the reference's signed arithmetic overflows.
pub fn get_running_micro_secs<C: RunningClock + ?Sized>(
    clock: &mut C,
) -> Result<i64, ProfileError> {
    match clock.read_running_clock() {
        ClockReading::FileTime { high, low } => Ok(micro_secs_from_file_time(high, low)),
        ClockReading::Monotonic(now) => micro_secs_from_timespec(now),
        ClockReading::TimeOfDay { tv_sec, tv_usec } => micro_secs_from_timeval(tv_sec, tv_usec),
    }
}

/// `LATENCY_PROBE::TIME_POINT`: a `high_resolution_clock` (libstdc++ `system_clock`)
/// time point, as its signed 64-bit nanosecond count.
pub type TimePoint = i64;

/// `LATENCY_PROBE::CHECKPOINT`.
#[derive(Clone, Debug, PartialEq)]
pub struct Checkpoint {
    pub name: String,
    pub timestamp: TimePoint,
    pub is_timer: bool,
    pub timer_delta: f64,
}

/// The `LATENCY_PROBE` state `to_string` reads (minimum type co-port; the members
/// that fill it belong to [`crate::HEADER_UNIT`]).
#[derive(Clone, Debug, PartialEq)]
pub struct LatencyProbe {
    pub start: TimePoint,
    pub name: String,
    pub checkpoints: Vec<Checkpoint>,
}

/// `LATENCY_PROBE::delta_ms`, an inline header member.
// UNPORTED(libs.core.include.core.application.pure.p001): LATENCY_PROBE::delta_ms
fn delta_ms(_a: TimePoint, _b: TimePoint) -> Result<f64, ProfileError> {
    Err(ProfileError::Unported {
        upstream: "LATENCY_PROBE::delta_ms",
        unit: crate::HEADER_UNIT,
    })
}

/// `std::left << std::setw( 30 ) << name`: pad with spaces to 30 bytes, never truncate.
fn push_left_30(out: &mut String, name: &str) {
    out.push_str(name);
    for _ in name.len()..30 {
        out.push(' ');
    }
}

/// `std::fixed << std::setprecision( 3 ) << value` on the pinned libstdc++/glibc
/// ("%.3f" in the C locale), including glibc's spelling of non-finite values.
pub fn push_fixed_3(out: &mut String, value: f64) {
    if value.is_nan() {
        out.push_str(if value.is_sign_negative() {
            "-nan"
        } else {
            "nan"
        });
    } else if value.is_infinite() {
        out.push_str(if value < 0.0 { "-inf" } else { "inf" });
    } else {
        // Rust's fixed-precision formatting is the exact, round-half-even decimal
        // expansion that glibc printf produces in the default rounding mode,
        // including the sign of negative zero.
        let _ = write!(out, "{value:.3}");
    }
}

impl LatencyProbe {
    /// `LATENCY_PROBE::to_string`.
    ///
    /// # Errors
    /// [`ProfileError::Unported`] at the first non-timer checkpoint (`delta_ms`).
    pub fn to_string(&self) -> Result<String, ProfileError> {
        let mut ss = String::new();
        let mut prev = self.start;
        ss.push_str("Probe stats for");
        ss.push_str(&self.name);
        ss.push('\n');

        for cp in &self.checkpoints {
            if cp.is_timer {
                ss.push_str("tmr: ");
                push_left_30(&mut ss, &cp.name);
                push_fixed_3(&mut ss, cp.timer_delta);
                ss.push('\n');
            } else {
                ss.push_str("chp: ");
                push_left_30(&mut ss, &cp.name);
                ss.push_str("since last : ");
                push_fixed_3(&mut ss, delta_ms(cp.timestamp, prev)?);
                ss.push(' ');
                ss.push_str("accumulated: ");
                push_fixed_3(&mut ss, delta_ms(cp.timestamp, self.start)?);
                ss.push('\n');

                prev = cp.timestamp;
            }
        }

        Ok(ss)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed(ClockReading);
    impl RunningClock for Fixed {
        fn read_running_clock(&mut self) -> ClockReading {
            self.0
        }
    }

    #[test]
    fn clock_bodies() {
        let mono = |tv_sec, tv_nsec| {
            get_running_micro_secs(&mut Fixed(ClockReading::Monotonic(Timespec {
                tv_sec,
                tv_nsec,
            })))
        };
        assert_eq!(mono(1, 999_999_999), Ok(1_999_999));
        assert_eq!(mono(-1, 1999), Ok(-999_999));
        assert_eq!(
            mono(i64::MAX / 1_000_000 + 1, 0),
            Err(ProfileError::Overflow)
        );
        assert_eq!(
            micro_secs_from_file_time(u32::MAX, u32::MAX),
            1_844_674_407_370_955_161
        );
        assert_eq!(micro_secs_from_timeval(2, -3), Ok(1_999_997));
    }

    #[test]
    fn timer_lines_and_deferral() {
        let mut probe = LatencyProbe {
            start: 0,
            name: "p".into(),
            checkpoints: vec![],
        };
        probe.checkpoints.push(Checkpoint {
            name: "t".into(),
            timestamp: 0,
            is_timer: true,
            timer_delta: 0.0625,
        });
        assert_eq!(
            probe.to_string(),
            Ok(format!("Probe stats forp\ntmr: t{}0.062\n", " ".repeat(29)))
        );
        probe.checkpoints.push(Checkpoint {
            name: "c".into(),
            timestamp: 5,
            is_timer: false,
            timer_delta: 0.0,
        });
        assert!(matches!(
            probe.to_string(),
            Err(ProfileError::Unported { .. })
        ));
    }
}
