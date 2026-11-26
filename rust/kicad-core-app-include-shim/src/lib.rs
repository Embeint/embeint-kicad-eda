// SPDX-License-Identifier: GPL-3.0-or-later
//! JSONL adapter for `include.core.application.pure.p001`. It mirrors
//! `port/oracle/shims/core_app_include_p001.cpp`: the same operations, the same validation
//! order and refusal messages, integers as JSON integers and `double`s as exact IEEE bits.
//! Every result comes from the portable port (`kicad_core::app_include_core`,
//! `kicad_math::mirror`); this file only decodes, scripts and encodes.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kicad_core::inplace_function::{InplaceFunction, Target, DEFAULT_CAPACITY};
use kicad_core::leak_at_exit::LeakAtExit;
use kicad_core::profile::{Milliseconds, TimePoint};
use kicad_core::throttle::{SteadyClock, Throttle};
use kicad_core::typeinfo::{self, KicadT};
use kicad_core::APP_INCLUDE_UNIT as UNIT;
use kicad_math::mirror::{self, FlipDirection, MirrorValue};
use kicad_math::Vector2;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::rc::Rc;

const MAX_TEXT: usize = 1 << 16;
const MAX_STEPS: usize = 256;
const MAX_CLOCK: usize = 4096;
const MAX_TOKEN: i64 = 1 << 40;
const SLOTS: usize = 4;
const LEAK_SLOTS: usize = 2;
const UNDEFINED: &str = "undefined: signed overflow";
const CHRONO: &str = "undefined: chrono overflow";

type Obj = Map<String, Value>;
type Res<T> = Result<T, String>;

fn fail<T>(message: &str) -> Res<T> {
    Err(message.to_string())
}

/// `Fields`: exactly the listed keys.
fn fields<'a>(v: &'a Value, required: &[&str]) -> Res<&'a Obj> {
    let o = v.as_object().ok_or("expected object")?;
    if o.len() != required.len() || required.iter().any(|k| !o.contains_key(*k)) {
        return fail("unexpected or missing field");
    }
    Ok(o)
}

fn member<'a>(o: &'a Obj, name: &str) -> Res<&'a Value> {
    o.get(name)
        .ok_or_else(|| "unexpected or missing field".to_string())
}

fn text(o: &Obj, name: &str) -> Res<String> {
    let s = member(o, name)?
        .as_str()
        .ok_or_else(|| format!("expected string: {name}"))?;
    if s.len() > MAX_TEXT {
        return Err(format!("string too long: {name}"));
    }
    Ok(s.to_string())
}

fn signed(v: &Value, name: &str, min: i64, max: i64) -> Res<i64> {
    v.as_i64()
        .filter(|n| (min..=max).contains(n))
        .ok_or_else(|| format!("integer out of range: {name}"))
}

fn array<'a>(o: &'a Obj, name: &str, max: usize) -> Res<&'a Vec<Value>> {
    let a = member(o, name)?
        .as_array()
        .ok_or_else(|| format!("expected array: {name}"))?;
    if a.len() > max {
        return Err(format!("array too long: {name}"));
    }
    Ok(a)
}

