// SPDX-License-Identifier: GPL-3.0-or-later
//! Port of `libs/core/profile.cpp` (`GetRunningMicroSecs`, `LATENCY_PROBE::to_string`;
//! unit [`crate::UNIT`]) and of the `libs/core/include/core/profile.h` header bodies
//! (`PROF_TIMER`, `SCOPED_PROF_TIMER`, `PROF_COUNTER` and the inline `LATENCY_PROBE`
//! members; unit [`crate::HEADER_UNIT`]).
//!
//! Portable code reads no clock. For `GetRunningMicroSecs` the host supplies the raw
//! platform reading through [`RunningClock`]; this module performs the reference's
//! arithmetic on it. The pinned Linux build compiles the `HAVE_CLOCK_GETTIME` body
//! (`clock_gettime( CLOCK_MONOTONIC )`); the `_WIN32` and `HAVE_GETTIMEOFDAY_FUNC`
//! bodies are ported as the other [`ClockReading`] forms. The header classes read
//! `std::chrono::high_resolution_clock::now()` (libstdc++'s `system_clock`), which the
//! host supplies as a [`TimePoint`] through [`ChronoClock`], one call per `now()`.
//!
//! `PROF_COUNTER` is atomic in the reference; portable code has no threads, so its
//! owner has exclusive access and the count is a plain integer with the same modular
//! arithmetic. Output streams (`std::ostream&`, default `std::cerr`) are a host-supplied
//! `String` sink in its default formatting state. Nondefault stream state and concurrent
//! operations remain unfinished. Scoped timers require explicit finish; ordinary drop
//! does not report a duration. Name copies and formatting still allocate infallibly, so
//! ProfileError::Allocation does not cover every allocation path.

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
    /// `std::length_error` from `std::vector::reserve` (`what()` is the payload).
    LengthError(&'static str),
    /// An allocation failed (the reference throws `bad_alloc`).
    Allocation,
    /// `PROF_TIMER::to_string` read `m_started`, which the reference leaves
    /// uninitialised when a named timer is built without `aAutostart` and never
    /// started (undefined).
    Indeterminate,
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
    /// [`ProfileError::Overflow`] where a time point difference overflows (undefined
    /// in the reference).
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
                push_fixed_3(&mut ss, self.delta_ms(cp.timestamp, prev)?);
                ss.push(' ');
                ss.push_str("accumulated: ");
                push_fixed_3(&mut ss, self.delta_ms(cp.timestamp, self.start)?);
                ss.push('\n');

                prev = cp.timestamp;
            }
        }

        Ok(ss)
    }
}

// ---- profile.h header bodies (unit libs.core.include.core.application.pure.p001) ----------

/// Host clock for `std::chrono::high_resolution_clock::now()`. One call is one `now()`.
pub trait ChronoClock {
    /// The current time point.
    fn now(&mut self) -> TimePoint;
}

/// A `std::chrono::duration` that `PROF_TIMER::SinceStart<DURATION>` can produce:
/// `std::chrono::duration_cast<DURATION>` of a signed 64-bit nanosecond count.
pub trait ChronoDuration: Sized {
    /// `duration_cast<Self>( std::chrono::nanoseconds( ns ) )`.
    fn from_nanoseconds(ns: i64) -> Self;
}

/// `std::chrono::duration<double, std::nano>`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DoubleNanoseconds(pub f64);
/// `std::chrono::duration<double, std::micro>`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DoubleMicroseconds(pub f64);
/// `std::chrono::duration<double, std::milli>`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DoubleMilliseconds(pub f64);
/// `std::chrono::duration<double>` (seconds).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DoubleSeconds(pub f64);
/// `std::chrono::nanoseconds`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Nanoseconds(pub i64);
/// `std::chrono::microseconds`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Microseconds(pub i64);
/// `std::chrono::milliseconds`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Milliseconds(pub i64);
/// `std::chrono::seconds`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seconds(pub i64);

