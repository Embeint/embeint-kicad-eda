// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
//! Portable code parity with KiCad's pinned IEC60664 calculator.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod iec60664;
mod tables;

pub use iec60664::{
    Field, Iec60664, InsulationType, MaterialGroup, OvervoltageCategory, PollutionDegree, State,
    Waveform,
};
