// SPDX-License-Identifier: GPL-3.0-or-later
//! The wxWidgets 3.2.4 library code the bodies reach, on the pinned `wxUSE_UNICODE_WCHAR`
//! build (32-bit signed `wchar_t` units). Sources: `include/wx/string.h`,
//! `src/common/string.cpp`, `src/common/strconv.cpp`, `src/common/filename.cpp`,
//! `src/common/uri.cpp` and `src/common/regex.cpp` of wxWidgets 3.2.4.
//!
//! Conversions through `wxConvLibc` and `wxConvFileName` call the host's converter
//! ([`Conv`]); the wrappers around it (how a failure becomes an empty string, which lengths
//! are passed, where a NUL ends the result) are ported here. `wxConvUTF8` and a default
//! `wxMBConvUTF8` are the strict UTF-8 converter: decoding is the accepted
//! `kicad_map_helpers::WxString::from_utf8`, encoding is [`strict_utf8`] (the same encoder
//! the accepted font port has, here with this crate's assertion callee).

use alloc::vec::Vec;
use core::ffi::CStr;

use kicad_map_helpers::{WChar, WxString};

use crate::host::{Conv, UnixHost, WxAssert};
use crate::{c_str, reserve, Result, UnixError};

/// The `wxFAIL_MSG` in `wxMBConvStrictUTF8::FromWChar` (`src/common/strconv.cpp:1165`) for a
/// code above U+10FFFF.
pub const STRICT_UTF8_ASSERT: WxAssert = WxAssert {
    line: 1165,
    function: "FromWChar",
    condition: "\"Assert failure\"",
    message: "trying to encode undefined Unicode character",
};

/// Build a `wxString` from units, reporting a refused allocation.
pub fn units(units: &[WChar]) -> Result<WxString> {
    Ok(WxString::from_units(units)?)
}

/// `wxT( "..." )` / `wxS( "..." )`: a wide literal of ASCII text.
pub fn lit(text: &str) -> Result<WxString> {
    let mut out = Vec::new();
    reserve(&mut out, text.len())?;
    out.extend(text.bytes().map(WChar::from));
    units(&out)
}

/// `a + b` for two wide strings (`operator+( const wxString&, const wxString& )` and the
/// `const wchar_t*` overloads, whose literal operands have no NUL).
pub fn concat(parts: &[&[WChar]]) -> Result<WxString> {
    let total = parts
        .iter()
        .map(|p| p.len())
        .fold(0usize, usize::saturating_add);
    let mut out = Vec::new();
    reserve(&mut out, total)?;
    for part in parts {
        out.extend_from_slice(part);
    }
    units(&out)
}

/// The wide string a `const wchar_t*` reads: the units before the first NUL.
fn wide_c_str(units: &[WChar]) -> &[WChar] {
    match units.iter().position(|&u| u == 0) {
        Some(end) => units.get(..end).unwrap_or(units),
        None => units,
    }
}

/// `ImplStr( psz )` with a process converter, as `wxString( const char* )`,
/// `operator=( const char* )`, `operator+=( const char* )` and `compare( const char* )`
/// use it: `ConvertStr( psz, npos, conv ).data`, a NUL-terminated wide buffer, so the
/// result ends at its first NUL unit. A null `psz` or a failed conversion is the empty
/// string. `psz` is read up to its NUL.
pub fn from_c_str<H: UnixHost + ?Sized>(
    host: &mut H,
    conv: Conv,
    psz: Option<&[u8]>,
) -> Result<WxString> {
    let Some(psz) = psz else {
        return Ok(WxString::new());
    };
    match host.mb_to_wc(conv, c_str(psz))? {
        Some(converted) => units(wide_c_str(&converted)),
        None => Ok(WxString::new()),
    }
}

/// `wxString::assign( const char* sz, size_t n )` with `wxConvLibc`: `ConvertStr( sz, n,
/// conv )` with an explicit length, every converted unit kept. `n == 0` converts nothing.
pub fn from_c_range<H: UnixHost + ?Sized>(
    host: &mut H,
    conv: Conv,
    bytes: &[u8],
) -> Result<WxString> {
    if bytes.is_empty() {
        return Ok(WxString::new());
    }
    match host.mb_to_wc(conv, bytes)? {
        Some(converted) => units(&converted),
        None => Ok(WxString::new()),
    }
}

