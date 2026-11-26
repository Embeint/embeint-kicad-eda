// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.

//! `std::ostream& operator<<( std::ostream& out, const wxSize& size )` and
//! `std::ostream& operator<<( std::ostream& out, const wxPoint& pt )`
//! (`libs/core/wx_stl_compat.cpp:48-59`), with the caller's `std::ostream` modelled byte for
//! byte.
//!
//! Each operator is a chain of five insertions into the caller's stream, which it returns:
//!
//! ```text
//! out << " width=\"" << size.GetWidth() << "\" height=\"" << size.GetHeight() << "\"";
//! out << " x=\"" << pt.x << "\" y=\"" << pt.y << "\"";
//! ```
//!
//! The output therefore depends on the stream's state, flags, width, fill, locale and
//! buffer. [`OStream`] models what libstdc++ does with them for a `char` stream. Every
//! function involved is the same in the GCC 13.3 headers, the GCC 14 runtime and the
//! GCC 15/16 sources, except the sentry destructor ([`SentryFlush`]):
//!
//! - `operator<<( ostream&, const char* )` is `__ostream_insert( out, s, strlen( s ) )`: a
//!   sentry; if it is good, padding to `width()` with `fill()` before the text unless
//!   `adjustfield` is exactly `left` (after it then), each fill byte through `sputc` and the
//!   text through one `sputn`; a short write sets `badbit` and stops; then `width( 0 )`. A
//!   `badbit` thrown by `setstate` inside is caught, `_M_setstate( badbit )` sets it again and
//!   rethrows only when `exceptions()` contains `badbit`; the width is then not reset.
//! - `operator<<( int )` inserts `static_cast<long>( n )`, or
//!   `static_cast<long>( static_cast<unsigned int>( n ) )` when `basefield` is exactly `oct`
//!   or `hex`, through `num_put::_M_insert_int`: digits, then the locale's `numpunct`
//!   grouping (`__add_grouping`), then a sign (decimal) or `0`/`0x`/`0X` prefix (`showbase`,
//!   nonzero values), `width( 0 )`, and one `sputn` of the result; a short write sets
//!   `badbit` through `setstate`, outside the `try` block.
//! - `sentry( os )`: the stream has no `tie()`; it is good, or else `failbit` is set through
//!   `setstate` when `badbit` is set. `setstate( s )` is `clear( rdstate() | s )`, which
//!   throws `ios_base::failure` when the new state intersects `exceptions()`.
//!
//! The only observable difference between the runtimes is `~sentry` for a stream with
//! `unitbuf` that is not `good()`: the older destructor still calls `pubsync()` (one more
//! buffer call, and `badbit` when it fails), the newer one does not; see [`SentryFlush`].
//! The reference runtime (ubuntu-24.04 `libstdc++6 14.2.0-4ubuntu2~24.04.1`) is
//! [`SentryFlush::Unconditional`].

use alloc::vec::Vec;

use kicad_math::stream::FmtFlags;

use crate::{WxPoint, WxSize};

/// Port bound on a bounded buffer's capacity, in bytes.
pub const MAX_SINK_CAPACITY: usize = 1 << 20;

/// Port bound on `width()` for an insertion into an unbounded buffer, whose padding the
/// reference would otherwise allocate in full.
pub const MAX_UNBOUNDED_WIDTH: i64 = 1 << 20;

/// `std::ios_base::iostate`: the three error bits (`goodbit` is none of them).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IoState {
    pub bad: bool,
    pub eof: bool,
    pub fail: bool,
}

impl IoState {
    /// `badbit`.
    pub const BAD: Self = Self {
        bad: true,
        eof: false,
        fail: false,
    };
    /// `failbit`.
    pub const FAIL: Self = Self {
        bad: false,
        eof: false,
        fail: true,
    };

    /// No bit is set (`goodbit`).
    pub const fn is_good(self) -> bool {
        !self.bad && !self.eof && !self.fail
    }

    /// `( self & other ) != 0`.
    pub const fn intersects(self, other: Self) -> bool {
        (self.bad && other.bad) || (self.eof && other.eof) || (self.fail && other.fail)
    }

    /// `self | other`.
    pub const fn union(self, other: Self) -> Self {
        Self {
            bad: self.bad || other.bad,
            eof: self.eof || other.eof,
            fail: self.fail || other.fail,
        }
    }
}

