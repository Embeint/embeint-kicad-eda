// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2021 Ola Rinta-Koski
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
#![no_std]
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::indexing_slicing
)]

//! Portable port of KiCad's font layer at pin 4cf939778df04d2b747bb2c4714fc5014a7565c7.
//!
//! This crate is the L1 font crate of `rust/PORT_PLAN.md`. It starts with unit
//! `include.font.application.ui.p001`, the inline bodies of `include/font/font.h` and
//! `include/font/outline_font.h`:
//!
//! - [`text_style`]: `TEXT_STYLE`, `TEXT_STYLE_FLAGS`, `ITALIC_TILT` and the four flag
//!   predicates;
//! - [`font`]: `KIFONT::FONT` as the object-safe [`font::Font`] trait (its four virtual style
//!   queries with their defaults, `GetName`, `NameAsToken`, `linesCount`, the cursorless
//!   `Draw` overload) and [`font::FontBase`], the members every font carries;
//! - [`font_stream`]: both `operator<<` of `font.h`, writing into the accepted byte-exact
//!   `std::ostream` model of `kicad-wx-stl-compat`, with the wxWidgets string insertion
//!   (`wxConvWhateverWorks`) they reach;
//! - [`outline_font`]: `KIFONT::OUTLINE_FONT`'s inline members and constants.
//!
//! Fonts are kept out of `kicad-core` (which the plan lists for L1 too) so that `libs/core`
//! consumers do not pull the math, wxString and stream models, and so the FreeType/HarfBuzz
//! host boundaries of the later outline font units stay in one crate. The crate does no I/O
//! and links no FreeType: a face is the [`outline_font::FtFace`] value a host supplies.
//!
//! Out-of-line bodies (`common/font/font.cpp`, `common/font/outline_font.cpp`) belong to the
//! units named in [`FONT_CPP_UNIT`] and [`OUTLINE_FONT_CPP_UNIT`]; where an owned inline body
//! calls one, the call returns [`FontError::Unported`] naming that owner.

extern crate alloc;

pub mod font;
pub mod font_stream;
pub mod outline_font;
pub mod text_style;

/// The unit this crate ports so far.
pub const UNIT: &str = "include.font.application.ui.p001";

/// The unit owning `common/font/font.cpp` (`FONT::Draw`, `FONT::FONT`, ...).
pub const FONT_CPP_UNIT: &str = "common.font.application.ui.p001";

/// The unit owning `common/font/outline_font.cpp` (`OUTLINE_FONT::OUTLINE_FONT`, `LoadFont`,
/// `GetInterline`, ...).
pub const OUTLINE_FONT_CPP_UNIT: &str = "common.font.application.ui.p002";

/// The unit record's `rust_module`.
pub mod include_font {
    pub use crate::{font, font_stream, outline_font, text_style};
}

/// Why a font operation did not produce the reference result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontError {
    /// The pinned C++ evaluation is undefined for this input (signed overflow, an
    /// out-of-range floating-to-integer conversion, a dangling or null pointer). No result
    /// exists to reproduce.
    Undefined {
        upstream: &'static str,
        reason: &'static str,
    },
    /// The body calls a function whose port is not available yet.
    Unported {
        upstream: &'static str,
        unit: &'static str,
    },
}

impl core::fmt::Display for FontError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Undefined { upstream, reason } => {
                write!(formatter, "{upstream} is undefined here: {reason}")
            }
            Self::Unported { upstream, unit } => {
                write!(formatter, "{upstream} is unported ({unit})")
            }
        }
    }
}
