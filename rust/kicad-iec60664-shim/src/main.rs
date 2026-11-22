// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kicad_iec60664::{
    Field, Iec60664, InsulationType, MaterialGroup, OvervoltageCategory, PollutionDegree, Waveform,
};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

const UNIT: &str = "pcb_calculator.calculator_panels.application.pure.p001";

fn keys(value: &Value, required: &[&str], optional: &[&str]) -> Result<(), String> {
    let object = value.as_object().ok_or("expected object")?;
    if required.iter().any(|key| !object.contains_key(*key)) {
        return Err("missing field".into());
    }
    if object
        .keys()
        .any(|key| !required.contains(&key.as_str()) && !optional.contains(&key.as_str()))
    {
        return Err("unexpected field".into());
    }
    Ok(())
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("expected string: {key}"))
}

fn integer(value: &Value, key: &str) -> Result<i32, String> {
    let n = value
        .get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("integer out of range: {key}"))?;
    i32::try_from(n).map_err(|_| format!("integer out of range: {key}"))
}

fn boolean(value: &Value, key: &str) -> Result<bool, String> {
    value
        .get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("expected boolean: {key}"))
}

fn number(value: &Value, key: &str) -> Result<f64, String> {
    let bits = value
        .get(key)
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_i64().and_then(|number| u64::try_from(number).ok()))
        })
        .ok_or("expected u64 bits")?;
    let n = f64::from_bits(bits);
    if !n.is_finite() {
        return Err("expected finite bits".into());
    }
    Ok(n)
}

fn snapshot(calc: &Iec60664) -> Value {
    let s = calc.state();
    json!({
        "rated_voltage": s.rated_voltage.to_bits(), "frequency": s.frequency.to_bits(),
        "coated": s.coated, "pcb_material": s.pcb_material,
        "rms_voltage": s.rms_voltage.to_bits(), "peak_voltage": s.peak_voltage.to_bits(),
        "transient_voltage": s.transient_voltage.to_bits(), "altitude": s.altitude.to_bits(),
        "clearance": calc.clearance_distance().to_bits(), "creepage": calc.creepage_distance().to_bits(),
        "min_groove_width": calc.min_groove_width().to_bits(), "field": s.field.0,
        "overvoltage_category": s.overvoltage_category.0, "material_group": s.material_group.0,
        "pollution_degree": s.pollution_degree.0, "insulation_type": s.insulation_type.0,
    })
}

