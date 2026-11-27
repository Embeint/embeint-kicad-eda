// SPDX-License-Identifier: GPL-3.0-or-later
//! The caller's program around the stream, as the C++ adapter builds it: its `SINK`
//! stream buffer (or none), its optional scripted `NUM_PUT` facet and its optional tied
//! stream. Each operation may throw a `std::runtime_error` at a scripted call, or be
//! cancelled there (a forced unwind), and logs the same event trace as the C++ classes. Only
//! the scripts and the trace live here; every stream behaviour is `kicad-wx-stl-compat`'s.

use kicad_math::stream::FmtFlags;
use kicad_wx_stl_compat::ostream::{
    stock_do_put_long, BadAlloc, CallContext, InsertError, IosBase, OStream, OstreambufIterator,
    StreamBuf, StreamHost, StreamRefusal,
};
use serde_json::{json, Value};

/// The most events one insertion can log is 36 (five sentries, each with two tied syncs,
/// the buffer, facet and cancellation events and one destructor sync); the trace refuses
/// beyond this.
pub const MAX_EVENTS: usize = 64;

/// An exception a host operation throws, as the C++ adapter's classes throw it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostException {
    /// `std::runtime_error( what )` from a scripted call.
    Runtime(&'static str),
    /// `std::bad_alloc` from the buffer's growth.
    BadAlloc,
    /// The forced unwind (`abi::__forced_unwind`) of the thread's cancellation at a scripted
    /// call, by the named operation.
    ForcedUnwind(&'static str),
}

/// Both buffers and the facet throw the same exceptions.
fn forced(exception: &HostException) -> bool {
    matches!(exception, HostException::ForcedUnwind(_))
}

/// One logged host call; `n` is the number of bytes the main buffer held at the time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Event {
    /// `overflow( byte )` returned `eof()`.
    Eof(u8),
    /// A scripted `std::runtime_error` was thrown by the named operation.
    Throw(&'static str),
    /// The named operation cancelled the thread.
    Cancel(&'static str),
    /// The main buffer's `sync()` returned this.
    Sync(i32),
    /// The tied stream's buffer's `sync()` returned this.
    TieSync(i32),
    /// The scripted facet's `do_put( long )` was called with this value.
    DoPut(i64),
}

/// A bounded event trace.
#[derive(Clone, Debug, Default)]
struct Trace {
    events: Vec<(usize, Event)>,
    overrun: bool,
}

impl Trace {
    fn log(&mut self, n: usize, event: Event) {
        if self.events.len() >= MAX_EVENTS || self.events.try_reserve(1).is_err() {
            self.overrun = true;
            return;
        }
        self.events.push((n, event));
    }
}

/// A scripted call number: the call with this 0-based index throws.
pub type CallIndex = Option<u64>;

/// When the scripted facet throws `std::runtime_error`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PutThrow {
    /// Before it delegates to the stock body (after its `set_before` changes).
    Before,
    /// After it delegates (and its `set_after` changes).
    After,
    /// Never.
    None,
}

/// Changes the scripted facet makes to its `ios_base&`: `width( w )`, `precision( p )`,
/// `flags( f )`, each when given.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IosChange {
    pub width: Option<i64>,
    pub precision: Option<i64>,
    pub flags: Option<FmtFlags>,
}

impl IosChange {
    fn apply(&self, io: &mut IosBase<'_>) {
        if let Some(width) = self.width {
            io.set_width(width);
        }
        if let Some(precision) = self.precision {
            io.set_precision(precision);
        }
        if let Some(flags) = self.flags {
            io.set_flags(flags);
        }
    }
}

/// The facet script. Each `do_put( long )` call logs itself, makes the `set_before`
/// changes, throws when it is call `call` and `throw` is before, delegates `delegate` times
/// to the stock body (with `fill` and `value` when given, else its own), makes the
/// `set_after` changes, throws when it is call `call` and `throw` is after, and returns the
/// last iterator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PutScript {
    pub call: u64,
    pub throw: PutThrow,
    pub set_before: IosChange,
    pub set_after: IosChange,
    pub fill: Option<u8>,
    pub value: Option<i64>,
    pub delegate: u8,
}

/// The operation whose call cancels the thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancelOp {
    Overflow,
    Sync,
    NumPut,
    TieSync,
}

/// The cancellation script: `op`'s call with this 0-based index cancels the thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cancel {
    pub op: CancelOp,
    pub call: u64,
}

