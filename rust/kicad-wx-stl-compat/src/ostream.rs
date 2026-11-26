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
//! The output therefore depends on the stream's state, flags, width, fill, locale, tie and
//! buffer. [`OStream`] models what libstdc++ does with them for a `char` stream. What the
//! caller's program supplies around the stream (its `streambuf`, the `num_put<char>` facet of
//! its locale and its `tie()`) is a [`StreamHost`], whose operations may throw as the C++
//! virtuals may. Every libstdc++ function involved is the same in the GCC 13.3 headers, the
//! GCC 14 runtime and the GCC 15/16 sources, except the sentry destructor ([`SentryFlush`]):
//!
//! - `sentry( os )` (`ostream.tcc`): if `tie()` is set and the stream is good,
//!   `tie()->flush()`, whose exception propagates out of the operator (no sentry exists to
//!   destroy); then the sentry is good, or else `failbit` is set through `setstate` when
//!   `badbit` is set. `setstate( s )` is `clear( rdstate() | s )`, which throws
//!   `ios_base::failure` when the new state intersects `exceptions()`.
//! - `operator<<( ostream&, const char* )` is `__ostream_insert( out, s, strlen( s ) )`
//!   (`ostream_insert.h`): a sentry; if it is good, padding to `width()` with `fill()` before
//!   the text unless `adjustfield` is exactly `left` (after it then), each fill byte through
//!   `sputc` and the text through one `sputn`; a short write sets `badbit` and stops; then
//!   `width( 0 )`. Anything thrown inside (a `setstate` failure or a buffer exception) is
//!   caught: `_M_setstate( badbit )` sets `badbit` and rethrows the same object only when
//!   `exceptions()` contains `badbit`; the width is then not reset.
//! - `operator<<( int )` inserts `static_cast<long>( n )`, or
//!   `static_cast<long>( static_cast<unsigned int>( n ) )` when `basefield` is exactly `oct`
//!   or `hex`, through `_M_insert`: a sentry; if it is good, the imbued facet's
//!   `num_put::put` (for the stock facet `_M_insert_int`: digits, the `numpunct` grouping, a
//!   sign or `0`/`0x`/`0X` prefix, `width( 0 )` and one `sputn` of the field); anything thrown
//!   is caught as above; a failed iterator sets `badbit` through `setstate` after the `try`
//!   block, within the sentry's scope.
//! - `~sentry` (`ostream`/`ostream.h`) does nothing while an exception propagates; otherwise,
//!   for a `unitbuf` stream, see [`SentryFlush`].
//! - `flush()` ([`OStream::flush`], reached through a tied stream) is a sentry, `pubsync()`
//!   in a `try` block (`-1` adds `badbit` through `setstate` after it, a throw is
//!   `_M_setstate( badbit )`), and the sentry's destructor.

use alloc::vec::Vec;

use kicad_math::stream::FmtFlags;

use crate::{WxPoint, WxSize};

/// Port bound on a [`StreamBuf`]'s capacity, in bytes.
pub const MAX_SINK_CAPACITY: usize = 1 << 20;

/// Port bound on `width()` for an insertion whose host reports no capacity: the reference
/// buffer would otherwise grow by the whole padding.
pub const MAX_UNBOUNDED_WIDTH: i64 = 1 << 20;

/// The grouping bytes [`NumPunct`] keeps. An `int` inserted as a `long` has at most 11
/// digits (octal of `UINT_MAX`), so `__add_grouping` reads at most its first 10 group sizes
/// and compares its index with the last one: any longer grouping behaves as its first 32
/// bytes do.
pub const GROUPING_PREFIX: usize = 32;

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

/// What `basic_ostream::sentry::~sentry` does for a stream with `unitbuf` when no exception
/// is propagating. The runtime library decides, because the `char` sentries run inside
/// libstdc++'s explicitly instantiated `__ostream_insert`, `_M_insert` and `flush`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SentryFlush {
    /// libstdc++ up to the GCC 14 branch (checked in GCC 13.3.0, 14.2.0, 14.3.0 and the
    /// `releases/gcc-14` branch): call `pubsync()` and on -1 `setstate( badbit )`, whatever
    /// the stream state, with no `try` block. The destructor is `noexcept`, so a `sync()`
    /// that throws, or a `setstate` that throws (`badbit` in `exceptions()`), calls
    /// `std::terminate`. The reference runtime (ubuntu-24.04 `libstdc++6
    /// 14.2.0-4ubuntu2~24.04.1`).
    #[default]
    Unconditional,
    /// libstdc++ from GCC 15 (checked in 15.2.0 and the GCC 16 development sources of the
    /// local host's runtime, LWG 4188): only when the stream is also `good()`; exceptions are
    /// disabled and anything `sync()` throws is caught, so a failure only sets `badbit`.
    WhenGood,
}