fn decode_bits(v: &Value, name: &str) -> Res<f64> {
    let s = v
        .as_str()
        .filter(|s| s.len() == 16 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
        .ok_or_else(|| format!("expected hex: {name}"))?;
    u64::from_str_radix(s, 16)
        .map(f64::from_bits)
        .map_err(|_| format!("expected hex: {name}"))
}

fn bits(v: f64) -> Value {
    json!(format!("{:016x}", v.to_bits()))
}

// ---- typeinfo.h ---------------------------------------------------------------------------

fn type_info(v: &Value) -> Res<Value> {
    let o = fields(v, &["op", "value"])?;
    let value = signed(member(o, "value")?, "value", -128, 127)?;
    let t = KicadT::new(value).map_err(|_| "integer out of range: value".to_string())?;
    Ok(json!({
        "base_type": typeinfo::base_type(t).value(),
        "is_null": typeinfo::is_null_type(t),
        "is_generated_table": typeinfo::is_generated_table_type(t),
        "is_instantiable": typeinfo::is_instantiable_type(t),
        "is_eeschema": typeinfo::is_eeschema_type(t),
        "is_pcbnew": typeinfo::is_pcbnew_type(t),
        "is_single_layer": typeinfo::is_single_layer_type(t),
        "is_gerbview": typeinfo::is_gerbview_type(t),
        "is_page_layout_editor": typeinfo::is_page_layout_editor_type(t),
        "is_misc": typeinfo::is_misc_type(t),
        "is_type_correct": typeinfo::is_type_correct(t),
    }))
}

fn enumerators(v: &Value) -> Res<Value> {
    fields(v, &["op"])?;
    Ok(Value::Array(
        typeinfo::ENUMERATORS
            .iter()
            .map(|(name, value)| json!([name, value.value()]))
            .collect(),
    ))
}

// ---- mirror.h -----------------------------------------------------------------------------

trait Scalar: MirrorValue {
    fn read(v: &Value, name: &str) -> Res<Self>;
    fn write(self) -> Value;
}
impl Scalar for i32 {
    fn read(v: &Value, name: &str) -> Res<Self> {
        Ok(signed(v, name, i32::MIN.into(), i32::MAX.into())? as i32)
    }
    fn write(self) -> Value {
        json!(self)
    }
}
impl Scalar for i64 {
    fn read(v: &Value, name: &str) -> Res<Self> {
        signed(v, name, i64::MIN, i64::MAX)
    }
    fn write(self) -> Value {
        json!(self)
    }
}
impl Scalar for f64 {
    fn read(v: &Value, name: &str) -> Res<Self> {
        decode_bits(v, name)
    }
    fn write(self) -> Value {
        bits(self)
    }
}

fn read_direction(o: &Obj) -> Res<FlipDirection> {
    let value = signed(
        member(o, "direction")?,
        "direction",
        i32::MIN.into(),
        i32::MAX.into(),
    )?;
    Ok(FlipDirection::from_int(value as i32))
}

fn mirror_scalar<T: Scalar>(o: &Obj, in_place: bool) -> Res<Value> {
    let point = T::read(member(o, "point")?, "point")?;
    let reference = T::read(member(o, "ref")?, "ref")?;
    if !in_place {
        let value = mirror::mirror_val(point, reference).map_err(|_| UNDEFINED.to_string())?;
        return Ok(json!({"value": value.write()}));
    }
    let mut updated = point;
    mirror::mirror(&mut updated, reference).map_err(|_| UNDEFINED.to_string())?;
    Ok(json!({"point": updated.write()}))
}

fn read_vector<T: Scalar>(o: &Obj, name: &str) -> Res<Vector2<T>> {
    let a = member(o, name)?
        .as_array()
        .filter(|a| a.len() == 2)
        .ok_or_else(|| format!("expected point: {name}"))?;
    let mut it = a.iter();
    let x = T::read(it.next().ok_or("expected point")?, name)?;
    let y = T::read(it.next().ok_or("expected point")?, name)?;
    Ok(Vector2::new(x, y))
}

fn mirror_vector<T: Scalar>(o: &Obj) -> Res<Value> {
    let mut point = read_vector::<T>(o, "point")?;
    let reference = read_vector::<T>(o, "ref")?;
    let direction = read_direction(o)?;
    mirror::mirror_in(&mut point, reference, direction).map_err(|_| UNDEFINED.to_string())?;
    Ok(json!({"point": [point.x.write(), point.y.write()]}))
}

fn mirror_op(v: &Value, op: &str) -> Res<Value> {
    if op == "mirror_axis" {
        let o = fields(v, &["op", "direction"])?;
        return Ok(json!({"degrees": bits(mirror::mirror_axis(read_direction(o)?).as_degrees())}));
    }
    if op == "mirror_vector" {
        let o = fields(v, &["op", "type", "point", "ref", "direction"])?;
        return match text(o, "type")?.as_str() {
            "VECTOR2I" => mirror_vector::<i32>(o),
            "VECTOR2L" => mirror_vector::<i64>(o),
            "VECTOR2D" => mirror_vector::<f64>(o),
            _ => fail("unknown type"),
        };
    }
    let o = fields(v, &["op", "type", "point", "ref"])?;
    let in_place = op == "mirror";
    match text(o, "type")?.as_str() {
        "int" => mirror_scalar::<i32>(o, in_place),
        "int64_t" => mirror_scalar::<i64>(o, in_place),
        "double" => mirror_scalar::<f64>(o, in_place),
        _ => fail("unknown type"),
    }
}

// ---- throttle.h ---------------------------------------------------------------------------

/// The scripted steady clock: one value per `now()`.
struct Script<'a> {
    values: &'a [i64],
    calls: usize,
}
impl SteadyClock for Script<'_> {
    fn now(&mut self) -> TimePoint {
        // The adapter supplies exactly one value per call, as the C++ one does.
        let value = self.values.get(self.calls).copied().unwrap_or_default();
        self.calls += 1;
        value
    }
}

fn throttle(v: &Value) -> Res<Value> {
    let o = fields(v, &["op", "interval_ms", "clock"])?;
    let interval = signed(member(o, "interval_ms")?, "interval_ms", i64::MIN, i64::MAX)?;
    let clock = array(o, "clock", MAX_CLOCK)?;
    if clock.is_empty() {
        return fail("expected array: clock");
    }
    let values = clock
        .iter()
        .map(|t| signed(t, "clock", i64::MIN, i64::MAX))
        .collect::<Res<Vec<_>>>()?;
    let mut script = Script {
        values: &values,
        calls: 0,
    };
    let mut throttle =
        Throttle::new(Milliseconds(interval), &mut script).map_err(|_| CHRONO.to_string())?;
    let mut results = Vec::new();
    for _ in 1..values.len() {
        results.push(json!(throttle
            .ready(&mut script)
            .map_err(|_| CHRONO.to_string())?));
    }
    Ok(json!({"results": results, "last": throttle.last(), "clock_calls": script.calls}))
}

