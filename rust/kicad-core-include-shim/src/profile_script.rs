// SPDX-License-Identifier: GPL-3.0-or-later
//! The `profile` operation: a script over `PROF_TIMER`, `SCOPED_PROF_TIMER`,
//! `LATENCY_PROBE` and `PROF_COUNTER` slots with a scripted clock, mirroring
//! `PROFILE_SCRIPT` in `port/oracle/shims/core_include_p001.cpp`.

use kicad_core::profile::{
    ChronoClock, ChronoDuration, DoubleMicroseconds, DoubleMilliseconds, DoubleNanoseconds,
    DoubleSeconds, LatencyProbe, Microseconds, Milliseconds, Nanoseconds, ProfCounter, ProfTimer,
    ProfileError, ScopedProfTimer, Seconds, TimePoint,
};
use kicad_core_shim::operations::{boolean, fields, signed, unsigned, Error};
use serde_json::{json, Value};

use crate::operations::{array, bits, signed_value, text, Result, MAX_STEPS};

pub const MAX_CLOCK: usize = 4096;
pub const CLOCK_LIMIT: i64 = (1 << 62) - 1;
pub const TIMERS: i64 = 4;
pub const PROBES: i64 = 2;
pub const COUNTERS: i64 = 2;
pub const MAX_STAGES: i64 = 1 << 16;

/// The scripted `high_resolution_clock`: each `now()` takes the next value. Past the
/// end it returns the epoch and the step is refused afterwards, as in the C++ adapter.
struct ScriptedClock {
    values: Vec<TimePoint>,
    calls: usize,
    exhausted: bool,
}

impl ChronoClock for ScriptedClock {
    fn now(&mut self) -> TimePoint {
        match self.values.get(self.calls) {
            Some(value) => {
                self.calls += 1;
                *value
            }
            None => {
                self.exhausted = true;
                0
            }
        }
    }
}

const DURATIONS: [&str; 8] = [
    "double_nano",
    "double_micro",
    "double_milli",
    "double_seconds",
    "nanoseconds",
    "microseconds",
    "milliseconds",
    "seconds",
];

/// A duration's JSON form: bits for a double count, the integer otherwise.
trait DurationJson: ChronoDuration {
    fn to_json(&self) -> Value;
}

macro_rules! double_json {
    ($($type:ty),*) => {$(
        impl DurationJson for $type {
            fn to_json(&self) -> Value {
                json!(bits(self.0))
            }
        }
    )*};
}

macro_rules! integer_json {
    ($($type:ty),*) => {$(
        impl DurationJson for $type {
            fn to_json(&self) -> Value {
                json!(self.0)
            }
        }
    )*};
}

double_json!(
    DoubleNanoseconds,
    DoubleMicroseconds,
    DoubleMilliseconds,
    DoubleSeconds
);
integer_json!(Nanoseconds, Microseconds, Milliseconds, Seconds);

/// Something generic over the duration type, like the C++ adapter's lambdas.
trait WithDuration {
    fn run<D: DurationJson>(&mut self) -> Result<Value>;
}

fn with_duration<W: WithDuration>(name: &str, action: &mut W) -> Result<Value> {
    match name {
        "double_nano" => action.run::<DoubleNanoseconds>(),
        "double_micro" => action.run::<DoubleMicroseconds>(),
        "double_milli" => action.run::<DoubleMilliseconds>(),
        "double_seconds" => action.run::<DoubleSeconds>(),
        "nanoseconds" => action.run::<Nanoseconds>(),
        "microseconds" => action.run::<Microseconds>(),
        "milliseconds" => action.run::<Milliseconds>(),
        "seconds" => action.run::<Seconds>(),
        _ => Err("unknown duration".into()),
    }
}

fn since_last(step: &Value) -> Result<bool> {
    if step.get("since_last").is_some() {
        boolean(step, "since_last")
    } else {
        Ok(false)
    }
}