/// What `basic_ostream::sentry::~sentry` does for a stream with `unitbuf`. The runtime
/// library decides, because the `char` sentries run inside libstdc++'s explicitly
/// instantiated `__ostream_insert` and `_M_insert`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SentryFlush {
    /// libstdc++ up to the GCC 14 branch (checked in GCC 13.3.0, 14.2.0, 14.3.0 and the
    /// `releases/gcc-14` branch): unless an exception is propagating, call `pubsync()` and
    /// on -1 `setstate( badbit )`, whatever the stream state. A `setstate` that throws there
    /// calls `std::terminate`, because the destructor is `noexcept`. The reference runtime.
    #[default]
    Unconditional,
    /// libstdc++ from GCC 15 (checked in 15.2.0 and the GCC 16 development sources of the
    /// local host's runtime): only when the stream is also `good()`, and with exceptions
    /// disabled while `badbit` is set.
    WhenGood,
}

/// The `numpunct<char>` facet of the stream's locale, as far as integer output reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NumPunct {
    /// `thousands_sep()`.
    pub thousands_sep: u8,
    /// `grouping()`: each byte is a `char` group size (signed on the reference platform).
    pub grouping: Vec<u8>,
}

impl Default for NumPunct {
    /// The classic locale: separator `,` and no grouping.
    fn default() -> Self {
        Self {
            thousands_sep: b',',
            grouping: Vec::new(),
        }
    }
}

/// A port refusal of a stream configuration or insertion: nothing was changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamRefusal {
    /// A bounded buffer larger than [`MAX_SINK_CAPACITY`].
    Capacity { requested: usize, maximum: usize },
    /// An insertion into an unbounded buffer with `width()` above [`MAX_UNBOUNDED_WIDTH`].
    UnboundedWidth { requested: i64, maximum: i64 },
}

/// How an insertion chain ended other than normally.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InsertError {
    /// `ios_base::failure` propagated out of the operator; the stream keeps every change
    /// made before the throw.
    Failure,
    /// [`SentryFlush::Unconditional`] only: `setstate( badbit )` threw inside `~sentry`, so
    /// the reference process calls `std::terminate`. The stream holds the state reached.
    Terminate,
    /// Refused by the port before any change.
    Refused(StreamRefusal),
}

/// The stream buffer: a byte sink that accepts up to a capacity (`overflow` returns `eof()`
/// once it is full) and whose `sync()` succeeds or fails as configured. It has no put area,
/// so `sputc` and the default `xsputn` go through `overflow` byte by byte.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamBuf {
    capacity: Option<usize>,
    sync_fails: bool,
    bytes: Vec<u8>,
    sync_calls: u64,
}

impl StreamBuf {
    /// A buffer accepting at most `capacity` bytes (`None`: unbounded) whose `sync()`
    /// returns -1 when `sync_fails`, otherwise 0.
    pub fn new(capacity: Option<usize>, sync_fails: bool) -> Result<Self, StreamRefusal> {
        if let Some(requested) = capacity.filter(|&c| c > MAX_SINK_CAPACITY) {
            return Err(StreamRefusal::Capacity {
                requested,
                maximum: MAX_SINK_CAPACITY,
            });
        }
        Ok(Self {
            capacity,
            sync_fails,
            bytes: Vec::new(),
            sync_calls: 0,
        })
    }

    /// The bytes written so far.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// How many times `pubsync()` was called.
    pub const fn sync_calls(&self) -> u64 {
        self.sync_calls
    }

    /// The configured capacity (`None`: unbounded).
    pub const fn capacity(&self) -> Option<usize> {
        self.capacity
    }

    fn has_room(&self) -> bool {
        self.capacity.is_none_or(|c| self.bytes.len() < c)
    }

    /// `sputc( c )`: whether `overflow( c )` accepted the byte.
    fn sputc(&mut self, byte: u8) -> bool {
        let accepted = self.has_room();
        if accepted {
            self.bytes.push(byte);
        }
        accepted
    }

    /// `sputn( s, n )` through the default `xsputn`: the count accepted before the first
    /// `overflow` failure.
    fn sputn(&mut self, text: &[u8]) -> usize {
        let mut written = 0usize;
        for &byte in text {
            if !self.sputc(byte) {
                break;
            }
            written += 1;
        }
        written
    }