fn step(calc: &mut Iec60664, s: &Value) -> Result<Value, String> {
    let result = match text(s, "op")? {
        "categories" => {
            keys(s, &["op"], &[])?;
            json!({
                "pollution": [PollutionDegree::PD1.0, PollutionDegree::PD2.0, PollutionDegree::PD3.0, PollutionDegree::PD4.0],
                "overvoltage": [OvervoltageCategory::I.0, OvervoltageCategory::II.0, OvervoltageCategory::III.0, OvervoltageCategory::IV.0],
                "material": [MaterialGroup::I.0, MaterialGroup::II.0, MaterialGroup::IIIA.0, MaterialGroup::IIIB.0, MaterialGroup::NOT_INSULATING.0],
                "insulation": [InsulationType::FUNCTIONAL.0, InsulationType::BASIC.0, InsulationType::REINFORCED.0],
                "field": [Field::INHOMOGENEOUS.0, Field::HOMOGENEOUS.0],
                "waveform": [Waveform::AC_50_OR_60HZ.0, Waveform::IMPULSE_1_2_50.0],
            })
        }
        "state" => {
            keys(s, &["op"], &[])?;
            Value::Null
        }
        "compute" => {
            keys(s, &["op"], &[])?;
            json!(calc.compute())
        }
        "impulse" => {
            keys(s, &["op"], &[])?;
            json!(calc.rated_impulse_withstand_voltage().to_bits())
        }
        "set" => {
            keys(s, &["op", "field", "value"], &[])?;
            match text(s, "field")? {
                "altitude" => calc.set_altitude(number(s, "value")?),
                "frequency" => calc.set_frequency(number(s, "value")?),
                "rated_voltage" => calc.set_rated_voltage(number(s, "value")?),
                "rms_voltage" => calc.set_rms_voltage(number(s, "value")?),
                "peak_voltage" => calc.set_peak_voltage(number(s, "value")?),
                "transient_voltage" => calc.set_transient_voltage(number(s, "value")?),
                "pollution_degree" => {
                    calc.set_pollution_degree(PollutionDegree(integer(s, "value")?))
                }
                "material_group" => calc.set_material_group(MaterialGroup(integer(s, "value")?)),
                "field" => calc.set_field(Field(integer(s, "value")?)),
                "insulation_type" => calc.set_insulation_type(InsulationType(integer(s, "value")?)),
                "overvoltage_category" => {
                    calc.set_overvoltage_category(OvervoltageCategory(integer(s, "value")?))
                }
                "pcb_material" => calc.set_pcb_material(boolean(s, "value")?),
                _ => return Err("unknown setter".into()),
            }
            Value::Null
        }
        "groove" => {
            keys(s, &["op", "pd"], &["distance"])?;
            let distance = s
                .get("distance")
                .map(|_| number(s, "distance"))
                .transpose()?;
            json!(calc
                .groove_width(PollutionDegree(integer(s, "pd")?), distance)
                .to_bits())
        }
        "altitude" => {
            keys(s, &["op", "voltage"], &[])?;
            json!(calc
                .altitude_correction_factor(number(s, "voltage")?)
                .to_bits())
        }
        "rms" => {
            keys(s, &["op", "voltage"], &["field"])?;
            let field = s
                .get("field")
                .map(|_| integer(s, "field").map(Field))
                .transpose()?;
            json!(calc
                .clearance_for_rms_voltage(number(s, "voltage")?, field)
                .to_bits())
        }
        "cti" => {
            keys(s, &["op", "voltage"], &[])?;
            json!(calc.material_group_from_cti(number(s, "voltage")?).0)
        }
        "transient" => {
            keys(s, &["op", "voltage", "pd", "field"], &[])?;
            json!(calc
                .clearance_to_withstand_transient_voltage(
                    number(s, "voltage")?,
                    PollutionDegree(integer(s, "pd")?),
                    Field(integer(s, "field")?)
                )
                .to_bits())
        }
        "peaks" => {
            keys(s, &["op", "voltage", "field"], &[])?;
            json!(calc
                .clearance_to_withstand_peaks(number(s, "voltage")?, Field(integer(s, "field")?))
                .to_bits())
        }
        "basic_creepage" => {
            keys(s, &["op", "voltage", "pd", "mg"], &[])?;
            json!(calc
                .basic_creepage_distance(
                    number(s, "voltage")?,
                    PollutionDegree(integer(s, "pd")?),
                    MaterialGroup(integer(s, "mg")?)
                )
                .to_bits())
        }
        "clearance" => {
            keys(s, &["op", "pd", "field", "altitude"], &[])?;
            json!(calc
                .compute_clearance_distance(
                    PollutionDegree(integer(s, "pd")?),
                    Field(integer(s, "field")?),
                    number(s, "altitude")?
                )
                .to_bits())
        }
        "creepage" => {
            keys(s, &["op", "pd", "mg"], &[])?;
            json!(calc
                .compute_creepage_distance(
                    PollutionDegree(integer(s, "pd")?),
                    MaterialGroup(integer(s, "mg")?)
                )
                .to_bits())
        }
        _ => return Err("unknown operation".into()),
    };
    Ok(json!({"value":result, "state":snapshot(calc)}))
}