// libstdc++ __duration_cast_impl: to a floating representation the common type is
// double, so the count converts to double and divides by the ratio's denominator; to
// an integer representation it is long, so the division truncates toward zero.
impl ChronoDuration for DoubleNanoseconds {
    fn from_nanoseconds(ns: i64) -> Self {
        Self(ns as f64)
    }
}
impl ChronoDuration for DoubleMicroseconds {
    fn from_nanoseconds(ns: i64) -> Self {
        Self(ns as f64 / 1000.0)
    }
}
impl ChronoDuration for DoubleMilliseconds {
    fn from_nanoseconds(ns: i64) -> Self {
        Self(ns as f64 / 1_000_000.0)
    }
}
impl ChronoDuration for DoubleSeconds {
    fn from_nanoseconds(ns: i64) -> Self {
        Self(ns as f64 / 1_000_000_000.0)
    }
}
impl ChronoDuration for Nanoseconds {
    fn from_nanoseconds(ns: i64) -> Self {
        Self(ns)
    }
}
impl ChronoDuration for Microseconds {
    fn from_nanoseconds(ns: i64) -> Self {
        Self(ns / 1000)
    }
}
impl ChronoDuration for Milliseconds {
    fn from_nanoseconds(ns: i64) -> Self {
        Self(ns / 1_000_000)
    }
}
impl ChronoDuration for Seconds {
    fn from_nanoseconds(ns: i64) -> Self {
        Self(ns / 1_000_000_000)
    }
}

/// `std::ostream << double` in a stream's default state: `%g` with precision 6 in the
/// C locale (glibc spelling of non-finite values).
pub fn push_general_6(out: &mut String, value: f64) {
    if value.is_nan() {
        out.push_str(if value.is_sign_negative() {
            "-nan"
        } else {
            "nan"
        });
        return;
    }
    if value.is_infinite() {
        out.push_str(if value < 0.0 { "-inf" } else { "inf" });
        return;
    }
    if value == 0.0 {
        out.push_str(if value.is_sign_negative() { "-0" } else { "0" });
        return;
    }

    // %g: X is the exponent of the %e conversion with precision P - 1 = 5 (after
    // rounding). If P > X >= -4 the value is printed as %f with precision P - 1 - X,
    // otherwise as %e with precision P - 1; trailing zeros and a trailing point are
    // then removed. Rust's exact formatting rounds half to even on the binary value,
    // as glibc does in the default rounding mode.
    let scientific = format!("{value:.5e}");
    let Some((mantissa, exponent)) = scientific.split_once('e') else {
        return;
    };
    let Ok(exponent) = exponent.parse::<i32>() else {
        return;
    };

    if !(-4..6).contains(&exponent) {
        out.push_str(strip_fraction_zeros(mantissa));
        out.push('e');
        out.push(if exponent < 0 { '-' } else { '+' });
        let _ = write!(out, "{:02}", exponent.unsigned_abs());
    } else {
        let precision = usize::try_from(5 - exponent).unwrap_or(0);
        let fixed = format!("{value:.precision$}");
        out.push_str(strip_fraction_zeros(&fixed));
    }
}

/// Remove trailing zeros of a fraction, then a trailing decimal point.
fn strip_fraction_zeros(text: &str) -> &str {
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.')
    } else {
        text
    }
}

/// `cnt << "ns"`, `( cnt / 1e3 ) << "µs"`, ... as `PROF_TIMER::Show` and `to_string`
/// print a nanosecond count.
fn push_scaled(out: &mut String, cnt: f64) {
    if cnt < 1e3 {
        push_general_6(out, cnt);
        out.push_str("ns");
    } else if cnt < 1e6 {
        push_general_6(out, cnt / 1e3);
        out.push_str("\u{b5}s");
    } else if cnt < 1e9 {
        push_general_6(out, cnt / 1e6);
        out.push_str("ms");
    } else {
        push_general_6(out, cnt / 1e9);
        out.push('s');
    }
}