/// `wxString::AsCharBuf( conv )` with a process converter (`fn_str()` with
/// [`Conv::FileName`], `mb_str()` and `ToStdString()` with [`Conv::Libc`]): every unit,
/// embedded NULs included, converted with an explicit length; a failed conversion is the
/// empty buffer. `.data()` of the result reads up to its first NUL ([`c_str`]);
/// `ToStdString()` keeps all of it.
pub fn as_char_buf<H: UnixHost + ?Sized>(
    host: &mut H,
    conv: Conv,
    text: &WxString,
) -> Result<Vec<u8>> {
    Ok(host.wc_to_mb(conv, text.units())?.unwrap_or_default())
}

/// `wxMBConvStrictUTF8::FromWChar` over exactly `units` (explicit length, NUL units encoded
/// too): each unit masked with 0x7FFFFFFF and encoded in one to four bytes up to U+10FFFF,
/// surrogates included. A larger code reaches the `wxFAIL_MSG` ([`STRICT_UTF8_ASSERT`]); when
/// the handler returns, the conversion fails (`None`).
pub fn strict_utf8<H: UnixHost + ?Sized>(host: &mut H, units: &[WChar]) -> Result<Option<Vec<u8>>> {
    let mut bytes = Vec::new();
    reserve(&mut bytes, units.len().saturating_mul(4))?;
    for &unit in units {
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
            host.on_assert(&STRICT_UTF8_ASSERT)?;
            return Ok(None);
        }
    }
    Ok(Some(bytes))
}

/// `wxString::utf8_str()` (`mb_str( wxMBConvUTF8() )`) and `ToStdString( wxConvUTF8 )`: the
/// strict encoding of every unit, or the empty buffer when it fails.
pub fn utf8_str<H: UnixHost + ?Sized>(host: &mut H, text: &WxString) -> Result<Vec<u8>> {
    Ok(strict_utf8(host, text.units())?.unwrap_or_default())
}

/// `wxString::FromUTF8( const char* utf8 )`: the strict decoding of the bytes before the
/// first NUL, embedded NUL units kept (the three-argument constructor keeps the length).
pub fn from_utf8(bytes: Option<&[u8]>) -> Result<WxString> {
    let Some(bytes) = bytes else {
        return Ok(WxString::from_utf8(None)?);
    };
    let text = c_str(bytes);
    let mut terminated = Vec::new();
    reserve(&mut terminated, text.len().saturating_add(1))?;
    terminated.extend_from_slice(text);
    terminated.push(0);
    let cstr = CStr::from_bytes_until_nul(&terminated).map_err(|_| UnixError::Alloc)?;
    Ok(WxString::from_utf8(Some(cstr))?)
}

/// `wxString( const char* psz, wxConvUTF8 )`: `m_impl( ImplStr( psz, conv ) )`, the strict
/// decoding as a NUL-terminated wide buffer, so it ends at the first NUL unit.
pub fn from_c_str_utf8(bytes: &[u8]) -> Result<WxString> {
    let decoded = from_utf8(Some(bytes))?;
    units(wide_c_str(decoded.units()))
}

/// `std::basic_string::find( str, pos )` on units.
pub fn find(hay: &[WChar], needle: &[WChar], start: usize) -> Option<usize> {
    if start > hay.len() {
        return None;
    }
    if needle.is_empty() {
        return Some(start);
    }
    let last = hay.len().checked_sub(needle.len())?;
    (start..=last).find(|&i| hay.get(i..i + needle.len()) == Some(needle))
}

/// `int wxString::Find( const wxString& sub )`: the first index (`wxNOT_FOUND` is `None`).
pub fn find_first(hay: &[WChar], needle: &[WChar]) -> Option<usize> {
    find(hay, needle, 0)
}

/// `bool wxString::Contains( const wxString& )`.
pub fn contains(hay: &[WChar], needle: &[WChar]) -> bool {
    find_first(hay, needle).is_some()
}

/// `wxString::Mid( nFirst, nCount )` (`npos` is `None`).
pub fn mid(text: &[WChar], first: usize, count: Option<usize>) -> Result<WxString> {
    let len = text.len();
    if first > len {
        return Ok(WxString::new());
    }
    let available = len - first;
    let count = count.map_or(available, |c| c.min(available));
    units(text.get(first..first + count).unwrap_or(&[]))
}

