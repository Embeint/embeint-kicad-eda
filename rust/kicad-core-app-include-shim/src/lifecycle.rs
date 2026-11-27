// SPDX-License-Identifier: GPL-3.0-or-later
//! `inplace_lifecycle`: `INPLACE_FUNCTION<long long( int )>` (default capacity) storing a
//! callable that records its own special members, as the C++ adapter's `PROBE` does: its
//! construction, every move and copy constructor (or a copy that fails), every call and
//! every destruction (of a live or a moved-from object). Each step reports the events it
//! caused, in order, so the move-then-destroy-the-source sequence of `moveFrom`, the
//! temporaries of the assignment operators and the strong guarantee of copy assignment are
//! compared event for event.

use super::{fail, fields, member, signed, storage_refusal, text, Obj, Res, MAX_STEPS, SLOTS};
use kicad_core::inplace_function::{Callable, CopyError, CopyFailure, InplaceFunction};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::rc::Rc;

/// The case's event log and the number of copies that may still succeed.
#[derive(Default)]
struct Log {
    events: Vec<String>,
    copies_left: i64,
}

/// The recording callable (`PROBE`).
struct Probe {
    log: Rc<RefCell<Log>>,
    id: i32,
    moved: bool,
}

impl Probe {
    /// `PROBE( aLog, aId )`.
    fn new(log: &Rc<RefCell<Log>>, id: i32) -> Self {
        log.borrow_mut().events.push(format!("construct {id}"));
        Probe {
            log: Rc::clone(log),
            id,
            moved: false,
        }
    }

    fn record(&self, event: String) {
        self.log.borrow_mut().events.push(event);
    }
}

impl Drop for Probe {
    /// `~PROBE()`.
    fn drop(&mut self) {
        let state = if self.moved { "moved-from" } else { "live" };
        self.record(format!("destroy {} {state}", self.id));
    }
}

impl Callable<(i32,), i64> for Probe {
    /// `long long operator()( int ) const`.
    fn invoke(&self, args: (i32,)) -> i64 {
        self.record(format!("call {}", self.id));
        i64::from(self.id) + i64::from(args.0)
    }

    /// `PROBE( PROBE&& ) noexcept`.
    fn move_construct(&mut self) -> Option<Self> {
        self.record(format!("move {}", self.id));
        self.moved = true;
        Some(Probe {
            log: Rc::clone(&self.log),
            id: self.id,
            moved: false,
        })
    }

    /// `PROBE( const PROBE& )`: throws once the case's copies are used up.
    fn copy_construct(&self) -> Result<Self, CopyFailure> {
        let mut log = self.log.borrow_mut();
        if log.copies_left == 0 {
            log.events.push(format!("copy-failed {}", self.id));
            return Err(CopyFailure);
        }
        log.copies_left -= 1;
        log.events.push(format!("copy {}", self.id));
        Ok(Probe {
            log: Rc::clone(&self.log),
            id: self.id,
            moved: false,
        })
    }
}

type Function = InplaceFunction<'static, (i32,), i64>;

fn slot_index(o: &Obj, name: &str) -> Res<usize> {
    Ok(signed(member(o, name)?, name, 0, SLOTS as i64 - 1)? as usize)
}

fn live<'a>(
    slots: &'a mut [Option<Function>; SLOTS],
    o: &Obj,
    name: &str,
) -> Res<&'a mut Function> {
    let index = slot_index(o, name)?;
    slots
        .get_mut(index)
        .and_then(Option::as_mut)
        .ok_or_else(|| "slot not constructed".to_string())
}

fn vacant(slots: &[Option<Function>; SLOTS], o: &Obj) -> Res<usize> {
    let index = slot_index(o, "slot")?;
    if slots.get(index).is_some_and(Option::is_some) {
        return fail("slot already constructed");
    }
    Ok(index)
}

fn read_id(o: &Obj) -> Res<i32> {
    Ok(signed(member(o, "id")?, "id", i32::MIN.into(), i32::MAX.into())? as i32)
}

const COPY_FAILED: &str = "copy failed";

/// The outcome of a copy: nothing, or the exception that ended the operation. The storage
/// invariant, which no C++ exception stands for, refuses the request.
fn copied(result: Result<(), CopyError>) -> Res<Value> {
    match result {
        Ok(()) => Ok(Value::Null),
        Err(CopyError::Copy(CopyFailure)) => Ok(json!({"exception": COPY_FAILED})),
        Err(CopyError::Storage(invariant)) => Err(storage_refusal(invariant)),
    }
}

