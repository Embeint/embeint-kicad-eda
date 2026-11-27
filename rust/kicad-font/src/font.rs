// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2021 Ola Rinta-Koski
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
//! `KIFONT::FONT` (`include/font/font.h:88-292`), the abstract base of stroke and outline
//! fonts, as the object-safe [`Font`] trait.
//!
//! Ported inline bodies: the four virtual style queries with their `false` defaults
//! (`IsStroke`, `IsOutline`, `IsBold`, `IsItalic`), `GetName`, `NameAsToken`, the protected
//! `linesCount`, the virtual destructor (nothing to do: dropping the members) and the `Draw`
//! overload without a cursor, which forwards to the out-of-line `Draw` ([`draw_at_cursor`],
//! a typed deferral).
//!
//! The declared pure virtuals `GetSuperSubSizeMultiplier` and `GetSuperSubBaselineOffset` are
//! trait methods. `GetInterline( double, const METRICS& )` and `GetTextAsGlyphs( ... )` are
//! declared in the header too, but their parameter types (`METRICS`, `GLYPH`) belong to unit
//! `include.font.application.pure.p001`, which is not ported; the trait gains them with that
//! unit. The other declared members (`GetFont`, `IsStroke( const wxString& )`,
//! `StringBoundaryLimits`, `GetAlignedDrawPosition`, `LinebreakText`, `drawSingleLineText`,
//! `boundingBoxSingleLine`, `getLinePositions`, `drawMarkup`, `wordbreakMarkup`,
//! `getDefaultFont` and the static font cache) are out-of-line bodies of `font.cpp`, unit
//! [`FONT_CPP_UNIT`].

use alloc::vec::Vec;

use kicad_map_helpers::wx_string::{WChar, WxString};
use kicad_math::Vector2I;

use crate::text_style::TextStyleFlags;
use crate::{FontError, FONT_CPP_UNIT};

/// The out-of-line `FONT::Draw` with a cursor, which the inline overload calls.
pub const DRAW_UPSTREAM: &str = "KIFONT::FONT::Draw(KIGFX::GAL*, const wxString&, const VECTOR2I&, \
     const VECTOR2I&, const TEXT_ATTRIBUTES&, const METRICS&, std::optional<VECTOR2I>, wxString*) const";

/// `FONT::NameAsToken`.
pub const NAME_AS_TOKEN_UPSTREAM: &str = "KIFONT::FONT::NameAsToken() const";

/// The members every font carries: `m_fontName` and `m_fontFileName` (protected in C++, set
/// by the out-of-line loaders).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct FontBase {
    /// `m_fontName`.
    pub font_name: WxString,
    /// `m_fontFileName`.
    pub font_file_name: WxString,
}

impl FontBase {
    /// `FONT::FONT()` (`font.cpp:128`, unit [`FONT_CPP_UNIT`]): an empty body, so both names
    /// are default-constructed empty `wxString`s. Co-ported here as the minimum needed to
    /// construct a font; the rest of `font.cpp` is not.
    pub const fn new() -> Self {
        Self {
            font_name: WxString::new(),
            font_file_name: WxString::new(),
        }
    }
}

/// `KIFONT::FONT`. Implementors are the concrete fonts (`OUTLINE_FONT`, later `STROKE_FONT`).
///
/// The provided `is_*` methods are the virtual functions' default bodies; an implementor
/// overrides them as the C++ subclass does. `get_name`, `name_as_token` and `lines_count`
/// are not virtual in C++: implementors must not override them.
pub trait Font {
    /// The font's base members.
    fn font_base(&self) -> &FontBase;

    /// `virtual bool IsStroke() const { return false; }`.
    fn is_stroke(&self) -> bool {
        false
    }

    /// `virtual bool IsOutline() const { return false; }`.
    fn is_outline(&self) -> bool {
        false
    }

    /// `virtual bool IsBold() const { return false; }`.
    fn is_bold(&self) -> bool {
        false
    }

    /// `virtual bool IsItalic() const { return false; }`.
    fn is_italic(&self) -> bool {
        false
    }

    /// `virtual double GetSuperSubSizeMultiplier() const = 0`.
    fn get_super_sub_size_multiplier(&self) -> f64;

    /// `virtual double GetSuperSubBaselineOffset( TEXT_STYLE_FLAGS ) const = 0`.
    fn get_super_sub_baseline_offset(&self, text_style: TextStyleFlags) -> f64;

    /// `const wxString& GetName() const { return m_fontName; }`.
    fn get_name(&self) -> &WxString {
        &self.font_base().font_name
    }

    /// `const char* NameAsToken() const { return GetName().utf8_str().data(); }`.
    ///
    /// `utf8_str()` returns a `wxScopedCharBuffer` temporary owning the converted bytes (the
    /// pinned wchar_t build converts into a new buffer); it is destroyed at the end of the
    /// return statement, so the returned pointer dangles and every use of it is undefined
    /// (`port/upstream-issues/font-name-as-token-dangling-buffer.md`). There is no reference
    /// result to reproduce, so this always refuses.
    fn name_as_token(&self) -> Result<Vec<u8>, FontError> {
        Err(FontError::Undefined {
            upstream: NAME_AS_TOKEN_UPSTREAM,
            reason: "returns a pointer into a destroyed temporary buffer",
        })
    }

