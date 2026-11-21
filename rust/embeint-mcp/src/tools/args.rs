// SPDX-License-Identifier: GPL-3.0-or-later
//! Tool arguments, validated as FastMCP validates them: `pre_parse_json` (objects and lists sent
//! as JSON text are decoded unless the parameter is a plain `str`), then pydantic's lax mode for
//! the parameter's annotation.  Only acceptance and the converted value must match Python; the
//! error text follows pydantic's form but not its exact wording (the parity fixtures compare the
//! failure and its tool, not the words).

// Some argument types and accessors serve only tools not yet ported
#![allow(dead_code)]

use serde_json::{Map, Number, Value as Json};

/// A parameter's Python annotation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arg {
    /// `str`
    Str,
    /// `str | None`
    OptStr,
    /// `int`
    Int,
    /// `int | None`
    OptInt,
    /// `float`
    Float,
    /// `float | None`
    OptFloat,
    /// `bool`
    Bool,
    /// `bool | None`
    OptBool,
    /// `dict`
    Dict,
    /// `dict | None`
    OptDict,
    /// `list[dict]`
    ListDict,
    /// `list[dict] | None`
    OptListDict,
    /// `list[str]`
    ListStr,
    /// `list[str] | None`
    OptListStr,
    /// `list[float] | None`
    OptListFloat,
    /// `list[list[float]]`
    ListListFloat,
    /// `list[list[float]] | None`
    OptListListFloat,
    /// `float | list[float] | None`
    OptFloatOrListFloat,
    /// `float | dict | None`
    OptFloatOrDict,
}

/// A parameter's default, as the Python signature gives it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Default {
    /// No default: the argument is required.
    Required,
    /// `= None`
    None,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(&'static str),
}

/// One parameter of a tool's Python signature.
#[derive(Clone, Copy, Debug)]
pub struct Param {
    pub name: &'static str,
    pub kind: Arg,
    pub default: Default,
}

/// A required parameter.
pub const fn req(name: &'static str, kind: Arg) -> Param {
    Param {
        name,
        kind,
        default: Default::Required,
    }
}

/// An optional parameter with its default.
pub const fn opt(name: &'static str, kind: Arg, default: Default) -> Param {
    Param {
        name,
        kind,
        default,
    }
}

/// The validated arguments of one call: every parameter is present, converted as pydantic
/// converts it (an `int` sent as `"5"` is 5, a `float` sent as 1 is 1.0), with its default when
/// it was not given.  Accessors panic only on a parameter the tool did not declare with that
/// type, which is a programming error caught by the tests.
#[derive(Clone, Debug, Default)]
pub struct Args(pub Map<String, Json>);

impl Args {
    pub fn get(&self, name: &str) -> &Json {
        self.0.get(name).unwrap_or(&Json::Null)
    }

    pub fn is_null(&self, name: &str) -> bool {
        self.get(name).is_null()
    }

    pub fn str(&self, name: &str) -> &str {
        self.get(name)
            .as_str()
            .unwrap_or_else(|| panic!("{name} is not a validated str"))
    }

    pub fn opt_str(&self, name: &str) -> Option<String> {
        self.get(name).as_str().map(str::to_owned)
    }

    pub fn int(&self, name: &str) -> i64 {
        self.opt_int(name)
            .unwrap_or_else(|| panic!("{name} is not a validated int"))
    }

    /// The int clamped to the i64 range: Python ints are unbounded and validation keeps them
    /// exact, so a range check on this value refuses an out-of-range one as Python does.  Read
    /// [`Args::get`] for the exact digits (e.g. to quote them in an error).
    pub fn opt_int(&self, name: &str) -> Option<i64> {
        match self.get(name) {
            Json::Number(number) if !is_float(number) => {
                Some(number.as_i64().unwrap_or_else(|| {
                    if number.to_string().starts_with('-') {
                        i64::MIN
                    } else {
                        i64::MAX
                    }
                }))
            }
            _ => None,
        }
    }

