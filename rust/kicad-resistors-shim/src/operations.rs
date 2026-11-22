// SPDX-License-Identifier: GPL-3.0-or-later
//! Pure wire conversions shared with the wasm parity probe; native I/O stays in main.
use kicad_resistors::reader::DatafileParserReader;
use kicad_resistors::{eseries, str_value, testing, Calculator, Error, Resistance};
use serde_json::{json, Value};
type Result<T> = std::result::Result<T, String>;

pub(crate) fn keys(value: &Value, required: &[&str], optional: &[&str]) -> Result<()> {
    let object = value.as_object().ok_or("expected object")?;
    if required.iter().any(|key| !object.contains_key(*key)) {
        return Err("missing field".into());
    }
    if object
        .keys()
        .any(|key| !required.contains(&key.as_str()) && !optional.contains(&key.as_str()))
    {
        return Err("unknown field".into());
    }
    Ok(())
}
pub(crate) fn field<'a>(value: &'a Value, key: &str) -> Result<&'a Value> {
    if !value.is_object() {
        return Err("expected object".into());
    }
    value.get(key).ok_or_else(|| "missing field".into())
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    field(value, key)?
        .as_str()
        .ok_or_else(|| "expected string".into())
}
fn integer(value: &Value, key: &str, min: i32, max: i32) -> Result<i32> {
    let number = field(value, key)?
        .as_i64()
        .ok_or_else(|| format!("integer out of range: {key}"))?;
    if number < i64::from(min) || number > i64::from(max) {
        return Err(format!("integer out of range: {key}"));
    }
    Ok(number as i32)
}
fn array(value: &Value, limit: usize) -> Result<&[Value]> {
    let items = value
        .as_array()
        .ok_or("array bound exceeded or wrong type")?;
    if items.len() > limit {
        return Err("array bound exceeded or wrong type".into());
    }
    Ok(items)
}
fn float(value: &Value) -> Result<f64> {
    unsigned(value)
        .map(f64::from_bits)
        .ok_or_else(|| "expected u64 IEEE754 bits".into())
}

fn unsigned(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|value| u64::try_from(value).ok()))
}
fn res(value: &Resistance) -> Value {
    json!({"value_bits": value.value.to_bits(), "name": value.name, "parts_bits": value.parts.iter().map(|part| part.to_bits()).collect::<Vec<_>>()})
}
fn table(values: &[Resistance]) -> Value {
    Value::Array(values.iter().map(res).collect())
}

fn parse_res(item: &Value) -> Result<Resistance> {
    keys(
        item,
        &["value_bits", "name"],
        &["parts_bits", "clear_parts"],
    )?;
    let value = float(field(item, "value_bits")?)?;
    let name = text(item, "name")?;
    if name.len() > 4096 {
        return Err("name bound exceeded".into());
    }
    let parts = if let Some(parts) = item.get("parts_bits") {
        array(parts, 256)?
            .iter()
            .map(float)
            .collect::<Result<Vec<_>>>()?
    } else {
        Vec::new()
    };
    let mut result = Resistance::new(value, name.into(), parts);
    if let Some(clear) = item.get("clear_parts") {
        if clear.as_bool().ok_or("expected boolean")? {
            result.parts.clear();
        }
    }
    Ok(result)
}