// ---- leak_at_exit.h -----------------------------------------------------------------------

/// A `LEAK_AT_EXIT<T>` instantiation of the cases.
trait LeakType: Default {
    const NAME: &'static str;
    fn decode(v: &Value) -> Res<Self>;
    fn encode(&self) -> Value;
    /// `->size()` for a container; `None` for `int` and the shared pointer.
    fn size(&self) -> Option<usize>;
    /// The shared pointer instantiation's slot.
    fn shared(&mut self) -> Option<&mut Option<Rc<String>>> {
        None
    }
}
impl LeakType for i32 {
    const NAME: &'static str = "int";
    fn decode(v: &Value) -> Res<Self> {
        Ok(signed(v, "value", i32::MIN.into(), i32::MAX.into())? as i32)
    }
    fn encode(&self) -> Value {
        json!(self)
    }
    fn size(&self) -> Option<usize> {
        None
    }
}
impl LeakType for String {
    const NAME: &'static str = "string";
    fn decode(v: &Value) -> Res<Self> {
        v.as_str()
            .filter(|s| s.len() <= MAX_TEXT)
            .map(str::to_string)
            .ok_or_else(|| "expected string: value".to_string())
    }
    fn encode(&self) -> Value {
        json!(self)
    }
    fn size(&self) -> Option<usize> {
        Some(self.len())
    }
}
impl LeakType for Vec<i32> {
    const NAME: &'static str = "vector_int";
    fn decode(v: &Value) -> Res<Self> {
        let a = v
            .as_array()
            .filter(|a| a.len() <= MAX_STEPS)
            .ok_or("expected array: value")?;
        a.iter()
            .map(|n| Ok(signed(n, "value", i32::MIN.into(), i32::MAX.into())? as i32))
            .collect()
    }
    fn encode(&self) -> Value {
        json!(self)
    }
    fn size(&self) -> Option<usize> {
        Some(self.len())
    }
}
impl LeakType for BTreeMap<String, i32> {
    const NAME: &'static str = "map_string_int";
    fn decode(v: &Value) -> Res<Self> {
        let a = v
            .as_array()
            .filter(|a| a.len() <= MAX_STEPS)
            .ok_or("expected array: value")?;
        let mut out = BTreeMap::new();
        for entry in a {
            let (key, value) = entry
                .as_array()
                .filter(|e| e.len() == 2)
                .and_then(|e| Some((e.first()?.as_str()?, e.get(1)?)))
                .filter(|(k, _)| k.len() <= MAX_TEXT)
                .ok_or("expected entry: value")?;
            let value = signed(value, "value", i32::MIN.into(), i32::MAX.into())? as i32;
            out.insert(key.to_string(), value);
        }
        Ok(out)
    }
    fn encode(&self) -> Value {
        Value::Array(self.iter().map(|(k, v)| json!([k, v])).collect())
    }
    fn size(&self) -> Option<usize> {
        Some(self.len())
    }
}
impl LeakType for Option<Rc<String>> {
    const NAME: &'static str = "shared_string";
    fn decode(_: &Value) -> Res<Self> {
        fail("shared values are set by share")
    }
    fn encode(&self) -> Value {
        match self {
            Some(s) => json!(s.as_str()),
            None => Value::Null,
        }
    }
    fn size(&self) -> Option<usize> {
        None
    }
    fn shared(&mut self) -> Option<&mut Option<Rc<String>>> {
        Some(self)
    }
}

/// `Get()`, `operator*`, `operator->` or `operator T&`.
fn access<'a, T: LeakType>(leak: &'a mut LeakAtExit<T>, via: &str) -> Res<&'a mut T> {
    match via {
        "get" => Ok(leak.get()),
        "deref" => Ok(leak.dereference()),
        "arrow" => Ok(leak.arrow()),
        "convert" => Ok(leak.as_reference()),
        _ => fail("unknown accessor"),
    }
}

