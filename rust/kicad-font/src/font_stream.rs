// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2021 Ola Rinta-Koski
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
//! The two `operator<<` of `include/font/font.h:297-310`:
//!
//! ```text
//! os << "[Font \"" << aFont.GetName() << "\"" << ( aFont.IsStroke() ? " stroke" : "" )
//!    << ( aFont.IsOutline() ? " outline" : "" ) << ( aFont.IsBold() ? " bold" : "" )
//!    << ( aFont.IsItalic() ? " italic" : "" ) << "]";
//! ```
//!
//! and the pointer overload `os << *aFont`. The stream is the accepted byte-exact
//! `std::ostream` model ([`OStream`]) of `kicad-wx-stl-compat`; every one of the eight
//! insertions goes through its own sentry, so the width pads only the first and an empty
//! literal still syncs a `unitbuf` stream. C++17 sequences the operands of the chain left to
//! right, so each virtual query runs after the insertions before it; the queries cannot touch
//! the stream.
//!
//! The name goes through wxWidgets 3.2.4's own `operator<<( std::ostream&, const wxString& )`
//! (`src/common/string.cpp`, wchar_t build): `os << str.c_str()` and then
//! `os << wxConvWhateverWorks.cWX2MB( str )`, which converts the units up to the first NUL
//! unit ([`whatever_works_wc2mb`]) and inserts the buffer's `data()`: bytes up to their first
//! NUL, or, for a null buffer, `setstate( badbit )`.

use alloc::vec::Vec;

use kicad_map_helpers::wx_string::WxString;
use kicad_wx_stl_compat::ostream::{
    check_resources, CallContext, InsertError, OStream, StreamHost,
};

use crate::font::Font;
use crate::{FontError, UNIT};

/// `wxOnAssert`, reached by `wxFAIL_MSG` in `wxMBConvStrictUTF8::FromWChar`.
pub const WX_ON_ASSERT: &str = "wxOnAssert";

/// Why a font insertion stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FontStreamError<E> {
    /// The stream threw, terminated or refused (see [`InsertError`]); it keeps what was done.
    Stream(InsertError<E>),
    /// A null `FONT*` (undefined), or the deferred `wxOnAssert` of the name conversion.
    Font(FontError),
    /// The converted name could not be allocated (the reference would throw
    /// `std::bad_alloc`); the stream keeps what was done.
    Alloc,
}

impl<E> From<InsertError<E>> for FontStreamError<E> {
    fn from(error: InsertError<E>) -> Self {
        Self::Stream(error)
    }
}

/// `wxConvWhateverWorks.cWX2MB( const wchar_t* )` in the classic "C" locale, the locale the
/// reference process runs in (nothing calls `setlocale`), on the string's units: `None` for
/// the null `wxCharBuffer` of a failed conversion, else the bytes without the terminator.
///
/// `cWC2MB( in )` is `DoConvertWC2MB( in, wxNO_LEN )`, so conversion stops at the first NUL
/// unit. `wxWhateverWorksConv::FromWChar` tries `wxConvLibc` first: `wcsrtombs` in the C
/// locale (glibc's ASCII charset) converts units 0x01-0x7F to one byte each and fails on any
/// other. It then tries `wxConvUTF8` (`wxMBConvStrictUTF8::FromWChar`): each unit masked with
/// 0x7FFFFFFF is encoded in one to four bytes up to 0x10FFFF, surrogates included (so a unit
/// 0x80000000 becomes a NUL byte); a larger code reaches `wxFAIL_MSG( "trying to encode
/// undefined Unicode character" )`, which calls `on_assert`, and then fails the conversion.
///
/// `on_assert` stands for the `wxOnAssert` call: an error stops the conversion there; `Ok`
/// continues as the reference does when the assertion handler returns.
pub fn whatever_works_wc2mb(
    text: &WxString,
    on_assert: &mut dyn FnMut() -> Result<(), FontError>,
) -> Result<Option<Vec<u8>>, FontStreamError<core::convert::Infallible>> {
    let units = text.units();
    let end = units
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(units.len());
    let units = units.get(..end).unwrap_or(units);
    let mut bytes = Vec::new();
    // wxConvLibc: wcsrtombs in the C locale.
    if units.iter().all(|&unit| (1..=0x7F).contains(&unit)) {
        bytes
            .try_reserve_exact(units.len())
            .map_err(|_| FontStreamError::Alloc)?;
        bytes.extend(units.iter().map(|&unit| unit as u8));
        return Ok(Some(bytes));
    }
    // wxConvUTF8 = wxMBConvStrictUTF8, sizing pass and conversion pass alike.
    bytes
        .try_reserve_exact(units.len().saturating_mul(4))
        .map_err(|_| FontStreamError::Alloc)?;
    for &unit in units {
        // wchar_t is UTF-32 here: code = *wp++ & 0x7fffffff.
        let code = u32::from_ne_bytes(unit.to_ne_bytes()) & 0x7FFF_FFFF;
        if code <= 0x7F {
            bytes.push(code as u8);
        } else if code <= 0x07FF {
            bytes.push(0xC0 | (code >> 6) as u8);
            bytes.push(0x80 | (code & 0x3F) as u8);
        } else if code <= 0xFFFF {
            bytes.push(0xE0 | (code >> 12) as u8);
            bytes.push(0x80 | ((code >> 6) & 0x3F) as u8);
            bytes.push(0x80 | (code & 0x3F) as u8);
        } else if code <= 0x10_FFFF {
            bytes.push(0xF0 | (code >> 18) as u8);
            bytes.push(0x80 | ((code >> 12) & 0x3F) as u8);
            bytes.push(0x80 | ((code >> 6) & 0x3F) as u8);
            bytes.push(0x80 | (code & 0x3F) as u8);
        } else {
            // wxFAIL_MSG, then break: "we only get here if an error occurs".
            on_assert().map_err(FontStreamError::Font)?;
            return Ok(None);
        }
    }
    Ok(Some(bytes))
}