/// `PROF_TIMER`: the elapsed time between its start and its stop (or now).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfTimer {
    /// `m_name`, a string printed in messages.
    name: String,
    /// `m_running`.
    running: bool,
    /// `m_started`. `None` is the reference's uninitialised value: the named
    /// constructor without `aAutostart` never writes it, only `Start()` does.
    started: Option<bool>,
    /// `m_starttime`, `m_lasttime`, `m_stoptime`: the epoch (zero) until written.
    start_time: TimePoint,
    last_time: TimePoint,
    stop_time: TimePoint,
}

impl ProfTimer {
    /// `PROF_TIMER( const std::string& aName, bool aAutostart = true )`.
    pub fn named<C: ChronoClock + ?Sized>(name: &str, autostart: bool, clock: &mut C) -> Self {
        let mut timer = Self {
            name: name.to_owned(),
            running: false,
            started: None,
            start_time: 0,
            last_time: 0,
            stop_time: 0,
        };

        if autostart {
            timer.start(clock);
        }

        timer
    }

    /// `PROF_TIMER()`: started, with an empty name.
    pub fn new<C: ChronoClock + ?Sized>(clock: &mut C) -> Self {
        let mut timer = Self {
            name: String::new(),
            running: false,
            started: None,
            start_time: 0,
            last_time: 0,
            stop_time: 0,
        };
        timer.start(clock);
        timer
    }

    /// `Start()`: start or restart.
    pub fn start<C: ChronoClock + ?Sized>(&mut self, clock: &mut C) {
        self.started = Some(true);
        self.running = true;
        self.start_time = clock.now();
        self.last_time = self.start_time;
    }

    /// `Stop()`: record the stop time if running.
    pub fn stop<C: ChronoClock + ?Sized>(&mut self, clock: &mut C) {
        if !self.running {
            return;
        }

        self.stop_time = clock.now();
        self.running = false;
    }

    /// `SinceStart<DURATION>( bool aSinceLast = false )`: the time from the start (or,
    /// with `since_last`, from the last reading) to the stop time, or to now while
    /// running. The end becomes the last reading.
    ///
    /// # Errors
    /// [`ProfileError::Overflow`] when the time point difference overflows (undefined
    /// in the reference); the last reading is updated first, as in the reference.
    pub fn since_start<D: ChronoDuration, C: ChronoClock + ?Sized>(
        &mut self,
        clock: &mut C,
        since_last: bool,
    ) -> Result<D, ProfileError> {
        let stoptime = if self.running {
            clock.now()
        } else {
            self.stop_time
        };
        let starttime = if since_last {
            self.last_time
        } else {
            self.start_time
        };

        self.last_time = stoptime;

        stoptime
            .checked_sub(starttime)
            .map(D::from_nanoseconds)
            .ok_or(ProfileError::Overflow)
    }

    /// `msecs( bool aSinceLast = false )`: [`Self::since_start`] in milliseconds.
    ///
    /// # Errors
    /// As [`Self::since_start`].
    pub fn msecs<C: ChronoClock + ?Sized>(
        &mut self,
        clock: &mut C,
        since_last: bool,
    ) -> Result<f64, ProfileError> {
        Ok(self
            .since_start::<DoubleMilliseconds, C>(clock, since_last)?
            .0)
    }

    /// `Show( std::ostream& aStream = std::cerr )`: `"<name> took <time><unit>\n"`.
    ///
    /// # Errors
    /// As [`Self::since_start`]; nothing is written then.
    pub fn show<C: ChronoClock + ?Sized>(
        &mut self,
        clock: &mut C,
        stream: &mut String,
    ) -> Result<(), ProfileError> {
        let cnt = self.since_start::<DoubleNanoseconds, C>(clock, false)?.0;

        if !self.name.is_empty() {
            stream.push_str(&self.name);
            stream.push_str(" took ");
        }

        push_scaled(stream, cnt);
        // std::endl
        stream.push('\n');
        Ok(())
    }