    pub fn float(&self, name: &str) -> f64 {
        self.opt_float(name)
            .unwrap_or_else(|| panic!("{name} is not a validated float"))
    }

    pub fn opt_float(&self, name: &str) -> Option<f64> {
        self.get(name).as_f64()
    }

    pub fn bool(&self, name: &str) -> bool {
        self.opt_bool(name)
            .unwrap_or_else(|| panic!("{name} is not a validated bool"))
    }

    pub fn opt_bool(&self, name: &str) -> Option<bool> {
        self.get(name).as_bool()
    }

    pub fn strs(&self, name: &str) -> Vec<String> {
        self.opt_strs(name).unwrap_or_default()
    }

    pub fn opt_strs(&self, name: &str) -> Option<Vec<String>> {
        self.get(name).as_array().map(|items| {
            items
                .iter()
                .filter_map(Json::as_str)
                .map(str::to_owned)
                .collect()
        })
    }

    pub fn floats(&self, name: &str) -> Option<Vec<f64>> {
        self.get(name)
            .as_array()
            .map(|items| items.iter().filter_map(Json::as_f64).collect())
    }

    /// `list[list[float]]`, e.g. points `[[x, y], ...]`.
    pub fn points(&self, name: &str) -> Option<Vec<Vec<f64>>> {
        self.get(name).as_array().map(|rows| {
            rows.iter()
                .map(|row| {
                    row.as_array()
                        .map(|r| r.iter().filter_map(Json::as_f64).collect())
                })
                .map(Option::unwrap_or_default)
                .collect()
        })
    }

    pub fn dicts(&self, name: &str) -> Option<&Vec<Json>> {
        self.get(name).as_array()
    }

    pub fn dict(&self, name: &str) -> Option<&Map<String, Json>> {
        self.get(name).as_object()
    }
}

/// The Python type name pydantic reports for a JSON value.
pub fn type_name(value: &Json) -> &'static str {
    match value {
        Json::Null => "NoneType",
        Json::Bool(_) => "bool",
        Json::Number(n) if is_float(n) => "float",
        Json::Number(_) => "int",
        Json::String(_) => "str",
        Json::Array(_) => "list",
        Json::Object(_) => "dict",
    }
}

/// A JSON number Python would read as a float (it has a fraction or an exponent).
fn is_float(number: &Number) -> bool {
    let text = number.to_string();
    text.contains(['.', 'e', 'E'])
}

fn float_json(value: f64) -> Option<Json> {
    // NaN and the infinities have no JSON form here (Python would print NaN/Infinity)
    Number::from_f64(value).map(Json::Number)
}

/// pydantic's lax `bool`: booleans, 0 and 1, and the strings it accepts.
pub fn lax_bool(value: &Json) -> Option<bool> {
    match value {
        Json::Bool(flag) => Some(*flag),
        Json::Number(n) => n.as_f64().and_then(|v| {
            if v == 0.0 {
                Some(false)
            } else if v == 1.0 {
                Some(true)
            } else {
                None
            }
        }),
        Json::String(text) => match text.to_ascii_lowercase().as_str() {
            "0" | "off" | "f" | "false" | "n" | "no" => Some(false),
            "1" | "on" | "t" | "true" | "y" | "yes" => Some(true),
            _ => None,
        },
        _ => None,
    }
}

const INT_PARSING: &str = "Input should be a valid integer, unable to parse string as an integer";

/// Digits with single underscores between them, as Python's int() reads them ("1_000"; not
/// "_1", "1_" or "1__0").  Returns the digits without underscores.
fn underscored_digits(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let well_formed = !bytes.is_empty()
        && bytes.iter().enumerate().all(|(index, b)| {
            b.is_ascii_digit()
                || (*b == b'_'
                    && index > 0
                    && bytes.get(index + 1).is_some_and(u8::is_ascii_digit)
                    && bytes[index - 1].is_ascii_digit())
        });
    well_formed.then(|| text.replace('_', ""))
}

