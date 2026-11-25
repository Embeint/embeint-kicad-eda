// SPDX-License-Identifier: GPL-3.0-or-later
use kicad_math::vector::{
    Vector2CastFrom, Vector2Scalar, Vector3CastFrom, VectorCommon, VectorComponent,
};
use kicad_math::{Vector2, Vector3, VectorDomainError};
use serde_json::{json, Value};
pub const UNIT: &str = "libs.kimath.include.math.application.ui.p002";
#[derive(Debug)]
pub enum Error {
    Invalid(String),
    Domain(VectorDomainError),
}
impl From<VectorDomainError> for Error {
    fn from(e: VectorDomainError) -> Self {
        Self::Domain(e)
    }
}
impl From<String> for Error {
    fn from(e: String) -> Self {
        Self::Invalid(e)
    }
}
type Result<T> = std::result::Result<T, Error>;
pub fn keys(v: &Value, required: &[&str], optional: &[&str]) -> std::result::Result<(), String> {
    let o = v.as_object().ok_or("expected object")?;
    if required.iter().any(|k| !o.contains_key(*k))
        || o.keys()
            .any(|k| !required.contains(&k.as_str()) && !optional.contains(&k.as_str()))
    {
        return Err("unexpected or missing field".into());
    }
    Ok(())
}
fn invalid<T>(s: &str) -> Result<T> {
    Err(Error::Invalid(s.into()))
}
trait Wire: VectorComponent {
    fn read(v: &Value) -> Result<Self>;
    fn wire(self) -> Value;
    fn add_float(v: Vector2<Self>, scalar: f64, sub: bool) -> Result<Vector2<Self>>;
}
macro_rules! signed_wire {
    ($t:ty) => {
        impl Wire for $t {
            fn read(v: &Value) -> Result<Self> {
                v.as_i64()
                    .and_then(|n| Self::try_from(n).ok())
                    .ok_or_else(|| Error::Invalid("invalid component".into()))
            }
            fn wire(self) -> Value {
                json!(self)
            }
            fn add_float(v: Vector2<Self>, s: f64, sub: bool) -> Result<Vector2<Self>> {
                Ok(if sub {
                    v.sub_float(s)?
                } else {
                    v.add_float(s)?
                })
            }
        }
    };
}
signed_wire!(i32);
signed_wire!(i64);
impl Wire for u32 {
    fn read(v: &Value) -> Result<Self> {
        v.as_u64()
            .and_then(|n| Self::try_from(n).ok())
            .ok_or_else(|| Error::Invalid("invalid component".into()))
    }
    fn wire(self) -> Value {
        json!(self)
    }
    fn add_float(_v: Vector2<Self>, _s: f64, _sub: bool) -> Result<Vector2<Self>> {
        invalid("unsupported component type")
    }
}
impl Wire for f64 {
    fn read(v: &Value) -> Result<Self> {
        let s = v
            .as_str()
            .ok_or_else(|| Error::Invalid("invalid component".into()))?;
        if s.len() != 16
            || !s
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return invalid("invalid component");
        }
        u64::from_str_radix(s, 16)
            .map(Self::from_bits)
            .map_err(|_| Error::Invalid("invalid component".into()))
    }
    fn wire(self) -> Value {
        json!(format!("{:016x}", self.to_bits()))
    }
    fn add_float(v: Vector2<Self>, s: f64, sub: bool) -> Result<Vector2<Self>> {
        Ok(if sub {
            v.try_sub_scalar(s)?
        } else {
            v.try_add_scalar(s)?
        })
    }
}
fn array<T: Wire>(v: &Value, size: usize) -> Result<Vec<T>> {
    let a = v
        .as_array()
        .filter(|a| a.len() == size)
        .ok_or_else(|| Error::Invalid("invalid vector".into()))?;
    a.iter().map(T::read).collect()
}
fn v2<T: Wire>(v: &Value) -> Result<Vector2<T>> {
    let a = array::<T>(v, 2)?;
    Ok(Vector2::new(a[0], a[1]))
}
fn v3<T: Wire>(v: &Value) -> Result<Vector3<T>> {
    let a = array::<T>(v, 3)?;
    Ok(Vector3::new(a[0], a[1], a[2]))
}
fn w2<T: Wire>(v: Vector2<T>) -> Value {
    json!([v.x.wire(), v.y.wire()])
}
fn w3<T: Wire>(v: Vector3<T>) -> Value {
    json!([v.x.wire(), v.y.wire(), v.z.wire()])
}
fn val(v: Value) -> Value {
    json!({"value":v})
}
fn mutation(v: Value, state: Value, same_ref: bool) -> Value {
    json!({"value":v,"state":state,"same_ref":same_ref})
}
fn run2<T: Wire + VectorCommon<T, Common = T>>(i: &Value, op: &str) -> Result<Value>
where
    T::Extended: Wire,
    i32: Vector2CastFrom<T>,
    i64: Vector2CastFrom<T>,
    f64: Vector2CastFrom<T>,
{
    let mut a = v2::<T>(&i["a"])?;
    let b = if i.get("b").is_some() && op != "distance" {
        v2::<T>(&i["b"])?
    } else {
        Vector2::default()
    };
    let s = if i.get("scalar").is_some() && !matches!(op, "divide" | "add_float" | "sub_float") {
        T::read(&i["scalar"])?
    } else {
        T::default()
    };
    Ok(match op {
        "new" | "copy" => val(w2(a)),
        "assign" => {
            let mut destination = Vector2::default();
            let address = std::ptr::from_ref(&destination);
            let returned = std::ptr::from_mut(destination.assign_from(a));
            mutation(
                w2(destination),
                w2(destination),
                std::ptr::eq(address, returned),
            )
        }
        "default" => val(w2(Vector2::<T>::default())),
        "traits" => {
            json!({"min":Vector2::<T>::ECOORD_MIN.wire(),"max":Vector2::<T>::ECOORD_MAX.wire()})
        }
        "compare" => {
            json!({"eq":a==b,"ne":a!=b,"lt":a<b,"le":a<=b,"gt":a>b,"ge":a>=b,"compare":a.lexicographical_compare(&b),"min":w2(*a.lexicographical_min(&b)),"max":w2(*a.lexicographical_max(&b)),"min_first":std::ptr::eq(&a,a.lexicographical_min(&b)),"max_first":std::ptr::eq(&a,a.lexicographical_max(&b))})
        }
        "add" => val(w2(kicad_math::vector::vector2_add(a, b)?)),
        "sub" => val(w2(kicad_math::vector::vector2_sub(a, b)?)),
        "dot" => val(a.dot(b)?.wire()),
        "operator_dot" => val(kicad_math::vector::vector2_dot(a, b)?.wire()),
        "cross" => val(a.cross(b)?.wire()),
        "squared_distance" => val(a.squared_distance(b)?.wire()),
        "distance" => val(a.distance(v2::<T::Extended>(&i["b"])?)?.wire()),
        "norm" => val(a.euclidean_norm()?.wire()),
        "squared_norm" => val(a.squared_euclidean_norm()?.wire()),
        "neg" => val(w2(a.try_neg()?)),
        "perpendicular" => val(w2(a.perpendicular()?)),
        "mul_scalar" => val(w2(kicad_math::vector::vector2_mul(a, s)?)),
        "scalar_mul" => val(w2(kicad_math::vector::scalar_mul_vector2(s, a)?)),
        "add_scalar" => val(w2(a.try_add_scalar(s)?)),
        "sub_scalar" => val(w2(a.try_sub_scalar(s)?)),
        "add_float" | "sub_float" => val(w2(T::add_float(
            a,
            f64::read(&i["scalar"])?,
            op == "sub_float",
        )?)),
        "divide" => val(w2(a.divide(f64::read(&i["scalar"])?)?)),
        "resize" => val(w2(a.resize(s)?)),
        "format" => val(json!(a.format()?)),
        "stream" => val(json!(a.stream()?)),
        "cast" | "cast_call" => val(match i["target"].as_str() {
            Some("i32") => w2(a.try_cast::<i32>()?),
            Some("i64") => w2(a.try_cast::<i64>()?),
            Some("f64") => w2(a.try_cast::<f64>()?),
            _ => return invalid("unsupported target type"),
        }),
        "add_assign" | "sub_assign" | "mul_assign" => {
            let address = std::ptr::from_ref(&a);
            let returned = match op {
                "add_assign" => a.add_assign(b)?,
                "sub_assign" => a.sub_assign(b)?,
                _ => a.mul_assign(b)?,
            };
            let same = std::ptr::eq(address, std::ptr::from_mut(returned));
            mutation(w2(a), w2(a), same)
        }
        "add_assign_scalar" | "sub_assign_scalar" | "mul_assign_scalar" => {
            let operand = match i.get("alias").and_then(Value::as_str) {
                Some("x") => Vector2Scalar::X,
                Some("y") => Vector2Scalar::Y,
                None => Vector2Scalar::Value(s),
                _ => return invalid("invalid scalar alias"),
            };
            let address = std::ptr::from_ref(&a);
            let returned = match op {
                "add_assign_scalar" => a.add_assign_operand(operand)?,
                "sub_assign_scalar" => a.sub_assign_operand(operand)?,
                _ => a.mul_assign_operand(operand)?,
            };
            let same = std::ptr::eq(address, std::ptr::from_mut(returned));
            mutation(w2(a), w2(a), same)
        }
        _ => return invalid("unknown operation"),
    })
}
fn run3<T: Wire>(i: &Value, op: &str) -> Result<Value>
where
    T::Extended: Wire,
    i32: Vector3CastFrom<T>,
    u32: Vector3CastFrom<T>,
    f64: Vector3CastFrom<T>,
{
    let mut a = v3::<T>(&i["a"])?;
    let b = if i.get("b").is_some() {
        v3::<T>(&i["b"])?
    } else {
        Vector3::default()
    };
    let s = if i.get("scalar").is_some() {
        T::read(&i["scalar"])?
    } else {
        T::default()
    };
    Ok(match op {
        "new" | "copy" => val(w3(a)),
        "assign" => {
            let mut destination = Vector3::default();
            let address = std::ptr::from_ref(&destination);
            let returned = std::ptr::from_mut(destination.assign_from(a));
            mutation(
                w3(destination),
                w3(destination),
                std::ptr::eq(address, returned),
            )
        }
        "default" => val(w3(Vector3::<T>::default())),
        "traits" => {
            json!({"min":Vector3::<T>::ECOORD_MIN.wire(),"max":Vector3::<T>::ECOORD_MAX.wire()})
        }
        "compare" => json!({"eq":a==b,"ne":a!=b}),
        "dot" => val(a.dot(b)?.wire()),
        "cross" => val(w3(a.cross(b)?)),
        "norm" => val(a.euclidean_norm()?.wire()),
        "normalize" => {
            let v = a.normalize()?;
            mutation(w3(v), w3(a), std::ptr::eq(&v, &a))
        }
        "set_all" => {
            let v = a.set_all(s);
            mutation(w3(v), w3(a), std::ptr::eq(&v, &a))
        }
        "mul_assign_scalar" | "div_assign_scalar" => {
            let s = match i.get("alias").and_then(Value::as_str) {
                Some("x") => a.x,
                Some("y") => a.y,
                Some("z") => a.z,
                None => s,
                _ => return invalid("invalid scalar alias"),
            };
            let address = std::ptr::from_ref(&a);
            let returned = if op == "mul_assign_scalar" {
                a.mul_assign_scalar(s)?
            } else {
                a.div_assign_scalar(s)?
            };
            let same = std::ptr::eq(address, std::ptr::from_mut(returned));
            mutation(w3(a), w3(a), same)
        }
        "stream" => val(json!(a.stream()?)),
        "cast" => val(match i["target"].as_str() {
            Some("i32") => w3(a.try_cast::<i32>()?),
            Some("u32") => w3(a.try_cast::<u32>()?),
            Some("f64") => w3(a.try_cast::<f64>()?),
            _ => return invalid("unsupported target type"),
        }),
        _ => return invalid("unknown operation"),
    })
}
pub fn run(i: &Value) -> Result<Value> {
    let op = i["op"]
        .as_str()
        .ok_or_else(|| Error::Invalid("unknown operation".into()))?;
    let mut fields = vec!["op", "dim", "type", "a"];
    let binary = matches!(
        op,
        "compare"
            | "add"
            | "sub"
            | "dot"
            | "operator_dot"
            | "cross"
            | "distance"
            | "squared_distance"
            | "add_assign"
            | "sub_assign"
            | "mul_assign"
            | "equals"
    );
    if binary {
        fields.push("b");
    }
    let scalar = matches!(
        op,
        "mul_scalar"
            | "scalar_mul"
            | "add_scalar"
            | "sub_scalar"
            | "divide"
            | "resize"
            | "set_all"
            | "add_float"
            | "sub_float"
    );
    let compound = matches!(
        op,
        "mul_assign_scalar" | "div_assign_scalar" | "add_assign_scalar" | "sub_assign_scalar"
    );
    if scalar || (compound && i.get("alias").is_none()) {
        fields.push("scalar");
    }
    if matches!(op, "cast" | "cast_call") {
        fields.push("target");
    }
    let optional = if compound {
        vec!["alias"]
    } else if op == "equals" {
        vec!["epsilon"]
    } else {
        vec![]
    };
    keys(i, &fields, &optional)?;
    if let Some(alias) = i.get("alias") {
        if !alias.is_string() || alias.as_str().is_none_or(str::is_empty) {
            return invalid("invalid scalar alias");
        }
    }
    // The scalar helper wrappers have double-only declarations.
    if op == "equals" || op == "ki_round" || op == "ki_round_xy" {
        if i["dim"] != 2 || i["type"] != "f64" {
            return invalid("unsupported component type");
        }
        let a = v2::<f64>(&i["a"])?;
        return Ok(if op == "equals" {
            val(json!(a.equals(
                v2::<f64>(&i["b"])?,
                i.get("epsilon").map(f64::read).transpose()?
            )?))
        } else if op == "ki_round" {
            val(w2(a.ki_round()?))
        } else {
            val(w2(kicad_math::vector::ki_round_xy(a.x, a.y)?))
        });
    }
    let result = match (i["dim"].as_u64(), i["type"].as_str()) {
        (Some(2), Some("i32")) => run2::<i32>(i, op),
        (Some(2), Some("i64")) => run2::<i64>(i, op),
        (Some(2), Some("f64")) => run2::<f64>(i, op),
        (Some(3), Some("i32")) => run3::<i32>(i, op),
        (Some(3), Some("u32")) => run3::<u32>(i, op),
        (Some(3), Some("f64")) => run3::<f64>(i, op),
        _ => invalid("unsupported component type"),
    };
    result.map_err(|error| match error {
        Error::Domain(VectorDomainError::Unported {
            upstream: "VECTOR floating arithmetic: distinct NaN operands",
            unit,
        }) => {
            // Attribute the still-unqualified NaN site to the actual upstream call,
            // not to a fictitious missing scalar dependency.
            let upstream = match (i["dim"].as_u64(), op) {
                (Some(3), "cross") => "VECTOR3<T>::Cross",
                (Some(3), "dot") => "VECTOR3<T>::Dot",
                (Some(3), "norm") => "VECTOR3<T>::EuclideanNorm",
                (Some(3), "normalize") => "VECTOR3<T>::Normalize",
                (Some(3), "mul_assign_scalar") => "VECTOR3<T>::operator*=",
                (Some(3), "div_assign_scalar") => "VECTOR3<T>::operator/=",
                (_, "cross") => "VECTOR2<T>::Cross",
                (_, "dot") => "VECTOR2<T>::Dot",
                (_, "operator_dot") => "operator*(VECTOR2<T>, VECTOR2<U>)",
                (_, "norm") => "VECTOR2<T>::EuclideanNorm",
                (_, "squared_norm") => "VECTOR2<T>::SquaredEuclideanNorm",
                (_, "distance") => "VECTOR2<T>::Distance",
                (_, "squared_distance") => "VECTOR2<T>::SquaredDistance",
                (_, "resize") => "VECTOR2<T>::Resize",
                (_, "divide") => "VECTOR2<T>::operator/",
                (_, "mul_assign" | "mul_assign_scalar") => "VECTOR2<T>::operator*=",
                (_, "add_assign" | "add_assign_scalar") => "VECTOR2<T>::operator+=",
                (_, "sub_assign" | "sub_assign_scalar") => "VECTOR2<T>::operator-=",
                (_, "add" | "add_scalar" | "add_float") => "operator+(VECTOR2<T>, U)",
                (_, "sub" | "sub_scalar" | "sub_float") => "operator-(VECTOR2<T>, U)",
                (_, "scalar_mul") => "operator*(T, VECTOR2<U>)",
                _ => "operator*(VECTOR2<T>, U)",
            };
            Error::Domain(VectorDomainError::Unported { upstream, unit })
        }
        other => other,
    })
}