/// The `numpunct<char>` facet of the stream's locale, as far as integer output reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NumPunct {
    thousands_sep: u8,
    grouping: [u8; GROUPING_PREFIX],
    grouping_len: usize,
}

impl NumPunct {
    /// A facet whose `thousands_sep()` is `thousands_sep` and whose `grouping()` is
    /// `grouping`. Only the grouping `num_put<char>` actually uses is kept: the bytes before
    /// the first NUL, and of those at most [`GROUPING_PREFIX`] (which behave as the whole).
    ///
    /// The NUL: `numpunct<char>` is a dual-ABI facet. Installing one into a locale
    /// (`locale::_Impl::_M_install_facet`, `src/c++98/locale.cc`) also installs an old-ABI
    /// `numpunct_shim` twin (`src/c++11/cxx11-shim_facets.cc`), and libstdc++'s own
    /// `num_put<char>`, compiled for the old ABI, builds its `__numpunct_cache` from that
    /// twin. The twin's `do_grouping()` returns its cached grouping as a `const char*`, so
    /// the string it returns ends at the first NUL byte. The same in GCC 13.3, 14.2 and
    /// master.
    pub fn new(thousands_sep: u8, grouping: &[u8]) -> Self {
        let mut kept = [0u8; GROUPING_PREFIX];
        let mut grouping_len = 0usize;
        for (slot, &byte) in kept
            .iter_mut()
            .zip(grouping.iter().take_while(|&&byte| byte != 0))
        {
            *slot = byte;
            grouping_len += 1;
        }
        Self {
            thousands_sep,
            grouping: kept,
            grouping_len,
        }
    }

    /// `thousands_sep()`.
    pub const fn thousands_sep(&self) -> u8 {
        self.thousands_sep
    }

    /// The grouping `num_put<char>` applies.
    pub fn effective_grouping(&self) -> &[u8] {
        self.grouping.get(..self.grouping_len).unwrap_or(&[])
    }
}

impl Default for NumPunct {
    /// The classic locale: separator `,` and no grouping.
    fn default() -> Self {
        Self::new(b',', &[])
    }
}

/// A port refusal of a stream configuration or insertion: nothing was changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamRefusal {
    /// A [`StreamBuf`] larger than [`MAX_SINK_CAPACITY`].
    Capacity { requested: usize, maximum: usize },
    /// An insertion with `width()` above [`MAX_UNBOUNDED_WIDTH`] into a host that reports no
    /// capacity.
    UnboundedWidth { requested: i64, maximum: i64 },
}

/// How a stream operation ended other than normally.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InsertError<E> {
    /// `ios_base::failure` thrown by a stream's `clear`: this stream's, or the tied stream's
    /// when its `flush()` throws one (the caller catches the same type either way; each
    /// stream's state shows which threw). The stream keeps every change made before it.
    Failure,
    /// An exception thrown by a host operation, propagated unchanged; the stream keeps every
    /// change made before it.
    Host(E),
    /// [`SentryFlush::Unconditional`] only: `sync()` or `setstate( badbit )` threw inside
    /// `~sentry`, so the reference process calls `std::terminate`. The streams hold the state
    /// reached.
    Terminate,
    /// Refused by the port before any change.
    Refused(StreamRefusal),
}

/// What the C++ program supplies around a stream: its `streambuf` (with no put area, so
/// every `sputc` is an `overflow`), the `num_put<char>` facet of its locale and its `tie()`.
///
/// Each operation may throw, as the C++ virtual may: `Err` is the thrown object, which the
/// stream propagates unchanged wherever libstdc++ rethrows it.
pub trait StreamHost {
    /// A C++ exception thrown by a host operation.
    type Exception;

    /// `sputc( byte )`, i.e. `overflow( byte )`: `Ok(true)` when accepted, `Ok(false)` for
    /// `eof()`.
    fn overflow(&mut self, byte: u8) -> Result<bool, Self::Exception>;