/// The production assertion: `wxOnAssert` is a wx host effect no unit ports.
fn defer_assert() -> Result<(), FontError> {
    // UNPORTED(include.font.application.ui.p001): wxOnAssert
    Err(FontError::Unported {
        upstream: WX_ON_ASSERT,
        unit: UNIT,
    })
}

/// wxWidgets' `operator<<( std::ostream&, const wxString& )`, as described in the module
/// documentation.
pub fn insert_wx_string<H: StreamHost>(
    out: &mut OStream<H>,
    text: &WxString,
    context: CallContext,
    on_assert: &mut dyn FnMut() -> Result<(), FontError>,
) -> Result<(), FontStreamError<H::Exception>> {
    let buffer = match whatever_works_wc2mb(text, on_assert) {
        Ok(buffer) => buffer,
        Err(FontStreamError::Font(error)) => return Err(FontStreamError::Font(error)),
        Err(_) => return Err(FontStreamError::Alloc),
    };
    out.insert_c_str(buffer.as_deref(), context)?;
    Ok(())
}

/// `std::ostream& operator<<( std::ostream& os, const KIFONT::FONT& aFont )`, called in
/// `context`, with `wxOnAssert` deferred.
pub fn insert_font<'a, H: StreamHost, F: Font + ?Sized>(
    out: &'a mut OStream<H>,
    font: &F,
    context: CallContext,
) -> Result<&'a mut OStream<H>, FontStreamError<H::Exception>> {
    insert_font_with(out, font, context, &mut defer_assert)
}

/// [`insert_font`] with the `wxOnAssert` call supplied by the caller.
pub fn insert_font_with<'a, H: StreamHost, F: Font + ?Sized>(
    out: &'a mut OStream<H>,
    font: &F,
    context: CallContext,
    on_assert: &mut dyn FnMut() -> Result<(), FontError>,
) -> Result<&'a mut OStream<H>, FontStreamError<H::Exception>> {
    check_resources(out)?;
    out.insert_c_str(Some(b"[Font \""), context)?;
    insert_wx_string(out, font.get_name(), context, on_assert)?;
    out.insert_c_str(Some(b"\""), context)?;
    let stroke: &[u8] = if font.is_stroke() { b" stroke" } else { b"" };
    out.insert_c_str(Some(stroke), context)?;
    let outline: &[u8] = if font.is_outline() { b" outline" } else { b"" };
    out.insert_c_str(Some(outline), context)?;
    let bold: &[u8] = if font.is_bold() { b" bold" } else { b"" };
    out.insert_c_str(Some(bold), context)?;
    let italic: &[u8] = if font.is_italic() { b" italic" } else { b"" };
    out.insert_c_str(Some(italic), context)?;
    out.insert_c_str(Some(b"]"), context)?;
    Ok(out)
}