    /// `pubsync()`.
    fn pubsync(&mut self) -> i32 {
        self.sync_calls = self.sync_calls.saturating_add(1);
        if self.sync_fails {
            -1
        } else {
            0
        }
    }
}

/// A `std::ostream` (`basic_ostream<char>`) on a [`StreamBuf`], with no `tie()`.
///
/// `precision()` is carried for completeness; integer and string output do not read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OStream {
    /// `flags()`.
    pub flags: FmtFlags,
    /// `width()`.
    pub width: i64,
    /// `precision()`.
    pub precision: i64,
    /// `fill()`, a byte.
    pub fill: u8,
    /// The `numpunct<char>` of the imbued locale.
    pub numpunct: NumPunct,
    /// The runtime's `~sentry` behaviour.
    pub sentry_flush: SentryFlush,
    state: IoState,
    exceptions: IoState,
    buf: StreamBuf,
}

/// An `ios_base::failure` thrown by `clear`/`setstate`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Thrown;

impl OStream {
    /// `std::ostream out( &buf )`: good, `skipws | dec`, width 0, precision 6, fill `' '`,
    /// no exceptions, the classic locale, and the reference runtime's sentry.
    pub fn new(buf: StreamBuf) -> Self {
        Self {
            flags: FmtFlags {
                skipws: true,
                dec: true,
                ..FmtFlags::default()
            },
            width: 0,
            precision: 6,
            fill: b' ',
            numpunct: NumPunct::default(),
            sentry_flush: SentryFlush::default(),
            state: IoState::default(),
            exceptions: IoState::default(),
            buf,
        }
    }

    /// `rdstate()`.
    pub const fn rdstate(&self) -> IoState {
        self.state
    }

    /// `exceptions()`.
    pub const fn exceptions(&self) -> IoState {
        self.exceptions
    }

    /// The stream buffer.
    pub const fn buf(&self) -> &StreamBuf {
        &self.buf
    }

    /// `clear( state )`: set the state, throwing when it intersects `exceptions()`.
    pub fn clear(&mut self, state: IoState) -> Result<(), Thrown> {
        self.state = state;
        if self.exceptions.intersects(self.state) {
            return Err(Thrown);
        }
        Ok(())
    }

    /// `exceptions( mask )`: set the mask, then `clear( rdstate() )`, which throws when the
    /// current state intersects it (the mask stays set).
    pub fn set_exceptions(&mut self, mask: IoState) -> Result<(), Thrown> {
        self.exceptions = mask;
        self.clear(self.state)
    }

    /// `setstate( bits )`.
    fn setstate(&mut self, bits: IoState) -> Result<(), Thrown> {
        self.clear(self.state.union(bits))
    }

    /// `adjustfield == left`.
    const fn left_adjusted(&self) -> bool {
        self.flags.left && !self.flags.right && !self.flags.internal
    }

    /// `basic_ostream::sentry::sentry( *this )`: whether the sentry is good. A throw from its
    /// `setstate` leaves no sentry to destroy.
    fn sentry_enter(&mut self) -> Result<bool, InsertError> {
        if self.state.is_good() {
            return Ok(true);
        }
        if self.state.bad {
            self.setstate(IoState::FAIL)
                .map_err(|Thrown| InsertError::Failure)?;
        }
        Ok(false)
    }

    /// `~sentry()` when no exception is propagating (during unwinding it does nothing).
    fn sentry_exit(&mut self) -> Result<(), InsertError> {
        if !self.flags.unitbuf {
            return Ok(());
        }
        match self.sentry_flush {
            SentryFlush::Unconditional => {
                if self.buf.pubsync() == -1 && self.setstate(IoState::BAD).is_err() {
                    return Err(InsertError::Terminate);
                }
            }
            SentryFlush::WhenGood => {
                // _Disable_exceptions: badbit is set without throwing.
                if self.state.is_good() && self.buf.pubsync() == -1 {
                    self.state.bad = true;
                }
            }
        }
        Ok(())
    }

    /// `__ostream_fill( out, n )`.
    fn ostream_fill(&mut self, count: i64) -> Result<(), Thrown> {
        let mut remaining = count;
        while remaining > 0 {
            if !self.buf.sputc(self.fill) {
                self.setstate(IoState::BAD)?;
                break;
            }
            remaining -= 1;
        }
        Ok(())
    }

