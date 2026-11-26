// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
#![no_std]
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

//! Portable port of `libs/core/wx_stl_compat.cpp` at KiCad pin
//! 4cf939778df04d2b747bb2c4714fc5014a7565c7 (unit
//! `libs.core.application.thin.replacement.p001`).
//!
//! The source defines four functions, each ported here:
//!
//! - `std::less<wxPoint>::operator()` ([`less::less_wx_point`]), compiled because the header
//!   defines `USE_KICAD_WXPOINT_LESS_AND_HASH` unconditionally. [`less::WxPointKey`] gives a
//!   `BTreeMap` the order a `std::map<wxPoint, T>` uses.
//! - `TowxString( const std::string_view& )` ([`to_wx_string::to_wx_string`]): the explicit
//!   length narrow `wxString` constructor, which converts through `wxConvLibc`, i.e. the C
//!   library's `mbsrtowcs` in the current `LC_CTYPE` locale. The locale is an explicit input.
//! - `operator<<( std::ostream&, const wxSize& )` and `operator<<( std::ostream&, const
//!   wxPoint& )` ([`ostream::insert_wx_size`], [`ostream::insert_wx_point`]), which insert into
//!   the caller's stream; [`ostream::OStream`] models that stream's state byte for byte, and
//!   [`ostream::StreamHost`] the caller's buffer, `num_put` facet and tie, whose operations
//!   may throw.
//!
//! The `USE_KICAD_WXSTRING_HASH` body (`std::hash<wxString>::operator()`) is not compiled in
//! the audited configuration: nothing in the pinned tree or its build defines the macro, and
//! wxWidgets 3.2 supplies `std::hash<wxString>` itself (`wx/string.h`, under
//! `wxUSE_STD_STRING`). It is not ported; a build that defines the macro itself would compile
//! a body this crate does not claim.
//!
//! The wx value types are reused from accepted ports: `WxString` from `kicad-map-helpers`
//! (wxWidgets 3.2.4 `wxUSE_UNICODE_WCHAR`, 32-bit signed `wchar_t` units) and `WxPoint` /
//! `WxSize` from `kicad-math`.

extern crate alloc;

pub mod less;
pub mod ostream;
pub mod to_wx_string;

/// The four ported bodies of `libs/core/wx_stl_compat.cpp`, under the unit's module path.
pub mod wx_stl_compat {
    pub use crate::less::less_wx_point;
    pub use crate::ostream::{insert_wx_point, insert_wx_size};
    pub use crate::to_wx_string::to_wx_string;
}

pub use kicad_map_helpers::wx_string::{WChar, WxError, WxString};
pub use kicad_math::vector2wx::{WxPoint, WxSize};

/// The unit this crate ports.
pub const UNIT: &str = "libs.core.application.thin.replacement.p001";