fn profile_error(error: ProfileError) -> Error {
    match error {
        ProfileError::Overflow => "time point difference overflows".into(),
        ProfileError::Indeterminate => "indeterminate m_started".into(),
        ProfileError::Allocation => "allocation failed".into(),
        ProfileError::LengthError(what) => what.into(),
        ProfileError::Unported { upstream, unit } => Error::Unported { upstream, unit },
    }
}

/// The C++ adapter's `ValidateFields`: `required` and `optional` keys only.
fn validate_fields(step: &Value, required: &[&str], optional: &[&str]) -> Result<()> {
    fields(step, required, optional).map(|_| ())
}

/// `PROFILE_SCRIPT::Validate`.
fn validate(step: &Value, base: bool) -> Result<()> {
    let op = text(step, "op")?;
    let timer_fields = |extra: &[&str], optional: &[&str]| -> Result<()> {
        let mut required = vec!["op"];
        if !base {
            required.push("timer");
        }
        required.extend_from_slice(extra);
        validate_fields(step, &required, optional)?;
        if !base {
            signed(step, "timer", 0, TIMERS - 1)?;
        }
        Ok(())
    };
    match op {
        "timer_start" | "timer_stop" | "timer_show" | "timer_to_string" | "timer_get_name" => {
            timer_fields(&[], &[])
        }
        "timer_msecs" => {
            timer_fields(&[], &["since_last"])?;
            since_last(step).map(|_| ())
        }
        "timer_since_start" => {
            timer_fields(&["duration"], &["since_last"])?;
            since_last(step)?;
            if !DURATIONS.contains(&text(step, "duration")?) {
                return Err("unknown duration".into());
            }
            Ok(())
        }
        _ if base => Err("unknown operation".into()),
        "timer_new" => {
            fields(step, &["op", "timer", "name"], &["autostart"])?;
            signed(step, "timer", 0, TIMERS - 1)?;
            text(step, "name")?;
            if step.get("autostart").is_some() {
                boolean(step, "autostart")?;
            }
            Ok(())
        }
        "timer_default" => {
            fields(step, &["op", "timer"], &[])?;
            signed(step, "timer", 0, TIMERS - 1).map(|_| ())
        }
        "scoped" => {
            fields(step, &["op", "duration", "base"], &[])?;
            if !DURATIONS.contains(&text(step, "duration")?) {
                return Err("unknown duration".into());
            }
            for inner in array(step, "base", MAX_STEPS)? {
                validate(inner, true)?;
            }
            Ok(())
        }
        "probe_new" => {
            fields(step, &["op", "probe", "name"], &["stages"])?;
            signed(step, "probe", 0, PROBES - 1)?;
            text(step, "name")?;
            if step.get("stages").is_some() {
                signed(step, "stages", i64::from(i32::MIN), MAX_STAGES)?;
            }
            Ok(())
        }
        "probe_reset" | "probe_to_string" => {
            fields(step, &["op", "probe"], &[])?;
            signed(step, "probe", 0, PROBES - 1).map(|_| ())
        }
        "probe_checkpoint" => {
            fields(step, &["op", "probe", "name"], &[])?;
            signed(step, "probe", 0, PROBES - 1)?;
            text(step, "name").map(|_| ())
        }
        "probe_add_timer" => {
            fields(step, &["op", "probe", "timer"], &[])?;
            signed(step, "probe", 0, PROBES - 1)?;
            signed(step, "timer", 0, TIMERS - 1).map(|_| ())
        }
        "probe_delta_ms" => {
            fields(step, &["op", "probe", "a", "b"], &[])?;
            signed(step, "probe", 0, PROBES - 1)?;
            signed(step, "a", i64::MIN, i64::MAX)?;
            signed(step, "b", i64::MIN, i64::MAX).map(|_| ())
        }
        "counter_new" => {
            fields(step, &["op", "counter"], &["name"])?;
            signed(step, "counter", 0, COUNTERS - 1)?;
            if step.get("name").is_some() {
                text(step, "name")?;
            }
            Ok(())
        }
        "counter_inc" | "counter_count" | "counter_reset" | "counter_show" => {
            fields(step, &["op", "counter"], &[])?;
            signed(step, "counter", 0, COUNTERS - 1).map(|_| ())
        }
        "counter_set" => {
            fields(step, &["op", "counter", "value"], &[])?;
            signed(step, "counter", 0, COUNTERS - 1)?;
            unsigned(step, "value").map(|_| ())
        }
        _ => Err("unknown operation".into()),
    }
}

