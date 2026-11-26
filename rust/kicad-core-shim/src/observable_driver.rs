// SPDX-License-Identifier: GPL-3.0-or-later
//! Scripted observable driver, mirroring `port/oracle/shims/core_p001.cpp`.
//!
//! The C++ adapter drives the real `UTIL::OBSERVABLE<OBSERVER>` header template; this
//! driver drives its port, [`kicad_core::observable_template`] (unit
//! `libs.core.include.core.application.pure.p001`), with the same script.
//!
//! Script domain (identical on both sides). Steps are validated completely before
//! anything runs. Everything the reference defines runs; the refusals are exactly the
//! undefined points, where the reference would use a dangling `this`, dereference a
//! null `impl_`, or keep using a range-for iterator its own list has invalidated:
//! * `drop` of an observable inside its own `notify`: `Notify` calls
//!   `leave_iteration()` on the destroyed object after its loop. (`NotifyIgnore`
//!   never touches `this` after the loop, so dropping there is defined.)
//! * `unsubscribe` with a null `impl_` (a null dereference);
//! * adding to a list an active `Notify`/`NotifyIgnore` loop is ranging over
//!   (`push_back` invalidates at least that loop's end iterator);
//! * a removal that erases from a list an active loop is ranging over. The list's
//!   `remove_observer` erases instead of nulling only when its count is 0, which
//!   inside a loop happens only after the unsigned count wrapped (an unpaired
//!   `leave_iteration` left it at `UINT_MAX`). Checked before the removal;
//! * a step after which an active loop's list has been compacted (`collect` erased
//!   nulls when a nested `leave_iteration` brought the count to 0) or destroyed (its
//!   last reference went, e.g. `leave_iteration`'s release or `on_observers_empty`
//!   inside the loop). Checked when the step returns, before the loop reads again.
//!
//! * `throw` while an enclosing `notify_ignore` belongs to an observable that was
//!   dropped inside it: the exception unwinds through that template's `catch( ... )`,
//!   which calls `leave_iteration()` on the destroyed object.
//!
//! The wrap itself, nested notifications in a wrapped loop that compact nothing,
//! and the base operations inside callbacks are defined and run. The adding and
//! removing refusals become a skip logged as `{"skipped": op}` when the step carries
//! `guard: true`; the post-step refusals have no guarded form.
//!
//! `throw` makes the observer being called throw an exception (outside a callback,
//! the step itself throws). It unwinds through every enclosing template, whose
//! `catch( ... )` calls `leave_iteration()` and rethrows, and is caught after the
//! top-level step, whose event then carries `"thrown": true`.

use crate::operations::{boolean, fields, signed, text, Error, Result};
use kicad_core::observable::{
    ArenaId, ImplId, Link, ObservableArena, ObservableError, ObserverId, ObserverPtr,
};
use kicad_core::observable_template::{self, NotifyError, NotifyHost, Observable};
use serde_json::{json, Value};

pub const OBSERVABLES: usize = 8;
pub const LINKS: usize = 16;
pub const OBSERVERS: i64 = 8;
pub const MAX_STEPS: usize = 2048;
pub const MAX_DEPTH: usize = 8;
pub const BUDGET: usize = 100_000;

enum Step {
    New {
        obs: usize,
    },
    Copy {
        obs: usize,
        from: usize,
    },
    Drop {
        obs: usize,
    },
    Subscribe {
        obs: usize,
        observer: ObserverPtr,
        link: usize,
        guard: bool,
    },
    SubscribeUnmanaged {
        obs: usize,
        observer: ObserverPtr,
        guard: bool,
    },
    Unsubscribe {
        obs: usize,
        observer: ObserverPtr,
        guard: bool,
    },
    /// `ignore: None` is `Notify`; `Some(pointer)` is `NotifyIgnore( ..., pointer, ... )`.
    Notify {
        obs: usize,
        event: i64,
        ignore: Option<ObserverPtr>,
        reactions: Vec<(ObserverId, Vec<Step>)>,
    },
    EnterIteration {
        obs: usize,
    },
    LeaveIteration {
        obs: usize,
    },
    OnObserversEmpty {
        obs: usize,
    },
    LinkNew {
        link: usize,
    },
    LinkToken {
        link: usize,
        obs: usize,
        observer: ObserverPtr,
    },
    LinkMove {
        link: usize,
        from: usize,
    },
    LinkAssign {
        link: usize,
        from: usize,
        guard: bool,
    },
    LinkReset {
        link: usize,
        guard: bool,
    },
    LinkDrop {
        link: usize,
        guard: bool,
    },
    /// The observer being notified throws (an exception, not a refusal).
    Throw,
}