fn sortable_parts(value: &Resistance) -> Result<()> {
    let nan = value.parts.iter().any(|value| value.is_nan());
    let number = value.parts.iter().find(|value| !value.is_nan());
    if nan
        && number.is_some_and(|first| {
            value
                .parts
                .iter()
                .any(|value| !value.is_nan() && value != first)
        })
    {
        return Err("parts violate sort precondition".into());
    }
    Ok(())
}
fn state(calc: &Calculator) -> Value {
    json!({"series":calc.series(),"target_bits":calc.target().to_bits(),"exclusions":calc.exclusions(),
        "buffer_1r_count":calc.buffer_1r().len(),"buffer_2r_count":calc.buffer_2r().len(),
        "results":calc.get_results().iter().map(|r| r.as_ref().map(res)).collect::<Vec<_>>()})
}
fn initialized(calc: &Calculator) -> Result<()> {
    let selected = calc
        .series_data()
        .get(calc.series() as usize)
        .ok_or("calculator series must be 0..4")?;
    if calc.exclusions().len() < selected.len() {
        return Err("NewCalc required before Calculate".into());
    }
    Ok(())
}
fn engine<T>(value: std::result::Result<T, Error>) -> Result<T> {
    value.map_err(|error| match error {
        Error::Domain(text) | Error::Logic(text) => text.into(),
    })
}
fn action(calc: &mut Calculator, action: &Value) -> Result<Value> {
    let command = text(action, "op")?;
    match command {
        "set_series" => {
            keys(action, &["op", "series"], &[])?;
            let series = unsigned(field(action, "series")?)
                .and_then(|value| u32::try_from(value).ok())
                .ok_or("integer out of range: series")?;
            calc.set_series(series);
        }
        "new" => {
            keys(action, &["op", "target_bits"], &[])?;
            if calc.series() > 4 {
                return Err("calculator series must be 0..4".into());
            }
            engine(calc.new_calc(float(field(action, "target_bits")?)?))?;
        }
        "exclude" => {
            keys(action, &["op", "values_bits"], &[])?;
            for value in array(field(action, "values_bits")?, 1024)? {
                engine(calc.exclude(float(value)?))?;
            }
        }
        "calculate" | "prepare" => {
            keys(action, &["op"], &[])?;
            initialized(calc)?;
            if command == "prepare" {
                engine(calc.prepare_1r())?;
                calc.prepare_2r();
            } else {
                if !calc.exclusions().contains(&false) {
                    return Err("2R buffer must be nonempty".into());
                }
                match calc.calculate() {
                    Ok(()) => (),
                    Err(Error::Logic(message)) => return Ok(json!({"exception":message})),
                    Err(Error::Domain(message)) => return Err(message.into()),
                }
            }
        }
        "solution" | "find" => {
            if calc.buffer_2r().is_empty()
                && !(command == "solution"
                    && calc.buffer_1r().is_empty()
                    && matches!(integer(action, "level", 2, 4)?, 3 | 4))
            {
                return Err("2R buffer must be nonempty".into());
            }
            if command == "solution" {
                keys(action, &["op", "level"], &[])?;
                let result = match integer(action, "level", 2, 4)? {
                    2 => calc.solution_2r(),
                    3 => calc.solution_3r(),
                    _ => calc.solution_4r(),
                };
                return match result {
                    Ok(value) => Ok(res(&value)),
                    Err(Error::Logic(message)) => Ok(json!({"exception":message})),
                    Err(Error::Domain(message)) => Err(message.into()),
                };
            }
            keys(action, &["op", "values_bits"], &[])?;
            let mut result = Vec::new();
            for value in array(field(action, "values_bits")?, 1024)? {
                let (first, second) = engine(calc.find(float(value)?))?;
                result.push(json!([res(first), res(second)]));
            }
            return Ok(Value::Array(result));
        }
        "build" => {
            keys(action, &["op", "series"], &[])?;
            return Ok(table(&Calculator::build_series(integer(
                action,
                "series",
                i32::MIN,
                i32::MAX,
            )?)));
        }
        "build_values" => {
            keys(action, &["op", "values"], &[])?;
            let values = array(field(action, "values")?, 256)?
                .iter()
                .map(|value| {
                    integer(&json!({"value":value}), "value", 0, 65535).map(|value| value as u16)
                })
                .collect::<Result<Vec<_>>>()?;
            return Ok(table(&engine(Calculator::build_series_data(&values))?));
        }
        "buffers" => {
            keys(action, &["op"], &[])?;
            return Ok(json!({"one":table(calc.buffer_1r()),"two":table(calc.buffer_2r())}));
        }
        "state" => keys(action, &["op"], &[])?,
        _ => return Err("unknown action".into()),
    }
    Ok(Value::Null)
}