/// `wxString::Left( nCount )`.
pub fn left(text: &[WChar], count: usize) -> Result<WxString> {
    units(text.get(..count.min(text.len())).unwrap_or(&[]))
}

/// `wxString::StartsWith( prefix, &rest )`: the rest of the string after the prefix.
pub fn starts_with(text: &[WChar], prefix: &[WChar]) -> Result<Option<WxString>> {
    // compare( 0, prefix.length(), prefix ): the first min(len, n) units against prefix.
    if text.len() < prefix.len() || text.get(..prefix.len()) != Some(prefix) {
        return Ok(None);
    }
    Ok(Some(units(text.get(prefix.len()..).unwrap_or(&[]))?))
}

/// `wxString::compare( const char* sz )`: the whole string against the converted literal
/// (`ImplStr( sz )`, NUL-terminated), as `std::wstring::compare` orders them. Only zero
/// versus nonzero is used.
pub fn equals_c_literal<H: UnixHost + ?Sized>(
    host: &mut H,
    text: &WxString,
    literal: &str,
) -> Result<bool> {
    let other = from_c_str(host, Conv::Libc, Some(literal.as_bytes()))?;
    Ok(text.units() == other.units())
}

/// `glibc`'s `wcscasecmp( s1, s2 )` over two NUL-terminated wide strings, with the host's
/// `towlower`: lower both, stop when the first is NUL or they differ, return the difference
/// as `wint_t` arithmetic converted to `int`.
fn wcscasecmp<H: UnixHost + ?Sized>(host: &mut H, a: &[WChar], b: &[WChar]) -> Result<i32> {
    let at = |s: &[WChar], i: usize| s.get(i).copied().unwrap_or(0);
    let mut i = 0usize;
    loop {
        let c1 = host.towlower(at(a, i))?;
        let c2 = host.towlower(at(b, i))?;
        i += 1;
        if c1 == 0 || c1 != c2 {
            // c1 - c2 in wint_t (unsigned int), then converted to int.
            let diff = u32::from_ne_bytes(c1.to_ne_bytes())
                .wrapping_sub(u32::from_ne_bytes(c2.to_ne_bytes()));
            return Ok(i32::from_ne_bytes(diff.to_ne_bytes()));
        }
    }
}

/// `int wxString::CmpNoCase( const wxString& s )`: compares the NUL-delimited chunks with
/// `wxStricmp` (`wcscasecmp`), then the embedded NULs between them.
pub fn cmp_no_case<H: UnixHost + ?Sized>(
    host: &mut H,
    this: &[WChar],
    that: &[WChar],
) -> Result<i32> {
    // c_str() buffers: each string followed by its terminator.
    let this_end = this.len();
    let that_end = that.len();
    let at = |s: &[WChar], i: usize| s.get(i).copied().unwrap_or(0);
    let (mut this_cur, mut that_cur) = (0usize, 0usize);
    loop {
        let rc = wcscasecmp(
            host,
            this.get(this_cur..).unwrap_or(&[]),
            that.get(that_cur..).unwrap_or(&[]),
        )?;
        if rc != 0 {
            return Ok(rc);
        }
        let chunk = this
            .get(this_cur..)
            .unwrap_or(&[])
            .iter()
            .position(|&u| u == 0)
            .unwrap_or(this_end - this_cur.min(this_end));
        this_cur += chunk;
        that_cur += chunk;
        while at(this, this_cur) == 0 {
            if this_cur == this_end {
                return Ok(if that_cur == that_end { 0 } else { -1 });
            }
            if that_cur == that_end {
                return Ok(1);
            }
            if at(that, that_cur) != 0 {
                return Ok(-1);
            }
            this_cur += 1;
            that_cur += 1;
        }
    }
}

/// The name and extension `wxFileName( fullpath )` keeps from `wxFileName::SplitPath` in the
/// native (Unix) format: the volume is always empty and the path does not affect them.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct FileName {
    /// `m_name`.
    pub name: WxString,
    /// `m_ext`.
    pub ext: WxString,
    /// `m_hasExt`.
    pub has_ext: bool,
}

const DOT: WChar = b'.' as WChar;
const SLASH: WChar = b'/' as WChar;

