// SPDX-License-Identifier: GPL-3.0-or-later
//! `include/core/throttle.h` (unit `include.core.application.pure.p001`): `THROTTLE`, a
//! rate limiter that fires at most once per interval.
//!
//! The C++ reads `std::chrono::steady_clock::now()`; here the host supplies every reading
//! through [`SteadyClock`], one call per `now()`, so the crate stays clock-free. Time
//! points and durations keep libstdc++'s representation: signed 64-bit nanoseconds for
//! `steady_clock` and signed 64-bit milliseconds for the interval. The C++ arithmetic is
//! undefined where a signed 64-bit operation overflows; those evaluations return
//! [`ThrottleError::Overflow`] instead, leaving the throttle unchanged.

use crate::profile::{Milliseconds, TimePoint};

/// Host clock for `std::chrono::steady_clock::now()`: nanoseconds since the clock's epoch.
pub trait SteadyClock {
    /// The current time point.
    fn now(&mut self) -> TimePoint;
}

/// An evaluation whose C++ arithmetic is undefined.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThrottleError {
    /// A signed 64-bit nanosecond conversion or difference overflows.
    Overflow,
}

/// `std::chrono::nanoseconds( aInterval )`: the implicit conversion `duration` arithmetic
/// applies to milliseconds (the count times 1,000,000).
fn interval_nanoseconds(interval: Milliseconds) -> Result<i64, ThrottleError> {
    interval
        .0
        .checked_mul(1_000_000)
        .ok_or(ThrottleError::Overflow)
}

/// `THROTTLE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Throttle {
    interval: Milliseconds,
    last: TimePoint,
}

impl Throttle {
    /// `THROTTLE( aInterval )`: `m_last = steady_clock::now() - aInterval`, so the first
    /// `Ready()` fires.
    pub fn new<C: SteadyClock + ?Sized>(
        interval: Milliseconds,
        clock: &mut C,
    ) -> Result<Self, ThrottleError> {
        let now = clock.now();
        let last = now
            .checked_sub(interval_nanoseconds(interval)?)
            .ok_or(ThrottleError::Overflow)?;
        Ok(Self { interval, last })
    }

    /// `Ready()`: true, restarting the interval at `now`, when at least the interval has
    /// elapsed since the previous true return (or since construction).
    pub fn ready<C: SteadyClock + ?Sized>(&mut self, clock: &mut C) -> Result<bool, ThrottleError> {
        let now = clock.now();
        let elapsed = now.checked_sub(self.last).ok_or(ThrottleError::Overflow)?;

        if elapsed >= interval_nanoseconds(self.interval)? {
            self.last = now;
            return Ok(true);
        }

        Ok(false)
    }

    /// The private `m_interval`.
    pub fn interval(&self) -> Milliseconds {
        self.interval
    }

    /// The private `m_last`.
    pub fn last(&self) -> TimePoint {
        self.last
    }
}