/// The observer interface of the script (`OBSERVER` in the C++ adapter).
pub struct Observer;

/// How a driver step ends abnormally: a refusal, or the scripted exception unwinding.
enum Fault {
    Refused(Error),
    Thrown,
}

impl From<Error> for Fault {
    fn from(error: Error) -> Self {
        Self::Refused(error)
    }
}

impl From<&str> for Fault {
    fn from(message: &str) -> Self {
        Self::Refused(message.into())
    }
}

type Outcome<T> = core::result::Result<T, Fault>;

fn index(input: &Value, name: &str, count: usize) -> Result<usize> {
    let max = i64::try_from(count).unwrap_or(i64::MAX) - 1;
    usize::try_from(signed(input, name, 0, max)?)
        .map_err(|_| Error::Invalid(format!("integer out of range: {name}")))
}

fn pointer(input: &Value, name: &str) -> Result<ObserverPtr> {
    match input.get(name) {
        Some(Value::Null) => Ok(None),
        _ => Ok(Some(observer_id(input, name)?)),
    }
}

fn observer_id(input: &Value, name: &str) -> Result<ObserverId> {
    let value = signed(input, name, 0, OBSERVERS - 1)?;
    u32::try_from(value)
        .map(ObserverId)
        .map_err(|_| Error::Invalid(format!("integer out of range: {name}")))
}

fn guard(input: &Value) -> Result<bool> {
    if input.get("guard").is_some() {
        boolean(input, "guard")
    } else {
        Ok(false)
    }
}

struct Parser {
    count: usize,
}

impl Parser {
    fn steps(&mut self, value: Option<&Value>, depth: usize) -> Result<Vec<Step>> {
        let list = value
            .and_then(Value::as_array)
            .ok_or("expected array: steps")?;
        list.iter().map(|step| self.step(step, depth)).collect()
    }