    /// `to_string()`: `"<name>: <time><unit>"`, or `"<name>: none"` if not started.
    ///
    /// # Errors
    /// As [`Self::since_start`], or [`ProfileError::Indeterminate`] when `m_started`
    /// was never written.
    pub fn to_string<C: ChronoClock + ?Sized>(
        &mut self,
        clock: &mut C,
    ) -> Result<String, ProfileError> {
        let cnt = self.since_start::<DoubleNanoseconds, C>(clock, false)?.0;
        let mut retv = String::new();

        if !self.name.is_empty() {
            retv.push_str(&self.name);
            retv.push_str(": ");
        }

        // Only Start() writes m_started, always with true, so the "none" branch is
        // reachable only through the uninitialised value.
        if !self.started.ok_or(ProfileError::Indeterminate)? {
            retv.push_str("none");
            return Ok(retv);
        }

        push_scaled(&mut retv, cnt);
        Ok(retv)
    }

    /// `GetName()`.
    pub fn get_name(&self) -> &str {
        &self.name
    }
}

/// `SCOPED_PROF_TIMER<DURATION>`: a `PROF_TIMER` whose destruction reports the time
/// since its construction. The reference writes it into the `DURATION&` given to the
/// constructor; here [`Self::finish`] (the destructor) returns it.
#[derive(Debug, PartialEq, Eq)]
#[must_use = "the measured duration is produced by finish()"]
pub struct ScopedProfTimer<D> {
    /// The `PROF_TIMER` base, started by its default constructor.
    base: ProfTimer,
    /// `m_counter`, the counter used to do the profiling.
    counter: ProfTimer,
    duration: core::marker::PhantomData<D>,
}

impl<D: ChronoDuration> ScopedProfTimer<D> {
    /// `SCOPED_PROF_TIMER( DURATION& aDuration )`: the base `PROF_TIMER()`, then the
    /// member `m_counter` (two clock reads, in that order).
    pub fn new<C: ChronoClock + ?Sized>(clock: &mut C) -> Self {
        let base = ProfTimer::new(clock);
        let counter = ProfTimer::new(clock);
        Self {
            base,
            counter,
            duration: core::marker::PhantomData,
        }
    }

    /// The `PROF_TIMER` base. Its operations do not affect the reported duration.
    pub fn base(&self) -> &ProfTimer {
        &self.base
    }

    /// The `PROF_TIMER` base, mutably.
    pub fn base_mut(&mut self) -> &mut ProfTimer {
        &mut self.base
    }

    /// `~SCOPED_PROF_TIMER()`: `m_duration = m_counter.SinceStart<DURATION>()`.
    ///
    /// # Errors
    /// As [`ProfTimer::since_start`].
    pub fn finish<C: ChronoClock + ?Sized>(mut self, clock: &mut C) -> Result<D, ProfileError> {
        self.counter.since_start::<D, C>(clock, false)
    }
}

/// `PROF_COUNTER`: an event counter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfCounter {
    name: String,
    count: u64,
}

impl Default for ProfCounter {
    fn default() -> Self {
        Self::new()
    }
}

impl ProfCounter {
    /// `PROF_COUNTER()`: named `"Anonymous"`, count 0.
    pub fn new() -> Self {
        Self::named("Anonymous")
    }

    /// `PROF_COUNTER( const std::string& aName )`: count 0.
    pub fn named(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            count: 0,
        }
    }

    /// `Count()`.
    pub fn count(&self) -> u64 {
        self.count
    }

    /// `Reset()`.
    pub fn reset(&mut self) {
        self.count = 0;
    }

    /// `operator++( int )`: increment (modulo 2^64) and return the previous count.
    pub fn post_increment(&mut self) -> u64 {
        let previous = self.count;
        self.count = previous.wrapping_add(1);
        previous
    }

    /// `Show( std::ostream& aStream = std::cerr )`: `"<name>: <count>\n"`.
    pub fn show(&self, stream: &mut String) {
        if !self.name.is_empty() {
            stream.push_str(&self.name);
            stream.push_str(": ");
        }

        let _ = write!(stream, "{}", self.count);
        stream.push('\n');
    }

    /// Set the private `m_count`. No reference member does this; harnesses use it to
    /// reach counts such as `2^64 - 1`.
    pub fn set_raw_count(&mut self, count: u64) {
        self.count = count;
    }
}

