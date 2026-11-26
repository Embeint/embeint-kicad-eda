// SPDX-License-Identifier: GPL-3.0-or-later
#![no_std]
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

//! Portable port of KiCad `libs/core/include/core/map_helpers.h` at pin
//! 4cf939778df04d2b747bb2c4714fc5014a7565c7 (unit
//! `libs.core.include.core.application.thin.replacement.p001`).
//!
//! The header's `get_opt` and `get_def` overloads look a key up in a `std::map` and
//! return a copy of the mapped value or of a default. Its `wxString` overloads need
//! a replacement for `wxString`: [`wx_string::WxString`] is the minimum co-port of
//! the pinned reference build (wxWidgets 3.2.4, `wxUSE_UNICODE_WCHAR`, Linux 32-bit
//! signed `wchar_t`), including `wxString::FromUTF8(const char*)` through the strict
//! UTF-8 decoder and the `std::less<wxString>` ordering a `std::map` uses.
//!
//! `std::map<K, V>` is modelled by `alloc::collections::BTreeMap<K, V>`, whose `Ord`
//! must be the C++ `std::less<K>` order. Every copy of a key, value or default is a
//! fallible [`TryClone`]; allocation failure is a typed error instead of the C++
//! `std::bad_alloc` exception, and never an abort or a default result.

extern crate alloc;

pub mod map_helpers;
pub mod wx_string;

pub use map_helpers::{
    get_def, get_def_cstr, get_def_cstr_wx, get_def_or_value_init, get_def_wx, get_def_wx_or_empty,
    get_opt, get_opt_cstr, get_opt_wx, TryClone, GET_DEF_CSTR_DEFAULT,
};
pub use wx_string::{WChar, WxError, WxString, MAX_UNITS};

/// The port unit implemented by this crate.
pub const UNIT: &str = "libs.core.include.core.application.thin.replacement.p001";