/// pydantic's lax `int`: bools, integral floats below 2**63, and strings holding an integer
/// (surrounding whitespace, a sign, leading zeros and single underscores allowed, and a fraction
/// of one or more zeros such as "5.0"; not "5.", "1e3" or "0x10").
fn lax_int(value: &Json) -> Result<Json, &'static str> {
    match value {
        Json::Bool(flag) => Ok(Json::from(i64::from(*flag))),
        // Python ints are unbounded and pydantic accepts any size: keep the exact digits
        Json::Number(n) if !is_float(n) => Ok(value.clone()),
        Json::Number(n) => match n.as_f64() {
            // pydantic converts an integral float only below 2**63 in magnitude
            Some(v) if v.fract() == 0.0 && v.abs() < 9_223_372_036_854_775_808.0 => {
                Ok(Json::from(v as i64))
            }
            Some(v) if v.fract() == 0.0 && v.is_finite() => Err(INT_PARSING),
            _ => Err("Input should be a valid integer, got a number with a fractional part"),
        },
        Json::String(text) => {
            let text = text.trim();
            let whole = match text.split_once('.') {
                Some((whole, fraction))
                    if !fraction.is_empty() && fraction.bytes().all(|b| b == b'0') =>
                {
                    whole
                }
                Some(_) => return Err(INT_PARSING),
                None => text,
            };
            let (negative, unsigned) = match whole.as_bytes().first() {
                Some(b'-') => (true, &whole[1..]),
                Some(b'+') => (false, &whole[1..]),
                _ => (false, whole),
            };
            let digits = underscored_digits(unsigned).ok_or(INT_PARSING)?;
            let digits = digits.trim_start_matches('0');
            if digits.is_empty() {
                return Ok(Json::from(0));
            }
            // Any size, exactly (serde_json keeps the digits with arbitrary_precision)
            let text = if negative {
                format!("-{digits}")
            } else {
                digits.to_owned()
            };
            serde_json::from_str::<Number>(&text)
                .map(Json::Number)
                .map_err(|_| INT_PARSING)
        }
        _ => Err("Input should be a valid integer"),
    }
}

/// The text Python's float() reads, with pydantic's underscore rule: underscores are dropped, but
/// a leading, trailing or doubled underscore is refused.
fn float_text(text: &str) -> Option<String> {
    let text = text.trim();
    let unsigned = text.strip_prefix(['-', '+']).unwrap_or(text);
    if unsigned.starts_with('_') || text.ends_with('_') || text.contains("__") {
        return None;
    }
    Some(text.replace('_', ""))
}

/// pydantic's lax `float`: bools, numbers, and strings Python's float() reads.
fn lax_float(value: &Json) -> Result<Json, &'static str> {
    let number = match value {
        Json::Bool(flag) => Some(if *flag { 1.0 } else { 0.0 }),
        Json::Number(n) => n.as_f64(),
        Json::String(text) => {
            // Python reads "nan", "inf" and "1e400" too; they have no JSON form, so float_json
            // refuses them below (a documented difference)
            match float_text(text).and_then(|clean| clean.parse::<f64>().ok()) {
                Some(v) => Some(v),
                None => {
                    return Err(
                        "Input should be a valid number, unable to parse string as a number",
                    )
                }
            }
        }
        _ => return Err("Input should be a valid number"),
    };
    number
        .and_then(float_json)
        .ok_or("Input should be a finite number (NaN and infinity are not supported here)")
}

fn lax_list(value: &Json, item: fn(&Json) -> Result<Json, &'static str>) -> Result<Json, String> {
    let Json::Array(items) = value else {
        return Err("Input should be a valid list".into());
    };
    items
        .iter()
        .enumerate()
        .map(|(index, element)| item(element).map_err(|problem| format!("{index}: {problem}")))
        .collect::<Result<Vec<_>, _>>()
        .map(Json::Array)
}