    /// `unsigned linesCount( const wxString& aText ) const` (protected): 0 for an empty
    /// string, otherwise one more than the number of `'\n'` units before the last unit (a
    /// trailing newline does not start a line).
    fn lines_count(&self, text: &WxString) -> u32 {
        lines_count(text)
    }
}

/// The body of `FONT::linesCount`, which reads no member.
pub fn lines_count(text: &WxString) -> u32 {
    let Some((_, head)) = text.units().split_last() else {
        // std::count does not work well with empty strings.
        return 0;
    };
    const NEWLINE: WChar = 0x0A;
    let count = head.iter().filter(|&&unit| unit == NEWLINE).count();
    // std::count's ptrdiff_t + 1, converted to unsigned: modulo 2^32.
    (count as u64).wrapping_add(1) as u32
}

/// The cursorless overload `FONT::Draw( aGal, aText, aPosition, aAttributes, aFontMetrics,
/// aMousePos = std::nullopt, aActiveUrl = nullptr )`: calls the full `Draw` with the cursor
/// `VECTOR2I( 0, 0 )` and every other argument unchanged.
///
/// `GAL`, `TEXT_ATTRIBUTES` and `METRICS` belong to units that are not ported, so they are
/// type parameters passed through unchanged; `None` is a null `GAL*` or `wxString*`.
#[allow(clippy::too_many_arguments)]
pub fn draw<F, G, A, M>(
    font: &F,
    gal: Option<&mut G>,
    text: &WxString,
    position: Vector2I,
    attributes: &A,
    font_metrics: &M,
    mouse_pos: Option<Vector2I>,
    active_url: Option<&mut WxString>,
) -> Result<(), FontError>
where
    F: Font + ?Sized,
    G: ?Sized,
    A: ?Sized,
    M: ?Sized,
{
    draw_at_cursor(
        font,
        gal,
        text,
        position,
        Vector2I::new(0, 0),
        attributes,
        font_metrics,
        mouse_pos,
        active_url,
    )
}

/// `FONT::Draw( aGal, aText, aPosition, aCursor, aAttributes, aFontMetrics, aMousePos,
/// aActiveUrl )`, out-of-line in `font.cpp` (unit [`FONT_CPP_UNIT`]).
#[allow(clippy::too_many_arguments)]
pub fn draw_at_cursor<F, G, A, M>(
    _font: &F,
    _gal: Option<&mut G>,
    _text: &WxString,
    _position: Vector2I,
    _cursor: Vector2I,
    _attributes: &A,
    _font_metrics: &M,
    _mouse_pos: Option<Vector2I>,
    _active_url: Option<&mut WxString>,
) -> Result<(), FontError>
where
    F: Font + ?Sized,
    G: ?Sized,
    A: ?Sized,
    M: ?Sized,
{
    // UNPORTED(common.font.application.ui.p001): KIFONT::FONT::Draw
    Err(FontError::Unported {
        upstream: DRAW_UPSTREAM,
        unit: FONT_CPP_UNIT,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kicad_map_helpers::wx_string::WxError;

    struct Plain(FontBase);

    impl Font for Plain {
        fn font_base(&self) -> &FontBase {
            &self.0
        }
        fn get_super_sub_size_multiplier(&self) -> f64 {
            0.0
        }
        fn get_super_sub_baseline_offset(&self, _: TextStyleFlags) -> f64 {
            0.0
        }
    }

    fn text(units: &[WChar]) -> Result<WxString, WxError> {
        WxString::from_units(units)
    }

    #[test]
    fn defaults_and_name() -> Result<(), WxError> {
        let mut base = FontBase::new();
        base.font_name = text(&[0x4B, 0])?;
        let font = Plain(base);
        assert!(!font.is_stroke() && !font.is_outline() && !font.is_bold() && !font.is_italic());
        assert_eq!(font.get_name().units(), &[0x4B, 0]);
        assert!(matches!(
            font.name_as_token(),
            Err(FontError::Undefined { .. })
        ));
        Ok(())
    }

    #[test]
    fn lines_count_skips_the_last_unit() -> Result<(), WxError> {
        let font = Plain(FontBase::new());
        assert_eq!(font.lines_count(&text(&[])?), 0);
        assert_eq!(font.lines_count(&text(&[10])?), 1);
        assert_eq!(font.lines_count(&text(&[10, 10])?), 2);
        assert_eq!(font.lines_count(&text(&[0x61, 10, 0x62])?), 2);
        assert_eq!(font.lines_count(&text(&[10, 0, 10, 13])?), 3);
        Ok(())
    }

    #[test]
    fn draw_forwards_to_the_deferred_body() -> Result<(), WxError> {
        let font = Plain(FontBase::new());
        let result = draw::<_, (), (), ()>(
            &font,
            None,
            &text(&[0x78])?,
            Vector2I::new(1, 2),
            &(),
            &(),
            None,
            None,
        );
        assert_eq!(
            result,
            Err(FontError::Unported {
                upstream: DRAW_UPSTREAM,
                unit: FONT_CPP_UNIT
            })
        );
        Ok(())
    }
}
