// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2021 Ola Rinta-Koski
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
//! `KIFONT::OUTLINE_FONT` (`include/font/outline_font.h:48-244`): its inline members and
//! constants.
//!
//! Ported inline bodies: `IsOutline`, `IsBold`, `IsItalic`, `IsFakeItalic`, `IsFakeBold`,
//! `SetFakeBold`, `SetFakeItal`, `GetFileName`, `GetSuperSubSizeMultiplier`, `GetEmSize`,
//! `GetSuperSubBaselineOffset`, `GetFace` and the private `faceSize` and `subscriptSize`
//! (public here so the out-of-line port can call them), with `EMBEDDING_PERMISSION` and the
//! class constants.
//!
//! Every other member is out-of-line in `outline_font.cpp` (unit [`OUTLINE_FONT_CPP_UNIT`]):
//! `GetEmbeddingPermission`, `LoadFont`, `GetInterline`, `GetTextAsGlyphs`,
//! `GetLinesAsGlyphs`, `LoadGlyphContours`, `SelectCharmap`, `loadFace`, `getBoundingBox`,
//! `getTextAsGlyphs` and `getTextAsGlyphsUnlocked`; so is the glyph contour cache they fill.
//!
//! The FreeType face is a host resource. [`FtFace`] carries what the inline bodies read from
//! `FT_FaceRec` (`style_flags`); the later port of `loadFace` stores one with
//! [`OutlineFont::set_face`].

use kicad_map_helpers::wx_string::WxString;
use kicad_math::util::ki_round;

use crate::font::{Font, FontBase};
use crate::text_style::{is_subscript, is_superscript, TextStyleFlags};
use crate::FontError;

/// `FT_STYLE_FLAG_ITALIC` (`freetype.h`: `1 << 0`).
pub const FT_STYLE_FLAG_ITALIC: i64 = 1;
/// `FT_STYLE_FLAG_BOLD` (`freetype.h`: `1 << 1`).
pub const FT_STYLE_FLAG_BOLD: i64 = 1 << 1;

/// The `FT_FaceRec` fields the inline bodies read through `m_face`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FtFace {
    /// `FT_Long style_flags` (a 64-bit `long` on the reference platform).
    pub style_flags: i64,
}

/// `OUTLINE_FONT::EMBEDDING_PERMISSION`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum EmbeddingPermission {
    Installable,
    Editable,
    PrintPreviewOnly,
    Restricted,
    Invalid,
}

impl EmbeddingPermission {
    /// Every enumerator in declaration order (values 0 to 4).
    pub const ALL: [Self; 5] = [
        Self::Installable,
        Self::Editable,
        Self::PrintPreviewOnly,
        Self::Restricted,
        Self::Invalid,
    ];
}

/// `m_outlineFontSizeCompensation`: outline fonts are scaled on full height, the stroke font
/// on cap height plus stroke width.
pub const OUTLINE_FONT_SIZE_COMPENSATION: f64 = 1.4;
/// `m_charSizeScaler`: `FT_Set_Char_Size()` takes 1/64ths of a point.
pub const CHAR_SIZE_SCALER: i32 = 64;
/// `m_subscriptSuperscriptSize`.
pub const SUBSCRIPT_SUPERSCRIPT_SIZE: f64 = 0.64;
/// `m_underlineOffsetScaler`.
pub const UNDERLINE_OFFSET_SCALER: f64 = -0.16;
/// `m_subscriptVerticalOffset`.
pub const SUBSCRIPT_VERTICAL_OFFSET: f64 = -0.25;
/// `m_superscriptVerticalOffset`.
pub const SUPERSCRIPT_VERTICAL_OFFSET: f64 = 0.45;

const FACE_SIZE_UPSTREAM: &str = "KIFONT::OUTLINE_FONT::faceSize(int) const";

/// `KIFONT::OUTLINE_FONT`.
#[derive(Debug, PartialEq, Eq)]
pub struct OutlineFont {
    base: FontBase,
    face: Option<FtFace>,
    face_size: i32,
    fake_bold: bool,
    fake_ital: bool,
    for_drawing_sheet: bool,
}

impl Default for OutlineFont {
    fn default() -> Self {
        Self::new()
    }
}

