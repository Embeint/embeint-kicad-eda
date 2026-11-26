// SPDX-License-Identifier: GPL-3.0-or-later
//! Minimum `wxString` co-port for `map_helpers.h`, targeting wxWidgets 3.2.4
//! (Ubuntu `3.2.4+dfsg-4build1`) with `wxUSE_UNICODE_WCHAR`, where `wxString`
//! stores a `std::wstring` of 32-bit signed `wchar_t` units. Initial local
//! comparison used wxWidgets 3.2.9; target-reference qualification is pending.
//! See `port/findings/core-map-helpers-review-and-reference-addendum.md`.
//!
//! Only what the header uses is ported:
//!
//! - construction from wide units (`wxString( const wchar_t*, size_t )`), which keeps
//!   every unit including embedded NULs, lone surrogates and negative values;
//! - `wxString::FromUTF8( const char* )`, i.e. `wxString( utf8, wxMBConvUTF8(), npos )`
//!   through `wxString::ConvertStr`, `wxMBConv::cMB2WC` and
//!   `wxMBConvStrictUTF8::ToWChar` (`src/common/strconv.cpp`);
//! - the `std::less<wxString>` order: `operator<` is `Cmp() < 0`, `wxString::compare`
//!   is `wxDoCmp` over `wxTmemcmp`, an element-wise signed `wchar_t` comparison with
//!   the shorter string first on a common prefix. The derived `Ord` of the unit
//!   vector is exactly that order.
//!
//! The Windows build (`WC_UTF16`, 16-bit `wchar_t` with surrogate-pair encoding) is not
//! modelled: its units, and which UTF-8 inputs it rejects, differ.

use alloc::vec::Vec;
use core::ffi::CStr;

/// One `wchar_t` of the reference build (Linux: 32-bit signed).
pub type WChar = i32;

/// Port-imposed bound on the units of one string (64 MiB of `wchar_t`). The reference
/// has no such bound beyond `std::wstring::max_size()` and the allocator; a larger
/// string is refused with [`WxError::CapacityExceeded`] before any allocation.
pub const MAX_UNITS: usize = 1 << 24;

/// A refused string allocation. The reference would throw `std::bad_alloc` (or, past
/// [`MAX_UNITS`], normally succeed); the port reports it instead of aborting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WxError {
    /// More than [`MAX_UNITS`] units were requested.
    CapacityExceeded { requested: usize, limit: usize },
    /// The allocator refused space for `requested` units.
    Alloc { requested: usize },
}

/// A `wxString` of the pinned wchar_t build: an owned sequence of `wchar_t` units.
#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WxString {
    units: Vec<WChar>,
}

/// Reserve exactly `count` units, refusing past [`MAX_UNITS`] or on allocator failure.
fn reserve(count: usize) -> Result<Vec<WChar>, WxError> {
    if count > MAX_UNITS {
        return Err(WxError::CapacityExceeded {
            requested: count,
            limit: MAX_UNITS,
        });
    }
    let mut units = Vec::new();
    units
        .try_reserve_exact(count)
        .map_err(|_| WxError::Alloc { requested: count })?;
    Ok(units)
}

/// `tableUtf8Lengths` of `strconv.cpp`: sequence length by lead byte, 0 if invalid
/// (continuation bytes 80..BF, C0 and C1, and F5..FF).
const fn utf8_length(lead: u8) -> u32 {
    match lead {
        0x00..=0x7F => 1,
        0xC2..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,
        _ => 0,
    }
}

