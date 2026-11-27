// SPDX-License-Identifier: GPL-3.0-or-later
//! `isa`: the `IsA` and `dyn_cast` templates of `typeinfo.h` over a fixture hierarchy whose
//! classes carry their own `ClassOf` predicates, as the C++ adapter's `FIX_*` classes do.
//! The fixture is generic: it is not one of KiCad's production hierarchies, whose `ClassOf`
//! implementations belong to the units that own them. It exercises the templates' own
//! branches: the null short-circuit of `IsA( const I* )`, the reference form, `dyn_cast`
//! passing a null pointer to `ClassOf`, accepted and rejected casts, a base-to-derived and a
//! derived-to-base cast, a cast whose C++ `static_cast` adjusts the pointer (a second base),
//! const and non-const forms, and a write through the casted pointer into the original.
//!
//! Every `ClassOf` call is counted, so the reply shows whether the template called it.

use super::{fail, fields, member, signed, text, Res};
use kicad_core::typeinfo::{dyn_cast, dyn_cast_mut, is_a, is_a_ref, ClassOf};
use serde_json::{json, Value};
use std::any::Any;
use std::cell::Cell;

thread_local! {
    /// `ClassOf` calls in the current case.
    static CALLS: Cell<u32> = const { Cell::new(0) };
}

fn called() {
    CALLS.with(|calls| calls.set(calls.get() + 1));
}

/// The class tag every fixture predicate reads (the fixture's `Type()`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Base,
    Derived,
    Leaf,
    Other,
    Nullable,
    Multi,
}

/// `FIX_BASE`.
struct FixBase {
    kind: Kind,
    value: i32,
}

/// `FIX_DERIVED : FIX_BASE`.
struct FixDerived {
    base: FixBase,
}

/// `FIX_LEAF : FIX_DERIVED`.
struct FixLeaf {
    derived: FixDerived,
}

/// `FIX_OTHER : FIX_BASE`, a sibling of `FIX_DERIVED`.
struct FixOther {
    base: FixBase,
}

/// `FIX_NULLABLE : FIX_BASE`, whose `ClassOf` accepts a null pointer.
struct FixNullable {
    base: FixBase,
}

/// `FIX_MULTI : FIX_SIDE, FIX_BASE`: `FIX_BASE` is its second base, so the C++
/// `static_cast` from `FIX_BASE*` adjusts the pointer (Rust references name the part
/// directly).
struct FixMulti {
    base: FixBase,
}

/// An object seen through a `FIX_BASE*`.
trait FixItem {
    fn base(&self) -> &FixBase;
    fn base_mut(&mut self) -> &mut FixBase;
    fn any(&self) -> &dyn Any;
    fn any_mut(&mut self) -> &mut dyn Any;
}

/// An object seen through a `FIX_DERIVED*`.
trait DerivedItem {
    fn derived(&self) -> &FixDerived;
    fn derived_mut(&mut self) -> &mut FixDerived;
    fn any(&self) -> &dyn Any;
    fn any_mut(&mut self) -> &mut dyn Any;
}

macro_rules! item {
    ($class:ty, $($base:ident).+) => {
        impl FixItem for $class {
            fn base(&self) -> &FixBase {
                &self.$($base).+
            }
            fn base_mut(&mut self) -> &mut FixBase {
                &mut self.$($base).+
            }
            fn any(&self) -> &dyn Any {
                self
            }
            fn any_mut(&mut self) -> &mut dyn Any {
                self
            }
        }
    };
}
item!(FixDerived, base);
item!(FixLeaf, derived.base);
item!(FixOther, base);
item!(FixNullable, base);
item!(FixMulti, base);