    /// `__ostream_write( out, s, n )`.
    fn ostream_write(&mut self, text: &[u8]) -> Result<(), Thrown> {
        if self.buf.sputn(text) != text.len() {
            self.setstate(IoState::BAD)?;
        }
        Ok(())
    }

    /// The `try` block of `__ostream_insert`.
    fn ostream_insert_body(&mut self, text: &[u8], length: i64) -> Result<(), Thrown> {
        let width = self.width;
        if width > length {
            let left = self.left_adjusted();
            if !left {
                self.ostream_fill(width - length)?;
            }
            if self.state.is_good() {
                self.ostream_write(text)?;
            }
            if left && self.state.is_good() {
                self.ostream_fill(width - length)?;
            }
        } else {
            self.ostream_write(text)?;
        }
        self.width = 0;
        Ok(())
    }

    /// `operator<<( ostream&, const char* )` of a string literal: `__ostream_insert`.
    fn insert_literal(&mut self, text: &[u8]) -> Result<(), InsertError> {
        // The literals are a few bytes long; the conversion cannot fail.
        let length = i64::try_from(text.len()).unwrap_or(i64::MAX);
        if self.sentry_enter()? && self.ostream_insert_body(text, length).is_err() {
            // catch( ... ) { __out._M_setstate( badbit ); }: rethrown only for badbit.
            self.state.bad = true;
            if self.exceptions.bad {
                return Err(InsertError::Failure);
            }
        }
        self.sentry_exit()
    }

    /// `operator<<( int )` through `_M_insert<long>`.
    fn insert_int(&mut self, value: i32) -> Result<(), InsertError> {
        let flags = self.flags;
        let oct = flags.oct && !flags.dec && !flags.hex;
        let hex = flags.hex && !flags.dec && !flags.oct;
        let value = if oct || hex {
            i64::from(u32::from_ne_bytes(value.to_ne_bytes()))
        } else {
            i64::from(value)
        };
        if self.sentry_enter()? {
            let text = self.num_put_long(value);
            // __np.put( ... ).failed(): one sputn of the formatted field.
            if self.buf.sputn(&text) != text.len() {
                // setstate( __err ) runs outside the try block, within the sentry's scope.
                self.setstate(IoState::BAD)
                    .map_err(|Thrown| InsertError::Failure)?;
            }
        }
        self.sentry_exit()
    }

    /// `num_put<char>::_M_insert_int<long>` up to the write: the formatted field.
    ///
    /// Stage 3 padding is not taken: in the owned bodies every integer insertion follows a
    /// string insertion that either reset `width()` to 0 or left the stream not good (so
    /// this sentry fails), so `__io.width()` is 0 here. The width reset is kept.
    fn num_put_long(&mut self, value: i64) -> Vec<u8> {
        let flags = self.flags;
        let octal = flags.oct && !flags.dec && !flags.hex;
        let hexadecimal = flags.hex && !flags.dec && !flags.oct;
        let decimal = !octal && !hexadecimal;
        // __unsigned_type( __v ) for a positive value or a non-decimal base (whose value
        // came from an unsigned int, so is nonnegative), else -__unsigned_type( __v ): both
        // are the magnitude.
        let magnitude = value.unsigned_abs();
        let digits = int_to_char(magnitude, octal, hexadecimal, flags.uppercase);

        let grouping = &self.numpunct.grouping;
        let digits = match grouping.first() {
            Some(&first) if group_size(first) > 0 && first != CHAR_MAX => {
                add_grouping(self.numpunct.thousands_sep, grouping, &digits)
            }
            _ => digits,
        };

        let mut field = Vec::with_capacity(digits.len() + 2);
        if decimal {
            if value < 0 {
                field.push(b'-');
            } else if flags.showpos {
                field.push(b'+');
            }
        } else if flags.showbase && value != 0 {
            field.push(b'0');
            if hexadecimal {
                field.push(if flags.uppercase { b'X' } else { b'x' });
            }
        }
        field.extend_from_slice(&digits);
        self.width = 0;
        field
    }
}

/// `__numeric_traits<char>::__max` on the reference platform (signed `char`).
const CHAR_MAX: u8 = 0x7F;

/// A grouping byte as the signed `char` it is on the reference platform.
const fn group_size(byte: u8) -> i64 {
    byte as i8 as i64
}