impl FileName {
    /// `wxFileName( fullpath )` (`Assign( fullpath, wxPATH_NATIVE )`), Unix format.
    pub fn new(fullpath: &[WChar]) -> Result<Self> {
        let mut last_dot = fullpath.iter().rposition(|&u| u == DOT);
        let last_slash = fullpath.iter().rposition(|&u| u == SLASH);
        // A dot that starts a path component (or the path) does not start an extension.
        if let Some(dot) = last_dot {
            if dot == 0 || fullpath.get(dot - 1) == Some(&SLASH) {
                last_dot = None;
            }
        }
        if let (Some(dot), Some(slash)) = (last_dot, last_slash) {
            if dot < slash {
                last_dot = None;
            }
        }
        let start = last_slash.map_or(0, |s| s + 1);
        let count = match (last_dot, last_slash) {
            (None, _) => None,
            (Some(dot), None) => Some(dot),
            (Some(dot), Some(slash)) => Some(dot - slash - 1),
        };
        let name = mid(fullpath, start, count)?;
        let (ext, has_ext) = match last_dot {
            None => (WxString::new(), false),
            Some(dot) => (mid(fullpath, dot + 1, None)?, true),
        };
        Ok(Self { name, ext, has_ext })
    }

    /// `GetName()`.
    pub fn name(&self) -> &WxString {
        &self.name
    }

    /// `GetFullName()`: the name, then `wxFILE_SEP_EXT` and the extension when it has one.
    pub fn full_name(&self) -> Result<WxString> {
        if self.has_ext {
            concat(&[self.name.units(), &[DOT], self.ext.units()])
        } else {
            units(self.name.units())
        }
    }
}

/// `wxURI::IsAlpha` and the other character classes, on a `char` (bytes as signed `char`
/// compare like their unsigned values against ASCII here, so `u8` is equivalent).
fn is_alpha(c: u8) -> bool {
    c.is_ascii_alphabetic()
}
fn is_digit(c: u8) -> bool {
    c.is_ascii_digit()
}
fn is_hex(c: u8) -> bool {
    c.is_ascii_hexdigit()
}
fn is_unreserved(c: u8) -> bool {
    is_alpha(c) || is_digit(c) || matches!(c, b'-' | b'.' | b'_' | b'~')
}
fn is_sub_delim(c: u8) -> bool {
    matches!(
        c,
        b'!' | b'$' | b'&' | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
    )
}

/// `wxURI_SCHEME` and the other `m_fields` bits.
pub const URI_SCHEME: u32 = 1;
/// `wxURI_USERINFO`.
pub const URI_USERINFO: u32 = 2;
/// `wxURI_SERVER`.
pub const URI_SERVER: u32 = 4;
/// `wxURI_PORT`.
pub const URI_PORT: u32 = 8;

/// `wxURIHostType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostType {
    /// `wxURI_REGNAME`.
    RegName,
    /// `wxURI_IPV4ADDRESS`.
    Ipv4,
    /// `wxURI_IPV6ADDRESS`.
    Ipv6,
    /// `wxURI_IPVFUTURE`.
    IpvFuture,
}

/// A NUL-terminated byte buffer read through a cursor, as the `const char*` the parser walks.
struct Cursor<'a> {
    buf: &'a [u8],
}

impl Cursor<'_> {
    /// `*p` at index `i`; the terminator is at `buf.len() - 1`. The reference never reads
    /// past it, so a read there is a porting error, reported instead of guessed.
    fn at(&self, i: usize) -> Result<u8> {
        self.buf
            .get(i)
            .copied()
            .ok_or(UnixError::Undefined("read past the URI terminator"))
    }
}

/// The parts of a `wxURI` that `GetSystemProxyConfig` reads: `m_scheme`, `m_userinfo`,
/// `m_server`, `m_port`, `m_hostType` and `m_fields`. `ParsePath`, `ParseQuery` and
/// `ParseFragment` only fill `m_path`, `m_query` and `m_fragment` and set their field bits;
/// nothing the bodies read depends on them, and the converters they call are pure, so they are
/// not modelled (their result only decides whether `Create` returns true, which is ignored).
#[derive(Debug, PartialEq, Eq)]
pub struct Uri {
    /// `m_scheme`.
    pub scheme: Vec<WChar>,
    /// `m_userinfo`.
    pub userinfo: Vec<WChar>,
    /// `m_server`.
    pub server: Vec<WChar>,
    /// `m_port`.
    pub port: Vec<WChar>,
    /// `m_hostType`.
    pub host_type: HostType,
    /// `m_fields`.
    pub fields: u32,
}