    fn step(&mut self, input: &Value, depth: usize) -> Result<Step> {
        self.count += 1;
        if self.count > MAX_STEPS {
            return Err("too many steps".into());
        }
        if depth > MAX_DEPTH {
            return Err("nesting too deep".into());
        }
        let op = text(input, "op")?;
        let base_op = |input: &Value| -> Result<usize> {
            fields(input, &["op", "obs"], &[])?;
            index(input, "obs", OBSERVABLES)
        };
        Ok(match op {
            "new" => {
                fields(input, &["op", "obs"], &[])?;
                Step::New {
                    obs: index(input, "obs", OBSERVABLES)?,
                }
            }
            "copy" => {
                fields(input, &["op", "obs", "from"], &[])?;
                Step::Copy {
                    obs: index(input, "obs", OBSERVABLES)?,
                    from: index(input, "from", OBSERVABLES)?,
                }
            }
            "drop" => {
                fields(input, &["op", "obs"], &[])?;
                Step::Drop {
                    obs: index(input, "obs", OBSERVABLES)?,
                }
            }
            "subscribe" => {
                fields(input, &["op", "obs", "observer", "link"], &["guard"])?;
                Step::Subscribe {
                    obs: index(input, "obs", OBSERVABLES)?,
                    observer: pointer(input, "observer")?,
                    link: index(input, "link", LINKS)?,
                    guard: guard(input)?,
                }
            }
            "subscribe_unmanaged" | "unsubscribe" => {
                fields(input, &["op", "obs", "observer"], &["guard"])?;
                let obs = index(input, "obs", OBSERVABLES)?;
                let observer = pointer(input, "observer")?;
                let guard = guard(input)?;
                if op == "unsubscribe" {
                    Step::Unsubscribe {
                        obs,
                        observer,
                        guard,
                    }
                } else {
                    Step::SubscribeUnmanaged {
                        obs,
                        observer,
                        guard,
                    }
                }
            }
            "notify" | "notify_ignore" => {
                let ignore_variant = op == "notify_ignore";
                if ignore_variant {
                    fields(input, &["op", "obs", "event", "ignore", "reactions"], &[])?;
                } else {
                    fields(input, &["op", "obs", "event", "reactions"], &[])?;
                }
                let obs = index(input, "obs", OBSERVABLES)?;
                let event = signed(input, "event", -1_000_000, 1_000_000)?;
                let ignore = if ignore_variant {
                    Some(pointer(input, "ignore")?)
                } else {
                    None
                };
                let list = input
                    .get("reactions")
                    .and_then(Value::as_array)
                    .ok_or("expected array: reactions")?;
                let mut reactions: Vec<(ObserverId, Vec<Step>)> = Vec::new();
                for reaction in list {
                    fields(reaction, &["observer", "steps"], &[])?;
                    let observer = observer_id(reaction, "observer")?;
                    if reactions.iter().any(|(o, _)| *o == observer) {
                        return Err("duplicate reaction".into());
                    }
                    let steps = self.steps(reaction.get("steps"), depth + 1)?;
                    reactions.push((observer, steps));
                }
                Step::Notify {
                    obs,
                    event,
                    ignore,
                    reactions,
                }
            }
            "enter_iteration" => Step::EnterIteration {
                obs: base_op(input)?,
            },
            "leave_iteration" => Step::LeaveIteration {
                obs: base_op(input)?,
            },
            "on_observers_empty" => Step::OnObserversEmpty {
                obs: base_op(input)?,
            },
            "link_new" => {
                fields(input, &["op", "link"], &[])?;
                Step::LinkNew {
                    link: index(input, "link", LINKS)?,
                }
            }
            "link_reset" | "link_drop" => {
                fields(input, &["op", "link"], &["guard"])?;
                let link = index(input, "link", LINKS)?;
                let guard = guard(input)?;
                if op == "link_reset" {
                    Step::LinkReset { link, guard }
                } else {
                    Step::LinkDrop { link, guard }
                }
            }
            "link_token" => {
                fields(input, &["op", "link", "obs", "observer"], &[])?;
                Step::LinkToken {
                    link: index(input, "link", LINKS)?,
                    obs: index(input, "obs", OBSERVABLES)?,
                    observer: pointer(input, "observer")?,
                }
            }
            "link_move" => {
                fields(input, &["op", "link", "from"], &[])?;
                Step::LinkMove {
                    link: index(input, "link", LINKS)?,
                    from: index(input, "from", LINKS)?,
                }
            }
            "throw" => {
                fields(input, &["op"], &[])?;
                Step::Throw
            }
            "link_assign" => {
                fields(input, &["op", "link", "from"], &["guard"])?;
                Step::LinkAssign {
                    link: index(input, "link", LINKS)?,
                    from: index(input, "from", LINKS)?,
                    guard: guard(input)?,
                }
            }
            _ => return Err("unknown operation".into()),
        })
    }
}

fn handle(error: ObservableError) -> Error {
    match error {
        ObservableError::NoImpl => "unsubscribe without impl".into(),
        ObservableError::StaleHandle => "stale observable handle".into(),
        ObservableError::ForeignHandle => "foreign observable handle".into(),
        ObservableError::Allocation => "allocation failed".into(),
    }
}

struct Driver {
    arena: ObservableArena,
    observables: Vec<Option<Observable<Observer>>>,
    /// Bumped whenever a slot receives a new observable, so a notification can tell
    /// its own object from a later one in the same slot.
    generations: [u64; OBSERVABLES],
    /// Executing notifications, outermost first: the slot and its generation.
    frames: Vec<(usize, u64)>,
    links: Vec<Link>,
    present: [bool; LINKS],
    /// Lists ranged over by active `Notify`/`NotifyIgnore` loops, outermost first, with
    /// each list's size when its loop began (the range-for's fixed end).
    active: Vec<(ImplId, usize)>,
    /// Observable slots whose `Notify` (not `NotifyIgnore`) loop is executing. A slot
    /// cannot be emptied while listed, so it still holds the notifying object.
    notifying: Vec<usize>,
    calls: Vec<Value>,
    budget: usize,
}

fn observer_json(pointer: ObserverPtr) -> Value {
    pointer.map_or(Value::Null, |o| json!(o.0))
}

impl Driver {
    fn tick(&mut self) -> Result<()> {
        self.budget += 1;
        if self.budget > BUDGET {
            return Err("execution budget exceeded".into());
        }
        Ok(())
    }

    fn observable(&mut self, obs: usize) -> Result<&mut Observable<Observer>> {
        self.observables
            .get_mut(obs)
            .and_then(Option::as_mut)
            .ok_or_else(|| "observable slot empty".into())
    }

