// SPDX-License-Identifier: GPL-3.0-or-later
//! The wxWidgets 3.2.4 wide-to-narrow conversions the font header bodies reach, on the pinned
//! `wxUSE_UNICODE_WCHAR` build (32-bit signed `wchar_t` units), in the classic "C" locale the
//! reference process runs in (nothing calls `setlocale`). Sources: `src/common/strconv.cpp`,
//! `src/common/wxcrt.cpp` and `include/wx/string.h` of wxWidgets 3.2.4.
//!
//! - [`strict_utf8`]: `wxMBConvStrictUTF8::FromWChar`, the converter behind both `wxConvUTF8`
//!   and a default `wxMBConvUTF8` (`MAP_INVALID_UTF8_NOT`).
//! - [`whatever_works`]: `wxConvWhateverWorks.cWX2MB( const wchar_t* )`.
//! - [`utf8_str`]: the bytes `wxString::utf8_str().data()` points to.
//!
//! A code above U+10FFFF reaches `wxFAIL_MSG( "trying to encode undefined Unicode character" )`
//! in the strict encoder. The assertion handler is a wx host effect no unit ports; the caller
//! supplies it as `on_assert`: an error stops the conversion there, `Ok` continues as the
//! reference does when the handler returns (the conversion then fails).

use alloc::vec::Vec;

use kicad_map_helpers::wx_string::{WChar, WxString};

use crate::{FontError, UNIT};

/// `wxOnAssert`, reached by `wxFAIL_MSG` in `wxMBConvStrictUTF8::FromWChar`.
pub const WX_ON_ASSERT: &str = "wxOnAssert";

/// Why a conversion did not complete.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConvError {
    /// The assertion handler did not return ([`deferred_assert`] or a caller's error).
    Font(FontError),
    /// The converted bytes could not be allocated (the reference would throw
    /// `std::bad_alloc`).
    Alloc,
}

/// The production `wxOnAssert`: deferred.
pub fn deferred_assert() -> Result<(), FontError> {
    // UNPORTED(include.font.application.ui.p001): wxOnAssert
    Err(FontError::Unported {
        upstream: WX_ON_ASSERT,
        unit: UNIT,
    })
}

/// `wxMBConvStrictUTF8::FromWChar` over exactly `units` (an explicit length, so NUL units are
/// encoded too): each unit masked with 0x7FFFFFFF in one to four bytes up to 0x10FFFF,
/// surrogates included, so a unit 0x80000000 becomes a NUL byte. `None` is `wxCONV_FAILED`,
/// after `on_assert` returned for a larger code.
pub fn strict_utf8(
    units: &[WChar],
    on_assert: &mut dyn FnMut() -> Result<(), FontError>,
) -> Result<Option<Vec<u8>>, ConvError> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(units.len().saturating_mul(4))
        .map_err(|_| ConvError::Alloc)?;
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
            on_assert().map_err(ConvError::Font)?;
            return Ok(None);
        }
    }
    Ok(Some(bytes))
}

/// `wxConvWhateverWorks.cWX2MB( const wchar_t* )` on the string's units: `None` for the null
/// `wxCharBuffer` of a failed conversion, else the bytes without the terminator.
///
/// `cWC2MB( in )` is `DoConvertWC2MB( in, wxNO_LEN )`, so conversion stops at the first NUL
/// unit. `wxWhateverWorksConv::FromWChar` tries `wxConvLibc` first: `wcsrtombs` in the C
/// locale (glibc's ASCII charset) converts units 0x01-0x7F to one byte each and fails on any
/// other; it then tries `wxConvUTF8` ([`strict_utf8`]). The sizing and conversion passes
/// convert the same units, so they agree.
pub fn whatever_works(
    text: &WxString,
    on_assert: &mut dyn FnMut() -> Result<(), FontError>,
) -> Result<Option<Vec<u8>>, ConvError> {
    let units = text.units();
    let end = units
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(units.len());
    let units = units.get(..end).unwrap_or(units);
    if units.iter().all(|&unit| (1..=0x7F).contains(&unit)) {
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(units.len())
            .map_err(|_| ConvError::Alloc)?;
        bytes.extend(units.iter().map(|&unit| unit as u8));
        return Ok(Some(bytes));
    }
    strict_utf8(units, on_assert)
}

/// The NUL-terminated string `wxString::utf8_str().data()` points to, without its NUL.
///
/// In the wchar_t build `utf8_str()` is `mb_str( wxMBConvUTF8() )`, i.e. `AsCharBuf( conv )`:
/// `AsChar( conv )` converts all `length()` units with an explicit length into the string's
/// own mutable cache `m_convertedToChar`, and the returned `wxScopedCharBuffer` is a
/// non-owning view of that cache. If the conversion fails, it is a non-owning view of a
/// static `""`. Either way the pointer outlives the temporary buffer: it stays valid until the
/// string is converted again or destroyed. A reader sees the bytes up to the first NUL byte
/// (from a NUL unit, or a unit 0x80000000).
pub fn utf8_str(
    text: &WxString,
    on_assert: &mut dyn FnMut() -> Result<(), FontError>,
) -> Result<Vec<u8>, ConvError> {
    let Some(mut bytes) = strict_utf8(text.units(), on_assert)? else {
        return Ok(Vec::new());
    };
    if let Some(end) = bytes.iter().position(|&b| b == 0) {
        bytes.truncate(end);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kicad_map_helpers::wx_string::WxError;

    #[derive(Debug)]
    struct Failed;
    impl From<WxError> for Failed {
        fn from(_: WxError) -> Self {
            Self
        }
    }
    impl From<ConvError> for Failed {
        fn from(_: ConvError) -> Self {
            Self
        }
    }

    fn convert(units: &[WChar]) -> Result<Option<Vec<u8>>, Failed> {
        Ok(whatever_works(
            &WxString::from_units(units)?,
            &mut deferred_assert,
        )?)
    }

    #[test]
    fn whatever_works_follows_libc_then_strict_utf8() -> Result<(), Failed> {
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
        assert_eq!(whatever_works(&text, &mut returned), Ok(None));
        Ok(())
    }

    #[test]
    fn utf8_str_converts_past_nul_units() -> Result<(), Failed> {
        let mut returned = || Ok(());
        let text = WxString::from_units(&[0x41, 0xE9, 0, 0x42])?;
        assert_eq!(utf8_str(&text, &mut deferred_assert)?, b"A\xc3\xa9");
        // The strict encoder runs past the NUL unit, so a later invalid code still asserts.
        let text = WxString::from_units(&[0x41, 0, 0x110000])?;
        assert!(utf8_str(&text, &mut deferred_assert).is_err());
        assert_eq!(utf8_str(&text, &mut returned)?, b"");
        let text = WxString::from_units(&[0x41, i32::MIN, 0x42])?;
        assert_eq!(utf8_str(&text, &mut deferred_assert)?, b"A");
        Ok(())
    }
}