fn run_leak<T: LeakType>(steps: &[Value]) -> Res<Value> {
    let mut slots: [Option<LeakAtExit<T>>; LEAK_SLOTS] = Default::default();
    let external = Rc::new("leaked".to_string());
    let shared = T::NAME == "shared_string";
    let mut results = Vec::new();

    let steps = steps
        .iter()
        .map(|s| {
            s.as_object()
                .filter(|o| o.contains_key("step"))
                .ok_or_else(|| "expected step object".to_string())
        })
        .collect::<Res<Vec<_>>>()?;

    for o in steps {
        let step = Value::Object(o.clone());
        let name = text(o, "step")?;
        if name == "use_count" {
            fields(&step, &["step"])?;
            if !shared {
                return fail("use_count needs shared_string");
            }
            results.push(json!(Rc::strong_count(&external)));
            continue;
        }
        let slot = signed(member(o, "slot")?, "slot", 0, LEAK_SLOTS as i64 - 1)? as usize;
        let leak = slots.get_mut(slot).ok_or("integer out of range: slot")?;
        if name == "construct" {
            fields(&step, &["step", "slot"])?;
            if leak.is_some() {
                return fail("slot already constructed");
            }
            *leak = Some(LeakAtExit::new());
            results.push(Value::Null);
            continue;
        }
        let Some(object) = leak.as_mut() else {
            return fail("slot not constructed");
        };
        match name.as_str() {
            "created" => {
                fields(&step, &["step", "slot"])?;
                results.push(json!(object.is_created()));
            }
            "identity" => {
                fields(&step, &["step", "slot"])?;
                // Every accessor returns the one object `Get()` created.
                let first: *const T = object.get();
                let same = core::ptr::eq(first, object.dereference())
                    && core::ptr::eq(first, object.arrow())
                    && core::ptr::eq(first, object.as_reference());
                results.push(json!(same));
            }
            "read" => {
                fields(&step, &["step", "slot", "via"])?;
                let via = text(o, "via")?;
                results.push(access(object, &via)?.encode());
            }
            "write" => {
                fields(&step, &["step", "slot", "via", "value"])?;
                let via = text(o, "via")?;
                let value = T::decode(member(o, "value")?)?;
                *access(object, &via)? = value;
                results.push(Value::Null);
            }
            "share" => {
                fields(&step, &["step", "slot", "via"])?;
                if !shared {
                    return fail("share needs shared_string");
                }
                let via = text(o, "via")?;
                if let Some(target) = access(object, &via)?.shared() {
                    *target = Some(Rc::clone(&external));
                }
                results.push(Value::Null);
            }
            "size" => {
                fields(&step, &["step", "slot"])?;
                match object.arrow().size() {
                    Some(size) => results.push(json!(size)),
                    None => return fail("size needs a container"),
                }
            }
            "destroy" => {
                fields(&step, &["step", "slot"])?;
                *leak = None;
                results.push(Value::Null);
            }
            _ => return fail("unknown step"),
        }
    }
    Ok(json!({"results": results}))
}

fn leak_op(v: &Value) -> Res<Value> {
    let o = fields(v, &["op", "type", "steps"])?;
    let kind = text(o, "type")?;
    let steps = array(o, "steps", MAX_STEPS)?;
    match kind.as_str() {
        "int" => run_leak::<i32>(steps),
        "string" => run_leak::<String>(steps),
        "vector_int" => run_leak::<Vec<i32>>(steps),
        "map_string_int" => run_leak::<BTreeMap<String, i32>>(steps),
        "shared_string" => run_leak::<Option<Rc<String>>>(steps),
        _ => fail("unknown type"),
    }
}

// ---- inplace_function.h -------------------------------------------------------------------

/// What a step does with the callable it names: construct a wrapper or assign one.
trait Use<A, R> {
    type Out;
    fn apply<F: Target<'static, A, R> + 'static>(self, callable: F) -> Self::Out;
}
struct Construct<const CAP: usize>;
impl<A: 'static, R: 'static, const CAP: usize> Use<A, R> for Construct<CAP> {
    type Out = InplaceFunction<'static, A, R, CAP>;
    fn apply<F: Target<'static, A, R> + 'static>(self, callable: F) -> Self::Out {
        InplaceFunction::from_callable(callable)
    }
}
struct Assign<'s, A, R, const CAP: usize>(&'s mut InplaceFunction<'static, A, R, CAP>);
impl<A: 'static, R: 'static, const CAP: usize> Use<A, R> for Assign<'_, A, R, CAP> {
    type Out = ();
    fn apply<F: Target<'static, A, R> + 'static>(self, callable: F) {
        self.0.assign_callable(callable);
    }
}

/// The 48-byte "big" callables: constructed only for the 64-byte capacity, because storing
/// one in the default 32 bytes does not compile.
trait Big {
    fn i64_i32<U: Use<(i32,), i64>>(u: U, values: [i32; 12]) -> Res<U::Out>;
    fn i64_i32_i32<U: Use<(i32, i32), i64>>(u: U, values: [i32; 12]) -> Res<U::Out>;
    fn i64<U: Use<(), i64>>(u: U, values: [i32; 12]) -> Res<U::Out>;
}
struct NoBig;
impl Big for NoBig {
    fn i64_i32<U: Use<(i32,), i64>>(_: U, _: [i32; 12]) -> Res<U::Out> {
        fail("callable too large for capacity")
    }
    fn i64_i32_i32<U: Use<(i32, i32), i64>>(_: U, _: [i32; 12]) -> Res<U::Out> {
        fail("callable too large for capacity")
    }
    fn i64<U: Use<(), i64>>(_: U, _: [i32; 12]) -> Res<U::Out> {
        fail("callable too large for capacity")
    }
}
struct WithBig;
fn sum(values: &[i32; 12]) -> i64 {
    values.iter().map(|&v| i64::from(v)).sum()
}
impl Big for WithBig {
    fn i64_i32<U: Use<(i32,), i64>>(u: U, values: [i32; 12]) -> Res<U::Out> {
        Ok(u.apply(move |x: i32| sum(&values) + i64::from(x)))
    }
    fn i64_i32_i32<U: Use<(i32, i32), i64>>(u: U, values: [i32; 12]) -> Res<U::Out> {
        Ok(u.apply(move |x: i32, y: i32| sum(&values) + i64::from(x) + i64::from(y)))
    }
    fn i64<U: Use<(), i64>>(u: U, values: [i32; 12]) -> Res<U::Out> {
        Ok(u.apply(move || sum(&values)))
    }
}