impl FixItem for FixBase {
    fn base(&self) -> &FixBase {
        self
    }
    fn base_mut(&mut self) -> &mut FixBase {
        self
    }
    fn any(&self) -> &dyn Any {
        self
    }
    fn any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl DerivedItem for FixDerived {
    fn derived(&self) -> &FixDerived {
        self
    }
    fn derived_mut(&mut self) -> &mut FixDerived {
        self
    }
    fn any(&self) -> &dyn Any {
        self
    }
    fn any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl DerivedItem for FixLeaf {
    fn derived(&self) -> &FixDerived {
        &self.derived
    }
    fn derived_mut(&mut self) -> &mut FixDerived {
        &mut self.derived
    }
    fn any(&self) -> &dyn Any {
        self
    }
    fn any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// `T::ClassOf( const FIX_BASE* )` for both static types: the predicate on the class tag.
trait Predicate {
    fn accepts(kind: Option<Kind>) -> bool;
}

impl Predicate for FixBase {
    fn accepts(kind: Option<Kind>) -> bool {
        kind.is_some()
    }
}
impl Predicate for FixDerived {
    fn accepts(kind: Option<Kind>) -> bool {
        matches!(kind, Some(Kind::Derived | Kind::Leaf))
    }
}
impl Predicate for FixLeaf {
    fn accepts(kind: Option<Kind>) -> bool {
        kind == Some(Kind::Leaf)
    }
}
impl Predicate for FixOther {
    fn accepts(kind: Option<Kind>) -> bool {
        kind == Some(Kind::Other)
    }
}
impl Predicate for FixNullable {
    fn accepts(kind: Option<Kind>) -> bool {
        kind.is_none_or(|kind| kind == Kind::Nullable)
    }
}
impl Predicate for FixMulti {
    fn accepts(kind: Option<Kind>) -> bool {
        kind == Some(Kind::Multi)
    }
}

/// `ClassOf` through a `FIX_BASE*`: count the call, test the tag, and `static_cast` to the
/// class's part of the object.
macro_rules! class_of_base {
    ($class:ty, |$item:ident| $cast:expr, |$item_mut:ident| $cast_mut:expr) => {
        impl ClassOf<dyn FixItem> for $class {
            fn class_of(object: Option<&dyn FixItem>) -> bool {
                called();
                <$class as Predicate>::accepts(object.map(|o| o.base().kind))
            }
            fn static_cast($item: &dyn FixItem) -> Option<&Self> {
                $cast
            }
            fn static_cast_mut<'r>(
                $item_mut: &'r mut (dyn FixItem + 'static),
            ) -> Option<&'r mut Self> {
                $cast_mut
            }
        }
    };
}
class_of_base!(FixBase, |o| Some(o.base()), |o| Some(o.base_mut()));
class_of_base!(
    FixDerived,
    |o| {
        let any = o.any();
        any.downcast_ref::<FixDerived>()
            .or_else(|| any.downcast_ref::<FixLeaf>().map(|leaf| &leaf.derived))
    },
    |o| {
        if o.any().is::<FixDerived>() {
            o.any_mut().downcast_mut::<FixDerived>()
        } else {
            o.any_mut()
                .downcast_mut::<FixLeaf>()
                .map(|leaf| &mut leaf.derived)
        }
    }
);
class_of_base!(FixLeaf, |o| o.any().downcast_ref(), |o| o
    .any_mut()
    .downcast_mut());
class_of_base!(FixOther, |o| o.any().downcast_ref(), |o| o
    .any_mut()
    .downcast_mut());
class_of_base!(FixNullable, |o| o.any().downcast_ref(), |o| o
    .any_mut()
    .downcast_mut());
class_of_base!(FixMulti, |o| o.any().downcast_ref(), |o| o
    .any_mut()
    .downcast_mut());

/// `ClassOf` through a `FIX_DERIVED*` (the implicit derived-to-base conversion of its
/// argument); only the classes a `FIX_DERIVED*` can be `static_cast` to.
macro_rules! class_of_derived {
    ($class:ty, |$item:ident| $cast:expr, |$item_mut:ident| $cast_mut:expr) => {
        impl ClassOf<dyn DerivedItem> for $class {
            fn class_of(object: Option<&dyn DerivedItem>) -> bool {
                called();
                <$class as Predicate>::accepts(object.map(|o| o.derived().base.kind))
            }
            fn static_cast($item: &dyn DerivedItem) -> Option<&Self> {
                $cast
            }
            fn static_cast_mut<'r>(
                $item_mut: &'r mut (dyn DerivedItem + 'static),
            ) -> Option<&'r mut Self> {
                $cast_mut
            }
        }
    };
}
class_of_derived!(FixBase, |o| Some(&o.derived().base), |o| Some(
    &mut o.derived_mut().base
));
class_of_derived!(FixDerived, |o| Some(o.derived()), |o| Some(o.derived_mut()));
class_of_derived!(FixLeaf, |o| o.any().downcast_ref(), |o| o
    .any_mut()
    .downcast_mut());