pub(super) fn lifecycle_op(v: &Value) -> Res<Value> {
    let o = fields(v, &["op", "copies", "steps"])?;
    let log = Rc::new(RefCell::new(Log {
        events: Vec::new(),
        copies_left: signed(member(o, "copies")?, "copies", 0, MAX_STEPS as i64)?,
    }));
    let steps = super::array(o, "steps", MAX_STEPS)?;
    let steps = steps
        .iter()
        .map(|s| {
            s.as_object()
                .filter(|o| o.contains_key("step"))
                .ok_or_else(|| "expected step object".to_string())
        })
        .collect::<Res<Vec<_>>>()?;

    let mut slots: [Option<Function>; SLOTS] = Default::default();
    let mut results = Vec::new();

    for o in steps {
        let step = Value::Object(o.clone());
        let outcome = match text(o, "step")?.as_str() {
            "copies" => {
                fields(&step, &["step", "count"])?;
                log.borrow_mut().copies_left =
                    signed(member(o, "count")?, "count", 0, MAX_STEPS as i64)?;
                Value::Null
            }
            "construct_empty" => {
                fields(&step, &["step", "slot"])?;
                let index = vacant(&slots, o)?;
                if let Some(slot) = slots.get_mut(index) {
                    *slot = Some(Function::new());
                }
                Value::Null
            }
            "construct" => {
                // `emplace( PROBE( log, id ) )`: an rvalue, moved into the storage.
                fields(&step, &["step", "slot", "id"])?;
                let index = vacant(&slots, o)?;
                let id = read_id(o)?;
                let function =
                    Function::from_callable(Probe::new(&log, id)).map_err(storage_refusal)?;
                if let Some(slot) = slots.get_mut(index) {
                    *slot = Some(function);
                }
                Value::Null
            }
            "construct_copy" => {
                // `PROBE p( log, id ); emplace( p );`: an lvalue, copied into the storage.
                fields(&step, &["step", "slot", "id"])?;
                let index = vacant(&slots, o)?;
                let id = read_id(o)?;
                let probe = Probe::new(&log, id);
                let made = Function::try_from_copy(&probe);
                drop(probe);
                match made {
                    Ok(function) => {
                        if let Some(slot) = slots.get_mut(index) {
                            *slot = Some(function);
                        }
                        Value::Null
                    }
                    Err(failure) => copied(Err(failure))?,
                }
            }
            "copy_construct" => {
                fields(&step, &["step", "slot", "from"])?;
                live(&mut slots, o, "from")?;
                let index = vacant(&slots, o)?;
                let made = live(&mut slots, o, "from")?.try_clone();
                match made {
                    Ok(function) => {
                        if let Some(slot) = slots.get_mut(index) {
                            *slot = Some(function);
                        }
                        Value::Null
                    }
                    Err(failure) => copied(Err(failure))?,
                }
            }
            "move_construct" => {
                fields(&step, &["step", "slot", "from"])?;
                live(&mut slots, o, "from")?;
                let index = vacant(&slots, o)?;
                let moved =
                    Function::take(live(&mut slots, o, "from")?).map_err(storage_refusal)?;
                if let Some(slot) = slots.get_mut(index) {
                    *slot = Some(moved);
                }
                Value::Null
            }
            "assign" => {
                // `*slot = PROBE( log, id );`
                fields(&step, &["step", "slot", "id"])?;
                let target = live(&mut slots, o, "slot")?;
                let id = read_id(o)?;
                target
                    .assign_callable(Probe::new(&log, id))
                    .map_err(storage_refusal)?;
                Value::Null
            }
            "assign_copy" => {
                // `PROBE p( log, id ); *slot = p;`
                fields(&step, &["step", "slot", "id"])?;
                let target = live(&mut slots, o, "slot")?;
                let id = read_id(o)?;
                let probe = Probe::new(&log, id);
                let result = target.try_assign_copy(&probe);
                drop(probe);
                copied(result)?
            }
            "copy_assign" | "move_assign" => {
                fields(&step, &["step", "slot", "from"])?;
                let target = slot_index(o, "slot")?;
                live(&mut slots, o, "slot")?;
                let from = slot_index(o, "from")?;
                live(&mut slots, o, "from")?;
                let copy = o.get("step").and_then(Value::as_str) == Some("copy_assign");
                if target == from {
                    let function = live(&mut slots, o, "slot")?;
                    if copy {
                        copied(function.try_copy_assign_self())?
                    } else {
                        function.move_assign_self();
                        Value::Null
                    }
                } else {
                    let mut source = slots
                        .get_mut(from)
                        .and_then(Option::take)
                        .ok_or("slot not constructed")?;
                    let mut result = Ok(());
                    if let Some(Some(destination)) = slots.get_mut(target) {
                        if copy {
                            result = destination.try_copy_assign(&source);
                        } else {
                            result = destination
                                .move_assign(&mut source)
                                .map_err(CopyError::from);
                        }
                    }
                    if let Some(slot) = slots.get_mut(from) {
                        *slot = Some(source);
                    }
                    copied(result)?
                }
            }
            "destroy" => {
                fields(&step, &["step", "slot"])?;
                live(&mut slots, o, "slot")?;
                if let Some(slot) = slots.get_mut(slot_index(o, "slot")?) {
                    *slot = None;
                }
                Value::Null
            }
            "bool" => {
                fields(&step, &["step", "slot"])?;
                json!(live(&mut slots, o, "slot")?.is_set())
            }
            "invoke" => {
                fields(&step, &["step", "slot", "arg"])?;
                let function = live(&mut slots, o, "slot")?;
                let arg =
                    signed(member(o, "arg")?, "arg", i32::MIN.into(), i32::MAX.into())? as i32;
                match function.call((arg,)) {
                    Ok(value) => json!({"value": value}),
                    Err(_) => json!({"exception": "bad_function_call"}),
                }
            }
            _ => return fail("unknown step"),
        };
        let events: Vec<String> = std::mem::take(&mut log.borrow_mut().events);
        results.push(json!({"outcome": outcome, "events": events}));
    }

    // The remaining slots are destroyed in slot order.
    for slot in &mut slots {
        *slot = None;
    }
    let events: Vec<String> = std::mem::take(&mut log.borrow_mut().events);
    Ok(json!({"results": results, "final_events": events}))
}