fn read_big(o: &Obj) -> Res<[i32; 12]> {
    let a = member(o, "values")?
        .as_array()
        .filter(|a| a.len() == 12)
        .ok_or("expected array: values")?;
    let mut out = [0i32; 12];
    for (slot, v) in out.iter_mut().zip(a) {
        *slot = signed(v, "values", i32::MIN.into(), i32::MAX.into())? as i32;
    }
    Ok(out)
}

fn read_k(o: &Obj) -> Res<i32> {
    Ok(signed(member(o, "k")?, "k", i32::MIN.into(), i32::MAX.into())? as i32)
}

/// One `INPLACE_FUNCTION` signature of the cases.
trait Signature {
    type A: 'static;
    type R: 'static;
    type Token: 'static;
    fn token(o: &Obj) -> Res<Self::Token>;
    fn with_callable<B: Big, U: Use<Self::A, Self::R>>(
        spec: &Value,
        token: &Rc<Self::Token>,
        u: U,
    ) -> Res<U::Out>;
    fn invoke<const CAP: usize>(
        function: &InplaceFunction<'static, Self::A, Self::R, CAP>,
        args: &Value,
    ) -> Res<Value>;
}

fn spec_kind(spec: &Value) -> Res<String> {
    let o = spec
        .as_object()
        .filter(|o| o.contains_key("kind"))
        .ok_or("expected callable object")?;
    text(o, "kind")
}

fn outcome<R: Into<Value>>(r: Result<R, kicad_core::inplace_function::BadFunctionCall>) -> Value {
    match r {
        Ok(value) => json!({"value": value.into()}),
        Err(_) => json!({"exception": "bad_function_call"}),
    }
}

fn int_arg(v: &Value) -> Res<i32> {
    Ok(signed(v, "args", i32::MIN.into(), i32::MAX.into())? as i32)
}

fn long_token(o: &Obj) -> Res<i64> {
    signed(member(o, "token")?, "token", -MAX_TOKEN, MAX_TOKEN)
}

struct SigI64I32;
impl Signature for SigI64I32 {
    type A = (i32,);
    type R = i64;
    type Token = i64;
    fn token(o: &Obj) -> Res<i64> {
        long_token(o)
    }
    fn with_callable<B: Big, U: Use<(i32,), i64>>(
        spec: &Value,
        token: &Rc<i64>,
        u: U,
    ) -> Res<U::Out> {
        match spec_kind(spec)?.as_str() {
            "add" => {
                let k = read_k(fields(spec, &["kind", "k"])?)?;
                Ok(u.apply(move |x: i32| i64::from(x) + i64::from(k)))
            }
            "mul" => {
                let k = read_k(fields(spec, &["kind", "k"])?)?;
                Ok(u.apply(move |x: i32| i64::from(x) * i64::from(k)))
            }
            "shared" => {
                let k = read_k(fields(spec, &["kind", "k"])?)?;
                let p = Rc::clone(token);
                Ok(u.apply(move |x: i32| *p + i64::from(x) + i64::from(k)))
            }
            "plain" => {
                fields(spec, &["kind"])?;
                Ok(u.apply(|x: i32| i64::from(x) * 2))
            }
            "big" => B::i64_i32(u, read_big(fields(spec, &["kind", "values"])?)?),
            _ => fail("unknown callable"),
        }
    }
    fn invoke<const CAP: usize>(
        function: &InplaceFunction<'static, (i32,), i64, CAP>,
        args: &Value,
    ) -> Res<Value> {
        let a = args.as_array().ok_or("expected array: args")?;
        let [x] = a.as_slice() else {
            return fail("wrong argument count");
        };
        Ok(outcome(function.call((int_arg(x)?,))))
    }
}

