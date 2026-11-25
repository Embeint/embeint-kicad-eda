// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Portable port of KiCad `libs/core` at pin 4cf939778df04d2b747bb2c4714fc5014a7565c7:
//!
//! * unit `libs.core.application.pure.p001`: `base64.cpp`, `observable.cpp`,
//!   `profile.cpp` and `version_compare.cpp`;
//! * unit `libs.core.include.core.application.pure.p001`: the `libs/core/include/core`
//!   header bodies (`OBSERVABLE<T>`, the `profile.h` timers, counters and latency
//!   probe members, `KI_UNION_FIND`, `KI_MST::FilterKruskal`, `MINOPTMAX<T>`,
//!   `KISPINLOCK`, `SYNC_QUEUE<T>`, `arrayDim`, `ignore_unused`, `to_underlying`).
//!
//! The crate has no filesystem, clock, thread or global state. Time is supplied by
//! the host through [`profile::RunningClock`] and [`profile::ChronoClock`]; observable
//! subscription lists live in an explicit [`observable::ObservableArena`] instead of
//! shared pointers; the atomics and locks of the headers have one owner at a time.

pub mod base64;
pub mod filter_kruskal;
pub mod helpers;
pub mod minoptmax;
pub mod observable;
pub mod observable_template;
pub mod profile;
pub mod spinlock;
pub mod stdalgo;
pub mod sync_queue;
pub mod union_find;
pub mod version_compare;

/// The unit record's `rust_module`: the four ported source files of the unit.
pub mod libcore {
    pub use crate::{base64, observable, profile, version_compare};
}

/// The header unit's `rust_module`: the ported `libs/core/include/core` bodies.
/// `profile.h`'s bodies share [`crate::profile`] with `profile.cpp`.
pub mod include_core {
    pub use crate::{
        filter_kruskal, helpers, minoptmax, observable_template, profile, spinlock, stdalgo,
        sync_queue, union_find,
    };
}

/// The port unit of `libs/core`'s sources.
pub const UNIT: &str = "libs.core.application.pure.p001";

/// The unit owning the `libs/core/include/core` header bodies (`OBSERVABLE<T>`,
/// `PROF_TIMER`, `LATENCY_PROBE` inline members, ...).
pub const HEADER_UNIT: &str = "libs.core.include.core.application.pure.p001";