fn dict_item(value: &Json) -> Result<Json, &'static str> {
    if value.is_object() {
        Ok(value.clone())
    } else {
        Err("Input should be a valid dictionary")
    }
}

fn str_item(value: &Json) -> Result<Json, &'static str> {
    if value.is_string() {
        Ok(value.clone())
    } else {
        Err("Input should be a valid string")
    }
}

fn float_list_item(value: &Json) -> Result<Json, &'static str> {
    lax_list(value, lax_float).map_err(|_| "Input should be a valid list of numbers")
}

/// Converts one present value as pydantic converts it for `kind`.
fn convert(kind: Arg, value: &Json) -> Result<Json, String> {
    let optional = matches!(
        kind,
        Arg::OptStr
            | Arg::OptInt
            | Arg::OptFloat
            | Arg::OptBool
            | Arg::OptDict
            | Arg::OptListDict
            | Arg::OptListStr
            | Arg::OptListFloat
            | Arg::OptListListFloat
            | Arg::OptFloatOrListFloat
            | Arg::OptFloatOrDict
    );
    if value.is_null() {
        return if optional {
            Ok(Json::Null)
        } else {
            Err(match kind {
                Arg::Str => "Input should be a valid string",
                Arg::Int => "Input should be a valid integer",
                Arg::Float => "Input should be a valid number",
                Arg::Bool => "Input should be a valid boolean",
                Arg::Dict => "Input should be a valid dictionary",
                _ => "Input should be a valid list",
            }
            .into())
        };
    }
    match kind {
        Arg::Str | Arg::OptStr => str_item(value).map_err(Into::into),
        Arg::Int | Arg::OptInt => lax_int(value).map_err(Into::into),
        Arg::Float | Arg::OptFloat => lax_float(value).map_err(Into::into),
        Arg::Bool | Arg::OptBool => lax_bool(value)
            .map(Json::Bool)
            .ok_or_else(|| "Input should be a valid boolean".into()),
        Arg::Dict | Arg::OptDict => dict_item(value).map_err(Into::into),
        Arg::ListDict | Arg::OptListDict => lax_list(value, dict_item),
        Arg::ListStr | Arg::OptListStr => lax_list(value, str_item),
        Arg::OptListFloat => lax_list(value, lax_float),
        Arg::ListListFloat | Arg::OptListListFloat => lax_list(value, float_list_item),
        // pydantic's smart union: a number (or numeric text) stays a float, a list a list
        Arg::OptFloatOrListFloat => match value {
            Json::Array(_) => lax_list(value, lax_float),
            _ => lax_float(value).map_err(Into::into),
        },
        Arg::OptFloatOrDict => match value {
            Json::Object(_) => Ok(value.clone()),
            _ => lax_float(value).map_err(Into::into),
        },
    }
}

fn default_json(default: Default) -> Json {
    match default {
        Default::Required | Default::None => Json::Null,
        Default::Bool(flag) => Json::Bool(flag),
        Default::Int(number) => Json::from(number),
        Default::Float(number) => float_json(number).unwrap_or(Json::Null),
        Default::Str(text) => Json::String(text.into()),
    }
}