struct SigI64I32I32;
impl Signature for SigI64I32I32 {
    type A = (i32, i32);
    type R = i64;
    type Token = i64;
    fn token(o: &Obj) -> Res<i64> {
        long_token(o)
    }
    fn with_callable<B: Big, U: Use<(i32, i32), i64>>(
        spec: &Value,
        token: &Rc<i64>,
        u: U,
    ) -> Res<U::Out> {
        match spec_kind(spec)?.as_str() {
            "add" => {
                let k = read_k(fields(spec, &["kind", "k"])?)?;
                Ok(u.apply(move |x: i32, y: i32| i64::from(x) + i64::from(y) + i64::from(k)))
            }
            "mul" => {
                let k = read_k(fields(spec, &["kind", "k"])?)?;
                Ok(u.apply(move |x: i32, y: i32| i64::from(x) * i64::from(k) + i64::from(y)))
            }
            "shared" => {
                fields(spec, &["kind"])?;
                let p = Rc::clone(token);
                Ok(u.apply(move |x: i32, y: i32| *p + i64::from(x) + i64::from(y)))
            }
            "plain" => {
                fields(spec, &["kind"])?;
                Ok(u.apply(|x: i32, y: i32| i64::from(x) - i64::from(y)))
            }
            "big" => B::i64_i32_i32(u, read_big(fields(spec, &["kind", "values"])?)?),
            _ => fail("unknown callable"),
        }
    }
    fn invoke<const CAP: usize>(
        function: &InplaceFunction<'static, (i32, i32), i64, CAP>,
        args: &Value,
    ) -> Res<Value> {
        let a = args.as_array().ok_or("expected array: args")?;
        let [x, y] = a.as_slice() else {
            return fail("wrong argument count");
        };
        let x = int_arg(x)?;
        let y = int_arg(y)?;
        Ok(outcome(function.call((x, y))))
    }
}

struct SigI64;
impl Signature for SigI64 {
    type A = ();
    type R = i64;
    type Token = i64;
    fn token(o: &Obj) -> Res<i64> {
        long_token(o)
    }
    fn with_callable<B: Big, U: Use<(), i64>>(spec: &Value, token: &Rc<i64>, u: U) -> Res<U::Out> {
        match spec_kind(spec)?.as_str() {
            "add" => {
                let k = read_k(fields(spec, &["kind", "k"])?)?;
                Ok(u.apply(move || i64::from(k)))
            }
            "shared" => {
                let k = read_k(fields(spec, &["kind", "k"])?)?;
                let p = Rc::clone(token);
                Ok(u.apply(move || *p + i64::from(k)))
            }
            "plain" => {
                fields(spec, &["kind"])?;
                Ok(u.apply(|| 42i64))
            }
            "big" => B::i64(u, read_big(fields(spec, &["kind", "values"])?)?),
            _ => fail("unknown callable"),
        }
    }
    fn invoke<const CAP: usize>(
        function: &InplaceFunction<'static, (), i64, CAP>,
        args: &Value,
    ) -> Res<Value> {
        let a = args.as_array().ok_or("expected array: args")?;
        if !a.is_empty() {
            return fail("wrong argument count");
        }
        Ok(outcome(function.call(())))
    }
}

struct SigString;
impl Signature for SigString {
    type A = (String,);
    type R = String;
    type Token = String;
    fn token(o: &Obj) -> Res<String> {
        text(o, "token")
    }
    fn with_callable<B: Big, U: Use<(String,), String>>(
        spec: &Value,
        token: &Rc<String>,
        u: U,
    ) -> Res<U::Out> {
        match spec_kind(spec)?.as_str() {
            "add" => {
                let s = text(fields(spec, &["kind", "s"])?, "s")?;
                Ok(u.apply(move |arg: String| arg + &s))
            }
            "shared" => {
                fields(spec, &["kind"])?;
                let p = Rc::clone(token);
                Ok(u.apply(move |arg: String| format!("{p}{arg}")))
            }
            "plain" => {
                fields(spec, &["kind"])?;
                Ok(u.apply(|arg: String| format!("{arg}{arg}")))
            }
            _ => fail("unknown callable"),
        }
    }
    fn invoke<const CAP: usize>(
        function: &InplaceFunction<'static, (String,), String, CAP>,
        args: &Value,
    ) -> Res<Value> {
        let a = args.as_array().ok_or("expected array: args")?;
        let [arg] = a.as_slice() else {
            return fail("wrong argument count");
        };
        let arg = arg
            .as_str()
            .filter(|s| s.len() <= MAX_TEXT)
            .ok_or("wrong argument count")?;
        Ok(outcome(function.call((arg.to_string(),))))
    }
}

type Slot<S, const CAP: usize> =
    Option<InplaceFunction<'static, <S as Signature>::A, <S as Signature>::R, CAP>>;

fn slot_index(o: &Obj, name: &str) -> Res<usize> {
    Ok(signed(member(o, name)?, name, 0, SLOTS as i64 - 1)? as usize)
}

fn live<'a, T>(slots: &'a mut [Option<T>; SLOTS], o: &Obj, name: &str) -> Res<&'a mut T> {
    let index = slot_index(o, name)?;
    slots
        .get_mut(index)
        .and_then(Option::as_mut)
        .ok_or_else(|| "slot not constructed".to_string())
}

