// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Portable port of KiCad's Unix `libs/kiplatform/os/unix/drivers.cpp` and
//! `libs/kiplatform/os/unix/sysinfo.cpp` at pin
//! 4cf939778df04d2b747bb2c4714fc5014a7565c7 (unit
//! `libs.kiplatform.os.unix.application.pure.p001`).
//!
//! On Unix the pinned sources query nothing: the 3Dconnexion driver check is always
//! `false`, and every `SYSINFO` query returns `false` without touching its output
//! argument. Those constant results are the reference behaviour, not stand-ins, so
//! this crate reproduces exactly that and reads no host state.

pub mod drivers;
pub mod sysinfo;

/// The unit record's `rust_module`: the two ported Unix sources.
pub mod unix {
    pub use crate::{drivers, sysinfo};
}

/// The port unit implemented by this crate.
pub const UNIT: &str = "libs.kiplatform.os.unix.application.pure.p001";

/// The unit owning `libs/kiplatform/include/kiplatform/{drivers,sysinfo,sysinfo_gtk}.h`,
/// whose declarations are co-ported here in the minimum form the bodies need.
pub const HEADER_UNIT: &str = "libs.kiplatform.include.kiplatform.application.pure.p001";