/// FastMCP's `pre_parse_json` and pydantic validation of the arguments model.  Returns the
/// validated arguments or pydantic-style error text.
pub fn validate(tool: &str, spec: &[Param], arguments: &Json) -> Result<Args, String> {
    let given = match arguments {
        Json::Object(map) => map.clone(),
        Json::Null => Map::new(),
        other => {
            return Err(format!(
                "1 validation error for {tool}Arguments\n  Input should be a valid dictionary [input_type={}]",
                type_name(other)
            ))
        }
    };
    let mut values = Map::new();
    let mut errors = Vec::new();
    for param in spec {
        let Some(mut value) = given.get(param.name).cloned() else {
            if param.default == Default::Required {
                errors.push(format!("{}\n  Field required [type=missing]", param.name));
            } else {
                values.insert(param.name.into(), default_json(param.default));
            }
            continue;
        };
        // Clients often send objects and lists as JSON text; FastMCP decodes it unless the
        // parameter is a plain str, and keeps the text when it decodes to a str or a number
        // (bool is a Python int, so it is kept as text too; "null" becomes None)
        if let (Json::String(text), true) = (&value, param.kind != Arg::Str) {
            if let Ok(parsed) = serde_json::from_str::<Json>(text) {
                if !matches!(parsed, Json::String(_) | Json::Number(_) | Json::Bool(_)) {
                    value = parsed;
                }
            }
        }
        match convert(param.kind, &value) {
            Ok(converted) => {
                values.insert(param.name.into(), converted);
            }
            Err(problem) => errors.push(format!(
                "{}\n  {problem} [input_type={}]",
                param.name,
                type_name(&value)
            )),
        }
    }
    if errors.is_empty() {
        Ok(Args(values))
    } else {
        let count = errors.len();
        Err(format!(
            "{count} validation error{} for {tool}Arguments\n{}",
            if count == 1 { "" } else { "s" },
            errors.join("\n")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn one(kind: Arg, value: Json) -> Result<Json, String> {
        validate("t", &[req("x", kind)], &json!({"x": value})).map(|args| args.get("x").clone())
    }

    /// The conversions pydantic 2.14 makes (probed with create_model in the MCP interpreter).
    #[test]
    fn follows_pydantic_lax_mode() {
        assert_eq!(one(Arg::Int, json!(true)), Ok(json!(1)));
        assert_eq!(one(Arg::Int, json!(1.0)), Ok(json!(1)));
        assert!(one(Arg::Int, json!(1.5)).is_err());
        assert_eq!(one(Arg::Int, json!("5")), Ok(json!(5)));
        assert_eq!(one(Arg::Int, json!("5.0")), Ok(json!(5)));
        assert_eq!(one(Arg::Int, json!(" 7 ")), Ok(json!(7)));
        assert!(one(Arg::Int, json!("1e3")).is_err());
        assert!(one(Arg::Int, json!("x")).is_err());
        assert!(one(Arg::Int, json!("5.")).is_err());
        assert!(one(Arg::Int, json!(".0")).is_err());
        assert!(one(Arg::Int, json!("0x10")).is_err());
        assert!(one(Arg::Int, json!("1 000")).is_err());
        assert_eq!(one(Arg::Int, json!("5.00")), Ok(json!(5)));
        assert_eq!(one(Arg::Int, json!("+5")), Ok(json!(5)));
        assert_eq!(one(Arg::Int, json!("-007")), Ok(json!(-7)));
        assert_eq!(one(Arg::Int, json!("00")), Ok(json!(0)));
        assert_eq!(one(Arg::Int, json!("1_0")), Ok(json!(10)));
        for refused in ["_1", "1_", "1__0", "5.0_0"] {
            assert!(one(Arg::Int, json!(refused)).is_err(), "{refused}");
        }
        assert_eq!(
            one(Arg::Int, json!(1e16)),
            Ok(json!(10_000_000_000_000_000_i64))
        );
        assert!(one(Arg::Int, json!(9.223_372_036_854_776e18)).is_err());
        // Any size, as pydantic accepts; the accessor clamps to i64
        let big = one(Arg::Int, json!("9999999999999999999999999")).unwrap();
        assert_eq!(big.to_string(), "9999999999999999999999999");
        let spec = [req("x", Arg::Int)];
        let args = validate("t", &spec, &json!({"x": "-0099999999999999999999"})).unwrap();
        assert_eq!(args.get("x").to_string(), "-99999999999999999999");
        assert_eq!(args.int("x"), i64::MIN);
        let args = validate("t", &spec, &json!({"x": 99999999999999999999_u128})).unwrap();
        assert_eq!(args.int("x"), i64::MAX);
        assert_eq!(one(Arg::Float, json!("1_000")), Ok(json!(1000.0)));
        assert_eq!(one(Arg::Float, json!("1_000.000_1")), Ok(json!(1000.0001)));
        assert_eq!(one(Arg::Float, json!("1e1_0")), Ok(json!(1e10)));
        assert_eq!(one(Arg::Float, json!("+.5")), Ok(json!(0.5)));
        assert_eq!(one(Arg::Float, json!("5.")), Ok(json!(5.0)));
        assert_eq!(one(Arg::Float, json!("  -1.5e3 ")), Ok(json!(-1500.0)));
        for refused in ["_1", "1__0"] {
            assert!(one(Arg::Float, json!(refused)).is_err(), "{refused}");
        }
        assert_eq!(
            one(Arg::OptListFloat, json!(["1_000"])),
            Ok(json!([1000.0]))
        );
        assert!(one(Arg::Int, Json::Null).is_err());
        assert_eq!(one(Arg::Float, json!(true)), Ok(json!(1.0)));
        assert_eq!(one(Arg::Float, json!(1)), Ok(json!(1.0)));
        assert_eq!(one(Arg::Float, json!("1.5")), Ok(json!(1.5)));
        assert_eq!(one(Arg::Float, json!(" 2 ")), Ok(json!(2.0)));
        assert!(one(Arg::Float, json!("x")).is_err());
        assert!(one(Arg::Float, Json::Null).is_err());
        assert_eq!(one(Arg::OptFloatOrListFloat, json!(1)), Ok(json!(1.0)));
        assert_eq!(
            one(Arg::OptFloatOrListFloat, json!([1, "2"])),
            Ok(json!([1.0, 2.0]))
        );
        assert_eq!(one(Arg::OptFloatOrListFloat, json!("1")), Ok(json!(1.0)));
        assert_eq!(
            one(Arg::OptFloatOrListFloat, json!([true])),
            Ok(json!([1.0]))
        );
        assert!(one(Arg::OptFloatOrListFloat, json!({})).is_err());
        assert_eq!(
            one(Arg::OptFloatOrDict, json!({"F.Cu": 1})),
            Ok(json!({"F.Cu": 1}))
        );
        assert!(one(Arg::OptFloatOrDict, json!([1])).is_err());
        assert_eq!(
            one(Arg::OptListFloat, json!([1, "2"])),
            Ok(json!([1.0, 2.0]))
        );
        assert!(one(Arg::OptListFloat, json!([null])).is_err());
        assert_eq!(
            one(Arg::ListListFloat, json!([[1, "2"]])),
            Ok(json!([[1.0, 2.0]]))
        );
        assert!(one(Arg::ListListFloat, json!([1])).is_err());
        assert!(one(Arg::Str, json!(1)).is_err());
        assert!(one(Arg::Str, json!(true)).is_err());
        // JSON text is decoded for any annotation but plain str
        assert_eq!(
            one(Arg::ListListFloat, json!("[[1, 2]]")),
            Ok(json!([[1.0, 2.0]]))
        );
        assert_eq!(one(Arg::OptInt, json!("null")), Ok(Json::Null));
        assert_eq!(one(Arg::Str, json!("[1]")), Ok(json!("[1]")));
    }

    #[test]
    fn defaults_fill_missing_arguments() {
        let spec = [
            opt("dpi", Arg::Int, Default::Int(100)),
            opt("frame", Arg::Bool, Default::Bool(true)),
            opt("stub_mm", Arg::Float, Default::Float(2.54)),
            opt("label", Arg::Str, Default::Str("local")),
            opt("net", Arg::OptStr, Default::None),
        ];
        let args = validate("t", &spec, &json!({})).unwrap();
        assert_eq!(args.int("dpi"), 100);
        assert!(args.bool("frame"));
        assert_eq!(args.float("stub_mm"), 2.54);
        assert_eq!(args.str("label"), "local");
        assert!(args.is_null("net"));
        let error =
            validate("t", &[req("a", Arg::Int), req("b", Arg::Str)], &json!({})).unwrap_err();
        assert!(
            error.starts_with("2 validation errors for tArguments"),
            "{error}"
        );
    }
}