fn vacant<T>(slots: &[Option<T>; SLOTS], o: &Obj) -> Res<usize> {
    let index = slot_index(o, "slot")?;
    if slots.get(index).is_some_and(Option::is_some) {
        return fail("slot already constructed");
    }
    Ok(index)
}

fn run_function<S: Signature, const CAP: usize, B: Big>(o: &Obj) -> Res<Value> {
    let token = Rc::new(S::token(o)?);
    let steps = array(o, "steps", MAX_STEPS)?;
    let steps = steps
        .iter()
        .map(|s| {
            s.as_object()
                .filter(|o| o.contains_key("step"))
                .ok_or_else(|| "expected step object".to_string())
        })
        .collect::<Res<Vec<_>>>()?;

    let mut slots: [Slot<S, CAP>; SLOTS] = Default::default();
    let mut results = Vec::new();

    for o in steps {
        let step = Value::Object(o.clone());
        match text(o, "step")?.as_str() {
            "use_count" => {
                fields(&step, &["step"])?;
                results.push(json!(Rc::strong_count(&token)));
            }
            "construct_empty" => {
                fields(&step, &["step", "slot"])?;
                let index = vacant(&slots, o)?;
                if let Some(slot) = slots.get_mut(index) {
                    *slot = Some(InplaceFunction::new());
                }
                results.push(Value::Null);
            }
            "construct" => {
                fields(&step, &["step", "slot", "callable"])?;
                let index = vacant(&slots, o)?;
                let function =
                    S::with_callable::<B, _>(member(o, "callable")?, &token, Construct::<CAP>)?;
                if let Some(slot) = slots.get_mut(index) {
                    *slot = Some(function);
                }
                results.push(Value::Null);
            }
            "copy_construct" => {
                fields(&step, &["step", "slot", "from"])?;
                let copy = live(&mut slots, o, "from")?.clone();
                let index = vacant(&slots, o)?;
                if let Some(slot) = slots.get_mut(index) {
                    *slot = Some(copy);
                }
                results.push(Value::Null);
            }
            "move_construct" => {
                fields(&step, &["step", "slot", "from"])?;
                live(&mut slots, o, "from")?;
                let index = vacant(&slots, o)?;
                let moved = InplaceFunction::take(live(&mut slots, o, "from")?);
                if let Some(slot) = slots.get_mut(index) {
                    *slot = Some(moved);
                }
                results.push(Value::Null);
            }
            "assign" => {
                fields(&step, &["step", "slot", "callable"])?;
                let target = live(&mut slots, o, "slot")?;
                S::with_callable::<B, _>(member(o, "callable")?, &token, Assign(target))?;
                results.push(Value::Null);
            }
            "copy_assign" | "move_assign" => {
                fields(&step, &["step", "slot", "from"])?;
                let target = slot_index(o, "slot")?;
                live(&mut slots, o, "slot")?;
                let from = slot_index(o, "from")?;
                live(&mut slots, o, "from")?;
                let copy = o.get("step").and_then(Value::as_str) == Some("copy_assign");
                // Assigning a wrapper to itself does nothing (this == &aOther); Rust's
                // exclusive borrows cannot express that call, so it is not made.
                if target != from {
                    let mut source = slots
                        .get_mut(from)
                        .and_then(Option::take)
                        .ok_or("slot not constructed")?;
                    if let Some(Some(destination)) = slots.get_mut(target) {
                        if copy {
                            destination.copy_assign(&source);
                        } else {
                            destination.move_assign(&mut source);
                        }
                    }
                    if let Some(slot) = slots.get_mut(from) {
                        *slot = Some(source);
                    }
                }
                results.push(Value::Null);
            }
            "destroy" => {
                fields(&step, &["step", "slot"])?;
                live(&mut slots, o, "slot")?;
                if let Some(slot) = slots.get_mut(slot_index(o, "slot")?) {
                    *slot = None;
                }
                results.push(Value::Null);
            }
            "bool" => {
                fields(&step, &["step", "slot"])?;
                results.push(json!(live(&mut slots, o, "slot")?.is_set()));
            }
            "invoke" => {
                fields(&step, &["step", "slot", "args"])?;
                let function = live(&mut slots, o, "slot")?;
                results.push(S::invoke(function, member(o, "args")?)?);
            }
            _ => return fail("unknown step"),
        }
    }

    // The remaining slots are destroyed before the final count.
    drop(slots);
    Ok(json!({"results": results, "final_use_count": Rc::strong_count(&token)}))
}

fn run_capacity<S: Signature>(o: &Obj) -> Res<Value> {
    let capacity = member(o, "capacity")?;
    if capacity.as_str() == Some("default") {
        return run_function::<S, DEFAULT_CAPACITY, NoBig>(o);
    }
    if capacity.is_u64() && capacity.as_u64() == Some(64) {
        return run_function::<S, 64, WithBig>(o);
    }
    fail("unknown capacity")
}

