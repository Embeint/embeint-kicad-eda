// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
#![no_std]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

//! Portable port of KiCad's `libs/kiplatform` platform layer. Only the Unix policy
//! source (`libs/kiplatform/os/unix/policy.cpp`) is ported so far; see `policy`.

pub mod policy;

/// The unit that owns `libs/kiplatform/os/unix/policy.cpp`.
pub const UNIT: &str = "libs.kiplatform.os.unix.application.thin.replacement.p001";
/// The unit that owns `libs/kiplatform/include/kiplatform/policy.h` (`PBOOL`).
pub const HEADER_UNIT: &str =
    "libs.kiplatform.include.kiplatform.application.thin.replacement.p001";