    fn vacant(&self, obs: usize) -> Result<()> {
        match self.observables.get(obs) {
            Some(None) => Ok(()),
            _ => Err("observable slot occupied".into()),
        }
    }

    fn link_vacant(&self, link: usize) -> Result<()> {
        if self.present.get(link).copied().unwrap_or(true) {
            return Err("link slot occupied".into());
        }
        Ok(())
    }

    fn link_present(&self, link: usize) -> Result<()> {
        if !self.present.get(link).copied().unwrap_or(false) {
            return Err("link slot empty".into());
        }
        Ok(())
    }

    fn link(&mut self, link: usize) -> Result<&mut Link> {
        self.links
            .get_mut(link)
            .ok_or_else(|| "link slot empty".into())
    }

    fn set_link(&mut self, link: usize, value: Link) -> Result<()> {
        *self.link(link)? = value;
        if let Some(flag) = self.present.get_mut(link) {
            *flag = true;
        }
        Ok(())
    }

    fn is_active(&self, id: ImplId) -> bool {
        self.active.iter().any(|(active, _)| *active == id)
    }

    /// True when adding to `obs`'s list would push into a list an active loop ranges over.
    fn adds_during_iteration(&mut self, obs: usize) -> Result<bool> {
        let id = self.observable(obs)?.base().impl_ref().map(|r| r.id());
        Ok(id.is_some_and(|id| self.is_active(id)))
    }

    /// True when `IMPL::remove_observer( observer )` on list `id` would erase from a
    /// list an active loop ranges over: it erases (rather than nulls) the first match
    /// when the count is 0, and does nothing when there is no match.
    fn erases_during_iteration(&self, id: Option<ImplId>, observer: ObserverPtr) -> Result<bool> {
        let Some(id) = id.filter(|id| self.is_active(*id)) else {
            return Ok(false);
        };
        Ok(self.arena.iteration_count(id).map_err(handle)? == 0
            && self
                .arena
                .observers(id)
                .map_err(handle)?
                .contains(&observer))
    }

    /// The removal `LINK::reset` on `link` would make, checked like
    /// [`Self::erases_during_iteration`].
    fn link_erases_during_iteration(&self, link: usize) -> Result<bool> {
        let value = self.links.get(link).ok_or("link slot empty")?;
        self.erases_during_iteration(value.token().map(|r| r.id()), value.observer())
    }

    /// After a step inside a callback: every active loop's list must still exist with
    /// the size it had when its loop began, or the loop would next read through an
    /// invalidated iterator.
    fn check_active(&self) -> Result<()> {
        for (id, size) in &self.active {
            match self.arena.observers(*id) {
                Err(_) => return Err("list destroyed during active iteration".into()),
                Ok(list) if list.len() != *size => {
                    return Err("list compacted during active iteration".into())
                }
                Ok(_) => {}
            }
        }
        Ok(())
    }

    fn skipped(&mut self, op: &str) {
        self.calls.push(json!({"skipped": op}));
    }

    /// Place a new observable in a vacant slot.
    fn place(&mut self, obs: usize, value: Observable<Observer>) {
        if let Some(slot) = self.observables.get_mut(obs) {
            *slot = Some(value);
        }
        if let Some(generation) = self.generations.get_mut(obs) {
            *generation = generation.wrapping_add(1);
        }
    }

    fn exec(&mut self, step: &Step, depth: usize) -> Outcome<()> {
        self.exec_step(step, depth)?;
        Ok(self.check_active()?)
    }

