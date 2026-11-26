// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.

//! `wxString TowxString( const std::string_view& view )` (`libs/core/wx_stl_compat.cpp:42-45`):
//! `return wxString( view.data(), view.length() );`.
//!
//! On the pinned reference build (wxWidgets 3.2.4, `wxUSE_UNICODE_WCHAR`, Linux) that
//! constructor is `assign( psz, nLength )`, i.e. `ImplStr( psz, n )` with the default
//! converter argument `wxConvLibc` (`wxSTRING_DEFAULT_CONV_ARG`), which is
//! `wxString::ConvertStr( psz, n, wxConvLibc )` (`src/common/string.cpp`):
//!
//! 1. a null pointer or a zero length gives the empty string;
//! 2. otherwise `wxMBConv::cMB2WC( psz, n, &wcLen )` sizes and converts with the base class
//!    `wxMBConv::ToWChar`, because `wxMBConvLibc` only implements `MB2WC`. For an explicit
//!    length that function converts the input chunk by chunk, each chunk running to the next
//!    NUL byte (or to an appended terminator), through `wxMBConvLibc::MB2WC`, i.e. `wxMB2WC`,
//!    i.e. `mbsrtowcs` in the C library's current `LC_CTYPE` locale. Every NUL byte inside the
//!    view, a trailing one included, becomes a 0 unit and is counted; the appended terminator
//!    is not;
//! 3. any chunk the C library rejects fails the whole conversion (`wxCONV_FAILED`), and
//!    `ConvertStr` then gives the empty string, as it does for a conversion of zero units.
//!
//! So unlike `wxString::FromUTF8( const char* )`, conversion does not stop at a NUL and uses
//! the C library's decoder, not wxWidgets' strict UTF-8 one. These paths are identical in
//! wxWidgets 3.2.4 and 3.2.9.
//!
//! The locale is an explicit input ([`LibcCtype`]), never read from a global. Its decoder is
//! glibc's (`iconv/gconv_simple.c`, identical in glibc 2.39 and 2.43 for these paths):
//!
//! - [`LibcCtype::C`]: the `C`/`POSIX` locale, whose charset is `ANSI_X3.4-1968`
//!   (`__gconv_transform_ascii_internal`): bytes above 0x7F are illegal;
//! - [`LibcCtype::Utf8`]: any UTF-8 locale such as `C.UTF-8` (`__gconv_transform_utf8_internal`).
//!   It accepts two- to six-byte forms up to 0x7FFFFFFF, including values above 0x10FFFF, and
//!   rejects lead bytes 0x80-0xC1, 0xFE and 0xFF, overlong three- to six-byte forms, UTF-16
//!   surrogates and truncated or broken sequences. A sequence cut short by a NUL is illegal,
//!   because `mbsrtowcs` converts the chunk with its NUL terminator, which is never a
//!   continuation byte.
//!
//! Other locales and C libraries are not modelled.

use alloc::vec::Vec;

use kicad_map_helpers::wx_string::MAX_UNITS;

use crate::{WChar, WxError, WxString};

/// The `LC_CTYPE` locale of the C library when `TowxString` runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibcCtype {
    /// glibc's `C` (or `POSIX`) locale: ASCII only.
    C,
    /// A glibc UTF-8 locale, e.g. `C.UTF-8`.
    Utf8,
}

/// The value bits of a UTF-8 lead byte and its sequence length, as glibc's
/// `utf8_internal_loop` classifies it; `None` for an illegal lead byte (0x80-0xC1, 0xFE,
/// 0xFF).
const fn utf8_lead(lead: u8) -> Option<(usize, u32)> {
    let ch = lead as u32;
    if ch >= 0xC2 && ch < 0xE0 {
        Some((2, ch & 0x1F))
    } else if ch & 0xF0 == 0xE0 {
        Some((3, ch & 0x0F))
    } else if ch & 0xF8 == 0xF0 {
        Some((4, ch & 0x07))
    } else if ch & 0xFC == 0xF8 {
        Some((5, ch & 0x03))
    } else if ch & 0xFE == 0xFC {
        Some((6, ch & 0x01))
    } else {
        None
    }
}

/// `mbsrtowcs` of one NUL-free chunk (its NUL terminator implied) in `ctype`: the number of
/// wide characters, each passed to `sink`, or `None` for `(size_t)-1` (`EILSEQ`).
fn convert_chunk(chunk: &[u8], ctype: LibcCtype, sink: &mut impl FnMut(WChar)) -> Option<usize> {
    let mut count = 0usize;
    let mut i = 0usize;
    while let Some(&lead) = chunk.get(i) {
        let value = if lead < 0x80 {
            i += 1;
            u32::from(lead)
        } else {
            if ctype == LibcCtype::C {
                return None;
            }
            let (length, mut value) = utf8_lead(lead)?;
            // Every trail byte must be 10xxxxxx; the implied terminator never is.
            for k in 1..length {
                let byte = *chunk.get(i + k)?;
                if byte & 0xC0 != 0x80 {
                    return None;
                }
                value = (value << 6) | u32::from(byte & 0x3F);
            }
            // Overlong forms of three or more bytes, and UTF-16 surrogates.
            let overlong = length > 2 && (value >> (5 * length - 4)) == 0;
            if overlong || (0xD800..=0xDFFF).contains(&value) {
                return None;
            }
            i += length;
            value
        };
        // At most 0x7FFFFFFF: always a nonnegative wchar_t.
        sink(WChar::try_from(value).ok()?);
        count = count.checked_add(1)?;
    }
    Some(count)
}

