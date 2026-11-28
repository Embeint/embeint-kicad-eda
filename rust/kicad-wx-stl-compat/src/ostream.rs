// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
// The stream algorithms (basic_ios::clear, sentry, __ostream_insert/_fill/_write, _M_insert,
// flush, _M_insert_int, __int_to_char, __add_grouping, __pad, ostreambuf_iterator, the
// default xsputn and the numpunct twin facet) and the exception runtime's behaviour they
// depend on (the uncaught count of __cxa_rethrow/__cxa_begin_catch and the forced-unwind and
// noexcept handling of the personality routine) are reimplemented from the GNU ISO C++
// Library (libstdc++ and libsupc++, GCC 13.3.0, 14.2.0 and 15.2.0): Copyright (C) 1997-2024
// Free Software Foundation, Inc., GPL-3.0-or-later with the GCC Runtime Library Exception
// 3.1. The abort on a forced unwind that is not rethrown follows the GNU C Library 2.39
// (nptl/unwind.c): Copyright (C) 2003-2024 Free Software Foundation, Inc.,
// LGPL-2.1-or-later. No library text is copied; this file is distributed under
// GPL-3.0-or-later, which those licences permit (LGPL-2.1 section 3; without the runtime
// exception). See rust/kicad-wx-stl-compat-shim/AUDIT.md (Licence and attribution).

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
//! caller's program supplies around the stream (its `streambuf`, which may be null, the
//! `num_put<char>` facet of its locale and its `tie()`) is a [`StreamHost`], whose operations
//! may throw as the C++ virtuals may, including the distinguished forced unwind
//! (`abi::__forced_unwind`, thread cancellation). Whether an exception is already propagating
//! where the operator is called is the explicit [`CallContext`]. Every libstdc++ function
//! involved is the same in the GCC 13.3 headers, the GCC 14 runtime and the GCC 15/16
//! sources, except the sentry destructor ([`SentryFlush`]):
//!
//! - `basic_ios::clear( state )` (`basic_ios.tcc`) stores `state`, or `state | badbit` when
//!   `rdbuf()` is null, and throws `ios_base::failure` when the stored state intersects
//!   `exceptions()`. `setstate( s )` is `clear( rdstate() | s )`. `init( nullptr )` starts
//!   with `badbit`, so a stream without a buffer is never good.
//! - `sentry( os )` (`ostream.tcc`): if `tie()` is set and the stream is good,
//!   `tie()->flush()`, whose exception propagates out of the operator (no sentry exists to
//!   destroy); then the sentry is good, or else `failbit` is set through `setstate` when
//!   `badbit` is set.
//! - `operator<<( ostream&, const char* )` is `__ostream_insert( out, s, strlen( s ) )`
//!   (`ostream_insert.h`): a sentry; if it is good, padding to `width()` with `fill()` before
//!   the text unless `adjustfield` is exactly `left` (after it then), each fill byte through
//!   `sputc` and the text through one `sputn`; a short write sets `badbit` and stops; then
//!   `width( 0 )`. A forced unwind thrown inside is caught first
//!   (`catch( __forced_unwind& )`): `_M_setstate( badbit )` and an unconditional rethrow.
//!   Anything else thrown inside (a `setstate` failure or a buffer exception) is caught:
//!   `_M_setstate( badbit )` sets `badbit` and rethrows the same object only when
//!   `exceptions()` contains `badbit`. Either way a propagating exception skips `width( 0 )`.
//! - `operator<<( int )` inserts `static_cast<long>( n )`, or
//!   `static_cast<long>( static_cast<unsigned int>( n ) )` when `basefield` is exactly `oct`
//!   or `hex`, through `_M_insert`: a sentry; if it is good, the imbued facet's
//!   `num_put::put( *this, *this, fill(), v )`, which passes the stream itself as the
//!   facet's mutable `ios_base&` ([`IosBase`]); for the stock facet `_M_insert_int`
//!   ([`stock_do_put_long`]: digits, the `numpunct` grouping, a sign or `0`/`0x`/`0X` prefix,
//!   padding to `io.width()` by `__pad`, `io.width( 0 )` and one `sputn` of the field); the
//!   same two handlers catch what is thrown; a failed iterator sets `badbit` through
//!   `setstate` after the `try` block, within the sentry's scope.
//! - `~sentry` (`ostream`/`ostream.h`) does nothing while `uncaught_exception()` is true:
//!   during the unwinding of an exception from the operator, a rethrown forced unwind
//!   included (`__cxa_rethrow` counts it as uncaught and nothing uncounts it), and whenever the
//!   [`CallContext`] says an exception was already propagating. Otherwise, for a `unitbuf`
//!   stream with a buffer, see [`SentryFlush`].
//! - `flush()` ([`OStream::flush`], reached through a tied stream) returns at once when
//!   `rdbuf()` is null; otherwise it is a sentry, `pubsync()` in a `try` block with the same
//!   two handlers (`-1` adds `badbit` through `setstate` after it), and the sentry's
//!   destructor.

use alloc::vec::Vec;

use kicad_math::stream::FmtFlags;

use crate::{WxPoint, WxSize};

/// Port bound on a [`StreamBuf`]'s capacity, in bytes.
pub const MAX_SINK_CAPACITY: usize = 1 << 20;

/// Port bound on `width()` for an insertion whose host reports no capacity: the reference
/// buffer would otherwise grow by the whole padding.
pub const MAX_UNBOUNDED_WIDTH: i64 = 1 << 20;