    fn exec_step(&mut self, step: &Step, depth: usize) -> Outcome<()> {
        self.tick()?;
        match step {
            Step::New { obs } => {
                self.vacant(*obs)?;
                self.place(*obs, Observable::new());
            }
            Step::Copy { obs, from } => {
                self.vacant(*obs)?;
                let mut source = self
                    .observables
                    .get_mut(*from)
                    .and_then(Option::take)
                    .ok_or("observable slot empty")?;
                let copy = Observable::inherit(&mut self.arena, &mut source);
                if let Some(slot) = self.observables.get_mut(*from) {
                    *slot = Some(source);
                }
                let copy = copy.map_err(handle)?;
                self.place(*obs, copy);
            }
            Step::Drop { obs } => {
                self.observable(*obs)?;
                // Notify calls leave_iteration() on itself after its loop, so a drop that
                // is reached inside the object's own Notify is undefined. Drops in
                // reactions that never run are never executed and so are not refused.
                if self.notifying.contains(obs) {
                    return Err("drop during own notification".into());
                }
                let base = self
                    .observables
                    .get_mut(*obs)
                    .and_then(Option::take)
                    .ok_or("observable slot empty")?;
                self.arena
                    .drop_observable(base.into_base())
                    .map_err(handle)?;
            }
            Step::Subscribe {
                obs,
                observer,
                link,
                guard,
            } => {
                self.observable(*obs)?;
                self.link_vacant(*link)?;
                if self.adds_during_iteration(*obs)? {
                    if *guard {
                        self.skipped("subscribe");
                        return Ok(());
                    }
                    return Err("subscribe during active iteration".into());
                }
                let Driver {
                    arena, observables, ..
                } = self;
                let observable = observables
                    .get_mut(*obs)
                    .and_then(Option::as_mut)
                    .ok_or("observable slot empty")?;
                let link_value = observable.subscribe(arena, *observer).map_err(handle)?;
                self.set_link(*link, link_value)?;
            }
            Step::SubscribeUnmanaged {
                obs,
                observer,
                guard,
            } => {
                self.observable(*obs)?;
                if self.adds_during_iteration(*obs)? {
                    if *guard {
                        self.skipped("subscribe_unmanaged");
                        return Ok(());
                    }
                    return Err("subscribe during active iteration".into());
                }
                let Driver {
                    arena, observables, ..
                } = self;
                let observable = observables
                    .get_mut(*obs)
                    .and_then(Option::as_mut)
                    .ok_or("observable slot empty")?;
                observable
                    .subscribe_unmanaged(arena, *observer)
                    .map_err(handle)?;
            }
            Step::Unsubscribe {
                obs,
                observer,
                guard,
            } => {
                let id = match self.observable(*obs)?.base().impl_ref() {
                    Some(reference) => reference.id(),
                    // OBSERVABLE_BASE::remove_observer dereferences a null impl_.
                    None if *guard => {
                        self.skipped("unsubscribe");
                        return Ok(());
                    }
                    None => return Err(handle(ObservableError::NoImpl).into()),
                };
                if self.erases_during_iteration(Some(id), *observer)? {
                    if *guard {
                        self.skipped("unsubscribe");
                        return Ok(());
                    }
                    return Err("remove during active iteration".into());
                }
                let Driver {
                    arena, observables, ..
                } = self;
                let observable = observables
                    .get_mut(*obs)
                    .and_then(Option::as_mut)
                    .ok_or("observable slot empty")?;
                observable.unsubscribe(arena, *observer).map_err(handle)?;
            }
            Step::Notify {
                obs,
                event,
                ignore,
                reactions,
            } => self.notify(*obs, *event, *ignore, reactions, depth)?,
            Step::EnterIteration { obs } => {
                let Driver {
                    arena, observables, ..
                } = self;
                let base = observables
                    .get(*obs)
                    .and_then(Option::as_ref)
                    .ok_or("observable slot empty")?
                    .base();
                arena.enter_iteration(base).map_err(handle)?;
            }
            Step::LeaveIteration { obs } => {
                let Driver {
                    arena, observables, ..
                } = self;
                let base = observables
                    .get_mut(*obs)
                    .and_then(Option::as_mut)
                    .ok_or("observable slot empty")?
                    .base_mut();
                arena.leave_iteration(base).map_err(handle)?;
            }
            Step::OnObserversEmpty { obs } => {
                let Driver {
                    arena, observables, ..
                } = self;
                let base = observables
                    .get_mut(*obs)
                    .and_then(Option::as_mut)
                    .ok_or("observable slot empty")?
                    .base_mut();
                arena.on_observers_empty(base).map_err(handle)?;
            }
            Step::LinkNew { link } => {
                self.link_vacant(*link)?;
                self.set_link(*link, Link::new())?;
            }
            Step::LinkToken {
                link,
                obs,
                observer,
            } => {
                self.link_vacant(*link)?;
                let Driver {
                    arena, observables, ..
                } = self;
                let base = observables
                    .get(*obs)
                    .and_then(Option::as_ref)
                    .ok_or("observable slot empty")?
                    .base();
                let token = arena.share_impl(base).map_err(handle)?;
                self.set_link(*link, Link::with_token(token, *observer))?;
            }
            Step::LinkMove { link, from } => {
                self.link_vacant(*link)?;
                self.link_present(*from)?;
                let moved = Link::move_from(self.link(*from)?);
                self.set_link(*link, moved)?;
            }
            Step::LinkAssign { link, from, guard } => {
                self.link_present(*link)?;
                self.link_present(*from)?;
                // Self-assignment returns before reset().
                if link != from && self.link_erases_during_iteration(*link)? {
                    if *guard {
                        self.skipped("link_assign");
                        return Ok(());
                    }
                    return Err("remove during active iteration".into());
                }
                self.arena
                    .move_assign_link_in(&mut self.links, *link, *from)
                    .map_err(handle)?;
            }
            Step::LinkReset { link, guard } => {
                self.link_present(*link)?;
                if self.link_erases_during_iteration(*link)? {
                    if *guard {
                        self.skipped("link_reset");
                        return Ok(());
                    }
                    return Err("remove during active iteration".into());
                }
                let Driver { arena, links, .. } = self;
                let value = links.get_mut(*link).ok_or("link slot empty")?;
                arena.reset_link(value).map_err(handle)?;
            }
            Step::LinkDrop { link, guard } => {
                self.link_present(*link)?;
                if self.link_erases_during_iteration(*link)? {
                    if *guard {
                        self.skipped("link_drop");
                        return Ok(());
                    }
                    return Err("remove during active iteration".into());
                }
                let value = core::mem::take(self.link(*link)?);
                if let Some(flag) = self.present.get_mut(*link) {
                    *flag = false;
                }
                self.arena.drop_link(value).map_err(handle)?;
            }
            Step::Throw => {
                // Every enclosing template's catch block calls leave_iteration() on its
                // observable; one destroyed inside its own NotifyIgnore makes that
                // undefined.
                if self.frames.iter().any(|(obs, generation)| {
                    self.observables.get(*obs).is_none_or(Option::is_none)
                        || self.generations.get(*obs) != Some(generation)
                }) {
                    return Err("throw through destroyed observable".into());
                }
                return Err(Fault::Thrown);
            }
        }
        Ok(())
    }