fn slot_index(step: &Value, name: &str, count: i64) -> Result<usize> {
    let value = signed(step, name, 0, count - 1)?;
    usize::try_from(value).map_err(|_| Error::Invalid(format!("integer out of range: {name}")))
}

/// A `PROF_TIMER` slot and whether its `m_started` has ever been written.
#[derive(Default)]
struct TimerSlot {
    timer: Option<ProfTimer>,
    started_written: bool,
}

struct Script {
    clock: ScriptedClock,
    timers: Vec<TimerSlot>,
    probes: Vec<Option<LatencyProbe>>,
    counters: Vec<Option<ProfCounter>>,
}

/// `SinceStart<DURATION>( ... )` on one timer, generic over the duration.
struct SinceStart<'a> {
    timer: &'a mut ProfTimer,
    clock: &'a mut ScriptedClock,
    since_last: bool,
}

impl WithDuration for SinceStart<'_> {
    fn run<D: DurationJson>(&mut self) -> Result<Value> {
        self.timer
            .since_start::<D, _>(self.clock, self.since_last)
            .map(|duration| duration.to_json())
            .map_err(profile_error)
    }
}

/// The PROF_TIMER operations, on a slot's timer or a scoped timer's base.
fn timer_step(
    step: &Value,
    timer: &mut ProfTimer,
    started_written: &mut bool,
    clock: &mut ScriptedClock,
) -> Result<Value> {
    match text(step, "op")? {
        "timer_start" => {
            timer.start(clock);
            *started_written = true;
            Ok(Value::Null)
        }
        "timer_stop" => {
            timer.stop(clock);
            Ok(Value::Null)
        }
        "timer_msecs" => timer
            .msecs(clock, since_last(step)?)
            .map(|ms| json!(bits(ms)))
            .map_err(profile_error),
        "timer_since_start" => {
            let mut action = SinceStart {
                timer,
                clock,
                since_last: since_last(step)?,
            };
            with_duration(text(step, "duration")?, &mut action)
        }
        "timer_show" => {
            let mut stream = String::new();
            timer.show(clock, &mut stream).map_err(profile_error)?;
            Ok(json!(stream))
        }
        "timer_to_string" => {
            // m_started is written only by Start(); the C++ adapter refuses reading it
            // before. The port reports the same through ProfileError::Indeterminate.
            if !*started_written {
                return Err("indeterminate m_started".into());
            }
            timer
                .to_string(clock)
                .map(|s| json!(s))
                .map_err(profile_error)
        }
        _ => Ok(json!(timer.get_name())),
    }
}

/// `SCOPED_PROF_TIMER<DURATION>`: construct, run the base steps, destroy.
struct Scoped<'a> {
    steps: &'a [Value],
    clock: &'a mut ScriptedClock,
}

impl WithDuration for Scoped<'_> {
    fn run<D: DurationJson>(&mut self) -> Result<Value> {
        let mut scoped: ScopedProfTimer<D> = ScopedProfTimer::new(self.clock);
        let mut base = Vec::new();
        let mut started = true;
        for step in self.steps {
            base.push(timer_step(
                step,
                scoped.base_mut(),
                &mut started,
                self.clock,
            )?);
        }
        let duration = scoped.finish(self.clock).map_err(profile_error)?;
        Ok(json!({"base": base, "duration": duration.to_json()}))
    }
}