/// The grouping bytes [`NumPunct`] keeps. A `long` has at most 22 digits (octal of
/// `ULONG_MAX`, which a facet can format), so `__add_grouping` reads at most its first 21
/// group sizes and compares its index with the last one: any longer grouping behaves as its
/// first 32 bytes do.
pub const GROUPING_PREFIX: usize = 32;

/// Port bound on the `io.width()` the stock `num_put<char>::do_put( long )` pads to. The
/// reference builds the padded field in `alloca( width )` stack storage, so a width this
/// large is outside any stack the reference runs on.
pub const MAX_PAD_WIDTH: i64 = 1 << 20;

/// The C++ context an operator is called in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CallContext {
    /// `std::uncaught_exception()` is true where the operator is called: an exception is
    /// already propagating, as when the operator runs in a destructor during stack
    /// unwinding. Every `~sentry` of the operator, and of the `flush()` of a tied stream it
    /// calls, then does nothing.
    pub uncaught_exception: bool,
}

impl CallContext {
    /// No exception is propagating.
    pub const NORMAL: Self = Self {
        uncaught_exception: false,
    };
    /// Called while an exception propagates.
    pub const UNWINDING: Self = Self {
        uncaught_exception: true,
    };
}

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
    /// disabled and anything `sync()` throws is caught by `catch( ... )`, so a failure only
    /// sets `badbit`. A forced unwind caught there is not rethrown, so glibc aborts the
    /// process ([`InsertError::Abort`]).
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

/// A port refusal of a stream configuration or insertion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamRefusal {
    /// A [`StreamBuf`] larger than [`MAX_SINK_CAPACITY`]; nothing was changed.
    Capacity { requested: usize, maximum: usize },
    /// An insertion with `width()` above [`MAX_UNBOUNDED_WIDTH`] into a host that reports no
    /// capacity; refused before any change.
    UnboundedWidth { requested: i64, maximum: i64 },
    /// The stock `do_put( long )` asked to pad to an `io.width()` above [`MAX_PAD_WIDTH`]
    /// (which a host facet can set), or its padded field could not be allocated. Refused
    /// before that body changed anything; the stream keeps what earlier steps did.
    PadWidth { requested: i64, maximum: i64 },
}

/// How a stream operation ended other than normally.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InsertError<E> {
    /// `ios_base::failure` thrown by a stream's `clear`: this stream's, or the tied stream's
    /// when its `flush()` throws one (the caller catches the same type either way; each
    /// stream's state shows which threw). The stream keeps every change made before it.
    Failure,
    /// An exception thrown by a host operation, propagated unchanged; the stream keeps every
    /// change made before it. A forced unwind ([`StreamHost::is_forced_unwind`]) always
    /// propagates.
    Host(E),
    /// [`SentryFlush::Unconditional`] only: `sync()` or `setstate( badbit )` threw inside
    /// `~sentry` (a forced unwind included), so the reference process calls
    /// `std::terminate`. The streams hold the state reached.
    Terminate,
    /// [`SentryFlush::WhenGood`] only: the destructor's `catch( ... )` caught a forced unwind
    /// from `sync()` and did not rethrow it, so glibc aborts the process (`FATAL: exception
    /// not rethrown`). The streams hold the state reached, with that stream's exception mask
    /// still disabled by the destructor.
    Abort,
    /// Refused by the port; see [`StreamRefusal`] for what was changed.
    Refused(StreamRefusal),
}

/// `ostreambuf_iterator<char>` as `num_put` uses it: whether a write through it failed. A
/// failed iterator writes nothing more.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OstreambufIterator {
    failed: bool,
}

impl OstreambufIterator {
    /// `ostreambuf_iterator( out )` on the stream's buffer: not failed.
    pub const fn new() -> Self {
        Self { failed: false }
    }

    /// `failed()`.
    pub const fn failed(self) -> bool {
        self.failed
    }
}

/// The `ios_base&` a `num_put::do_put` receives, which is the stream being written: its
/// formatting state, which the facet may read and change exactly as `io.flags( f )`,
/// `io.width( w )` and `io.precision( p )` change the stream, and the `numpunct<char>` of
/// its locale, which the stock body reads through `io.getloc()`.
///
/// Not modelled: a facet that calls `io.imbue()`, uses `iword`/`pword` or
/// `register_callback`, or reaches the `basic_ios` (state, mask, fill, buffer) through a
/// downcast.
#[derive(Debug)]
pub struct IosBase<'a> {
    flags: &'a mut FmtFlags,
    width: &'a mut i64,
    precision: &'a mut i64,
    numpunct: &'a NumPunct,
}

impl<'a> IosBase<'a> {
    /// The `ios_base` part of a stream, for calling a facet outside an insertion.
    pub fn new(
        flags: &'a mut FmtFlags,
        width: &'a mut i64,
        precision: &'a mut i64,
        numpunct: &'a NumPunct,
    ) -> Self {
        Self {
            flags,
            width,
            precision,
            numpunct,
        }
    }

    /// `flags()`.
    pub fn flags(&self) -> FmtFlags {
        *self.flags
    }

    /// `flags( flags )`: the previous flags.
    pub fn set_flags(&mut self, flags: FmtFlags) -> FmtFlags {
        core::mem::replace(self.flags, flags)
    }

    /// `width()`.
    pub fn width(&self) -> i64 {
        *self.width
    }

    /// `width( width )`: the previous width.
    pub fn set_width(&mut self, width: i64) -> i64 {
        core::mem::replace(self.width, width)
    }

    /// `precision()`.
    pub fn precision(&self) -> i64 {
        *self.precision
    }

