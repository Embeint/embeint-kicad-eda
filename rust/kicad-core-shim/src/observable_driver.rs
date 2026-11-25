// SPDX-License-Identifier: GPL-3.0-or-later
//! Scripted observable driver, mirroring `port/oracle/shims/core_p001.cpp`.
//!
//! The C++ adapter drives the real `UTIL::OBSERVABLE<OBSERVER>` header template.
//! That template is owned by `libs.core.include.core.application.pure.p001` and is
//! not ported; this driver plays its role on the Rust side using the same base
//! primitives the template calls (`add_observer` + `LINK( impl_, ... )` for
//! `Subscribe`, `enter_iteration` / range over `impl_->observers_` /
//! `leave_iteration` for `Notify`, the same without `leave_iteration` for
//! `NotifyIgnore`). It is harness code, not a port of those bodies.
//!
//! Script domain (identical on both sides):
//! * steps are validated completely before anything runs;
//! * a step may not drop an observable that an enclosing `notify` is notifying, and
//!   `enter_iteration`, `leave_iteration` and `on_observers_empty` are top-level only
//!   (both would destroy or rewire state an active range-for is reading);
//! * adding to a list that an active `Notify` loop is ranging over invalidates the
//!   loop's iterators in the reference. It is refused, or skipped and logged with
//!   `guard: true`; likewise `unsubscribe` with a null `impl_` (a null dereference).