// `static_cast` from `FIX_DERIVED*` to these classes does not compile, so `dyn_cast` is
// never instantiated for them (the request is refused first); `IsA` only needs `ClassOf`.
class_of_derived!(FixOther, |_o| None, |_o| None);
class_of_derived!(FixNullable, |_o| None, |_o| None);
class_of_derived!(FixMulti, |_o| None, |_o| None);

/// The object a case builds.
enum Object {
    Null,
    Base(Box<dyn FixItem>),
    Derived(Box<dyn DerivedItem>),
}

/// The object a case builds, seen through a `FIX_BASE*` and, for the classes derived from
/// `FIX_DERIVED`, through a `FIX_DERIVED*`.
struct Built {
    item: Box<dyn FixItem>,
    derived: Option<Box<dyn DerivedItem>>,
}

fn build(class: &str, value: i32) -> Res<Built> {
    let base = |kind| FixBase { kind, value };
    let plain = |item: Box<dyn FixItem>| Built {
        item,
        derived: None,
    };
    Ok(match class {
        "base" => plain(Box::new(base(Kind::Base))),
        "derived" => {
            let make = || FixDerived {
                base: base(Kind::Derived),
            };
            Built {
                item: Box::new(make()),
                derived: Some(Box::new(make())),
            }
        }
        "leaf" => {
            let make = || FixLeaf {
                derived: FixDerived {
                    base: base(Kind::Leaf),
                },
            };
            Built {
                item: Box::new(make()),
                derived: Some(Box::new(make())),
            }
        }
        "other" => plain(Box::new(FixOther {
            base: base(Kind::Other),
        })),
        "nullable" => plain(Box::new(FixNullable {
            base: base(Kind::Nullable),
        })),
        "multi" => plain(Box::new(FixMulti {
            base: base(Kind::Multi),
        })),
        _ => return fail("unknown class"),
    })
}

const FORMS: [&str; 7] = [
    "isa_ptr",
    "isa_ptr_type",
    "isa_ref",
    "dyn_cast_const",
    "dyn_cast",
    "derived_isa",
    "derived_dyn_cast",
];
const TARGETS: [&str; 6] = ["base", "derived", "leaf", "other", "nullable", "multi"];

/// A cast's result: null, or the value read through it and whether it designates the
/// original object's part of that class.
fn cast_result<T: FixPart>(result: Option<&T>, expected: Option<*const T>) -> Value {
    match result {
        None => Value::Null,
        Some(r) => json!({
            "value": r.value(),
            "same": expected.is_some_and(|e| core::ptr::eq(r, e)),
        }),
    }
}

/// The `m_value` of a fixture class's object.
trait FixPart {
    fn value(&self) -> i32;
    fn value_mut(&mut self) -> &mut i32;
}
macro_rules! part {
    ($class:ty, $($base:ident).*) => {
        impl FixPart for $class {
            fn value(&self) -> i32 {
                self$(.$base)*.value
            }
            fn value_mut(&mut self) -> &mut i32 {
                &mut self$(.$base)*.value
            }
        }
    };
}
part!(FixBase,);
part!(FixDerived, base);
part!(FixLeaf, derived.base);
part!(FixOther, base);
part!(FixNullable, base);
part!(FixMulti, base);