impl OutlineFont {
    /// `OUTLINE_FONT::OUTLINE_FONT()` (`outline_font.cpp:44`, unit
    /// [`OUTLINE_FONT_CPP_UNIT`](crate::OUTLINE_FONT_CPP_UNIT)), co-ported as the minimum
    /// needed to construct the type: `m_face( NULL )`, `m_faceSize( 16 )`, both fake styles
    /// and `m_forDrawingSheet` false, empty names. The constructor also initialises the
    /// process-wide `FT_Library` once under a static mutex; a portable crate holds no
    /// global state or FreeType library, so that initialisation is the host's when it
    /// supplies faces. Nothing an inline body reads depends on it.
    pub const fn new() -> Self {
        Self {
            base: FontBase::new(),
            face: None,
            face_size: 16,
            fake_bold: false,
            fake_ital: false,
            for_drawing_sheet: false,
        }
    }

    /// The protected `FONT` members, which the out-of-line loaders set (`m_fontName`,
    /// `m_fontFileName`).
    pub fn font_base_mut(&mut self) -> &mut FontBase {
        &mut self.base
    }

    /// Store `m_face`, as the out-of-line `loadFace` does (`None` is a null face).
    pub fn set_face(&mut self, face: Option<FtFace>) {
        self.face = face;
    }

    /// `m_forDrawingSheet`, set by the out-of-line `LoadFont`.
    pub const fn for_drawing_sheet(&self) -> bool {
        self.for_drawing_sheet
    }

    /// Set `m_forDrawingSheet`, as the out-of-line `LoadFont` does.
    pub fn set_for_drawing_sheet(&mut self, for_drawing_sheet: bool) {
        self.for_drawing_sheet = for_drawing_sheet;
    }

    /// `bool IsFakeItalic() const { return m_fakeItal; }`.
    pub const fn is_fake_italic(&self) -> bool {
        self.fake_ital
    }

    /// `bool IsFakeBold() const { return m_fakeBold; }`.
    pub const fn is_fake_bold(&self) -> bool {
        self.fake_bold
    }

    /// `void SetFakeBold() { m_fakeBold = true; }`.
    pub fn set_fake_bold(&mut self) {
        self.fake_bold = true;
    }

    /// `void SetFakeItal() { m_fakeItal = true; }`.
    pub fn set_fake_ital(&mut self) {
        self.fake_ital = true;
    }

    /// `const wxString& GetFileName() const { return m_fontFileName; }`.
    pub fn get_file_name(&self) -> &WxString {
        &self.base.font_file_name
    }

    /// `double GetEmSize( double aSize ) const { return aSize * m_outlineFontSizeCompensation; }`:
    /// the edge of the em square for glyphs drawn at text size `size`.
    ///
    /// IEEE 754 fixes every non-NaN product. A NaN `size` is the only NaN operand, and the
    /// reference's x86-64 SSE2 multiplication returns it quieted with its sign and payload;
    /// that is spelled out here so wasm, which leaves NaN results open, gives the same bits.
    pub fn get_em_size(&self, size: f64) -> f64 {
        if size.is_nan() {
            return f64::from_bits(size.to_bits() | 0x0008_0000_0000_0000);
        }
        size * OUTLINE_FONT_SIZE_COMPENSATION
    }

    /// `const FT_Face& GetFace() const { return m_face; }` (`None`: null).
    pub const fn get_face(&self) -> &Option<FtFace> {
        &self.face
    }

    /// `int faceSize( int aSize ) const`:
    /// `return aSize * m_charSizeScaler * m_outlineFontSizeCompensation;`, i.e. the `int`
    /// product `aSize * 64`, converted to `double`, times 1.4, truncated to `int` by the
    /// return. Undefined (refused) when `aSize * 64` overflows `int` or the truncated value is
    /// outside `int`.
    pub fn face_size_of(&self, size: i32) -> Result<i32, FontError> {
        let scaled = size
            .checked_mul(CHAR_SIZE_SCALER)
            .ok_or(FontError::Undefined {
                upstream: FACE_SIZE_UPSTREAM,
                reason: "signed overflow in aSize * m_charSizeScaler",
            })?;
        let value = f64::from(scaled) * OUTLINE_FONT_SIZE_COMPENSATION;
        // A double converts to int when its truncation is representable: -2^31 - 1 < v < 2^31.
        if value > -2_147_483_649.0 && value < 2_147_483_648.0 {
            Ok(value as i32)
        } else {
            Err(FontError::Undefined {
                upstream: FACE_SIZE_UPSTREAM,
                reason: "double to int conversion out of range",
            })
        }
    }

    /// `int faceSize() const { return faceSize( m_faceSize ); }`.
    pub fn face_size(&self) -> Result<i32, FontError> {
        self.face_size_of(self.face_size)
    }