/// `wxMBConv::ToWChar( dst, dstLen, src, srcLen )` with an explicit nonzero `srcLen` and
/// `GetMBNulLen() == 1`, for the sizing pass (`sink` ignores units) and the conversion pass.
/// Returns the written count, or `None` for `wxCONV_FAILED`.
fn to_wchar(src: &[u8], ctype: LibcCtype, sink: &mut impl FnMut(WChar)) -> Option<usize> {
    let mut written = 0usize;
    let mut pos = 0usize;
    loop {
        // The chunk runs to the next NUL, or to the terminator appended after the input.
        let rest = src.get(pos..)?;
        let end = rest
            .iter()
            .position(|&byte| byte == 0)
            .map_or(src.len(), |offset| pos + offset);
        let chunk = src.get(pos..end)?;
        written = written.checked_add(convert_chunk(chunk, ctype, sink)?)?;
        pos = end;

        // The appended terminator ends the input and is not counted.
        if pos == src.len() {
            break;
        }

        // A NUL inside the input is counted; MB2WC already stored it as the chunk's
        // terminating L'\0'.
        written = written.checked_add(1)?;
        sink(0);
        pos += 1;

        if pos >= src.len() {
            break;
        }
    }
    Some(written)
}

/// `TowxString( view )` with the C library in `ctype`.
///
/// `view` is `None` for a view with a null `data()` (a default-constructed
/// `std::string_view`). A null or empty view, input the C library rejects anywhere, and
/// input converting to no units all give the empty string, as in the reference. The only
/// errors are a refused allocation and a result longer than the port's
/// [`MAX_UNITS`](kicad_map_helpers::wx_string::MAX_UNITS) bound.
pub fn to_wx_string(view: Option<&[u8]>, ctype: LibcCtype) -> Result<WxString, WxError> {
    // ConvertStr: "anything to do?"
    let Some(src) = view.filter(|bytes| !bytes.is_empty()) else {
        return Ok(WxString::new());
    };

    // cMB2WC: the sizing pass. Failure gives *outLen = 0, which ConvertStr turns into "".
    let Some(length) = to_wchar(src, ctype, &mut |_| {}) else {
        return Ok(WxString::new());
    };
    if length == 0 {
        return Ok(WxString::new());
    }
    if length > MAX_UNITS {
        return Err(WxError::CapacityExceeded {
            requested: length,
            limit: MAX_UNITS,
        });
    }

    // The conversion pass into wxWCharBuffer( dstLen ). It converts the same bytes, so it
    // cannot fail or differ; a failure is still the reference's empty result.
    let mut units = Vec::new();
    units
        .try_reserve_exact(length)
        .map_err(|_| WxError::Alloc { requested: length })?;
    if to_wchar(src, ctype, &mut |unit| units.push(unit)) != Some(length) {
        return Ok(WxString::new());
    }

    // m_impl.assign( str.data, str.len ) copies the converted buffer.
    WxString::from_units(&units)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units(bytes: &[u8], ctype: LibcCtype) -> Vec<WChar> {
        to_wx_string(Some(bytes), ctype)
            .map(|text| text.units().to_vec())
            .unwrap_or_default()
    }

    #[test]
    fn embedded_and_trailing_nuls_are_kept() {
        for ctype in [LibcCtype::C, LibcCtype::Utf8] {
            assert_eq!(units(b"ab\0cd", ctype), [0x61, 0x62, 0, 0x63, 0x64]);
            assert_eq!(units(b"ab\0", ctype), [0x61, 0x62, 0]);
            assert_eq!(units(b"\0", ctype), [0]);
            assert_eq!(units(b"\0\0a\0", ctype), [0, 0, 0x61, 0]);
            assert!(units(b"", ctype).is_empty());
            assert!(matches!(to_wx_string(None, ctype), Ok(text) if text.is_empty()));
        }
    }

    #[test]
    fn c_locale_rejects_every_high_byte() {
        assert!(units(b"abc\x80", LibcCtype::C).is_empty());
        assert!(units(b"\xC3\xA9", LibcCtype::C).is_empty());
        assert_eq!(units(b"\x7F", LibcCtype::C), [0x7F]);
    }

    #[test]
    fn utf8_follows_glibc() {
        let utf8 = |bytes: &[u8]| units(bytes, LibcCtype::Utf8);
        assert_eq!(utf8(b"\xC3\xA9"), [0xE9]);
        assert_eq!(utf8(b"\xF4\x90\x80\x80"), [0x110000]);
        assert_eq!(utf8(b"\xF8\x88\x80\x80\x80"), [0x200000]);
        assert_eq!(utf8(b"\xFD\xBF\xBF\xBF\xBF\xBF"), [0x7FFF_FFFF]);
        assert_eq!(utf8(b"\xEF\xBF\xBE"), [0xFFFE]);
        for bad in [
            &b"\xC0\x80"[..],
            b"\xC1\xBF",
            b"\xE0\x80\x80",
            b"\xF0\x8F\xBF\xBF",
            b"\xED\xA0\x80",
            b"\xED\xBF\xBF",
            b"\xFE",
            b"\xFF",
            b"\x80",
            b"a\xE2\x82",
            b"\xE2\x82\0\xAC",
            b"ok\0\xC3",
        ] {
            assert!(utf8(bad).is_empty(), "{bad:?}");
        }
    }
}