impl Uri {
    /// `wxURI( const wxString& uri )`: `Create( uri )`, which parses `uri.utf8_str()` as a C
    /// string (up to its first NUL).
    pub fn new<H: UnixHost + ?Sized>(host: &mut H, uri: &WxString) -> Result<Self> {
        let utf8 = utf8_str(host, uri)?;
        let text = c_str(&utf8);
        let mut buf = Vec::new();
        reserve(&mut buf, text.len().saturating_add(1))?;
        buf.extend_from_slice(text);
        buf.push(0);
        let mut uri = Self {
            scheme: Vec::new(),
            userinfo: Vec::new(),
            server: Vec::new(),
            port: Vec::new(),
            host_type: HostType::RegName,
            fields: 0,
        };
        let cursor = Cursor { buf: &buf };
        let p = uri.parse_scheme(&cursor, 0)?;
        uri.parse_authority(host, &cursor, p)?;
        Ok(uri)
    }

    /// `HasPort()`.
    pub fn has_port(&self) -> bool {
        self.fields & URI_PORT != 0
    }

    fn push(target: &mut Vec<WChar>, c: u8) -> Result<()> {
        // Every appended char is ASCII (the parser only appends classified characters and
        // escapes), so wxUniChar( char ) keeps its value.
        reserve(target, 1)?;
        target.push(WChar::from(c));
        Ok(())
    }

    /// `AppendNextEscaped( s, p )`.
    fn append_next_escaped(target: &mut Vec<WChar>, c: &Cursor<'_>, p: &mut usize) -> Result<()> {
        if c.at(*p)? == b'%' && is_hex(c.at(*p + 1)?) && is_hex(c.at(*p + 2)?) {
            for _ in 0..3 {
                Self::push(target, c.at(*p)?)?;
                *p += 1;
            }
        } else {
            const HEX: &[u8; 16] = b"0123456789abcdef";
            // const char c = *p++ (signed char); (c >> 4) & 15 and c & 15 take its low bits.
            let ch = c.at(*p)?;
            *p += 1;
            Self::push(target, b'%')?;
            Self::push(target, HEX[usize::from(ch >> 4)])?;
            Self::push(target, HEX[usize::from(ch & 15)])?;
        }
        Ok(())
    }

    fn parse_scheme(&mut self, c: &Cursor<'_>, start: usize) -> Result<usize> {
        let mut p = start;
        if is_alpha(c.at(p)?) {
            Self::push(&mut self.scheme, c.at(p)?)?;
            p += 1;
            loop {
                let ch = c.at(p)?;
                if !(is_alpha(ch) || is_digit(ch) || matches!(ch, b'+' | b'-' | b'.')) {
                    break;
                }
                Self::push(&mut self.scheme, ch)?;
                p += 1;
            }
            if c.at(p)? == b':' {
                self.fields |= URI_SCHEME;
                p += 1;
            } else {
                p = start;
                self.scheme.clear();
            }
        }
        Ok(p)
    }

    fn parse_authority<H: UnixHost + ?Sized>(
        &mut self,
        host: &mut H,
        c: &Cursor<'_>,
        start: usize,
    ) -> Result<usize> {
        let mut p = start;
        if c.at(p)? == b'/' && c.at(p + 1)? == b'/' {
            p += 2;
            // m_scheme != "file": compare( const char* ) converts the literal with wxConvLibc.
            let scheme = units(&self.scheme)?;
            if !equals_c_literal(host, &scheme, "file")? {
                p = self.parse_user_info(c, p)?;
                p = self.parse_server(host, c, p)?;
                return self.parse_port(c, p);
            }
        }
        Ok(p)
    }

    fn parse_user_info(&mut self, c: &Cursor<'_>, start: usize) -> Result<usize> {
        let mut p = start;
        loop {
            let ch = c.at(p)?;
            if ch == 0 || matches!(ch, b'@' | b'/' | b'#' | b'?') {
                break;
            }
            if is_unreserved(ch) || is_sub_delim(ch) || ch == b':' {
                Self::push(&mut self.userinfo, ch)?;
                p += 1;
            } else {
                Self::append_next_escaped(&mut self.userinfo, c, &mut p)?;
            }
        }
        // if( *uri++ == '@' )
        let at = c.at(p)?;
        p += 1;
        if at == b'@' {
            self.fields |= URI_USERINFO;
        } else {
            p = start;
            self.userinfo.clear();
        }
        Ok(p)
    }