/// The forms through a `FIX_BASE*` for one target class.
fn through_base<T>(
    form: &str,
    object: Option<&mut (dyn FixItem + 'static)>,
    write: Option<i32>,
) -> Value
where
    T: ClassOf<dyn FixItem> + FixPart,
{
    match form {
        "isa_ptr" | "isa_ptr_type" => json!(is_a::<T, dyn FixItem>(object.as_deref())),
        "isa_ref" => json!(object.is_some_and(|o| is_a_ref::<T, dyn FixItem>(o))),
        "dyn_cast_const" => {
            let expected = object
                .as_deref()
                .and_then(|o| T::static_cast(o))
                .map(|e| e as *const T);
            cast_result(dyn_cast::<T, dyn FixItem>(object.as_deref()), expected)
        }
        _ => {
            let expected = object
                .as_deref()
                .and_then(|o| T::static_cast(o))
                .map(|e| e as *const T);
            let result = dyn_cast_mut::<T, dyn FixItem>(object);
            let reply = cast_result(result.as_deref(), expected);
            if let (Some(r), Some(write)) = (result, write) {
                *r.value_mut() = write;
            }
            reply
        }
    }
}

/// The forms through a `FIX_DERIVED*`.
fn through_derived<T>(form: &str, object: Option<&mut (dyn DerivedItem + 'static)>) -> Value
where
    T: ClassOf<dyn DerivedItem> + FixPart,
{
    if form == "derived_isa" {
        return json!(is_a::<T, dyn DerivedItem>(object.as_deref()));
    }
    let expected = object
        .as_deref()
        .and_then(|o| T::static_cast(o))
        .map(|e| e as *const T);
    CALLS.with(|calls| calls.set(0));
    let result = dyn_cast_mut::<T, dyn DerivedItem>(object);
    cast_result(result.as_deref(), expected)
}

pub(super) fn isa_op(v: &Value) -> Res<Value> {
    let o = fields(v, &["op", "form", "target", "object", "write"])?;
    let form = text(o, "form")?;
    if !FORMS.contains(&form.as_str()) {
        return fail("unknown form");
    }
    let target = text(o, "target")?;
    if !TARGETS.contains(&target.as_str()) {
        return fail("unknown target");
    }
    let spec = member(o, "object")?;
    let built = if spec.is_null() {
        None
    } else {
        let spec = fields(spec, &["class", "value"])?;
        let class = text(spec, "class")?;
        let value = signed(
            member(spec, "value")?,
            "value",
            i32::MIN.into(),
            i32::MAX.into(),
        )? as i32;
        Some(build(&class, value)?)
    };
    let write = member(o, "write")?;
    let write = if write.is_null() {
        None
    } else {
        Some(signed(write, "write", i32::MIN.into(), i32::MAX.into())? as i32)
    };
    if write.is_some() && form != "dyn_cast" {
        return fail("only dyn_cast writes");
    }
    if form == "isa_ref" && built.is_none() {
        return fail("reference needs an object");
    }
    let derived = form.starts_with("derived_");
    let mut object = match built {
        None => Object::Null,
        Some(Built {
            derived: Some(derived_item),
            ..
        }) if derived => Object::Derived(derived_item),
        Some(_) if derived => return fail("object is not a FIX_DERIVED"),
        Some(Built { item, .. }) => Object::Base(item),
    };
    if form == "derived_dyn_cast" && !["base", "derived", "leaf"].contains(&target.as_str()) {
        return fail("cast does not compile");
    }

    CALLS.with(|calls| calls.set(0));
    let result = if derived {
        let item = match &mut object {
            Object::Derived(item) => Some(item.as_mut()),
            _ => None,
        };
        match target.as_str() {
            "base" => through_derived::<FixBase>(&form, item),
            "derived" => through_derived::<FixDerived>(&form, item),
            "leaf" => through_derived::<FixLeaf>(&form, item),
            "other" => through_derived::<FixOther>(&form, item),
            "nullable" => through_derived::<FixNullable>(&form, item),
            _ => through_derived::<FixMulti>(&form, item),
        }
    } else {
        let item = match &mut object {
            Object::Base(item) => Some(item.as_mut()),
            _ => None,
        };
        match target.as_str() {
            "base" => through_base::<FixBase>(&form, item, write),
            "derived" => through_base::<FixDerived>(&form, item, write),
            "leaf" => through_base::<FixLeaf>(&form, item, write),
            "other" => through_base::<FixOther>(&form, item, write),
            "nullable" => through_base::<FixNullable>(&form, item, write),
            _ => through_base::<FixMulti>(&form, item, write),
        }
    };
    let calls = CALLS.with(Cell::get);
    let after = match &object {
        Object::Null => Value::Null,
        Object::Base(item) => json!(item.base().value),
        Object::Derived(item) => json!(item.derived().base.value),
    };
    Ok(json!({"result": result, "class_of_calls": calls, "object": after}))
}