pub(crate) fn run(input: &Value) -> Result<Value> {
    match text(input, "op")? {
        "unit_scales" => {
            keys(input, &["op"], &[])?;
            Ok(Value::Object(
                kicad_resistors::scales::VALUES
                    .iter()
                    .map(|(name, value)| ((*name).to_owned(), json!(value.to_bits())))
                    .collect(),
            ))
        }
        "reader" => {
            keys(input, &["op", "readers", "parsers", "actions"], &[])?;
            let mut readers = Vec::new();
            for item in array(field(input, "readers")?, 16)? {
                keys(item, &["text", "source"], &[])?;
                let contents = text(item, "text")?;
                let source = text(item, "source")?;
                if contents.len() > 4096 || source.len() > 4096 {
                    return Err("reader string bound exceeded".into());
                }
                readers.push((contents.to_owned(), source.to_owned()));
            }
            let reader = |value: &Value| -> Result<Option<&(String, String)>> {
                if value.is_null() {
                    return Ok(None);
                }
                let index = integer(&json!({"index":value}), "index", 0, 15)? as usize;
                Ok(Some(readers.get(index).ok_or("reader index out of range")?))
            };
            let mut parsers = Vec::new();
            let mut initial = Vec::new();
            for item in array(field(input, "parsers")?, 16)? {
                let handle = reader(item)?;
                initial.push(handle);
                parsers.push(DatafileParserReader::from_lexer_state(handle));
            }
            let same = |a: Option<&(String, String)>, b: Option<&(String, String)>| match (a, b) {
                (None, None) => true,
                (Some(a), Some(b)) => std::ptr::eq(a, b),
                _ => false,
            };
            let mut result = Vec::new();
            for action in array(field(input, "actions")?, 64)? {
                keys(action, &["op", "parser"], &["reader"])?;
                let index = integer(action, "parser", 0, 15)? as usize;
                let parser = parsers.get_mut(index).ok_or("parser index out of range")?;
                let command = text(action, "op")?;
                if command == "bind" {
                    parser.bind_lexer_state(reader(field(action, "reader")?)?);
                } else if command != "get" || action.get("reader").is_some() {
                    return Err("unknown reader action".into());
                }
                let found = parser.get_reader();
                let identity = if let Some(found) = found {
                    Some(
                        readers
                            .iter()
                            .position(|item| std::ptr::eq(item, found))
                            .ok_or("unregistered real reader")?,
                    )
                } else {
                    None
                };
                result.push(json!({"reader":identity,"same_as_initial":same(found,initial.get(index).copied().flatten()),"repeated":same(found,parser.get_reader())}));
            }
            Ok(Value::Array(result))
        }
        "collector" => {
            keys(input, &["op", "target_bits", "candidates"], &[])?;
            let candidates = array(field(input, "candidates")?, 256)?
                .iter()
                .map(|value| {
                    let candidate = parse_res(value)?;
                    sortable_parts(&candidate)?;
                    Ok(candidate)
                })
                .collect::<Result<Vec<_>>>()?;
            match testing::collect(float(field(input, "target_bits")?)?, &candidates) {
                Ok((best, again, callbacks)) => {
                    Ok(json!({"best":res(&best),"again":res(&again),"callbacks":callbacks}))
                }
                Err(Error::Logic(message)) => Ok(json!({"exception":message})),
                Err(Error::Domain(message)) => Err(message.into()),
            }
        }
        "helpers" => {
            keys(input, &["op", "first", "second", "text", "symbol"], &[])?;
            let first = parse_res(field(input, "first")?)?;
            let second = parse_res(field(input, "second")?)?;
            sortable_parts(&first)?;
            sortable_parts(&second)?;
            let expression = text(input, "text")?;
            let symbol = text(input, "symbol")?;
            if expression.len() > 4096 || symbol.len() != 1 {
                return Err("invalid expression probe".into());
            }
            let symbol = symbol.chars().next().ok_or("invalid expression probe")?;
            let mut combinations = Vec::new();
            for parallel in [false, true] {
                for simple in [false, true] {
                    combinations.push(res(&testing::combine(&first, &second, parallel, simple)));
                }
            }
            Ok(
                json!({"unique":[testing::unique(&first),testing::unique(&second)],
                "better":testing::better(&first,&second),"reverse_better":testing::better(&second,&first),
                "less":kicad_resistors::resistance_less(&first,&second),"less_value":kicad_resistors::resistance_less_value(&first,second.value),
                "embraced":testing::embrace(expression,symbol),"combinations":combinations}),
            )
        }
        "series" => {
            keys(input, &["op", "series", "decades"], &[])?;
            let series = integer(input, "series", i32::MIN, i32::MAX)?;
            let mut decades = Vec::new();
            for exponent in array(field(input, "decades")?, 1024)? {
                let exponent = integer(
                    &json!({"exponent":exponent}),
                    "exponent",
                    i32::MIN,
                    i32::MAX,
                )?;
                decades.push(json!({"exponent":exponent,"values_bits":eseries::in_decade(series,exponent).iter().map(|value|value.to_bits()).collect::<Vec<_>>()}));
            }
            Ok(
                json!({"values":eseries::values(series),"named_values":eseries::values(series),"decades":decades}),
            )
        }
        "format" => {
            keys(input, &["op", "values_bits"], &[])?;
            Ok(Value::Array(
                array(field(input, "values_bits")?, 4096)?
                    .iter()
                    .map(|value| float(value).map(|value| json!(str_value(value))))
                    .collect::<Result<Vec<_>>>()?,
            ))
        }
        "resistance" => {
            keys(input, &["op", "values"], &[])?;
            let mut result = Vec::new();
            for item in array(field(input, "values")?, 256)? {
                result.push(res(&parse_res(item)?));
            }
            Ok(Value::Array(result))
        }
        "calculator" => {
            keys(input, &["op", "actions"], &[])?;
            let mut calc = Calculator::default();
            let mut result = Vec::new();
            for command in array(field(input, "actions")?, 64)? {
                let returned = action(&mut calc, command)?;
                result.push(json!({"returned":returned,"state":state(&calc)}));
            }
            Ok(Value::Array(result))
        }
        _ => Err("unknown operation".into()),
    }
}