    fn parse_server<H: UnixHost + ?Sized>(
        &mut self,
        host: &mut H,
        c: &Cursor<'_>,
        start: usize,
    ) -> Result<usize> {
        let mut p = start;
        if c.at(p)? == b'[' {
            p += 1;
            if parse_ipv6(c, &mut p)? && c.at(p)? == b']' {
                self.host_type = HostType::Ipv6;
                self.server = assign_libc(host, c, start + 1, p)?;
                p += 1;
            } else {
                p = start + 1;
                if parse_ipvfuture(c, &mut p)? && c.at(p)? == b']' {
                    self.host_type = HostType::IpvFuture;
                    self.server = assign_libc(host, c, start + 1, p)?;
                    p += 1;
                } else {
                    p = start;
                }
            }
        } else if parse_ipv4(c, &mut p)? {
            self.host_type = HostType::Ipv4;
            self.server = assign_libc(host, c, start, p)?;
        } else {
            p = start;
        }
        if self.host_type == HostType::RegName {
            p = start;
            loop {
                let ch = c.at(p)?;
                if ch == 0 || matches!(ch, b'/' | b':' | b'#' | b'?') {
                    break;
                }
                if is_unreserved(ch) || is_sub_delim(ch) {
                    Self::push(&mut self.server, ch)?;
                    p += 1;
                } else {
                    Self::append_next_escaped(&mut self.server, c, &mut p)?;
                }
            }
        }
        self.fields |= URI_SERVER;
        Ok(p)
    }

    fn parse_port(&mut self, c: &Cursor<'_>, start: usize) -> Result<usize> {
        let mut p = start;
        if c.at(p)? == b':' {
            p += 1;
            while is_digit(c.at(p)?) {
                Self::push(&mut self.port, c.at(p)?)?;
                p += 1;
            }
            self.fields |= URI_PORT;
        }
        Ok(p)
    }
}

/// `m_server.assign( start, uri - start )`: `wxString::assign( const char*, size_t )`, converted
/// with `wxConvLibc`.
fn assign_libc<H: UnixHost + ?Sized>(
    host: &mut H,
    c: &Cursor<'_>,
    from: usize,
    to: usize,
) -> Result<Vec<WChar>> {
    let bytes = c
        .buf
        .get(from..to)
        .ok_or(UnixError::Undefined("URI range"))?;
    let text = from_c_range(host, Conv::Libc, bytes)?;
    let mut out = Vec::new();
    reserve(&mut out, text.len())?;
    out.extend_from_slice(text.units());
    Ok(out)
}

/// `wxURI::ParseH16`: advances first (`*++uri`), so the character the cursor is on is not
/// examined.
fn parse_h16(c: &Cursor<'_>, p: &mut usize) -> Result<bool> {
    *p += 1;
    if !is_hex(c.at(*p)?) {
        return Ok(false);
    }
    *p += 1;
    if is_hex(c.at(*p)?) {
        *p += 1;
        if is_hex(c.at(*p)?) {
            *p += 1;
            if is_hex(c.at(*p)?) {
                *p += 1;
            }
        }
    }
    Ok(true)
}

/// One `dec-octet` check of `ParseIPv4address` after its first digit: `IsDigit(*++uri) &&
/// IsDigit(*++uri) && !(...)` returns false from the parser; otherwise a trailing digit is
/// consumed.
fn ipv4_octet_rest(c: &Cursor<'_>, p: &mut usize) -> Result<bool> {
    *p += 1;
    if is_digit(c.at(*p)?) {
        *p += 1;
        if is_digit(c.at(*p)?) {
            let d2 = c.at(*p - 2)?;
            let d1 = c.at(*p - 1)?;
            let d0 = c.at(*p)?;
            let below = d2 < b'2' || (d2 == b'2' && (d1 < b'5' || (d1 == b'5' && d0 <= b'5')));
            if !below {
                return Ok(false);
            }
        }
    }
    if is_digit(c.at(*p)?) {
        *p += 1;
    }
    Ok(true)
}