impl Script {
    fn timer(&mut self, step: &Value) -> Result<&mut TimerSlot> {
        let index = slot_index(step, "timer", TIMERS)?;
        self.timers
            .get_mut(index)
            .filter(|slot| slot.timer.is_some())
            .ok_or_else(|| "timer slot empty".into())
    }

    fn probe(&mut self, step: &Value) -> Result<&mut LatencyProbe> {
        let index = slot_index(step, "probe", PROBES)?;
        self.probes
            .get_mut(index)
            .and_then(Option::as_mut)
            .ok_or_else(|| "probe slot empty".into())
    }

    fn counter(&mut self, step: &Value) -> Result<&mut ProfCounter> {
        let index = slot_index(step, "counter", COUNTERS)?;
        self.counters
            .get_mut(index)
            .and_then(Option::as_mut)
            .ok_or_else(|| "counter slot empty".into())
    }

    fn step(&mut self, step: &Value) -> Result<Value> {
        let op = text(step, "op")?;
        match op {
            "timer_new" => {
                let index = slot_index(step, "timer", TIMERS)?;
                let autostart = if step.get("autostart").is_some() {
                    boolean(step, "autostart")?
                } else {
                    true
                };
                let timer = ProfTimer::named(text(step, "name")?, autostart, &mut self.clock);
                if let Some(slot) = self.timers.get_mut(index) {
                    *slot = TimerSlot {
                        timer: Some(timer),
                        started_written: autostart,
                    };
                }
                Ok(Value::Null)
            }
            "timer_default" => {
                let index = slot_index(step, "timer", TIMERS)?;
                let timer = ProfTimer::new(&mut self.clock);
                if let Some(slot) = self.timers.get_mut(index) {
                    *slot = TimerSlot {
                        timer: Some(timer),
                        started_written: true,
                    };
                }
                Ok(Value::Null)
            }
            _ if op.starts_with("timer_") => {
                let index = slot_index(step, "timer", TIMERS)?;
                let Script { clock, timers, .. } = self;
                let slot = timers
                    .get_mut(index)
                    .filter(|slot| slot.timer.is_some())
                    .ok_or("timer slot empty")?;
                let TimerSlot {
                    timer,
                    started_written,
                } = slot;
                let timer = timer.as_mut().ok_or("timer slot empty")?;
                timer_step(step, timer, started_written, clock)
            }
            "scoped" => {
                let steps = array(step, "base", MAX_STEPS)?;
                let mut action = Scoped {
                    steps,
                    clock: &mut self.clock,
                };
                with_duration(text(step, "duration")?, &mut action)
            }
            "probe_new" => {
                let index = slot_index(step, "probe", PROBES)?;
                let stages = if step.get("stages").is_some() {
                    i32::try_from(signed(step, "stages", i64::from(i32::MIN), MAX_STAGES)?)
                        .map_err(|_| "integer out of range: stages")?
                } else {
                    8
                };
                let name = text(step, "name")?;
                if let Some(slot) = self.probes.get_mut(index) {
                    *slot = None;
                    match LatencyProbe::new(name, stages) {
                        Ok(probe) => *slot = Some(probe),
                        Err(ProfileError::LengthError(what)) => {
                            return Ok(json!({"exception": "length_error", "what": what}))
                        }
                        Err(error) => return Err(profile_error(error)),
                    }
                }
                Ok(Value::Null)
            }
            "probe_reset" => {
                let index = slot_index(step, "probe", PROBES)?;
                let Script { clock, probes, .. } = self;
                let probe = probes
                    .get_mut(index)
                    .and_then(Option::as_mut)
                    .ok_or("probe slot empty")?;
                probe.reset(clock);
                Ok(Value::Null)
            }
            "probe_checkpoint" => {
                let index = slot_index(step, "probe", PROBES)?;
                let name = text(step, "name")?;
                let Script { clock, probes, .. } = self;
                let probe = probes
                    .get_mut(index)
                    .and_then(Option::as_mut)
                    .ok_or("probe slot empty")?;
                probe.checkpoint(clock, name).map_err(profile_error)?;
                Ok(Value::Null)
            }
            "probe_add_timer" => {
                self.probe(step)?;
                self.timer(step)?;
                let probe_index = slot_index(step, "probe", PROBES)?;
                let timer_index = slot_index(step, "timer", TIMERS)?;
                let Script {
                    clock,
                    timers,
                    probes,
                    ..
                } = self;
                let probe = probes
                    .get_mut(probe_index)
                    .and_then(Option::as_mut)
                    .ok_or("probe slot empty")?;
                let timer = timers
                    .get_mut(timer_index)
                    .and_then(|slot| slot.timer.as_mut())
                    .ok_or("timer slot empty")?;
                probe.add_timer(timer, clock).map_err(profile_error)?;
                Ok(Value::Null)
            }
            "probe_to_string" => self
                .probe(step)?
                .to_string()
                .map(|s| json!(s))
                .map_err(profile_error),
            "probe_delta_ms" => {
                let a = signed(step, "a", i64::MIN, i64::MAX)?;
                let b = signed(step, "b", i64::MIN, i64::MAX)?;
                let probe = self.probe(step)?;
                // a - b on signed int64 nanosecond durations.
                if a.checked_sub(b).is_none() {
                    return Err("time point difference overflows".into());
                }
                probe
                    .delta_ms(a, b)
                    .map(|ms| json!(bits(ms)))
                    .map_err(profile_error)
            }
            "counter_new" => {
                let index = slot_index(step, "counter", COUNTERS)?;
                let counter = if step.get("name").is_some() {
                    ProfCounter::named(text(step, "name")?)
                } else {
                    ProfCounter::new()
                };
                if let Some(slot) = self.counters.get_mut(index) {
                    *slot = Some(counter);
                }
                Ok(Value::Null)
            }
            "counter_inc" => Ok(json!(self.counter(step)?.post_increment())),
            "counter_count" => Ok(json!(self.counter(step)?.count())),
            "counter_reset" => {
                self.counter(step)?.reset();
                Ok(Value::Null)
            }
            "counter_show" => {
                let mut stream = String::new();
                self.counter(step)?.show(&mut stream);
                Ok(json!(stream))
            }
            _ => {
                let value = unsigned(step, "value")?;
                self.counter(step)?.set_raw_count(value);
                Ok(Value::Null)
            }
        }
    }
}

/// Run a `profile` operation.
pub fn run(input: &Value) -> Result<Value> {
    fields(input, &["op", "clock", "steps"], &[])?;
    let mut values = Vec::new();
    for value in array(input, "clock", MAX_CLOCK)? {
        // Every difference the timers and probes take is between two scripted values
        // or the epoch; bounding the values keeps every such subtraction defined.
        values.push(signed_value(value, "t", -CLOCK_LIMIT, CLOCK_LIMIT)?);
    }
    let steps = array(input, "steps", MAX_STEPS)?;
    for step in steps {
        validate(step, false)?;
    }

    let mut script = Script {
        clock: ScriptedClock {
            values,
            calls: 0,
            exhausted: false,
        },
        timers: (0..TIMERS).map(|_| TimerSlot::default()).collect(),
        probes: (0..PROBES).map(|_| None).collect(),
        counters: (0..COUNTERS).map(|_| None).collect(),
    };
    let mut results = Vec::new();
    for step in steps {
        results.push(script.step(step)?);
        if script.clock.exhausted {
            return Err("clock script exhausted".into());
        }
    }
    Ok(json!({"results": results, "clock_calls": script.clock.calls}))
}