use crate::operations::{boolean, fields, signed, text, Error, Result};
use kicad_core::observable::{
    ImplId, Link, ObservableArena, ObservableBase, ObservableError, ObserverId, ObserverPtr,
};
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
    },
    LinkReset {
        link: usize,
    },
    LinkDrop {
        link: usize,
    },
}

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
    notifying: Vec<usize>,
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
            let obs = index(input, "obs", OBSERVABLES)?;
            if depth > 0 {
                return Err("base operation inside notification".into());
            }
            Ok(obs)
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
                let obs = index(input, "obs", OBSERVABLES)?;
                if self.notifying.contains(&obs) {
                    return Err("drop during own notification".into());
                }
                Step::Drop { obs }
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
                self.notifying.push(obs);
                for reaction in list {
                    fields(reaction, &["observer", "steps"], &[])?;
                    let observer = observer_id(reaction, "observer")?;
                    if reactions.iter().any(|(o, _)| *o == observer) {
                        return Err("duplicate reaction".into());
                    }
                    let steps = self.steps(reaction.get("steps"), depth + 1)?;
                    reactions.push((observer, steps));
                }
                self.notifying.pop();
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
            "link_new" | "link_reset" | "link_drop" => {
                fields(input, &["op", "link"], &[])?;
                let link = index(input, "link", LINKS)?;
                match op {
                    "link_new" => Step::LinkNew { link },
                    "link_reset" => Step::LinkReset { link },
                    _ => Step::LinkDrop { link },
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
            "link_move" | "link_assign" => {
                fields(input, &["op", "link", "from"], &[])?;
                let link = index(input, "link", LINKS)?;
                let from = index(input, "from", LINKS)?;
                if op == "link_move" {
                    Step::LinkMove { link, from }
                } else {
                    Step::LinkAssign { link, from }
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
        ObservableError::Allocation => "allocation failed".into(),
    }
}

struct Driver {
    arena: ObservableArena,
    observables: Vec<Option<ObservableBase>>,
    links: Vec<Link>,
    present: [bool; LINKS],
    /// Lists ranged over by active `Notify`/`NotifyIgnore` loops.
    active: Vec<ImplId>,
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

    fn observable(&mut self, obs: usize) -> Result<&mut ObservableBase> {
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

    /// True when adding to `obs`'s list would push into a list an active loop ranges over.
    fn adds_during_iteration(&mut self, obs: usize) -> Result<bool> {
        let id = self.observable(obs)?.impl_ref().map(|r| r.id());
        Ok(id.is_some_and(|id| self.active.contains(&id)))
    }

    fn skipped(&mut self, op: &str) {
        self.calls.push(json!({"skipped": op}));
    }

    fn exec(&mut self, step: &Step, depth: usize) -> Result<()> {
        self.tick()?;
        match step {
            Step::New { obs } => {
                self.vacant(*obs)?;
                if let Some(slot) = self.observables.get_mut(*obs) {
                    *slot = Some(ObservableBase::new());
                }
            }
            Step::Copy { obs, from } => {
                self.vacant(*obs)?;
                let mut source = self
                    .observables
                    .get_mut(*from)
                    .and_then(Option::take)
                    .ok_or("observable slot empty")?;
                let copy = self.arena.copy_observable(&mut source);
                if let Some(slot) = self.observables.get_mut(*from) {
                    *slot = Some(source);
                }
                let copy = copy.map_err(handle)?;
                if let Some(slot) = self.observables.get_mut(*obs) {
                    *slot = Some(copy);
                }
            }
            Step::Drop { obs } => {
                let base = self
                    .observables
                    .get_mut(*obs)
                    .and_then(Option::take)
                    .ok_or("observable slot empty")?;
                self.arena.drop_observable(base);
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
                let base = observables
                    .get_mut(*obs)
                    .and_then(Option::as_mut)
                    .ok_or("observable slot empty")?;
                arena.add_observer(base, *observer).map_err(handle)?;
                let token = arena.share_impl(base).map_err(handle)?;
                self.set_link(*link, Link::with_token(token, *observer))?;
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
                let base = observables
                    .get_mut(*obs)
                    .and_then(Option::as_mut)
                    .ok_or("observable slot empty")?;
                arena.add_observer(base, *observer).map_err(handle)?;
            }
            Step::Unsubscribe {
                obs,
                observer,
                guard,
            } => {
                let Driver {
                    arena, observables, ..
                } = self;
                let base = observables
                    .get(*obs)
                    .and_then(Option::as_ref)
                    .ok_or("observable slot empty")?;
                match arena.remove_observer(base, *observer) {
                    Err(ObservableError::NoImpl) if *guard => self.skipped("unsubscribe"),
                    result => result.map_err(handle)?,
                }
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
                    .ok_or("observable slot empty")?;
                arena.enter_iteration(base).map_err(handle)?;
            }
            Step::LeaveIteration { obs } => {
                let Driver {
                    arena, observables, ..
                } = self;
                let base = observables
                    .get_mut(*obs)
                    .and_then(Option::as_mut)
                    .ok_or("observable slot empty")?;
                arena.leave_iteration(base).map_err(handle)?;
            }
            Step::OnObserversEmpty { obs } => {
                let Driver {
                    arena, observables, ..
                } = self;
                let base = observables
                    .get_mut(*obs)
                    .and_then(Option::as_mut)
                    .ok_or("observable slot empty")?;
                arena.on_observers_empty(base);
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
                    .ok_or("observable slot empty")?;
                let token = arena.share_impl(base).map_err(handle)?;
                self.set_link(*link, Link::with_token(token, *observer))?;
            }
            Step::LinkMove { link, from } => {
                self.link_vacant(*link)?;
                self.link_present(*from)?;
                let moved = Link::move_from(self.link(*from)?);
                self.set_link(*link, moved)?;
            }
            Step::LinkAssign { link, from } => {
                self.link_present(*link)?;
                self.link_present(*from)?;
                self.arena
                    .move_assign_link_in(&mut self.links, *link, *from)
                    .map_err(handle)?;
            }
            Step::LinkReset { link } => {
                self.link_present(*link)?;
                let Driver { arena, links, .. } = self;
                let value = links.get_mut(*link).ok_or("link slot empty")?;
                arena.reset_link(value).map_err(handle)?;
            }
            Step::LinkDrop { link } => {
                self.link_present(*link)?;
                let value = core::mem::take(self.link(*link)?);
                if let Some(flag) = self.present.get_mut(*link) {
                    *flag = false;
                }
                self.arena.drop_link(value).map_err(handle)?;
            }
        }
        Ok(())
    }

    /// The `OBSERVABLE<T>::Notify` / `NotifyIgnore` loop over `impl_->observers_`.
    fn notify(
        &mut self,
        obs: usize,
        event: i64,
        ignore: Option<ObserverPtr>,
        reactions: &[(ObserverId, Vec<Step>)],
        depth: usize,
    ) -> Result<()> {
        let Some(id) = self.observable(obs)?.impl_ref().map(|r| r.id()) else {
            return Ok(());
        };
        {
            let Driver {
                arena, observables, ..
            } = &mut *self;
            let base = observables
                .get(obs)
                .and_then(Option::as_ref)
                .ok_or("observable slot empty")?;
            arena.enter_iteration(base).map_err(handle)?;
        }
        self.active.push(id);
        // The range-for end is fixed when the loop starts; entries are re-read each step.
        let end = self.arena.observers(id).map_err(handle)?.len();
        for position in 0..end {
            let entry = self
                .arena
                .observers(id)
                .map_err(handle)?
                .get(position)
                .copied()
                .flatten();
            let Some(observer) = entry else { continue };
            if ignore == Some(Some(observer)) {
                continue;
            }
            self.tick()?;
            self.calls
                .push(json!({"depth": depth, "event": event, "observer": observer.0}));
            if let Some((_, steps)) = reactions.iter().find(|(o, _)| *o == observer) {
                for step in steps {
                    self.exec(step, depth + 1)?;
                }
            }
        }
        self.active.pop();
        if ignore.is_none() {
            let Driver {
                arena, observables, ..
            } = self;
            let base = observables
                .get_mut(obs)
                .and_then(Option::as_mut)
                .ok_or("observable slot empty")?;
            arena.leave_iteration(base).map_err(handle)?;
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
                Some(base) => {
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
    let mut parser = Parser {
        count: 0,
        notifying: Vec::new(),
    };
    let steps = parser.steps(input.get("steps"), 0)?;
    let mut driver = Driver {
        arena: ObservableArena::new(),
        observables: (0..OBSERVABLES).map(|_| None).collect(),
        links: (0..LINKS).map(|_| Link::new()).collect(),
        present: [false; LINKS],
        active: Vec::new(),
        calls: Vec::new(),
        budget: 0,
    };
    let mut events = Vec::new();
    for step in &steps {
        driver.exec(step, 0)?;
        let calls = core::mem::take(&mut driver.calls);
        events.push(json!({"calls": calls, "state": driver.snapshot()?}));
    }
    // Destroy everything like the C++ adapter's scope exit, so the arena's lifetime
    // accounting is exercised too; nothing further is observable.
    for base in driver.observables.into_iter().flatten() {
        driver.arena.drop_observable(base);
    }
    for (link, present) in driver.links.into_iter().zip(driver.present) {
        if present {
            driver.arena.drop_link(link).map_err(handle)?;
        }
    }
    Ok(json!({"events": events}))
}
