// SPDX-License-Identifier: GPL-3.0-or-later
//! The caller's program around the stream, as the C++ adapter builds it: its `SINK`
//! stream buffer, its optional throwing `NUM_PUT` facet and its optional tied stream. Each
//! operation may throw a `std::runtime_error` at a scripted call, and logs the same event
//! trace as the C++ classes. Only the scripts and the trace live here; every stream
//! behaviour is `kicad-wx-stl-compat`'s.

use kicad_wx_stl_compat::ostream::{
    BadAlloc, InsertError, OStream, StockPut, StreamBuf, StreamHost, StreamRefusal,
};
use serde_json::{json, Value};

/// The most events one insertion can log is 30 (five sentries, each with two tied syncs,
/// three buffer or facet events and one destructor sync); the trace refuses beyond this.
pub const MAX_EVENTS: usize = 64;

/// An exception a host operation throws, as the C++ adapter's classes throw it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostException {
    /// `std::runtime_error( what )` from a scripted call.
    Runtime(&'static str),
    /// `std::bad_alloc` from the buffer's growth.
    BadAlloc,
}

/// One logged host call; `n` is the number of bytes the main buffer held at the time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Event {
    /// `overflow( byte )` returned `eof()`.
    Eof(u8),
    /// A scripted `std::runtime_error` was thrown by the named operation.
    Throw(&'static str),
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

/// The facet script: the `do_put( long )` call that throws, before or after the stock body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PutScript {
    pub call: u64,
    pub after: bool,
}

/// The tied stream's buffer: unbounded (a flush never writes), with a scripted `sync()`.
#[derive(Clone, Debug)]
pub struct TieSink {
    sync_fails: bool,
    sync_throw: CallIndex,
    sync_calls: u64,
    pending: Vec<Event>,
}

impl TieSink {
    pub fn new(sync_fails: bool, sync_throw: CallIndex) -> Self {
        Self {
            sync_fails,
            sync_throw,
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

    fn overflow(&mut self, _byte: u8) -> Result<bool, HostException> {
        // flush() never writes.
        Ok(true)
    }

    fn sync(&mut self) -> Result<i32, HostException> {
        let index = self.sync_calls;
        self.sync_calls += 1;
        if Some(index) == self.sync_throw {
            self.note(Event::Throw("tie_sync"));
            return Err(HostException::Runtime("tie_sync"));
        }
        let result = if self.sync_fails { -1 } else { 0 };
        self.note(Event::TieSync(result));
        Ok(result)
    }
}

/// The main stream's buffer, facet and tie.
#[derive(Clone, Debug)]
pub struct Sink {
    buf: StreamBuf,
    overflow_throw: CallIndex,
    overflow_calls: u64,
    sync_throw: CallIndex,
    sync_calls: u64,
    put: Option<PutScript>,
    put_calls: u64,
    tie: Option<OStream<TieSink>>,
    trace: Trace,
}

impl Sink {
    /// A buffer of `capacity` bytes (`None`: unbounded) whose `sync()` returns -1 when
    /// `sync_fails`, with the given scripts.
    pub fn new(
        capacity: Option<usize>,
        sync_fails: bool,
        overflow_throw: CallIndex,
        sync_throw: CallIndex,
        put: Option<PutScript>,
    ) -> Result<Self, StreamRefusal> {
        Ok(Self {
            buf: StreamBuf::new(capacity, sync_fails)?,
            overflow_throw,
            overflow_calls: 0,
            sync_throw,
            sync_calls: 0,
            put,
            put_calls: 0,
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

    fn overflow(&mut self, byte: u8) -> Result<bool, HostException> {
        let index = self.overflow_calls;
        self.overflow_calls += 1;
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

    fn do_put_long(&mut self, stock: StockPut<'_>) -> Result<bool, HostException> {
        let Some(script) = self.put else {
            // The stock facet: no hook and no event.
            return stock.put(self);
        };
        let index = self.put_calls;
        self.put_calls += 1;
        self.log(Event::DoPut(stock.value()));
        let throws = index == script.call;
        if throws && !script.after {
            self.log(Event::Throw("num_put"));
            return Err(HostException::Runtime("num_put"));
        }
        let failed = stock.put(self)?;
        if throws {
            self.log(Event::Throw("num_put"));
            return Err(HostException::Runtime("num_put"));
        }
        Ok(failed)
    }

    fn has_tie(&self) -> bool {
        self.tie.is_some()
    }

    fn flush_tie(&mut self) -> Result<(), InsertError<HostException>> {
        let Some(tie) = self.tie.as_mut() else {
            return Ok(());
        };
        let result = tie.flush();
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