/// `wxMBConvStrictUTF8::ToWChar( dst, dstLen, src, wxNO_LEN )` on a NUL-terminated
/// source, for both the sizing pass (`dst == NULL`) and the conversion pass: the
/// explicit length becomes `strlen( src ) + 1`, so the terminating NUL is converted
/// as an ordinary unit and counted. Returns the written count, or `None` for
/// `wxCONV_FAILED`.
///
/// The decoder checks only the lead-byte table and the `10xxxxxx` continuation
/// pattern. Like the reference it accepts overlong three- and four-byte forms
/// (`E0 80 80` is U+0000), UTF-16 surrogates (`ED A0 80` is 0xD800) and values up to
/// 0x13FFFF (`F4 BF BF BF`). On the 32-bit `wchar_t` build every code is stored
/// unchanged.
fn strict_utf8_to_wchar(src: &[u8], mut sink: impl FnMut(WChar)) -> Option<usize> {
    // Masks extracting a lead byte's value bits, by zero-based sequence length.
    const LEAD_VALUE_MASK: [u8; 4] = [0x7F, 0x1F, 0x0F, 0x07];
    let mut remaining = src.len();
    let mut written = 0usize;
    let mut p = 0usize;
    loop {
        if remaining == 0 {
            return Some(written);
        }
        let c = *src.get(p)?;
        let code = if c < 0x80 {
            remaining -= 1;
            u32::from(c)
        } else {
            let len = utf8_length(c);
            if len == 0 {
                return None;
            }
            let len_bytes = usize::try_from(len).ok()?;
            if remaining < len_bytes {
                return None;
            }
            remaining -= len_bytes;
            let mask = *LEAD_VALUE_MASK.get(len_bytes - 1)?;
            let mut value = u32::from(c & mask);
            for _ in 1..len {
                p += 1;
                let next = *src.get(p)?;
                if next & 0xC0 != 0x80 {
                    return None;
                }
                value = (value << 6) | u32::from(next & 0x3F);
            }
            value
        };
        // At most 0x13FFFF (or 0x7F from one byte): always a positive wchar_t.
        sink(WChar::try_from(code).ok()?);
        written = written.checked_add(1)?;
        p += 1;
    }
}

impl WxString {
    /// `wxString()`: the empty string.
    pub const fn new() -> Self {
        Self { units: Vec::new() }
    }

    /// `wxString( const wchar_t* aUnits, size_t aCount )`: a copy of every unit.
    pub fn from_units(units: &[WChar]) -> Result<Self, WxError> {
        let mut copy = reserve(units.len())?;
        copy.extend_from_slice(units);
        Ok(Self { units: copy })
    }

    /// `wxString::FromUTF8( const char* aUtf8 )` of the pinned wchar_t build.
    ///
    /// `None` is a null pointer and gives the empty string (`ConvertStr`'s `!psz`).
    /// Conversion stops at the first NUL, so a `CStr` built from bytes with embedded
    /// NULs converts only its first segment. Input the strict decoder rejects gives
    /// the empty string, not an error, as does input converting to no units
    /// (`ConvertStr`'s `!wcLen`). The only errors are refused allocations.
    pub fn from_utf8(utf8: Option<&CStr>) -> Result<Self, WxError> {
        let Some(utf8) = utf8 else {
            return Ok(Self::new());
        };
        let source = utf8.to_bytes_with_nul();
        // Sizing pass, as cMB2WC: the count includes the converted terminating NUL.
        let Some(with_nul) = strict_utf8_to_wchar(source, |_| {}) else {
            return Ok(Self::new());
        };
        let length = with_nul.saturating_sub(1);
        if length == 0 {
            return Ok(Self::new());
        }
        // Refuse before allocating; the buffer holds the NUL as wxWCharBuffer does.
        if length > MAX_UNITS {
            return Err(WxError::CapacityExceeded {
                requested: length,
                limit: MAX_UNITS,
            });
        }
        // The limit bounds the string, not the temporary converted terminator.
        // length was checked above; reserve one extra slot without imposing a
        // second logical-length check on this buffer.
        let mut units = Vec::new();
        units
            .try_reserve_exact(with_nul)
            .map_err(|_| WxError::Alloc {
                requested: with_nul,
            })?;
        // The second pass converts the same bytes, so it cannot fail; a failure is
        // still mapped to the reference's empty result instead of a panic.
        if strict_utf8_to_wchar(source, |unit| units.push(unit)) != Some(with_nul) {
            return Ok(Self::new());
        }
        // The last unit is the converted terminator; the string excludes it.
        units.truncate(length);
        Ok(Self { units })
    }