/// `std::ostream& operator<<( std::ostream& os, const KIFONT::FONT* aFont )`: `os << *aFont`.
/// A null pointer (`None`) is dereferenced, which is undefined; it is refused before any
/// change to the stream.
pub fn insert_font_ptr<'a, H: StreamHost, F: Font + ?Sized>(
    out: &'a mut OStream<H>,
    font: Option<&F>,
    context: CallContext,
) -> Result<&'a mut OStream<H>, FontStreamError<H::Exception>> {
    let Some(font) = font else {
        return Err(FontStreamError::Font(FontError::Undefined {
            upstream: "operator<<(std::ostream&, const KIFONT::FONT*)",
            reason: "dereferences a null FONT pointer",
        }));
    };
    insert_font(out, font, context)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::FontBase;
    use crate::text_style::TextStyleFlags;
    use kicad_map_helpers::wx_string::{WChar, WxError};
    use kicad_wx_stl_compat::ostream::{IoState, StreamBuf, StreamRefusal};

    struct Scripted {
        base: FontBase,
        flags: [bool; 4],
    }

    impl Font for Scripted {
        fn font_base(&self) -> &FontBase {
            &self.base
        }
        fn is_stroke(&self) -> bool {
            self.flags[0]
        }
        fn is_outline(&self) -> bool {
            self.flags[1]
        }
        fn is_bold(&self) -> bool {
            self.flags[2]
        }
        fn is_italic(&self) -> bool {
            self.flags[3]
        }
        fn get_super_sub_size_multiplier(&self) -> f64 {
            0.0
        }
        fn get_super_sub_baseline_offset(&self, _: TextStyleFlags) -> f64 {
            0.0
        }
    }

    #[derive(Debug)]
    enum TestError {
        Wx,
        Stream,
    }
    impl From<WxError> for TestError {
        fn from(_: WxError) -> Self {
            Self::Wx
        }
    }
    impl From<StreamRefusal> for TestError {
        fn from(_: StreamRefusal) -> Self {
            Self::Stream
        }
    }

    fn font(name: &[WChar], flags: [bool; 4]) -> Result<Scripted, WxError> {
        let mut base = FontBase::new();
        base.font_name = WxString::from_units(name)?;
        Ok(Scripted { base, flags })
    }

    fn convert(units: &[WChar]) -> Result<Option<Vec<u8>>, TestError> {
        whatever_works_wc2mb(&WxString::from_units(units)?, &mut defer_assert)
            .map_err(|_| TestError::Stream)
    }

    #[test]
    fn conversion_follows_libc_then_strict_utf8() -> Result<(), TestError> {
        assert_eq!(convert(&[0x61, 0x7F])?, Some(b"a\x7f".to_vec()));
        assert_eq!(convert(&[0x80])?, Some(b"\xc2\x80".to_vec()));
        assert_eq!(convert(&[0xD800])?, Some(b"\xed\xa0\x80".to_vec()));
        assert_eq!(convert(&[0x10FFFF])?, Some(b"\xf4\x8f\xbf\xbf".to_vec()));
        assert_eq!(convert(&[0x61, 0, 0xFF])?, Some(b"a".to_vec()));
        assert_eq!(convert(&[i32::MIN])?, Some(b"\0".to_vec()));
        assert_eq!(convert(&[])?, Some(Vec::new()));
        assert!(convert(&[0x110000]).is_err());
        let mut returned = || Ok(());
        let text = WxString::from_units(&[0x41, -1])?;
        assert_eq!(whatever_works_wc2mb(&text, &mut returned), Ok(None));
        Ok(())
    }

    #[test]
    fn chain_formats_each_query() -> Result<(), TestError> {
        let mut out = OStream::new(StreamBuf::new(None, false)?);
        insert_font(
            &mut out,
            &font(&[0x4E], [true, true, true, true])?,
            CallContext::NORMAL,
        )
        .map_err(|_| TestError::Stream)?;
        assert_eq!(
            out.host().bytes(),
            b"[Font \"N\" stroke outline bold italic]"
        );
        let mut out = OStream::new(StreamBuf::new(None, false)?);
        out.width = 9;
        out.fill = b'*';
        insert_font(&mut out, &font(&[], [false; 4])?, CallContext::NORMAL)
            .map_err(|_| TestError::Stream)?;
        assert_eq!(out.host().bytes(), b"**[Font \"\"]");
        Ok(())
    }

    #[test]
    fn null_buffer_sets_badbit_and_later_insertions_fail() -> Result<(), TestError> {
        let mut out = OStream::new(StreamBuf::new(None, false)?);
        let mut returned = || Ok(());
        let result = insert_font_with(
            &mut out,
            &font(&[0x7FFF_FFFF], [false; 4])?,
            CallContext::NORMAL,
            &mut returned,
        );
        assert!(result.is_ok());
        assert_eq!(out.host().bytes(), b"[Font \"");
        assert_eq!(
            out.rdstate(),
            IoState {
                bad: true,
                eof: false,
                fail: true
            }
        );
        let mut out = OStream::new(StreamBuf::new(None, false)?);
        let result = insert_font(
            &mut out,
            &font(&[0x7FFF_FFFF], [false; 4])?,
            CallContext::NORMAL,
        );
        assert_eq!(
            result.err(),
            Some(FontStreamError::Font(FontError::Unported {
                upstream: WX_ON_ASSERT,
                unit: UNIT
            }))
        );
        Ok(())
    }

    #[test]
    fn null_pointer_is_refused() -> Result<(), TestError> {
        let mut out = OStream::new(StreamBuf::new(None, false)?);
        let result = insert_font_ptr::<_, Scripted>(&mut out, None, CallContext::NORMAL);
        assert!(matches!(
            result,
            Err(FontStreamError::Font(FontError::Undefined { .. }))
        ));
        assert!(out.host().bytes().is_empty());
        Ok(())
    }
}