    /// `precision( precision )`: the previous precision.
    pub fn set_precision(&mut self, precision: i64) -> i64 {
        core::mem::replace(self.precision, precision)
    }

    /// The `numpunct<char>` of `getloc()`.
    pub fn numpunct(&self) -> &NumPunct {
        self.numpunct
    }
}

/// What the C++ program supplies around a stream: its `streambuf` (with no put area, so
/// every `sputc` is an `overflow`; or none, [`has_rdbuf`](Self::has_rdbuf)), the
/// `num_put<char>` facet of its locale and its `tie()`.
///
/// Each operation may throw, as the C++ virtual may: `Err` is the thrown object, which the
/// stream propagates unchanged wherever libstdc++ rethrows it.
pub trait StreamHost {
    /// A C++ exception thrown by a host operation.
    type Exception;

    /// Whether `exception` is a forced unwind (`abi::__forced_unwind`, as thread
    /// cancellation throws), which libstdc++'s stream functions catch separately and always
    /// rethrow. The host decides; the default is none.
    fn is_forced_unwind(_exception: &Self::Exception) -> bool {
        false
    }

    /// Whether `rdbuf()` is not null. Without a buffer the stream is never good, `flush()`
    /// does nothing and no buffer operation below is called.
    fn has_rdbuf(&self) -> bool {
        true
    }

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

    /// `num_put<char>::do_put( out, io, fill, long )` of the imbued facet: the iterator it
    /// returns. `io` is the stream being written, which the facet may change. The default
    /// is the stock facet, [`stock_do_put_long`], which a host facet may call (with any
    /// iterator, `ios_base`, fill and value, any number of times) before or after changing
    /// `io` or throwing. An `Err` other than [`InsertError::Host`] or
    /// [`InsertError::Failure`] (an `ios_base::failure` the facet throws) leaves the
    /// operator unchanged.
    fn do_put_long(
        &mut self,
        out: OstreambufIterator,
        io: &mut IosBase<'_>,
        fill: u8,
        value: i64,
    ) -> Result<OstreambufIterator, InsertError<Self::Exception>> {
        stock_do_put_long(self, out, io, fill, value)
    }

    /// Whether `tie()` is not null.
    fn has_tie(&self) -> bool {
        false
    }

