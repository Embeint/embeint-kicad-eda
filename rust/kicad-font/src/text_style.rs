// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2021 Ola Rinta-Koski
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
//! The text style flags of `include/font/font.h` (lines 41-85).

/// `enum TEXT_STYLE`: one bit per style.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum TextStyle {
    Bold = 1,
    Italic = 1 << 1,
    Subscript = 1 << 2,
    Superscript = 1 << 3,
    Overbar = 1 << 4,
    Underline = 1 << 5,
}

impl TextStyle {
    /// Every enumerator in declaration order.
    pub const ALL: [Self; 6] = [
        Self::Bold,
        Self::Italic,
        Self::Subscript,
        Self::Superscript,
        Self::Overbar,
        Self::Underline,
    ];

    /// The enumerator's value.
    pub const fn bits(self) -> TextStyleFlags {
        self as TextStyleFlags
    }
}

/// `using TEXT_STYLE_FLAGS = unsigned int`.
pub type TextStyleFlags = u32;

/// `ITALIC_TILT`: the scaling factor on relative dY coordinates that tilts stroke glyphs, and
/// the estimate used for outline fonts. `1.0 / 8` is exact.
pub const ITALIC_TILT: f64 = 1.0 / 8.0;

/// `IsBold( TEXT_STYLE_FLAGS )`: `aFlags & TEXT_STYLE::BOLD`, as a bool.
pub const fn is_bold(flags: TextStyleFlags) -> bool {
    flags & TextStyle::Bold.bits() != 0
}

/// `IsItalic( TEXT_STYLE_FLAGS )`.
pub const fn is_italic(flags: TextStyleFlags) -> bool {
    flags & TextStyle::Italic.bits() != 0
}

/// `IsSuperscript( TEXT_STYLE_FLAGS )`.
pub const fn is_superscript(flags: TextStyleFlags) -> bool {
    flags & TextStyle::Superscript.bits() != 0
}

/// `IsSubscript( TEXT_STYLE_FLAGS )`.
pub const fn is_subscript(flags: TextStyleFlags) -> bool {
    flags & TextStyle::Subscript.bits() != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predicates_test_one_bit_each() {
        assert_eq!(TextStyle::ALL.map(TextStyle::bits), [1, 2, 4, 8, 16, 32]);
        assert!(is_bold(1) && !is_bold(2) && is_bold(u32::MAX));
        assert!(is_italic(2) && !is_italic(1));
        assert!(is_subscript(4) && !is_subscript(8));
        assert!(is_superscript(8) && !is_superscript(4));
        assert!(!is_bold(0x30) && !is_italic(0x30));
        assert_eq!(ITALIC_TILT.to_bits(), 0x3fc0_0000_0000_0000);
    }
}
