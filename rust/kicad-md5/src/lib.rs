// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Behaviour of `MD5_HASH` at KiCad pin 4cf939778df04d2b747bb2c4714fc5014a7565c7.
//! The stored digest, validity and running context are deliberately independent.

pub mod md5_hash;

pub use md5_hash::{LengthError, Md5Hash};