    /// `tie()->flush()`, called only when [`has_tie`](Self::has_tie) and the stream is good,
    /// in the operator's `context`. A tied [`OStream`] gives [`OStream::flush`].
    fn flush_tie(&mut self, _context: CallContext) -> Result<(), InsertError<Self::Exception>> {
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
/// `std::bad_alloc` in the reference adapter's buffer. It deliberately has no allocating
/// `Clone` implementation.
#[derive(Debug, PartialEq, Eq)]
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

/// The longest unpadded `long` field: 22 octal digits, 21 separators and a two-byte prefix.
const FIELD: usize = 48;

/// The stock `num_put<char>::do_put( out, io, fill, long )`, i.e. `_M_insert_int<long>`
/// (`locale_facets.tcc`): the field formatted from `io.flags()` and the `numpunct` of
/// `io.getloc()`; when `io.width()` exceeds it, `__pad` to that width with `fill`
/// (`adjustfield` exactly `left`: after the field; exactly `internal`: after a leading sign,
/// or a leading `0x`/`0X`; otherwise before it); `io.width( 0 )`; then `__write` through
/// `out`: one `sputn` of the whole field unless `out` has already failed, and the returned
/// iterator has failed when that write was short. A buffer exception propagates as
/// [`InsertError::Host`]; a width above [`MAX_PAD_WIDTH`] is refused before any change.
pub fn stock_do_put_long<H: StreamHost + ?Sized>(
    host: &mut H,
    out: OstreambufIterator,
    io: &mut IosBase<'_>,
    fill: u8,
    value: i64,
) -> Result<OstreambufIterator, InsertError<H::Exception>> {
    let flags = io.flags();
    let field = format_long(flags, io.numpunct(), value);
    let length = i64::try_from(field.len).unwrap_or(i64::MAX);
    let width = io.width();
    if width > length {
        let refused = InsertError::Refused(StreamRefusal::PadWidth {
            requested: width,
            maximum: MAX_PAD_WIDTH,
        });
        let total = match usize::try_from(width) {
            Ok(total) if width <= MAX_PAD_WIDTH => total,
            _ => return Err(refused),
        };
        let mut padded = Vec::new();
        if padded.try_reserve_exact(total).is_err() {
            return Err(refused);
        }
        pad(&mut padded, flags, fill, total, field.as_slice());
        io.set_width(0);
        write_field(host, out, &padded)
    } else {
        io.set_width(0);
        write_field(host, out, field.as_slice())
    }
}

/// `__write( out, s, n )`: `out._M_put( s, n )`.
fn write_field<H: StreamHost + ?Sized>(
    host: &mut H,
    out: OstreambufIterator,
    bytes: &[u8],
) -> Result<OstreambufIterator, InsertError<H::Exception>> {
    if out.failed {
        return Ok(out);
    }
    let written = host.xsputn(bytes).map_err(InsertError::Host)?;
    Ok(OstreambufIterator {
        failed: written != bytes.len(),
    })
}

/// `__pad<char>::_S_pad`: `field` padded with `fill` to `total` bytes (more than its
/// length), in reserved storage.
fn pad(out: &mut Vec<u8>, flags: FmtFlags, fill: u8, total: usize, field: &[u8]) {
    let padding = total.saturating_sub(field.len());
    let left = flags.left && !flags.right && !flags.internal;
    let internal = flags.internal && !flags.left && !flags.right;
    if left {
        out.extend_from_slice(field);
        out.resize(total, fill);
        return;
    }
    let kept = if !internal {
        0
    } else {
        match field {
            [b'-' | b'+', ..] => 1,
            [b'0', b'x' | b'X', ..] => 2,
            _ => 0,
        }
    };
    let (head, tail) = field.split_at(kept.min(field.len()));
    out.extend_from_slice(head);
    out.resize(head.len() + padding, fill);
    out.extend_from_slice(tail);
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
/// `precision()` is carried because a host facet may read and change it; integer and string
/// output do not read it.
#[derive(Debug, PartialEq, Eq)]
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
    /// `std::ostream out( buf )`: `skipws | dec`, width 0, precision 6, fill `' '`, no
    /// exceptions, the classic locale, and the reference runtime's sentry; good, or `badbit`
    /// when the host has no buffer (`basic_ios::init( nullptr )`).
    pub fn new(host: H) -> Self {
        let state = if host.has_rdbuf() {
            IoState::default()
        } else {
            IoState::BAD
        };
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
            state,
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

    /// `clear( state )`: store the state (with `badbit` added when there is no buffer),
    /// throwing when it intersects `exceptions()`.
    pub fn clear(&mut self, state: IoState) -> Result<(), Thrown> {
        self.state = if self.host.has_rdbuf() {
            state
        } else {
            state.union(IoState::BAD)
        };
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

    /// `catch( __forced_unwind& ) { _M_setstate( badbit ); throw; }` and then
    /// `catch( ... ) { _M_setstate( badbit ); }`: set `badbit` (without consulting the
    /// buffer, as `_M_setstate` does not), then rethrow the same object when it is a forced
    /// unwind or when `exceptions()` contains `badbit`.
    fn caught(&mut self, raised: Raised<H::Exception>) -> Result<(), InsertError<H::Exception>> {
        self.state.bad = true;
        let forced = matches!(&raised, Raised::Host(exception) if H::is_forced_unwind(exception));
        if forced || self.exceptions.bad {
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
    fn sentry_enter(&mut self, context: CallContext) -> Result<bool, InsertError<H::Exception>> {
        if self.host.has_tie() && self.state.is_good() {
            self.host.flush_tie(context)?;
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

    /// `~sentry()` when the operator's own exception is not propagating (then it does
    /// nothing, and callers return before reaching this). It also does nothing in a context
    /// where an exception was already propagating.
    fn sentry_exit(&mut self, context: CallContext) -> Result<(), InsertError<H::Exception>> {
        if !self.flags.unitbuf || context.uncaught_exception {
            return Ok(());
        }
        match self.sentry_flush {
            SentryFlush::Unconditional => {
                // if( _M_os.rdbuf() && _M_os.rdbuf()->pubsync() == -1 ) setstate( badbit ),
                // in a noexcept destructor.
                if !self.host.has_rdbuf() {
                    return Ok(());
                }
                match self.host.sync() {
                    // Thrown out of a noexcept destructor, a forced unwind included.
                    Err(_) => return Err(InsertError::Terminate),
                    Ok(-1) => {
                        if self.setstate(IoState::BAD).is_err() {
                            return Err(InsertError::Terminate);
                        }
                    }
                    Ok(_) => {}
                }
            }
            SentryFlush::WhenGood => {
                if !self.state.is_good() {
                    return Ok(());
                }
                // _Disable_exceptions for the scope, which restores the mask on exit.
                let mask = core::mem::take(&mut self.exceptions);
                if self.host.has_rdbuf() {
                    match self.host.sync() {
                        Ok(-1) => self.state.bad = true,
                        Ok(_) => {}
                        // catch( ... ) { setstate( badbit ); }, which ends a forced unwind
                        // without rethrowing it: the process aborts inside the scope.
                        Err(exception) => {
                            self.state.bad = true;
                            if H::is_forced_unwind(&exception) {
                                return Err(InsertError::Abort);
                            }
                        }
                    }
                }
                self.exceptions = mask;
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
    fn insert_literal(
        &mut self,
        text: &[u8],
        context: CallContext,
    ) -> Result<(), InsertError<H::Exception>> {
        // The literals are a few bytes long; the conversion cannot fail.
        let length = i64::try_from(text.len()).unwrap_or(i64::MAX);
        if self.sentry_enter(context)? {
            if let Err(raised) = self.ostream_insert_body(text, length) {
                self.caught(raised)?;
            }
        }
        self.sentry_exit(context)
    }

    /// `operator<<( int )` through `_M_insert<long>`.
    fn insert_int(
        &mut self,
        value: i32,
        context: CallContext,
    ) -> Result<(), InsertError<H::Exception>> {
        let flags = self.flags;
        let oct = flags.oct && !flags.dec && !flags.hex;
        let hex = flags.hex && !flags.dec && !flags.oct;
        let value = if oct || hex {
            i64::from(u32::from_ne_bytes(value.to_ne_bytes()))
        } else {
            i64::from(value)
        };
        if self.sentry_enter(context)? {
            let fill = self.fill;
            // __np.put( *this, *this, this->fill(), __v ): the stream is the facet's ios_base.
            let mut io = IosBase {
                flags: &mut self.flags,
                width: &mut self.width,
                precision: &mut self.precision,
                numpunct: &self.numpunct,
            };
            let put = self
                .host
                .do_put_long(OstreambufIterator::new(), &mut io, fill, value);
            match put {
                // __np.put( ... ).failed(): setstate( __err ) runs outside the try block,
                // within the sentry's scope.
                Ok(out) if out.failed() => self
                    .setstate(IoState::BAD)
                    .map_err(|Thrown| InsertError::Failure)?,
                Ok(_) => {}
                Err(InsertError::Host(exception)) => self.caught(Raised::Host(exception))?,
                Err(InsertError::Failure) => self.caught(Raised::Failure)?,
                // Not an exception the try block handles.
                Err(other) => return Err(other),
            }
        }
        self.sentry_exit(context)
    }

    /// `basic_ostream::flush()`, as `tie()->flush()` calls it on a tied stream, in the
    /// caller's `context`.
    pub fn flush(&mut self, context: CallContext) -> Result<(), InsertError<H::Exception>> {
        // if( __streambuf_type* __buf = this->rdbuf() ): no sentry without a buffer.
        if !self.host.has_rdbuf() {
            return Ok(());
        }
        if self.sentry_enter(context)? {
            match self.host.sync() {
                // __err |= badbit; setstate( __err ) after the try block.
                Ok(-1) => self
                    .setstate(IoState::BAD)
                    .map_err(|Thrown| InsertError::Failure)?,
                Ok(_) => {}
                Err(exception) => self.caught(Raised::Host(exception))?,
            }
        }
        self.sentry_exit(context)
    }

    /// `operator<<( basic_ostream<char>&, const char* s )` (`<ostream>`, identical in the
    /// GCC 13.3 headers and later): `setstate( badbit )` for a null pointer (`None`), outside
    /// any sentry, so a mask containing `badbit` throws `ios_base::failure`; otherwise
    /// `__ostream_insert( out, s, strlen( s ) )`, i.e. the bytes before the first NUL, as for
    /// a literal. Added for other units' stream insertions (the `KIFONT::FONT` operator, unit
    /// `include.font.application.ui.p001`); the wxPoint/wxSize operators do not use it.
    /// Call [`check_resources`] first, as the operators of this module do.
    pub fn insert_c_str(
        &mut self,
        text: Option<&[u8]>,
        context: CallContext,
    ) -> Result<(), InsertError<H::Exception>> {
        let Some(bytes) = text else {
            return self
                .setstate(IoState::BAD)
                .map_err(|Thrown| InsertError::Failure);
        };
        let length = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        self.insert_literal(bytes.get(..length).unwrap_or(bytes), context)
    }
}

/// The field the stock `num_put<char>::_M_insert_int<long>` formats before padding, without
/// allocating: from `flags` (`basefield`, `showpos`, `showbase`, `uppercase`) and the
/// grouping of `numpunct`.
fn format_long(flags: FmtFlags, numpunct: &NumPunct, value: i64) -> Field {
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

    // ( __v > 0 || !__dec ) ? __unsigned_type( __v ) : -__unsigned_type( __v ).
    let magnitude = if value > 0 || !decimal {
        u64::from_ne_bytes(value.to_ne_bytes())
    } else {
        value.unsigned_abs()
    };
    let digits = int_to_char(magnitude, octal, hexadecimal, flags.uppercase);
    let grouping = numpunct.effective_grouping();
    match grouping.first() {
        Some(&first) if group_size(first) > 0 && first != CHAR_MAX => {
            add_grouping(
                &mut field,
                numpunct.thousands_sep,
                grouping,
                digits.as_slice(),
            );
        }
        _ => field.extend(digits.as_slice()),
    }
    field
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
pub fn check_resources<H: StreamHost>(out: &OStream<H>) -> Result<(), InsertError<H::Exception>> {
    if out.host.capacity_left().is_none() && out.width > MAX_UNBOUNDED_WIDTH {
        return Err(InsertError::Refused(StreamRefusal::UnboundedWidth {
            requested: out.width,
            maximum: MAX_UNBOUNDED_WIDTH,
        }));
    }
    Ok(())
}

/// `std::ostream& operator<<( std::ostream& out, const wxSize& size )`:
/// `out << " width=\"" << size.GetWidth() << "\" height=\"" << size.GetHeight() << "\""`,
/// called in `context`. Returns the same stream; a propagated exception stops the chain.
pub fn insert_wx_size<'a, H: StreamHost>(
    out: &'a mut OStream<H>,
    size: &WxSize,
    context: CallContext,
) -> Result<&'a mut OStream<H>, InsertError<H::Exception>> {
    check_resources(out)?;
    out.insert_literal(b" width=\"", context)?;
    out.insert_int(size.x, context)?;
    out.insert_literal(b"\" height=\"", context)?;
    out.insert_int(size.y, context)?;
    out.insert_literal(b"\"", context)?;
    Ok(out)
}

/// `std::ostream& operator<<( std::ostream& out, const wxPoint& pt )`:
/// `out << " x=\"" << pt.x << "\" y=\"" << pt.y << "\""`, called in `context`. Returns the
/// same stream; a propagated exception stops the chain.
pub fn insert_wx_point<'a, H: StreamHost>(
    out: &'a mut OStream<H>,
    point: &WxPoint,
    context: CallContext,
) -> Result<&'a mut OStream<H>, InsertError<H::Exception>> {
    check_resources(out)?;
    out.insert_literal(b" x=\"", context)?;
    out.insert_int(point.x, context)?;
    out.insert_literal(b"\" y=\"", context)?;
    out.insert_int(point.y, context)?;
    out.insert_literal(b"\"", context)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    type Stream = OStream<StreamBuf>;

    const N: CallContext = CallContext::NORMAL;

    fn stream(capacity: Option<usize>) -> Result<Stream, StreamRefusal> {
        Ok(OStream::new(StreamBuf::new(capacity, false)?))
    }

    #[test]
    fn default_stream_formats_both_members() -> Result<(), StreamRefusal> {
        let mut out = stream(None)?;
        assert!(insert_wx_size(&mut out, &WxSize::new(i32::MIN, i32::MAX), N).is_ok());
        assert_eq!(
            out.host().bytes(),
            b" width=\"-2147483648\" height=\"2147483647\""
        );
        let mut out = stream(None)?;
        assert!(insert_wx_point(&mut out, &WxPoint::new(0, -7), N).is_ok());
        assert_eq!(out.host().bytes(), b" x=\"0\" y=\"-7\"");
        Ok(())
    }

    #[test]
    fn width_pads_only_the_first_literal_with_a_byte_fill() -> Result<(), StreamRefusal> {
        let mut out = stream(None)?;
        out.width = 6;
        out.fill = 0xFF;
        assert!(insert_wx_point(&mut out, &WxPoint::new(1, 2), N).is_ok());
        assert_eq!(out.host().bytes(), b"\xFF\xFF x=\"1\" y=\"2\"");
        assert_eq!(out.width, 0);
        Ok(())
    }

    #[test]
    fn grouping_and_bases_follow_libstdcxx() -> Result<(), StreamRefusal> {
        let mut out = stream(None)?;
        out.numpunct = NumPunct::new(b'.', &[1, 2]);
        out.flags.showpos = true;
        assert!(insert_wx_point(&mut out, &WxPoint::new(123_456, -5), N).is_ok());
        assert_eq!(out.host().bytes(), b" x=\"+1.23.45.6\" y=\"-5\"");

        let mut out = stream(None)?;
        out.flags.dec = false;
        out.flags.oct = true;
        out.flags.showbase = true;
        out.numpunct = NumPunct::new(b',', &[1]);
        assert!(insert_wx_point(&mut out, &WxPoint::new(-1, 0), N).is_ok());
        // The widest field: 11 octal digits, 10 separators and the prefix.
        assert_eq!(out.host().bytes(), b" x=\"03,7,7,7,7,7,7,7,7,7,7\" y=\"0\"");

        let mut out = stream(None)?;
        out.flags.dec = false;
        out.flags.hex = true;
        out.flags.showbase = true;
        out.flags.uppercase = true;
        assert!(insert_wx_point(&mut out, &WxPoint::new(-1, 0), N).is_ok());
        assert_eq!(out.host().bytes(), b" x=\"0XFFFFFFFF\" y=\"0\"");

        // The grouping ends at its first NUL byte: [3, 0] groups by three throughout.
        let mut out = stream(None)?;
        out.numpunct = NumPunct::new(b',', &[3, 0, 1]);
        assert!(insert_wx_point(&mut out, &WxPoint::new(i32::MAX, 0), N).is_ok());
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
        assert!(insert_wx_point(&mut out, &WxPoint::new(12, 3), N).is_ok());
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
            insert_wx_point(&mut out, &WxPoint::new(1, 2), N).err(),
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
            let _ = insert_wx_size(&mut out, &WxSize::new(1, 1), N);
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
            let reply = insert_wx_point(&mut out, &WxPoint::new(1, 2), N).err();
            assert_eq!(reply, Some(outcome), "{capacity} {width}");
            assert_eq!(out.host().sync_calls(), syncs);
        }
        let mut out = OStream::new(StreamBuf::new(Some(4), true)?);
        out.flags.unitbuf = true;
        out.sentry_flush = SentryFlush::WhenGood;
        assert_eq!(out.set_exceptions(IoState::BAD), Ok(()));
        assert_eq!(
            insert_wx_point(&mut out, &WxPoint::new(1, 2), N).err(),
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
        assert!(insert_wx_size(&mut out, &WxSize::new(1, 2), N).is_ok());
        assert_eq!((out.host().bytes(), out.rdstate()), (&b""[..], eof));
        Ok(())
    }

    /// A host whose operations throw on request, recording what was called.
    #[derive(Debug, Default)]
    struct Scripted {
        bytes: Vec<u8>,
        overflow_throw: Option<usize>,
        overflow_calls: usize,
        overflow_cancel: Option<usize>,
        put_throw_after: bool,
        /// The facet sets this width, `showpos` and precision 42, then delegates.
        put_width: Option<i64>,
        sync_throws: bool,
        sync_cancels: bool,
        null_rdbuf: bool,
        tie: Option<OStream<StreamBuf>>,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Exc {
        Overflow,
        Put,
        Sync,
        Alloc,
        /// A forced unwind.
        Cancel,
    }

    impl StreamHost for Scripted {
        type Exception = Exc;

        fn overflow(&mut self, byte: u8) -> Result<bool, Exc> {
            let call = self.overflow_calls;
            self.overflow_calls += 1;
            if Some(call) == self.overflow_cancel {
                return Err(Exc::Cancel);
            }
            if Some(call) == self.overflow_throw {
                return Err(Exc::Overflow);
            }
            self.bytes.try_reserve(1).map_err(|_| Exc::Alloc)?;
            self.bytes.push(byte);
            Ok(true)
        }

        fn sync(&mut self) -> Result<i32, Exc> {
            if self.sync_cancels {
                return Err(Exc::Cancel);
            }
            if self.sync_throws {
                return Err(Exc::Sync);
            }
            Ok(0)
        }

        fn has_rdbuf(&self) -> bool {
            !self.null_rdbuf
        }

        fn do_put_long(
            &mut self,
            out: OstreambufIterator,
            io: &mut IosBase<'_>,
            fill: u8,
            value: i64,
        ) -> Result<OstreambufIterator, InsertError<Exc>> {
            if let Some(width) = self.put_width {
                io.set_width(width);
                let mut flags = io.flags();
                flags.showpos = true;
                io.set_flags(flags);
                io.set_precision(42);
            }
            let out = stock_do_put_long(self, out, io, fill, value)?;
            if self.put_throw_after {
                return Err(InsertError::Host(Exc::Put));
            }
            Ok(out)
        }

        fn is_forced_unwind(exception: &Exc) -> bool {
            *exception == Exc::Cancel
        }

        fn has_tie(&self) -> bool {
            self.tie.is_some()
        }

        fn flush_tie(&mut self, context: CallContext) -> Result<(), InsertError<Exc>> {
            match self.tie.as_mut().map(|tie| tie.flush(context)) {
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
            let reply = insert_wx_point(&mut out, &WxPoint::new(123, 4), N).err();
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
            insert_wx_size(&mut out, &WxSize::new(7, 8), N).err(),
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
                insert_wx_point(&mut out, &WxPoint::new(1, 2), N).err(),
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
        assert!(insert_wx_point(&mut out, &WxPoint::new(1, 2), N).is_ok());
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
            insert_wx_point(&mut out, &WxPoint::new(1, 2), N).err(),
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
        let mut before = stream(None)?;
        before.width = MAX_UNBOUNDED_WIDTH + 1;
        assert!(matches!(
            insert_wx_size(&mut out, &WxSize::new(0, 0), N),
            Err(InsertError::Refused(_))
        ));
        assert_eq!(out, before);
        Ok(())
    }

    const BAD_FAIL: IoState = IoState {
        bad: true,
        eof: false,
        fail: true,
    };

    #[test]
    fn an_insertion_during_unwinding_never_flushes_in_a_destructor() -> Result<(), StreamRefusal> {
        // Good, unitbuf, failing sync and badbit in the mask terminate after the first
        // literal normally; while another exception propagates every ~sentry does nothing.
        let run = |context| -> Result<_, StreamRefusal> {
            let mut out = OStream::new(StreamBuf::new(None, true)?);
            out.flags.unitbuf = true;
            assert_eq!(out.set_exceptions(IoState::BAD), Ok(()));
            let reply = insert_wx_point(&mut out, &WxPoint::new(1, 2), context).err();
            Ok((
                reply,
                out.host().bytes().to_vec(),
                out.host().sync_calls(),
                out.rdstate(),
            ))
        };
        assert_eq!(
            run(N)?,
            (
                Some(InsertError::Terminate),
                b" x=\"".to_vec(),
                1,
                IoState::BAD
            )
        );
        assert_eq!(
            run(CallContext::UNWINDING)?,
            (None, b" x=\"1\" y=\"2\"".to_vec(), 0, IoState::default())
        );

        // The context reaches the tied stream's flush: its body still syncs, its own
        // unitbuf destructor does not.
        for (context, syncs) in [(N, 10), (CallContext::UNWINDING, 5)] {
            let mut tie = OStream::new(StreamBuf::new(None, false)?);
            tie.flags.unitbuf = true;
            let mut out = OStream::new(Scripted {
                tie: Some(tie),
                ..Scripted::default()
            });
            assert!(insert_wx_point(&mut out, &WxPoint::new(1, 2), context).is_ok());
            assert_eq!(
                out.host().tie.as_ref().map(|t| t.host().sync_calls()),
                Some(syncs)
            );
        }
        Ok(())
    }

    #[test]
    fn a_facet_changes_the_stream_it_formats() {
        // The facet sets width 6, showpos and precision 42, then delegates to the stock body,
        // which pads and resets the width.
        let mut out = OStream::new(Scripted {
            put_width: Some(6),
            ..Scripted::default()
        });
        out.fill = b'*';
        assert!(insert_wx_point(&mut out, &WxPoint::new(7, 8), N).is_ok());
        assert_eq!(out.host().bytes, b" x=\"****+7\" y=\"****+8\"");
        assert!(out.flags.showpos);
        assert_eq!((out.width, out.precision), (0, 42));

        // Internal adjustment pads after the sign or the 0x prefix.
        for (hex, value, field) in [(false, -5, &b"-****5"[..]), (true, 255, &b"0x**ff"[..])] {
            let mut out = OStream::new(Scripted {
                put_width: Some(6),
                ..Scripted::default()
            });
            out.fill = b'*';
            out.flags = FmtFlags {
                dec: !hex,
                hex,
                internal: true,
                showbase: true,
                ..FmtFlags::default()
            };
            assert!(insert_wx_size(&mut out, &WxSize::new(value, 0), N).is_ok());
            assert_eq!(out.host().bytes.get(8..14), Some(field));
        }
    }

    #[test]
    fn the_stock_body_formats_any_long_with_the_io_flags() -> Result<(), StreamRefusal> {
        let check = |flags: FmtFlags,
                     grouping: &[u8],
                     value: i64,
                     expected: &[u8]|
         -> Result<(), StreamRefusal> {
            let mut buf = StreamBuf::new(None, false)?;
            let numpunct = NumPunct::new(b',', grouping);
            let (mut flags, mut width, mut precision) = (flags, 0, 6);
            let mut io = IosBase::new(&mut flags, &mut width, &mut precision, &numpunct);
            let out = stock_do_put_long(&mut buf, OstreambufIterator::new(), &mut io, b' ', value);
            assert_eq!(out, Ok(OstreambufIterator::new()));
            assert_eq!(buf.bytes(), expected);
            Ok(())
        };
        let dec = FmtFlags {
            dec: true,
            ..FmtFlags::default()
        };
        check(dec, &[], i64::MIN, b"-9223372036854775808")?;
        let hex = FmtFlags {
            hex: true,
            ..FmtFlags::default()
        };
        // A negative long in a non-decimal base is its two's complement.
        check(hex, &[], -1, b"ffffffffffffffff")?;
        let oct = FmtFlags {
            oct: true,
            showbase: true,
            ..FmtFlags::default()
        };
        // The widest field: 22 octal digits, 21 separators and the prefix.
        check(
            oct,
            &[1],
            -1,
            b"01,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7",
        )
    }

    #[test]
    fn a_failed_iterator_writes_nothing_more_and_wide_padding_is_refused(
    ) -> Result<(), StreamRefusal> {
        // A short write fails the iterator; a second write through it is skipped.
        let mut buf = StreamBuf::new(Some(2), false)?;
        let numpunct = NumPunct::default();
        let (mut flags, mut width, mut precision) = (FmtFlags::default(), 0, 6);
        let mut io = IosBase::new(&mut flags, &mut width, &mut precision, &numpunct);
        let first = stock_do_put_long(&mut buf, OstreambufIterator::new(), &mut io, b' ', 123);
        assert_eq!(first.clone().map(OstreambufIterator::failed), Ok(true));
        let second = first.and_then(|it| stock_do_put_long(&mut buf, it, &mut io, b' ', 4));
        assert_eq!(second.map(OstreambufIterator::failed), Ok(true));
        assert_eq!(buf.bytes(), b"12");

        io.set_width(MAX_PAD_WIDTH + 1);
        assert_eq!(
            stock_do_put_long(&mut buf, OstreambufIterator::new(), &mut io, b' ', 4),
            Err(InsertError::Refused(StreamRefusal::PadWidth {
                requested: MAX_PAD_WIDTH + 1,
                maximum: MAX_PAD_WIDTH
            }))
        );
        assert_eq!(io.width(), MAX_PAD_WIDTH + 1);
        Ok(())
    }

    #[test]
    fn a_forced_unwind_always_propagates_after_setting_badbit() {
        // Mask empty: an ordinary exception is swallowed, a forced unwind is not; the width
        // reset is skipped and no later sentry adds failbit.
        let mut out = OStream::new(Scripted {
            overflow_cancel: Some(0),
            ..Scripted::default()
        });
        out.width = 9;
        assert_eq!(
            insert_wx_point(&mut out, &WxPoint::new(1, 2), N).err(),
            Some(InsertError::Host(Exc::Cancel))
        );
        assert_eq!(
            (out.rdstate(), out.width, out.host().overflow_calls),
            (IoState::BAD, 9, 1)
        );

        // The rethrow counts as uncaught: the unitbuf destructor it passes does nothing,
        // although a destructor that runs normally would throw from its sync.
        let mut out = OStream::new(Scripted {
            overflow_cancel: Some(1),
            sync_throws: true,
            ..Scripted::default()
        });
        out.flags.unitbuf = true;
        assert_eq!(
            insert_wx_point(&mut out, &WxPoint::new(1, 2), N).err(),
            Some(InsertError::Host(Exc::Cancel))
        );

        // A forced unwind from a destructor's sync: thrown out of the GCC 14 noexcept
        // destructor, swallowed (and so aborted) by the GCC 15 one.
        for (flush, outcome, mask) in [
            (
                SentryFlush::Unconditional,
                InsertError::Terminate,
                IoState::FAIL,
            ),
            (
                SentryFlush::WhenGood,
                InsertError::Abort,
                IoState::default(),
            ),
        ] {
            let mut out = OStream::new(Scripted {
                sync_cancels: true,
                ..Scripted::default()
            });
            out.flags.unitbuf = true;
            out.sentry_flush = flush;
            assert_eq!(out.set_exceptions(IoState::FAIL), Ok(()));
            assert_eq!(
                insert_wx_point(&mut out, &WxPoint::new(1, 2), N).err(),
                Some(outcome)
            );
            assert_eq!(out.exceptions(), mask);
            assert_eq!(out.host().bytes, b" x=\"");
        }
    }

    #[test]
    fn a_stream_without_a_buffer_is_never_good() {
        let mut out = OStream::new(Scripted {
            null_rdbuf: true,
            sync_throws: true,
            ..Scripted::default()
        });
        assert_eq!(out.rdstate(), IoState::BAD);
        assert_eq!(out.clear(IoState::default()), Ok(()));
        assert_eq!(out.rdstate(), IoState::BAD);
        out.width = 9;
        out.flags.unitbuf = true;
        // The first sentry adds failbit; no buffer operation runs, the width is kept and no
        // destructor syncs.
        assert!(insert_wx_point(&mut out, &WxPoint::new(1, 2), N).is_ok());
        assert_eq!(
            (out.rdstate(), out.width, out.host().overflow_calls),
            (BAD_FAIL, 9, 0)
        );
        // With badbit in the mask, the clear that stores the forced badbit throws.
        let mut out = OStream::new(Scripted {
            null_rdbuf: true,
            ..Scripted::default()
        });
        assert_eq!(out.set_exceptions(IoState::BAD), Err(Thrown));

        // A tied stream without a buffer: its flush does nothing, not even a sentry.
        let mut tie = OStream::new(Scripted {
            null_rdbuf: true,
            sync_throws: true,
            ..Scripted::default()
        });
        tie.flags.unitbuf = true;
        assert_eq!(tie.flush(N), Ok(()));
        assert_eq!(tie.rdstate(), IoState::BAD);
    }
}