impl LatencyProbe {
    /// `LATENCY_PROBE( const std::string& aName, int aStages = 8 )`: `m_start` is the
    /// epoch; `aStages` checkpoints are reserved.
    ///
    /// # Errors
    /// [`ProfileError::LengthError`] (`"vector::reserve"`) for a negative `stages`,
    /// which converts to a count above `max_size()`; [`ProfileError::Allocation`].
    pub fn new(name: &str, stages: i32) -> Result<Self, ProfileError> {
        let mut checkpoints = Vec::new();
        let stages =
            usize::try_from(stages).map_err(|_| ProfileError::LengthError("vector::reserve"))?;
        checkpoints
            .try_reserve(stages)
            .map_err(|_| ProfileError::Allocation)?;
        Ok(Self {
            start: 0,
            name: name.to_owned(),
            checkpoints,
        })
    }

    /// `Reset()`: restart at now and drop every checkpoint.
    pub fn reset<C: ChronoClock + ?Sized>(&mut self, clock: &mut C) {
        self.start = clock.now();
        self.checkpoints.clear();
    }

    /// `Checkpoint( const std::string& aName )`: record now under `name`.
    ///
    /// # Errors
    /// [`ProfileError::Allocation`]; nothing is recorded then.
    pub fn checkpoint<C: ChronoClock + ?Sized>(
        &mut self,
        clock: &mut C,
        name: &str,
    ) -> Result<(), ProfileError> {
        let chk = Checkpoint {
            name: name.to_owned(),
            timestamp: clock.now(),
            is_timer: false,
            timer_delta: 0.0,
        };
        self.push(chk)
    }

    /// `AddTimer( PROF_TIMER& aTimer )`: record the timer's name and `msecs()`.
    ///
    /// # Errors
    /// As [`ProfTimer::since_start`], or [`ProfileError::Allocation`].
    pub fn add_timer<C: ChronoClock + ?Sized>(
        &mut self,
        timer: &mut ProfTimer,
        clock: &mut C,
    ) -> Result<(), ProfileError> {
        let name = timer.get_name().to_owned();
        let timer_delta = timer.msecs(clock, false)?;
        self.push(Checkpoint {
            name,
            timestamp: 0,
            is_timer: true,
            timer_delta,
        })
    }

    /// `m_checkpoints.push_back( chk )`.
    fn push(&mut self, chk: Checkpoint) -> Result<(), ProfileError> {
        self.checkpoints
            .try_reserve(1)
            .map_err(|_| ProfileError::Allocation)?;
        self.checkpoints.push(chk);
        Ok(())
    }

    /// `delta_ms( TIME_POINT a, TIME_POINT b )`: `a - b` in milliseconds, a double.
    ///
    /// # Errors
    /// [`ProfileError::Overflow`] when `a - b` overflows (undefined in the reference).
    pub fn delta_ms(&self, a: TimePoint, b: TimePoint) -> Result<f64, ProfileError> {
        a.checked_sub(b)
            .map(|ns| DoubleMilliseconds::from_nanoseconds(ns).0)
            .ok_or(ProfileError::Overflow)
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
    fn timer_and_checkpoint_lines() {
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
        assert_eq!(
            probe.to_string(),
            Ok(format!(
                "Probe stats forp\ntmr: t{pad}0.062\nchp: c{pad}since last : 0.000 accumulated: 0.000\n",
                pad = " ".repeat(29)
            ))
        );
    }
}