impl Cancel {
    fn hits(cancel: Option<Self>, op: CancelOp, index: u64) -> bool {
        cancel.is_some_and(|c| c.op == op && c.call == index)
    }
}

/// The tied stream's buffer: unbounded (a flush never writes), with a scripted `sync()`, or
/// none.
#[derive(Clone, Debug)]
pub struct TieSink {
    sync_fails: bool,
    sync_throw: CallIndex,
    cancel: Option<Cancel>,
    null_rdbuf: bool,
    sync_calls: u64,
    pending: Vec<Event>,
}

impl TieSink {
    pub fn new(
        sync_fails: bool,
        sync_throw: CallIndex,
        cancel: Option<Cancel>,
        null_rdbuf: bool,
    ) -> Self {
        Self {
            sync_fails,
            sync_throw,
            cancel,
            null_rdbuf,
            sync_calls: 0,
            pending: Vec::new(),
        }
    }

    /// How many times `sync()` was called.
    pub const fn sync_calls(&self) -> u64 {
        self.sync_calls
    }

    fn note(&mut self, event: Event) {
        // At most two per flush; drained after each.
        if self.pending.len() < MAX_EVENTS && self.pending.try_reserve(1).is_ok() {
            self.pending.push(event);
        }
    }
}

impl StreamHost for TieSink {
    type Exception = HostException;

    fn is_forced_unwind(exception: &HostException) -> bool {
        forced(exception)
    }

    fn has_rdbuf(&self) -> bool {
        !self.null_rdbuf
    }

    fn overflow(&mut self, _byte: u8) -> Result<bool, HostException> {
        // flush() never writes.
        Ok(true)
    }

    fn sync(&mut self) -> Result<i32, HostException> {
        let index = self.sync_calls;
        self.sync_calls += 1;
        if Cancel::hits(self.cancel, CancelOp::TieSync, index) {
            self.note(Event::Cancel("tie_sync"));
            return Err(HostException::ForcedUnwind("tie_sync"));
        }
        if Some(index) == self.sync_throw {
            self.note(Event::Throw("tie_sync"));
            return Err(HostException::Runtime("tie_sync"));
        }
        let result = if self.sync_fails { -1 } else { 0 };
        self.note(Event::TieSync(result));
        Ok(result)
    }
}

/// The main stream's buffer (or none), facet and tie.
#[derive(Debug)]
pub struct Sink {
    buf: StreamBuf,
    null_rdbuf: bool,
    overflow_throw: CallIndex,
    overflow_calls: u64,
    sync_throw: CallIndex,
    sync_calls: u64,
    put: Option<PutScript>,
    put_calls: u64,
    cancel: Option<Cancel>,
    tie: Option<OStream<TieSink>>,
    trace: Trace,
}

/// The main buffer's scripts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SinkScripts {
    pub overflow_throw: CallIndex,
    pub sync_throw: CallIndex,
    pub put: Option<PutScript>,
    pub cancel: Option<Cancel>,
    /// The stream is constructed on a null `rdbuf()`.
    pub null_rdbuf: bool,
}

impl Sink {
    /// A buffer of `capacity` bytes (`None`: unbounded) whose `sync()` returns -1 when
    /// `sync_fails`, with the given scripts.
    pub fn new(
        capacity: Option<usize>,
        sync_fails: bool,
        scripts: SinkScripts,
    ) -> Result<Self, StreamRefusal> {
        Ok(Self {
            buf: StreamBuf::new(capacity, sync_fails)?,
            null_rdbuf: scripts.null_rdbuf,
            overflow_throw: scripts.overflow_throw,
            overflow_calls: 0,
            sync_throw: scripts.sync_throw,
            sync_calls: 0,
            put: scripts.put,
            put_calls: 0,
            cancel: scripts.cancel,
            tie: None,
            trace: Trace::default(),
        })
    }

    /// `tie( &tied )`.
    pub fn set_tie(&mut self, tied: OStream<TieSink>) {
        self.tie = Some(tied);
    }

    /// The tied stream.
    pub const fn tie(&self) -> Option<&OStream<TieSink>> {
        self.tie.as_ref()
    }

    /// The bytes written.
    pub fn bytes(&self) -> &[u8] {
        self.buf.bytes()
    }

    /// How many times `sync()` was called, a throwing call included.
    pub const fn sync_calls(&self) -> u64 {
        self.sync_calls
    }