    /// `int subscriptSize( int aSize ) const`:
    /// `return KiROUND( faceSize( aSize ) * m_subscriptSuperscriptSize );`, for superscripts
    /// too. `faceSize` is in `int` range, so the product never reaches KiROUND's overflow
    /// branch.
    pub fn subscript_size_of(&self, size: i32) -> Result<i32, FontError> {
        let face = self.face_size_of(size)?;
        ki_round::<f64, i32>(f64::from(face) * SUBSCRIPT_SUPERSCRIPT_SIZE, false).map_err(|_| {
            FontError::Unported {
                upstream: "kimathLogOverflow",
                unit: "libs.kimath.src.math.util.application.ui.p001",
            }
        })
    }

    /// `int subscriptSize() const { return subscriptSize( m_faceSize ); }`.
    pub fn subscript_size(&self) -> Result<i32, FontError> {
        self.subscript_size_of(self.face_size)
    }
}

impl Font for OutlineFont {
    fn font_base(&self) -> &FontBase {
        &self.base
    }

    /// `bool IsOutline() const override { return true; }`.
    fn is_outline(&self) -> bool {
        true
    }

    /// `return m_face && ( m_fakeBold || ( m_face->style_flags & FT_STYLE_FLAG_BOLD ) );`.
    fn is_bold(&self) -> bool {
        self.face
            .is_some_and(|face| self.fake_bold || face.style_flags & FT_STYLE_FLAG_BOLD != 0)
    }

    /// `return m_face && ( m_fakeItal || ( m_face->style_flags & FT_STYLE_FLAG_ITALIC ) );`.
    fn is_italic(&self) -> bool {
        self.face
            .is_some_and(|face| self.fake_ital || face.style_flags & FT_STYLE_FLAG_ITALIC != 0)
    }

    /// `return m_subscriptSuperscriptSize;`.
    fn get_super_sub_size_multiplier(&self) -> f64 {
        SUBSCRIPT_SUPERSCRIPT_SIZE
    }

    /// The subscript offset when the subscript bit is set, else the superscript offset when
    /// that bit is set, else 0.0; returned as `-offset * m_outlineFontSizeCompensation` (so
    /// a plain run gives -0.0).
    fn get_super_sub_baseline_offset(&self, text_style: TextStyleFlags) -> f64 {
        let mut offset = 0.0;
        if is_subscript(text_style) {
            offset = SUBSCRIPT_VERTICAL_OFFSET;
        } else if is_superscript(text_style) {
            offset = SUPERSCRIPT_VERTICAL_OFFSET;
        }
        // The stored offsets are y-up face units carrying the size compensation.
        -offset * OUTLINE_FONT_SIZE_COMPENSATION
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructed_state() {
        let font = OutlineFont::new();
        assert!(font.is_outline() && !font.is_stroke());
        assert!(!font.is_bold() && !font.is_italic());
        assert_eq!(font.get_face(), &None);
        assert_eq!(font.face_size(), Ok(1433));
        assert_eq!(font.subscript_size(), Ok(917));
        assert!(font.get_name().is_empty() && font.get_file_name().is_empty());
    }

    #[test]
    fn style_needs_a_face() {
        let mut font = OutlineFont::new();
        font.set_fake_bold();
        font.set_fake_ital();
        assert!(font.is_fake_bold() && font.is_fake_italic());
        assert!(!font.is_bold() && !font.is_italic());
        font.set_face(Some(FtFace { style_flags: 0 }));
        assert!(font.is_bold() && font.is_italic());
        let mut font = OutlineFont::new();
        font.set_face(Some(FtFace { style_flags: 3 }));
        assert!(font.is_bold() && font.is_italic());
        font.set_face(Some(FtFace { style_flags: -4 }));
        assert!(!font.is_bold() && !font.is_italic());
    }

    #[test]
    fn sizes_and_offsets() {
        let font = OutlineFont::new();
        assert_eq!(font.get_em_size(2.0), 2.8);
        assert_eq!(
            font.get_super_sub_baseline_offset(0).to_bits(),
            (-0.0f64).to_bits()
        );
        assert_eq!(font.get_super_sub_baseline_offset(12), 0.25 * 1.4);
        assert_eq!(font.get_super_sub_baseline_offset(8), -0.45 * 1.4);
        assert_eq!(
            font.face_size_of(33_554_431),
            Err(FontError::Undefined {
                upstream: FACE_SIZE_UPSTREAM,
                reason: "double to int conversion out of range"
            })
        );
        assert!(font.face_size_of(33_554_432).is_err());
        assert_eq!(font.face_size_of(-1), Ok(-89));
        assert_eq!(font.subscript_size_of(-1), Ok(-57));
    }
}