    /// `OBSERVABLE<T>::Notify` / `NotifyIgnore` through the ported template.
    fn notify(
        &mut self,
        obs: usize,
        event: i64,
        ignore: Option<ObserverPtr>,
        reactions: &[(ObserverId, Vec<Step>)],
        depth: usize,
    ) -> Outcome<()> {
        let id = self.observable(obs)?.base().impl_ref().map(|r| r.id());
        let generation = self.generations.get(obs).copied().unwrap_or(0);
        // enter_iteration is modular: a count left at UINT_MAX by an unpaired
        // leave_iteration becomes 0 inside the loop, which is defined. Only a later
        // erase, compaction or destruction of this list during the loop is undefined;
        // the removal checks and check_active refuse exactly those.
        if let Some(id) = id {
            // The range-for end is fixed when the loop starts.
            let end = self.arena.observers(id).map_err(handle)?.len();
            self.active.push((id, end));
        }
        self.frames.push((obs, generation));
        if ignore.is_none() {
            self.notifying.push(obs);
        }
        let mut run = NotifyRun {
            driver: self,
            obs,
            generation,
            event,
            reactions,
            depth,
        };
        let result = match ignore {
            None => observable_template::notify(&mut run),
            Some(pointer) => observable_template::notify_ignore(&mut run, pointer),
        };
        match result {
            Ok(()) => {}
            Err(NotifyError::Observer(fault) | NotifyError::UnwoundDestroyed(fault)) => {
                return Err(fault)
            }
            Err(NotifyError::Observable(error)) => return Err(handle(error).into()),
            Err(NotifyError::ListInvalidated) => {
                return Err("list invalidated during active iteration".into())
            }
            Err(NotifyError::Destroyed) => return Err("drop during own notification".into()),
        }
        if ignore.is_none() {
            self.notifying.pop();
        }
        self.frames.pop();
        if id.is_some() {
            self.active.pop();
        }
        Ok(())
    }