    /// The `wchar_t` units, without a terminator.
    pub fn units(&self) -> &[WChar] {
        &self.units
    }

    /// `wxString::length()`.
    pub fn len(&self) -> usize {
        self.units.len()
    }

    /// `wxString::empty()`.
    pub fn is_empty(&self) -> bool {
        self.units.is_empty()
    }

    /// The copy constructor, with a refused allocation reported.
    pub fn try_clone(&self) -> Result<Self, WxError> {
        Self::from_units(&self.units)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf8(bytes: &[u8]) -> Result<WxString, WxError> {
        let mut terminated = Vec::new();
        terminated.extend_from_slice(bytes);
        terminated.push(0);
        match CStr::from_bytes_until_nul(&terminated) {
            Ok(text) => WxString::from_utf8(Some(text)),
            Err(_) => Err(WxError::Alloc { requested: 0 }),
        }
    }

    #[test]
    fn strict_decoder_matches_the_reference_table() -> Result<(), WxError> {
        assert_eq!(utf8(b"A\xC3\xA9")?.units(), &[0x41, 0xE9]);
        // Overlong and surrogate forms pass the reference's checks.
        assert_eq!(utf8(b"\xE0\x80\x80x")?.units(), &[0, 0x78]);
        assert_eq!(utf8(b"\xED\xA0\x80")?.units(), &[0xD800]);
        assert_eq!(utf8(b"\xF4\xBF\xBF\xBF")?.units(), &[0x13FFFF]);
        // Rejected lead bytes, truncation and bad continuations give the empty string.
        for bad in [
            &b"\xC0\x80"[..],
            b"\xC1\xBF",
            b"\x80",
            b"\xF5\x80\x80\x80",
            b"a\xC3",
            b"\xE2\x28\xA1",
        ] {
            assert!(utf8(bad)?.is_empty());
        }
        // Embedded NUL: only the first segment converts.
        assert_eq!(utf8(b"ab\0\xFF")?.units(), &[0x61, 0x62]);
        assert!(WxString::from_utf8(None)?.is_empty());
        Ok(())
    }

    #[test]
    fn order_is_signed_unit_order_with_prefix_first() -> Result<(), WxError> {
        let a = WxString::from_units(&[-1])?;
        let b = WxString::from_units(&[0])?;
        let ab = WxString::from_units(&[0, 0])?;
        assert!(a < b && b < ab && WxString::new() < a);
        Ok(())
    }

    #[test]
    fn capacity_is_refused_before_allocation() {
        let requested = MAX_UNITS + 1;
        assert_eq!(
            reserve(requested),
            Err(WxError::CapacityExceeded {
                requested,
                limit: MAX_UNITS
            })
        );
    }

    #[test]
    fn utf8_limit_excludes_the_buffer_terminator() -> Result<(), WxError> {
        let mut input = Vec::new();
        input
            .try_reserve_exact(MAX_UNITS + 2)
            .map_err(|_| WxError::Alloc {
                requested: MAX_UNITS + 2,
            })?;
        input.resize(MAX_UNITS, b'a');
        input.push(0);
        let source = CStr::from_bytes_with_nul(&input).unwrap_or(c"");
        let text = WxString::from_utf8(Some(source))?;
        assert_eq!(text.len(), MAX_UNITS);
        assert_eq!(text, WxString::from_units(text.units())?);
        input.pop();
        input.push(b'a');
        input.push(0);
        let source = CStr::from_bytes_with_nul(&input).unwrap_or(c"");
        assert_eq!(
            WxString::from_utf8(Some(source)),
            Err(WxError::CapacityExceeded {
                requested: MAX_UNITS + 1,
                limit: MAX_UNITS,
            })
        );
        Ok(())
    }
}