    /// `sputn( bytes )`, i.e. `xsputn`: the count written. The default is
    /// `basic_streambuf::xsputn` on a buffer with no put area (`streambuf.tcc`): `overflow`
    /// byte by byte until it returns `eof()` or throws, so the bytes before a throw stay
    /// written.
    fn xsputn(&mut self, bytes: &[u8]) -> Result<usize, Self::Exception> {
        let mut written = 0usize;
        for &byte in bytes {
            if !self.overflow(byte)? {
                break;
            }
            written += 1;
        }
        Ok(written)
    }

    /// `pubsync()`, i.e. `sync()`.
    fn sync(&mut self) -> Result<i32, Self::Exception>;

    /// `num_put<char>::do_put( out, io, fill, long )` of the imbued facet, which returns the
    /// iterator's `failed()`. The default is the stock facet: [`StockPut::put`]. A host facet
    /// may call it before or after throwing.
    fn do_put_long(&mut self, stock: StockPut<'_>) -> Result<bool, Self::Exception> {
        stock.put(self)
    }

    /// Whether `tie()` is not null.
    fn has_tie(&self) -> bool {
        false
    }

    /// `tie()->flush()`, called only when [`has_tie`](Self::has_tie) and the stream is good.
    /// A tied [`OStream`] gives [`OStream::flush`].
    fn flush_tie(&mut self) -> Result<(), InsertError<Self::Exception>> {
        Ok(())
    }

    /// How many more bytes the buffer accepts before `overflow` returns `eof()`, when it is
    /// bounded; `None` when it is not. Used only for the [`MAX_UNBOUNDED_WIDTH`] bound.
    fn capacity_left(&self) -> Option<usize> {
        None
    }
}

/// `std::bad_alloc` from a [`StreamBuf`] that could not grow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BadAlloc;

/// A byte buffer with no put area that accepts up to a capacity (`overflow` returns `eof()`
/// once it is full) and whose `sync()` succeeds or fails as configured. Growth is fallible:
/// a refused allocation is thrown as [`BadAlloc`], as `std::string::push_back` throws
/// `std::bad_alloc` in the reference adapter's buffer.
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

    /// How many times `sync()` was called.
    pub const fn sync_calls(&self) -> u64 {
        self.sync_calls
    }

    /// The configured capacity (`None`: unbounded).
    pub const fn capacity(&self) -> Option<usize> {
        self.capacity
    }
}

impl StreamHost for StreamBuf {
    type Exception = BadAlloc;

    fn overflow(&mut self, byte: u8) -> Result<bool, BadAlloc> {
        if self.capacity.is_some_and(|c| self.bytes.len() >= c) {
            return Ok(false);
        }
        self.bytes.try_reserve(1).map_err(|_| BadAlloc)?;
        self.bytes.push(byte);
        Ok(true)
    }

    fn sync(&mut self) -> Result<i32, BadAlloc> {
        self.sync_calls = self.sync_calls.saturating_add(1);
        Ok(if self.sync_fails { -1 } else { 0 })
    }

    fn capacity_left(&self) -> Option<usize> {
        self.capacity.map(|c| c.saturating_sub(self.bytes.len()))
    }
}

/// The longest integer field: 11 octal digits, 10 separators and a two-byte prefix.
const FIELD: usize = 32;

/// The formatted field of the stock `num_put<char>::do_put( long )`, ready for its single
/// `sputn`.
#[derive(Clone, Copy, Debug)]
pub struct StockPut<'a> {
    value: i64,
    field: &'a [u8],
}

impl StockPut<'_> {
    /// The value being inserted.
    pub const fn value(&self) -> i64 {
        self.value
    }

    /// The bytes the stock facet writes.
    pub const fn field(&self) -> &[u8] {
        self.field
    }

    /// The rest of the stock `_M_insert_int`: `__write` of the field through the
    /// `ostreambuf_iterator`, one `sputn`; the iterator has `failed()` when it was short.
    /// A buffer exception propagates.
    pub fn put<H: StreamHost + ?Sized>(self, host: &mut H) -> Result<bool, H::Exception> {
        Ok(host.xsputn(self.field)? != self.field.len())
    }
}

/// A fixed buffer filled from the front; bytes beyond its size are dropped, which the
/// field bound above rules out.
struct Field {
    bytes: [u8; FIELD],
    len: usize,
}

impl Field {
    const fn new() -> Self {
        Self {
            bytes: [0; FIELD],
            len: 0,
        }
    }

    fn push(&mut self, byte: u8) {
        if let Some(slot) = self.bytes.get_mut(self.len) {
            *slot = byte;
            self.len += 1;
        }
    }