/// `wxURI::ParseIPv4address`.
fn parse_ipv4(c: &Cursor<'_>, p: &mut usize) -> Result<bool> {
    let mut count = 0usize;
    if is_digit(c.at(*p)?) {
        count += 1;
        if !ipv4_octet_rest(c, p)? {
            return Ok(false);
        }
        while count < 4 {
            if c.at(*p)? != b'.' {
                break;
            }
            *p += 1;
            if !is_digit(c.at(*p)?) {
                break;
            }
            if !ipv4_octet_rest(c, p)? {
                return Ok(false);
            }
            count += 1;
        }
    }
    Ok(count == 4)
}

/// `wxURI::ParseIPv6address`, with its pre-incrementing `ParseH16` calls.
fn parse_ipv6(c: &Cursor<'_>, p: &mut usize) -> Result<bool> {
    let back = |p: &mut usize| -> Result<()> {
        *p = p
            .checked_sub(1)
            .ok_or(UnixError::Undefined("URI cursor before its buffer"))?;
        Ok(())
    };
    let mut num_prefix = 0usize;
    let max_postfix;
    let mut end_hex = false;
    while num_prefix < 6 {
        if !parse_h16(c, p)? {
            back(p)?;
            end_hex = true;
            break;
        }
        if c.at(*p)? != b':' {
            break;
        }
        num_prefix += 1;
    }
    if !end_hex && !parse_h16(c, p)? {
        back(p)?;
        if num_prefix != 0 {
            return Ok(false);
        }
        if c.at(*p)? == b':' {
            *p += 1;
            if c.at(*p)? != b':' {
                return Ok(false);
            }
            max_postfix = 5;
        } else {
            max_postfix = 6;
        }
    } else if c.at(*p)? != b':' || c.at(*p + 1)? != b':' {
        if num_prefix != 6 {
            return Ok(false);
        }
        loop {
            back(p)?;
            if c.at(*p)? == b':' {
                break;
            }
        }
        *p += 1;
        let start = *p;
        if parse_h16(c, p)? && c.at(*p)? == b':' && parse_h16(c, p)? {
            return Ok(true);
        }
        *p = start;
        return parse_ipv4(c, p);
    } else {
        *p += 2;
        max_postfix = if num_prefix > 3 { 0 } else { 4 - num_prefix };
    }
    let allow_alt_ending = max_postfix == 0;
    for _ in 0..max_postfix {
        if !parse_h16(c, p)? || c.at(*p)? != b':' {
            return Ok(false);
        }
    }
    if num_prefix <= 4 {
        let start = *p;
        if parse_h16(c, p)? && c.at(*p)? == b':' && parse_h16(c, p)? {
            return Ok(true);
        }
        *p = start;
        if parse_ipv4(c, p)? {
            return Ok(true);
        }
        *p = start;
        if !allow_alt_ending {
            return Ok(false);
        }
    }
    if num_prefix <= 5 {
        parse_h16(c, p)?;
    }
    Ok(true)
}

/// `wxURI::ParseIPvFuture`.
fn parse_ipvfuture(c: &Cursor<'_>, p: &mut usize) -> Result<bool> {
    *p += 1;
    if c.at(*p)? != b'v' {
        return Ok(false);
    }
    *p += 1;
    if !is_hex(c.at(*p)?) {
        return Ok(false);
    }
    loop {
        *p += 1;
        if !is_hex(c.at(*p)?) {
            break;
        }
    }
    if c.at(*p)? != b'.' {
        return Ok(false);
    }
    *p += 1;
    let ch = c.at(*p)?;
    if !(is_unreserved(ch) || is_sub_delim(ch) || ch == b':') {
        return Ok(false);
    }
    loop {
        *p += 1;
        let ch = c.at(*p)?;
        if !(is_unreserved(ch) || is_sub_delim(ch) || ch == b':') {
            break;
        }
    }
    Ok(true)
}

/// `wxURI::CharToHex`.
fn char_to_hex(c: u8) -> u8 {
    match c {
        b'A'..=b'F' => c - b'A' + 10,
        b'a'..=b'f' => c - b'a' + 10,
        _ => c.wrapping_sub(b'0'),
    }
}