    fn snapshot(&self) -> Result<Value> {
        let mut labels: Vec<ImplId> = Vec::new();
        let mut label = |id: Option<ImplId>| -> Value {
            match id {
                None => Value::Null,
                Some(id) => {
                    let position = labels.iter().position(|l| *l == id).unwrap_or_else(|| {
                        labels.push(id);
                        labels.len() - 1
                    });
                    json!(position)
                }
            }
        };
        let mut observables = Vec::new();
        for slot in &self.observables {
            observables.push(match slot {
                None => Value::Null,
                Some(observable) => {
                    let base = observable.base();
                    let impl_label = label(base.impl_ref().map(|r| r.id()));
                    json!({"impl": impl_label, "size": self.arena.size(base).map_err(handle)?})
                }
            });
        }
        let mut links = Vec::new();
        for (link, present) in self.links.iter().zip(self.present) {
            links.push(if present {
                json!({"impl": label(link.token().map(|r| r.id())), "linked": link.is_linked(), "observer": observer_json(link.observer())})
            } else {
                Value::Null
            });
        }
        let mut impls = Vec::new();
        for id in labels {
            impls.push(json!({
                "iteration_count": self.arena.iteration_count(id).map_err(handle)?,
                "observers": self.arena.observers(id).map_err(handle)?.iter().copied().map(observer_json).collect::<Vec<_>>(),
                "shared": self.arena.is_shared(id).map_err(handle)?,
                "use_count": self.arena.use_count(id).map_err(handle)?,
            }));
        }
        Ok(json!({"impls": impls, "links": links, "observables": observables}))
    }
}

/// Run an `observable` operation.
pub fn run(input: &Value) -> Result<Value> {
    fields(input, &["op", "steps"], &[])?;
    let mut parser = Parser { count: 0 };
    let steps = parser.steps(input.get("steps"), 0)?;
    let mut driver = Driver {
        arena: ObservableArena::new(ArenaId(0)),
        observables: (0..OBSERVABLES).map(|_| None).collect(),
        generations: [0; OBSERVABLES],
        frames: Vec::new(),
        links: (0..LINKS).map(|_| Link::new()).collect(),
        present: [false; LINKS],
        active: Vec::new(),
        notifying: Vec::new(),
        calls: Vec::new(),
        budget: 0,
    };
    let mut events = Vec::new();
    for step in &steps {
        let thrown = match driver.exec(step, 0) {
            Ok(()) => false,
            Err(Fault::Thrown) => {
                // Caught after the top-level step: every enclosing loop has unwound.
                driver.active.clear();
                driver.notifying.clear();
                driver.frames.clear();
                true
            }
            Err(Fault::Refused(error)) => return Err(error),
        };
        let calls = core::mem::take(&mut driver.calls);
        let mut event = json!({"calls": calls, "state": driver.snapshot()?});
        if thrown {
            event["thrown"] = json!(true);
        }
        events.push(event);
    }
    // Destroy everything in the C++ adapter's scope-exit order (links, then
    // observables), so the arena's lifetime accounting is exercised too; nothing
    // further is observable.
    for (link, present) in driver.links.into_iter().zip(driver.present) {
        if present {
            driver.arena.drop_link(link).map_err(handle)?;
        }
    }
    for observable in driver.observables.into_iter().flatten() {
        driver
            .arena
            .drop_observable(observable.into_base())
            .map_err(handle)?;
    }
    Ok(json!({"events": events}))
}

/// One notification's host: the driver, the notifying slot and the reactions to run.
struct NotifyRun<'a> {
    driver: &'a mut Driver,
    obs: usize,
    generation: u64,
    event: i64,
    reactions: &'a [(ObserverId, Vec<Step>)],
    depth: usize,
}

impl NotifyHost<Observer> for NotifyRun<'_> {
    type Error = Fault;

    fn parts(&mut self) -> (&mut ObservableArena, Option<&mut Observable<Observer>>) {
        let Driver {
            arena,
            observables,
            generations,
            ..
        } = &mut *self.driver;
        let this = if generations.get(self.obs) == Some(&self.generation) {
            observables.get_mut(self.obs).and_then(Option::as_mut)
        } else {
            None
        };
        (arena, this)
    }

    /// `OBSERVER::OnEvent( event )`: log the call and run this observer's reaction.
    fn call(&mut self, observer: ObserverId) -> Outcome<()> {
        self.driver.tick()?;
        self.driver
            .calls
            .push(json!({"depth": self.depth, "event": self.event, "observer": observer.0}));
        if let Some((_, steps)) = self.reactions.iter().find(|(o, _)| *o == observer) {
            for step in steps {
                self.driver.exec(step, self.depth + 1)?;
            }
        }
        Ok(())
    }
}