    /// Whether more events happened than [`MAX_EVENTS`].
    pub const fn overrun(&self) -> bool {
        self.trace.overrun
    }

    /// The trace as JSON: `[kind, n, detail]` per event.
    pub fn events_json(&self) -> Value {
        Value::Array(
            self.trace
                .events
                .iter()
                .map(|&(n, event)| match event {
                    Event::Eof(byte) => json!(["eof", n, byte]),
                    Event::Throw(what) => json!(["throw", n, what]),
                    Event::Cancel(what) => json!(["cancel", n, what]),
                    Event::Sync(result) => json!(["sync", n, result]),
                    Event::TieSync(result) => json!(["tie_sync", n, result]),
                    Event::DoPut(value) => json!(["do_put", n, value]),
                })
                .collect(),
        )
    }

    fn log(&mut self, event: Event) {
        let n = self.buf.bytes().len();
        self.trace.log(n, event);
    }
}

impl StreamHost for Sink {
    type Exception = HostException;

    fn is_forced_unwind(exception: &HostException) -> bool {
        forced(exception)
    }

    fn has_rdbuf(&self) -> bool {
        !self.null_rdbuf
    }

    fn overflow(&mut self, byte: u8) -> Result<bool, HostException> {
        let index = self.overflow_calls;
        self.overflow_calls += 1;
        if Cancel::hits(self.cancel, CancelOp::Overflow, index) {
            self.log(Event::Cancel("overflow"));
            return Err(HostException::ForcedUnwind("overflow"));
        }
        if Some(index) == self.overflow_throw {
            self.log(Event::Throw("overflow"));
            return Err(HostException::Runtime("overflow"));
        }
        match self.buf.overflow(byte) {
            Ok(true) => Ok(true),
            Ok(false) => {
                self.log(Event::Eof(byte));
                Ok(false)
            }
            Err(BadAlloc) => Err(HostException::BadAlloc),
        }
    }

    fn sync(&mut self) -> Result<i32, HostException> {
        let index = self.sync_calls;
        self.sync_calls += 1;
        if Cancel::hits(self.cancel, CancelOp::Sync, index) {
            self.log(Event::Cancel("sync"));
            return Err(HostException::ForcedUnwind("sync"));
        }
        if Some(index) == self.sync_throw {
            self.log(Event::Throw("sync"));
            return Err(HostException::Runtime("sync"));
        }
        let result = self
            .buf
            .sync()
            .map_err(|BadAlloc| HostException::BadAlloc)?;
        self.log(Event::Sync(result));
        Ok(result)
    }

    fn do_put_long(
        &mut self,
        out: OstreambufIterator,
        io: &mut IosBase<'_>,
        fill: u8,
        value: i64,
    ) -> Result<OstreambufIterator, InsertError<HostException>> {
        let Some(script) = self.put else {
            // The stock facet: no hook and no event.
            return stock_do_put_long(self, out, io, fill, value);
        };
        let index = self.put_calls;
        self.put_calls += 1;
        self.log(Event::DoPut(value));
        if Cancel::hits(self.cancel, CancelOp::NumPut, index) {
            self.log(Event::Cancel("num_put"));
            return Err(InsertError::Host(HostException::ForcedUnwind("num_put")));
        }
        script.set_before.apply(io);
        let throws = script.throw != PutThrow::None && index == script.call;
        if throws && script.throw == PutThrow::Before {
            self.log(Event::Throw("num_put"));
            return Err(InsertError::Host(HostException::Runtime("num_put")));
        }
        let fill = script.fill.unwrap_or(fill);
        let value = script.value.unwrap_or(value);
        let mut out = out;
        for _ in 0..script.delegate {
            out = stock_do_put_long(self, out, io, fill, value)?;
        }
        script.set_after.apply(io);
        if throws {
            self.log(Event::Throw("num_put"));
            return Err(InsertError::Host(HostException::Runtime("num_put")));
        }
        Ok(out)
    }

    fn has_tie(&self) -> bool {
        self.tie.is_some()
    }

    fn flush_tie(&mut self, context: CallContext) -> Result<(), InsertError<HostException>> {
        let Some(tie) = self.tie.as_mut() else {
            return Ok(());
        };
        let result = tie.flush(context);
        let pending = core::mem::take(&mut tie.host_mut().pending);
        for event in pending {
            self.log(event);
        }
        result
    }

    fn capacity_left(&self) -> Option<usize> {
        self.buf.capacity_left()
    }
}