/// `__int_to_char`: the digits of `value` in the selected base.
fn int_to_char(value: u64, octal: bool, hexadecimal: bool, uppercase: bool) -> Vec<u8> {
    const LOWER: &[u8; 16] = b"0123456789abcdef";
    const UPPER: &[u8; 16] = b"0123456789ABCDEF";
    let (base, digits) = if octal {
        (8, LOWER)
    } else if hexadecimal {
        (16, if uppercase { UPPER } else { LOWER })
    } else {
        (10, LOWER)
    };
    let mut reversed = Vec::new();
    let mut rest = value;
    loop {
        let digit = usize::try_from(rest % base).unwrap_or(0);
        reversed.push(digits.get(digit).copied().unwrap_or(b'0'));
        rest /= base;
        if rest == 0 {
            break;
        }
    }
    reversed.reverse();
    reversed
}

/// `__add_grouping( s, sep, grouping, size, first, last )`. Called only when the first group
/// size is positive and not `CHAR_MAX`, so `grouping` is not empty.
fn add_grouping(separator: u8, grouping: &[u8], digits: &[u8]) -> Vec<u8> {
    let size = |index: usize| grouping.get(index).copied().map_or(0, group_size);
    let last_index = grouping.len().saturating_sub(1);
    let mut index = 0usize;
    let mut repeats = 0usize;
    // The number of leading digits before the first separator.
    let mut lead = i64::try_from(digits.len()).unwrap_or(i64::MAX);
    loop {
        let group = size(index);
        let raw = grouping.get(index).copied().unwrap_or(0);
        if !(lead > group && group > 0 && raw != CHAR_MAX) {
            break;
        }
        lead -= group;
        if index < last_index {
            index += 1;
        } else {
            repeats += 1;
        }
    }

    let mut out = Vec::with_capacity(digits.len() * 2);
    let mut source = digits.iter().copied();
    let lead = usize::try_from(lead).unwrap_or(0);
    out.extend(source.by_ref().take(lead));
    let mut copy_group = |out: &mut Vec<u8>, group: i64| {
        out.push(separator);
        out.extend(source.by_ref().take(usize::try_from(group).unwrap_or(0)));
    };
    for _ in 0..repeats {
        copy_group(&mut out, size(index));
    }
    while index > 0 {
        index -= 1;
        copy_group(&mut out, size(index));
    }
    out
}

/// Refuse a width the port would not pad into an unbounded buffer, before any change.
fn check_resources(out: &OStream) -> Result<(), InsertError> {
    if out.buf.capacity.is_none() && out.width > MAX_UNBOUNDED_WIDTH {
        return Err(InsertError::Refused(StreamRefusal::UnboundedWidth {
            requested: out.width,
            maximum: MAX_UNBOUNDED_WIDTH,
        }));
    }
    Ok(())
}

/// `std::ostream& operator<<( std::ostream& out, const wxSize& size )`:
/// `out << " width=\"" << size.GetWidth() << "\" height=\"" << size.GetHeight() << "\""`.
/// Returns the same stream; a propagated exception stops the chain.
pub fn insert_wx_size<'a>(
    out: &'a mut OStream,
    size: &WxSize,
) -> Result<&'a mut OStream, InsertError> {
    check_resources(out)?;
    out.insert_literal(b" width=\"")?;
    out.insert_int(size.x)?;
    out.insert_literal(b"\" height=\"")?;
    out.insert_int(size.y)?;
    out.insert_literal(b"\"")?;
    Ok(out)
}