/// `wxURI::Unescape( uri )`: decode `%XX` escapes in the strict UTF-8 encoding of `uri` and
/// decode the result with `FromUTF8` (up to its first NUL).
///
/// For a one-byte encoding `"%"` the reference forms `end - 2`, a pointer before the buffer,
/// which is undefined; that input is refused.
pub fn unescape<H: UnixHost + ?Sized>(host: &mut H, uri: &WxString) -> Result<WxString> {
    let bytes = utf8_str(host, uri)?;
    let len = bytes.len();
    let mut out = Vec::new();
    reserve(&mut out, len)?;
    let mut s = 0usize;
    while s != len {
        let mut ch = bytes.get(s).copied().unwrap_or(0);
        if ch == b'%' {
            // s < end - 2
            let limit = len.checked_sub(2).ok_or(UnixError::Undefined(
                "wxURI::Unescape forms end - 2 before the buffer",
            ))?;
            let h1 = bytes.get(s + 1).copied().unwrap_or(0);
            let h2 = bytes.get(s + 2).copied().unwrap_or(0);
            if s < limit && is_hex(h1) && is_hex(h2) {
                ch = (char_to_hex(h1) << 4) | char_to_hex(h2);
                s += 2;
            }
        }
        out.push(ch);
        s += 1;
    }
    from_utf8(Some(&out))
}

/// `wxFileSystem::URLToFileName( url )` (`wxFileName::URLToFileName`) on Unix: strip
/// `file://` or `file:`, unescape, replace `'/'` with `wxFILE_SEP_PATH` (also `'/'`, so
/// unchanged) and split the result as a native file name.
pub fn url_to_file_name<H: UnixHost + ?Sized>(host: &mut H, url: &WxString) -> Result<FileName> {
    let path = match starts_with(url.units(), lit("file://")?.units())? {
        Some(rest) => rest,
        None => match starts_with(url.units(), lit("file:")?.units())? {
            Some(rest) => rest,
            None => url.try_clone()?,
        },
    };
    let path = unescape(host, &path)?;
    FileName::new(path.units())
}

/// The format of the `wxLogError` in `wxRegExImpl::Matches` when matching fails with an
/// error.
pub const REGEX_MATCH_ERROR: &str = "Failed to find match for regular expression: %s";

/// What `GetErrorMsg( rc )` returns for a match error of a pattern that compiled: the PCRE2
/// message for the compile's stored error code, `"no error"` (`pcre2_get_error_message` of
/// PCRE2's success code 100), observed by the oracle.
pub const REGEX_MATCH_ERROR_ARG: &str = "no error";

/// `isSpoolName( aFileName )`: `wxRegEx( "^gtkprint_?[A-Z0-9]{6}$" ).Matches( aFileName )`.
///
/// wx 3.2.4 matches with PCRE2 in UTF-32 mode with `PCRE2_DOTALL` and without
/// `PCRE2_DOLLAR_ENDONLY`, so `$` also matches before a final `"\n"`. A subject that is not
/// valid UTF-32 (a surrogate, a code above U+10FFFF, or a negative unit) is a match error:
/// wx logs it and the result is false.
pub fn is_spool_name<H: UnixHost + ?Sized>(host: &mut H, file_name: &WxString) -> Result<bool> {
    let subject = file_name.units();
    let invalid = subject.iter().any(|&u| {
        let code = u32::from_ne_bytes(u.to_ne_bytes());
        code > 0x10_FFFF || (0xD800..=0xDFFF).contains(&code)
    });
    if invalid {
        host.wx_log_error(REGEX_MATCH_ERROR, REGEX_MATCH_ERROR_ARG)?;
        return Ok(false);
    }
    let prefix: [WChar; 8] = [
        b'g' as WChar,
        b't' as WChar,
        b'k' as WChar,
        b'p' as WChar,
        b'r' as WChar,
        b'i' as WChar,
        b'n' as WChar,
        b't' as WChar,
    ];
    let Some(rest) = subject.strip_prefix(&prefix[..]) else {
        return Ok(false);
    };
    let rest = rest.strip_prefix(&[b'_' as WChar][..]).unwrap_or(rest);
    let class = |u: WChar| {
        (WChar::from(b'A')..=WChar::from(b'Z')).contains(&u)
            || (WChar::from(b'0')..=WChar::from(b'9')).contains(&u)
    };
    if rest.len() < 6 || !rest.iter().take(6).all(|&u| class(u)) {
        return Ok(false);
    }
    let tail = rest.get(6..).unwrap_or(&[]);
    Ok(tail.is_empty() || tail == [WChar::from(b'\n')])
}