fn function_op(v: &Value) -> Res<Value> {
    let o = fields(v, &["op", "signature", "capacity", "token", "steps"])?;
    match text(o, "signature")?.as_str() {
        "i64(i32)" => run_capacity::<SigI64I32>(o),
        "i64(i32,i32)" => run_capacity::<SigI64I32I32>(o),
        "i64()" => run_capacity::<SigI64>(o),
        "string(string)" => run_capacity::<SigString>(o),
        _ => fail("unknown signature"),
    }
}

fn run(v: &Value) -> Res<Value> {
    let o = v
        .as_object()
        .filter(|o| o.contains_key("op"))
        .ok_or("unexpected or missing field")?;
    let op = text(o, "op")?;
    match op.as_str() {
        "typeinfo" => type_info(v),
        "enumerators" => enumerators(v),
        "mirror_val" | "mirror" | "mirror_vector" | "mirror_axis" => mirror_op(v, &op),
        "throttle" => throttle(v),
        "leak_at_exit" => leak_op(v),
        "inplace_function" => function_op(v),
        _ => fail("unknown operation"),
    }
}

pub fn dispatch(line: &str) -> Value {
    let mut unit = Value::Null;
    let mut case = Value::Null;
    let failure = |unit: Value, case: Value, kind: &str, message: &str| json!({"case":case,"error":{"kind":kind,"message":message},"unit":unit});
    let request = match kicad_difftest::request::parse(line) {
        Ok(request) => request,
        Err(_) => {
            return failure(
                unit,
                case,
                "invalid_request",
                "invalid JSON or duplicate key",
            )
        }
    };
    if let Some(value) = request.get("unit").filter(|v| v.is_string()) {
        unit = value.clone();
    }
    if let Some(value) = request.get("case").filter(|v| v.is_string()) {
        case = value.clone();
    }
    let shaped = request.as_object().is_some_and(|o| {
        o.len() == 3 && ["unit", "case", "input"].iter().all(|k| o.contains_key(*k))
    });
    if !shaped
        || unit.as_str().is_none_or(str::is_empty)
        || case.as_str().is_none_or(str::is_empty)
        || !request["input"].is_object()
    {
        return failure(
            unit,
            case,
            "invalid_request",
            "expected unit, case, input object",
        );
    }
    if unit != UNIT {
        return failure(unit, case, "unknown_unit", "unit has no registered shim");
    }
    match run(&request["input"]) {
        Ok(result) => json!({"case":case,"result":result,"unit":unit}),
        Err(message) => failure(unit, case, "invalid_input", &message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const CASES: usize = 3599;
    const CONTROLS: usize = 293;

    #[test]
    fn public_cases_all_succeed() {
        let mut count = 0;
        for line in
            include_str!("../../../port/cases/include.core.application.pure.p001.jsonl").lines()
        {
            let reply = dispatch(line);
            assert!(reply.get("result").is_some(), "{reply}");
            count += 1;
        }
        assert_eq!(count, CASES);
    }

    #[test]
    fn controls_refuse_with_the_recorded_error() -> Result<(), serde_json::Error> {
        let mut count = 0;
        for line in
            include_str!("../../../port/oracle/fixtures/core-app-include-p001-controls.jsonl")
                .lines()
        {
            let control: Value = serde_json::from_str(line)?;
            let reply = dispatch(&control["request"].to_string());
            assert_eq!(reply["error"], control["error"], "{reply}");
            assert!(reply.get("result").is_none(), "{reply}");
            count += 1;
        }
        assert_eq!(count, CONTROLS);
        Ok(())
    }

    #[test]
    fn envelope_failures_are_not_results() {
        assert_eq!(dispatch("{")["error"]["kind"], "invalid_request");
        let other = json!({"unit": "x", "case": "c", "input": {"op": "enumerators"}});
        assert_eq!(
            dispatch(&other.to_string())["error"]["kind"],
            "unknown_unit"
        );
        let extra = json!({"unit": UNIT, "case": "c", "input": {}, "x": 1});
        assert_eq!(
            dispatch(&extra.to_string())["error"]["kind"],
            "invalid_request"
        );
    }

    /// The enumerator table is the header's: 115 names, -1 to 113, S3D_CACHE_T at 104.
    #[test]
    fn enumerator_ordinals() {
        assert_eq!(typeinfo::ENUMERATORS.len(), 115);
        assert_eq!(typeinfo::ENUMERATORS.first().map(|e| e.1.value()), Some(-1));
        for pair in typeinfo::ENUMERATORS.windows(2) {
            assert_eq!(pair[0].1.value() + 1, pair[1].1.value());
        }
        assert_eq!(KicadT::S3D_CACHE_T.value(), 104);
        assert_eq!(KicadT::MAX_STRUCT_TYPE_ID.value(), 113);
    }
}