/// `std::ostream& operator<<( std::ostream& out, const wxPoint& pt )`:
/// `out << " x=\"" << pt.x << "\" y=\"" << pt.y << "\""`. Returns the same stream; a
/// propagated exception stops the chain.
pub fn insert_wx_point<'a>(
    out: &'a mut OStream,
    point: &WxPoint,
) -> Result<&'a mut OStream, InsertError> {
    check_resources(out)?;
    out.insert_literal(b" x=\"")?;
    out.insert_int(point.x)?;
    out.insert_literal(b"\" y=\"")?;
    out.insert_int(point.y)?;
    out.insert_literal(b"\"")?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(capacity: Option<usize>) -> Result<OStream, StreamRefusal> {
        Ok(OStream::new(StreamBuf::new(capacity, false)?))
    }

    #[test]
    fn default_stream_formats_both_members() -> Result<(), StreamRefusal> {
        let mut out = stream(None)?;
        assert!(insert_wx_size(&mut out, &WxSize::new(i32::MIN, i32::MAX)).is_ok());
        assert_eq!(
            out.buf().bytes(),
            b" width=\"-2147483648\" height=\"2147483647\""
        );
        let mut out = stream(None)?;
        assert!(insert_wx_point(&mut out, &WxPoint::new(0, -7)).is_ok());
        assert_eq!(out.buf().bytes(), b" x=\"0\" y=\"-7\"");
        Ok(())
    }

    #[test]
    fn width_pads_only_the_first_literal_with_a_byte_fill() -> Result<(), StreamRefusal> {
        let mut out = stream(None)?;
        out.width = 6;
        out.fill = 0xFF;
        assert!(insert_wx_point(&mut out, &WxPoint::new(1, 2)).is_ok());
        assert_eq!(out.buf().bytes(), b"\xFF\xFF x=\"1\" y=\"2\"");
        assert_eq!(out.width, 0);
        Ok(())
    }

    #[test]
    fn grouping_and_bases_follow_libstdcxx() -> Result<(), StreamRefusal> {
        let mut out = stream(None)?;
        out.numpunct = NumPunct {
            thousands_sep: b'.',
            grouping: alloc::vec![1, 2],
        };
        out.flags.showpos = true;
        assert!(insert_wx_point(&mut out, &WxPoint::new(123_456, -5)).is_ok());
        assert_eq!(out.buf().bytes(), b" x=\"+1.23.45.6\" y=\"-5\"");

        let mut out = stream(None)?;
        out.flags.dec = false;
        out.flags.hex = true;
        out.flags.showbase = true;
        out.flags.uppercase = true;
        assert!(insert_wx_point(&mut out, &WxPoint::new(-1, 0)).is_ok());
        assert_eq!(out.buf().bytes(), b" x=\"0XFFFFFFFF\" y=\"0\"");
        Ok(())
    }

    #[test]
    fn short_write_sets_badbit_and_later_sentries_set_failbit() -> Result<(), StreamRefusal> {
        let mut out = stream(Some(6))?;
        assert!(insert_wx_point(&mut out, &WxPoint::new(12, 3)).is_ok());
        assert_eq!(out.buf().bytes(), b" x=\"12");
        assert_eq!(
            out.rdstate(),
            IoState {
                bad: true,
                eof: false,
                fail: true
            }
        );

        let mut out = stream(Some(2))?;
        assert_eq!(out.set_exceptions(IoState::BAD), Ok(()));
        out.width = 9;
        assert_eq!(
            insert_wx_point(&mut out, &WxPoint::new(1, 2)).err(),
            Some(InsertError::Failure)
        );
        // The throw skipped width( 0 ).
        assert_eq!((out.buf().bytes(), out.width), (&b"  "[..], 9));
        Ok(())
    }

    #[test]
    fn sentry_flush_differs_only_for_a_stream_that_is_not_good() -> Result<(), StreamRefusal> {
        let run = |flush| -> Result<(IoState, u64), StreamRefusal> {
            let mut out = OStream::new(StreamBuf::new(None, true)?);
            out.flags.unitbuf = true;
            out.sentry_flush = flush;
            let _ = out.clear(IoState {
                bad: false,
                eof: true,
                fail: false,
            });
            let _ = insert_wx_size(&mut out, &WxSize::new(1, 1));
            Ok((out.rdstate(), out.buf().sync_calls()))
        };
        let all = IoState {
            bad: true,
            eof: true,
            fail: true,
        };
        assert_eq!(run(SentryFlush::Unconditional)?, (all, 5));
        assert_eq!(
            run(SentryFlush::WhenGood)?,
            (
                IoState {
                    bad: false,
                    eof: true,
                    fail: false
                },
                0
            )
        );
        Ok(())
    }

    #[test]
    fn unbounded_width_and_capacity_limits_refuse_before_any_change() -> Result<(), StreamRefusal> {
        assert!(StreamBuf::new(Some(MAX_SINK_CAPACITY + 1), false).is_err());
        let mut out = stream(None)?;
        out.width = MAX_UNBOUNDED_WIDTH + 1;
        let before = out.clone();
        assert!(matches!(
            insert_wx_size(&mut out, &WxSize::new(0, 0)),
            Err(InsertError::Refused(_))
        ));
        assert_eq!(out, before);
        Ok(())
    }
}
