// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Portable port of KiCad `libs/core` at pin 4cf939778df04d2b747bb2c4714fc5014a7565c7
//! (unit `libs.core.application.pure.p001`): `base64.cpp`, `observable.cpp`,
//! `profile.cpp` and `version_compare.cpp`.
//!
//! The crate has no filesystem, clock, thread or global state. Time is supplied by
//! the host through [`profile::RunningClock`]; observable subscription lists live in
//! an explicit [`observable::ObservableArena`] instead of shared pointers.

pub mod base64;
pub mod observable;
pub mod profile;
pub mod version_compare;

/// The unit record's `rust_module`: the four ported source files of the unit.
pub mod libcore {
    pub use crate::{base64, observable, profile, version_compare};
}

/// The port unit implemented by this crate.
pub const UNIT: &str = "libs.core.application.pure.p001";

/// The unit owning the `libs/core/include/core` header bodies (`OBSERVABLE<T>`,
/// `PROF_TIMER`, `LATENCY_PROBE` inline members, ...). Calls into those bodies are
/// typed deferrals naming this owner.
pub const HEADER_UNIT: &str = "libs.core.include.core.application.pure.p001";