fn calculate(input: &Value) -> Result<Value, String> {
    keys(input, &["steps"], &[])?;
    let steps = input
        .get("steps")
        .and_then(Value::as_array)
        .ok_or("expected 1..256 steps")?;
    if steps.is_empty() || steps.len() > 256 {
        return Err("expected 1..256 steps".into());
    }
    let mut calc = Iec60664::new();
    steps
        .iter()
        .map(|s| step(&mut calc, s))
        .collect::<Result<Vec<_>, _>>()
        .map(Value::Array)
}

fn dispatch(line: &str) -> Value {
    let mut unit = Value::Null;
    let mut case = Value::Null;
    let parsed = kicad_difftest::request::parse(line);
    let reply = parsed
        .map_err(|_| ("invalid_request", "invalid JSON or duplicate key".into()))
        .and_then(|request| {
            if let Some(v) = request.get("unit").filter(|v| v.is_string()) {
                unit = v.clone();
            }
            if let Some(v) = request.get("case").filter(|v| v.is_string()) {
                case = v.clone();
            }
            if keys(&request, &["unit", "case", "input"], &[]).is_err()
                || unit.as_str().is_none_or(str::is_empty)
                || case.as_str().is_none_or(str::is_empty)
                || !request.get("input").is_some_and(Value::is_object)
            {
                return Err((
                    "invalid_request",
                    "expected unit, case, input object".into(),
                ));
            }
            if unit.as_str() != Some(UNIT) {
                return Err(("unknown_unit", "unit has no registered shim".into()));
            }
            calculate(&request["input"]).map_err(|e| ("invalid_input", e))
        });
    match reply {
        Ok(result) => json!({"unit":unit,"case":case,"result":result}),
        Err((kind, message)) => {
            json!({"unit":unit,"case":case,"error":{"kind":kind,"message":message}})
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    for line in io::stdin().lock().lines() {
        serde_json::to_writer(&mut out, &dispatch(&line?))?;
        writeln!(out)?;
        out.flush()?;
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::{dispatch, UNIT};
    use serde_json::json;

    #[test]
    fn negative_zero_integer_bits_and_full_unsigned_domain_are_preserved() {
        let wire = format!(
            r#"{{"unit":"{UNIT}","case":"zero","input":{{"steps":[{{"op":"set","field":"altitude","value":-0}}]}}}}"#
        );
        let reply = dispatch(&wire);
        assert_eq!(reply, dispatch(&wire.replace(":-0", ":0")));
        assert_eq!(reply["result"][0]["state"]["altitude"], 0);
        let signed_zero = dispatch(&wire.replace(":-0", ":9223372036854775808"));
        assert_eq!(
            signed_zero["result"][0]["state"]["altitude"],
            json!(1_u64 << 63)
        );
    }

    #[test]
    fn invalid_identity_and_field_messages_preserve_complete_envelopes() {
        let reply = dispatch(
            &json!({"unit":UNIT,"case":"","input":{"steps":[{"op":"state"}]}}).to_string(),
        );
        assert_eq!(
            reply,
            json!({"unit":UNIT,"case":"","error":{
            "kind":"invalid_request","message":"expected unit, case, input object"}})
        );
        let reply = dispatch(
            &json!({"unit":UNIT,"case":"extra","input":{"steps":[{"op":"state","extra":0}]}})
                .to_string(),
        );
        assert_eq!(
            reply,
            json!({"unit":UNIT,"case":"extra","error":{
            "kind":"invalid_input","message":"unexpected field"}})
        );
        assert_eq!(
            dispatch("{")["error"]["message"],
            "invalid JSON or duplicate key"
        );
    }

    #[test]
    fn all_refusal_envelopes_match_the_shared_oracle_contract(
    ) -> Result<(), Box<dyn std::error::Error>> {
        for line in include_str!("../../../port/oracle/iec60664-input-guards.jsonl").lines() {
            let fixture: serde_json::Value = serde_json::from_str(line)?;
            let wire = fixture["wire"].as_str().ok_or("missing fixture wire")?;
            assert_eq!(dispatch(wire), fixture["expected"], "request: {wire}");
        }
        Ok(())
    }
}