    fn extend(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.push(byte);
        }
    }

    fn as_slice(&self) -> &[u8] {
        self.bytes.get(..self.len).unwrap_or(&[])
    }
}

/// A `std::ostream` (`basic_ostream<char>`) on a [`StreamHost`].
///
/// `precision()` is carried for completeness; integer and string output do not read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OStream<H> {
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
    host: H,
}

/// An `ios_base::failure` thrown by `clear`/`setstate`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Thrown;

/// What propagates out of a `try` block of an insertion.
enum Raised<E> {
    Failure,
    Host(E),
}

impl<E> Raised<E> {
    /// The same object, leaving the operator.
    fn into_error(self) -> InsertError<E> {
        match self {
            Self::Failure => InsertError::Failure,
            Self::Host(exception) => InsertError::Host(exception),
        }
    }
}

impl<H: StreamHost> OStream<H> {
    /// `std::ostream out( &buf )`: good, `skipws | dec`, width 0, precision 6, fill `' '`,
    /// no exceptions, the classic locale, and the reference runtime's sentry.
    pub fn new(host: H) -> Self {
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
            host,
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

    /// The host: the stream buffer, facet and tie.
    pub const fn host(&self) -> &H {
        &self.host
    }

    /// The host, mutably.
    pub fn host_mut(&mut self) -> &mut H {
        &mut self.host
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
    /// current state intersects it. The mask stays set and the stream stays usable.
    pub fn set_exceptions(&mut self, mask: IoState) -> Result<(), Thrown> {
        self.exceptions = mask;
        self.clear(self.state)
    }

    /// `setstate( bits )`.
    fn setstate(&mut self, bits: IoState) -> Result<(), Thrown> {
        self.clear(self.state.union(bits))
    }

    /// `catch( ... ) { _M_setstate( badbit ); }`: set `badbit` and rethrow the same object
    /// only when `exceptions()` contains it.
    fn caught(&mut self, raised: Raised<H::Exception>) -> Result<(), InsertError<H::Exception>> {
        self.state.bad = true;
        if self.exceptions.bad {
            return Err(raised.into_error());
        }
        Ok(())
    }

    /// `adjustfield == left`.
    const fn left_adjusted(&self) -> bool {
        self.flags.left && !self.flags.right && !self.flags.internal
    }

    /// `basic_ostream::sentry::sentry( *this )`: whether the sentry is good. A throw from the
    /// tie's flush or from `setstate` leaves no sentry to destroy.
    fn sentry_enter(&mut self) -> Result<bool, InsertError<H::Exception>> {
        if self.host.has_tie() && self.state.is_good() {
            self.host.flush_tie()?;
        }
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
    fn sentry_exit(&mut self) -> Result<(), InsertError<H::Exception>> {
        if !self.flags.unitbuf {
            return Ok(());
        }
        match self.sentry_flush {
            SentryFlush::Unconditional => match self.host.sync() {
                // Thrown out of a noexcept destructor.
                Err(_) => return Err(InsertError::Terminate),
                Ok(-1) => {
                    if self.setstate(IoState::BAD).is_err() {
                        return Err(InsertError::Terminate);
                    }
                }
                Ok(_) => {}
            },
            SentryFlush::WhenGood => {
                // _Disable_exceptions, and catch( ... ) { setstate( badbit ); }.
                if self.state.is_good() && !matches!(self.host.sync(), Ok(result) if result != -1) {
                    self.state.bad = true;
                }
            }
        }
        Ok(())
    }

    /// `__ostream_fill( out, n )`.
    fn ostream_fill(&mut self, count: i64) -> Result<(), Raised<H::Exception>> {
        let mut remaining = count;
        while remaining > 0 {
            if !self.host.overflow(self.fill).map_err(Raised::Host)? {
                self.setstate(IoState::BAD)
                    .map_err(|Thrown| Raised::Failure)?;
                break;
            }
            remaining -= 1;
        }
        Ok(())
    }

    /// `__ostream_write( out, s, n )`.
    fn ostream_write(&mut self, text: &[u8]) -> Result<(), Raised<H::Exception>> {
        if self.host.xsputn(text).map_err(Raised::Host)? != text.len() {
            self.setstate(IoState::BAD)
                .map_err(|Thrown| Raised::Failure)?;
        }
        Ok(())
    }

    /// The `try` block of `__ostream_insert`.
    fn ostream_insert_body(
        &mut self,
        text: &[u8],
        length: i64,
    ) -> Result<(), Raised<H::Exception>> {
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
    fn insert_literal(&mut self, text: &[u8]) -> Result<(), InsertError<H::Exception>> {
        // The literals are a few bytes long; the conversion cannot fail.
        let length = i64::try_from(text.len()).unwrap_or(i64::MAX);
        if self.sentry_enter()? {
            if let Err(raised) = self.ostream_insert_body(text, length) {
                self.caught(raised)?;
            }
        }
        self.sentry_exit()
    }

    /// `operator<<( int )` through `_M_insert<long>`.
    fn insert_int(&mut self, value: i32) -> Result<(), InsertError<H::Exception>> {
        let flags = self.flags;
        let oct = flags.oct && !flags.dec && !flags.hex;
        let hex = flags.hex && !flags.dec && !flags.oct;
        let value = if oct || hex {
            i64::from(u32::from_ne_bytes(value.to_ne_bytes()))
        } else {
            i64::from(value)
        };
        if self.sentry_enter()? {
            let field = self.num_put_field(value);
            let stock = StockPut {
                value,
                field: field.as_slice(),
            };
            match self.host.do_put_long(stock) {
                // __np.put( ... ).failed(): setstate( __err ) runs outside the try block,
                // within the sentry's scope.
                Ok(true) => self
                    .setstate(IoState::BAD)
                    .map_err(|Thrown| InsertError::Failure)?,
                Ok(false) => {}
                Err(exception) => self.caught(Raised::Host(exception))?,
            }
        }
        self.sentry_exit()
    }

    /// The field the stock `num_put<char>::_M_insert_int<long>` writes, without allocating.
    ///
    /// Stage 3 padding is not taken: in the owned bodies every integer insertion follows a
    /// string insertion that either reset `width()` to 0 or left the stream not good (so
    /// this sentry fails), and no host operation can change this stream's width, so
    /// `__io.width()` is 0 here and its `width( 0 )` changes nothing.
    fn num_put_field(&self, value: i64) -> Field {
        let flags = self.flags;
        let octal = flags.oct && !flags.dec && !flags.hex;
        let hexadecimal = flags.hex && !flags.dec && !flags.oct;
        let decimal = !octal && !hexadecimal;

        let mut field = Field::new();
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

        // __unsigned_type( __v ) for a positive value or a non-decimal base (whose value
        // came from an unsigned int, so is nonnegative), else -__unsigned_type( __v ): both
        // are the magnitude.
        let digits = int_to_char(value.unsigned_abs(), octal, hexadecimal, flags.uppercase);
        let grouping = self.numpunct.effective_grouping();
        match grouping.first() {
            Some(&first) if group_size(first) > 0 && first != CHAR_MAX => {
                add_grouping(
                    &mut field,
                    self.numpunct.thousands_sep,
                    grouping,
                    digits.as_slice(),
                );
            }
            _ => field.extend(digits.as_slice()),
        }
        field
    }

    /// `basic_ostream::flush()`, as `tie()->flush()` calls it on a tied stream.
    pub fn flush(&mut self) -> Result<(), InsertError<H::Exception>> {
        if self.sentry_enter()? {
            match self.host.sync() {
                // __err |= badbit; setstate( __err ) after the try block.
                Ok(-1) => self
                    .setstate(IoState::BAD)
                    .map_err(|Thrown| InsertError::Failure)?,
                Ok(_) => {}
                Err(exception) => self.caught(Raised::Host(exception))?,
            }
        }
        self.sentry_exit()
    }
}

/// `__numeric_traits<char>::__max` on the reference platform (signed `char`).
const CHAR_MAX: u8 = 0x7F;

/// A grouping byte as the signed `char` it is on the reference platform.
const fn group_size(byte: u8) -> i64 {
    byte as i8 as i64
}

/// `__int_to_char`: the digits of `value` in the selected base.
fn int_to_char(value: u64, octal: bool, hexadecimal: bool, uppercase: bool) -> Field {
    const LOWER: &[u8; 16] = b"0123456789abcdef";
    const UPPER: &[u8; 16] = b"0123456789ABCDEF";
    let (base, digits) = if octal {
        (8, LOWER)
    } else if hexadecimal {
        (16, if uppercase { UPPER } else { LOWER })
    } else {
        (10, LOWER)
    };
    // Right-justified, as libstdc++ fills its buffer from the end.
    let mut reversed = Field::new();
    let mut rest = value;
    loop {
        let digit = usize::try_from(rest % base).unwrap_or(0);
        reversed.push(digits.get(digit).copied().unwrap_or(b'0'));
        rest /= base;
        if rest == 0 {
            break;
        }
    }
    let mut out = Field::new();
    for &byte in reversed.as_slice().iter().rev() {
        out.push(byte);
    }
    out
}

/// `__add_grouping( s, sep, grouping, size, first, last )`, appending to `out`. Called only
/// when the first group size is positive and not `CHAR_MAX`, so `grouping` is not empty.
fn add_grouping(out: &mut Field, separator: u8, grouping: &[u8], digits: &[u8]) {
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

    let mut source = digits.iter().copied();
    let lead = usize::try_from(lead).unwrap_or(0);
    for byte in source.by_ref().take(lead) {
        out.push(byte);
    }
    let mut copy_group = |out: &mut Field, group: i64| {
        out.push(separator);
        for byte in source.by_ref().take(usize::try_from(group).unwrap_or(0)) {
            out.push(byte);
        }
    };
    for _ in 0..repeats {
        copy_group(out, size(index));
    }
    while index > 0 {
        index -= 1;
        copy_group(out, size(index));
    }
}

/// Refuse a width the port would not pad into a host without a capacity, before any change.
fn check_resources<H: StreamHost>(out: &OStream<H>) -> Result<(), InsertError<H::Exception>> {
    if out.host.capacity_left().is_none() && out.width > MAX_UNBOUNDED_WIDTH {
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
pub fn insert_wx_size<'a, H: StreamHost>(
    out: &'a mut OStream<H>,
    size: &WxSize,
) -> Result<&'a mut OStream<H>, InsertError<H::Exception>> {
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
pub fn insert_wx_point<'a, H: StreamHost>(
    out: &'a mut OStream<H>,
    point: &WxPoint,
) -> Result<&'a mut OStream<H>, InsertError<H::Exception>> {
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

    type Stream = OStream<StreamBuf>;

    fn stream(capacity: Option<usize>) -> Result<Stream, StreamRefusal> {
        Ok(OStream::new(StreamBuf::new(capacity, false)?))
    }

    #[test]
    fn default_stream_formats_both_members() -> Result<(), StreamRefusal> {
        let mut out = stream(None)?;
        assert!(insert_wx_size(&mut out, &WxSize::new(i32::MIN, i32::MAX)).is_ok());
        assert_eq!(
            out.host().bytes(),
            b" width=\"-2147483648\" height=\"2147483647\""
        );
        let mut out = stream(None)?;
        assert!(insert_wx_point(&mut out, &WxPoint::new(0, -7)).is_ok());
        assert_eq!(out.host().bytes(), b" x=\"0\" y=\"-7\"");
        Ok(())
    }

    #[test]
    fn width_pads_only_the_first_literal_with_a_byte_fill() -> Result<(), StreamRefusal> {
        let mut out = stream(None)?;
        out.width = 6;
        out.fill = 0xFF;
        assert!(insert_wx_point(&mut out, &WxPoint::new(1, 2)).is_ok());
        assert_eq!(out.host().bytes(), b"\xFF\xFF x=\"1\" y=\"2\"");
        assert_eq!(out.width, 0);
        Ok(())
    }

    #[test]
    fn grouping_and_bases_follow_libstdcxx() -> Result<(), StreamRefusal> {
        let mut out = stream(None)?;
        out.numpunct = NumPunct::new(b'.', &[1, 2]);
        out.flags.showpos = true;
        assert!(insert_wx_point(&mut out, &WxPoint::new(123_456, -5)).is_ok());
        assert_eq!(out.host().bytes(), b" x=\"+1.23.45.6\" y=\"-5\"");

        let mut out = stream(None)?;
        out.flags.dec = false;
        out.flags.oct = true;
        out.flags.showbase = true;
        out.numpunct = NumPunct::new(b',', &[1]);
        assert!(insert_wx_point(&mut out, &WxPoint::new(-1, 0)).is_ok());
        // The widest field: 11 octal digits, 10 separators and the prefix.
        assert_eq!(out.host().bytes(), b" x=\"03,7,7,7,7,7,7,7,7,7,7\" y=\"0\"");

        let mut out = stream(None)?;
        out.flags.dec = false;
        out.flags.hex = true;
        out.flags.showbase = true;
        out.flags.uppercase = true;
        assert!(insert_wx_point(&mut out, &WxPoint::new(-1, 0)).is_ok());
        assert_eq!(out.host().bytes(), b" x=\"0XFFFFFFFF\" y=\"0\"");

        // The grouping ends at its first NUL byte: [3, 0] groups by three throughout.
        let mut out = stream(None)?;
        out.numpunct = NumPunct::new(b',', &[3, 0, 1]);
        assert!(insert_wx_point(&mut out, &WxPoint::new(i32::MAX, 0)).is_ok());
        assert_eq!(out.host().bytes(), b" x=\"2,147,483,647\" y=\"0\"");

        // A grouping longer than the kept prefix behaves as the prefix.
        let long = [2u8; 100];
        assert_eq!(
            NumPunct::new(b',', &long).effective_grouping(),
            &long[..GROUPING_PREFIX]
        );
        Ok(())
    }

    #[test]
    fn short_write_sets_badbit_and_later_sentries_set_failbit() -> Result<(), StreamRefusal> {
        let mut out = stream(Some(6))?;
        assert!(insert_wx_point(&mut out, &WxPoint::new(12, 3)).is_ok());
        assert_eq!(out.host().bytes(), b" x=\"12");
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
        assert_eq!((out.host().bytes(), out.width), (&b"  "[..], 9));
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
            Ok((out.rdstate(), out.host().sync_calls()))
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
    fn a_failing_first_write_unwinds_past_the_terminating_flush() -> Result<(), StreamRefusal> {
        // Good, unitbuf, failing sync, badbit in the mask: a buffer below the first literal
        // throws ios_base::failure before any destructor flush; one that holds it terminates
        // in the reference destructor and throws at the next sentry on GCC 15.
        for (capacity, width, outcome, syncs) in [
            (0, 0, InsertError::Failure, 0),
            (3, 0, InsertError::Failure, 0),
            (4, 0, InsertError::Terminate, 1),
            (5, 6, InsertError::Failure, 0),
            (6, 6, InsertError::Terminate, 1),
        ] {
            let mut out = OStream::new(StreamBuf::new(Some(capacity), true)?);
            out.flags.unitbuf = true;
            out.width = width;
            assert_eq!(out.set_exceptions(IoState::BAD), Ok(()));
            let reply = insert_wx_point(&mut out, &WxPoint::new(1, 2)).err();
            assert_eq!(reply, Some(outcome), "{capacity} {width}");
            assert_eq!(out.host().sync_calls(), syncs);
        }
        let mut out = OStream::new(StreamBuf::new(Some(4), true)?);
        out.flags.unitbuf = true;
        out.sentry_flush = SentryFlush::WhenGood;
        assert_eq!(out.set_exceptions(IoState::BAD), Ok(()));
        assert_eq!(
            insert_wx_point(&mut out, &WxPoint::new(1, 2)).err(),
            Some(InsertError::Failure)
        );
        assert_eq!(
            (out.host().bytes(), out.host().sync_calls()),
            (&b" x=\""[..], 1)
        );
        Ok(())
    }

    #[test]
    fn a_setup_failure_leaves_a_usable_stream() -> Result<(), StreamRefusal> {
        let mut out = stream(None)?;
        let eof = IoState {
            bad: false,
            eof: true,
            fail: false,
        };
        assert_eq!(out.clear(eof), Ok(()));
        assert_eq!(out.set_exceptions(eof), Err(Thrown));
        // No setstate runs for a stream that is not bad: the insertion returns quietly.
        assert!(insert_wx_size(&mut out, &WxSize::new(1, 2)).is_ok());
        assert_eq!((out.host().bytes(), out.rdstate()), (&b""[..], eof));
        Ok(())
    }

    /// A host whose operations throw on request, recording what was called.
    #[derive(Debug, Default)]
    struct Scripted {
        bytes: Vec<u8>,
        overflow_throw: Option<usize>,
        overflow_calls: usize,
        put_throw_after: bool,
        sync_throws: bool,
        tie: Option<OStream<StreamBuf>>,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Exc {
        Overflow,
        Put,
        Sync,
        Alloc,
    }

    impl StreamHost for Scripted {
        type Exception = Exc;

        fn overflow(&mut self, byte: u8) -> Result<bool, Exc> {
            let call = self.overflow_calls;
            self.overflow_calls += 1;
            if Some(call) == self.overflow_throw {
                return Err(Exc::Overflow);
            }
            self.bytes.try_reserve(1).map_err(|_| Exc::Alloc)?;
            self.bytes.push(byte);
            Ok(true)
        }

        fn sync(&mut self) -> Result<i32, Exc> {
            if self.sync_throws {
                return Err(Exc::Sync);
            }
            Ok(0)
        }

        fn do_put_long(&mut self, stock: StockPut<'_>) -> Result<bool, Exc> {
            let failed = stock.put(self)?;
            if self.put_throw_after {
                return Err(Exc::Put);
            }
            Ok(failed)
        }

        fn has_tie(&self) -> bool {
            self.tie.is_some()
        }

        fn flush_tie(&mut self) -> Result<(), InsertError<Exc>> {
            match self.tie.as_mut().map(OStream::flush) {
                Some(Err(InsertError::Failure)) => Err(InsertError::Failure),
                Some(Err(InsertError::Terminate)) => Err(InsertError::Terminate),
                Some(Err(_)) => Err(InsertError::Host(Exc::Alloc)),
                _ => Ok(()),
            }
        }
    }

    #[test]
    fn host_exceptions_keep_partial_output_and_propagate_only_for_badbit() {
        // The overflow throwing on the seventh byte: " x=\"12" written.
        for mask_bad in [false, true] {
            let mut out = OStream::new(Scripted {
                overflow_throw: Some(6),
                ..Scripted::default()
            });
            if mask_bad {
                assert_eq!(out.set_exceptions(IoState::BAD), Ok(()));
            }
            let reply = insert_wx_point(&mut out, &WxPoint::new(123, 4)).err();
            assert_eq!(out.host().bytes, b" x=\"12");
            if mask_bad {
                assert_eq!(reply, Some(InsertError::Host(Exc::Overflow)));
                assert_eq!(out.rdstate(), IoState::BAD);
            } else {
                // Swallowed with badbit; the next sentry adds failbit.
                assert_eq!(reply, None);
                assert!(out.rdstate().bad && out.rdstate().fail);
            }
        }

        // A facet throwing after the stock body wrote the field.
        let mut out = OStream::new(Scripted {
            put_throw_after: true,
            ..Scripted::default()
        });
        assert_eq!(out.set_exceptions(IoState::BAD), Ok(()));
        assert_eq!(
            insert_wx_size(&mut out, &WxSize::new(7, 8)).err(),
            Some(InsertError::Host(Exc::Put))
        );
        assert_eq!(out.host().bytes, b" width=\"7");
    }

    #[test]
    fn a_throwing_sync_terminates_only_in_the_reference_destructor() {
        for (flush, outcome, bad) in [
            (
                SentryFlush::Unconditional,
                Some(InsertError::Terminate),
                false,
            ),
            (SentryFlush::WhenGood, None, true),
        ] {
            let mut out = OStream::new(Scripted {
                sync_throws: true,
                ..Scripted::default()
            });
            out.flags.unitbuf = true;
            out.sentry_flush = flush;
            assert_eq!(
                insert_wx_point(&mut out, &WxPoint::new(1, 2)).err(),
                outcome
            );
            assert_eq!(out.rdstate().bad, bad);
        }
    }

    #[test]
    fn a_tied_stream_is_flushed_by_every_good_sentry() -> Result<(), StreamRefusal> {
        let mut out = OStream::new(Scripted {
            tie: Some(OStream::new(StreamBuf::new(None, false)?)),
            ..Scripted::default()
        });
        assert!(insert_wx_point(&mut out, &WxPoint::new(1, 2)).is_ok());
        assert_eq!(
            out.host().tie.as_ref().map(|t| t.host().sync_calls()),
            Some(5)
        );

        // A tie whose sync fails with badbit in its mask: its ios_base::failure leaves the
        // first sentry, before any output.
        let mut tie = OStream::new(StreamBuf::new(None, true)?);
        assert_eq!(tie.set_exceptions(IoState::BAD), Ok(()));
        let mut out = OStream::new(Scripted {
            tie: Some(tie),
            ..Scripted::default()
        });
        assert_eq!(
            insert_wx_point(&mut out, &WxPoint::new(1, 2)).err(),
            Some(InsertError::Failure)
        );
        assert!(out.host().bytes.is_empty() && out.rdstate().is_good());
        Ok(())
    }

    #[test]
    fn a_full_bounded_buffer_returns_eof_before_growing() -> Result<(), StreamRefusal> {
        // The capacity check precedes the fallible reservation.
        let mut buf = StreamBuf::new(Some(0), false)?;
        assert_eq!(buf.overflow(b'a'), Ok(false));
        assert_eq!(buf.capacity_left(), Some(0));
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
