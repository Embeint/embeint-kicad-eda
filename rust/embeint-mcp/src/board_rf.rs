// SPDX-License-Identifier: GPL-3.0-or-later
//! Controlled-impedance track widths from the board stackup: a port of
//! `tools/live_schematic_mcp/board_rf.py`.
//!
//! Microstrip: Hammerstad and Jensen (1980) with their strip-thickness correction.  Grounded
//! coplanar waveguide (CPWG): the conformal-mapping formula with a ground plane below, with
//! Hilberg's approximation of K(k)/K'(k).  Units are millimetres.
//!
//! The arithmetic is Python's, operation for operation (`**` is `pow`, `math.log` is `ln`), so the
//! numbers are the same; the errors Python raises for impossible inputs (a negative square root, a
//! division by zero, a complex power) are raised here with the same text.

use embeint_ipc::client::Error;
use serde_json::Value as Json;

use crate::board_design::{py_fixed, py_float, py_list_str};

const ETA0: f64 = 376.730313668;

fn domain_error() -> Error {
    Error::Value("math domain error".into())
}

/// `math.sqrt(x)`.
fn sqrt(x: f64) -> Result<f64, Error> {
    if x < 0.0 {
        Err(domain_error())
    } else {
        Ok(x.sqrt())
    }
}

/// `math.log(x)`.
fn ln(x: f64) -> Result<f64, Error> {
    if x <= 0.0 {
        Err(domain_error())
    } else {
        Ok(x.ln())
    }
}

/// `a / b` for floats (ZeroDivisionError).
fn div(a: f64, b: f64) -> Result<f64, Error> {
    if b == 0.0 {
        Err(Error::Value("float division by zero".into()))
    } else {
        Ok(a / b)
    }
}

/// `base ** exponent` for floats: a negative base with a fractional exponent is a complex number
/// in Python, which the next `math.sqrt` refuses; that refusal is raised here.
fn pow(base: f64, exponent: f64) -> Result<f64, Error> {
    if base < 0.0 && exponent.fract() != 0.0 {
        return Err(Error::Value("must be real number, not complex".into()));
    }
    Ok(base.powf(exponent))
}

/// `math.cosh(x)` (OverflowError beyond the float range).
fn cosh(x: f64) -> Result<f64, Error> {
    let value = x.cosh();
    if value.is_infinite() && x.is_finite() {
        return Err(Error::Value("math range error".into()));
    }
    Ok(value)
}

/// `math.exp(x)` (OverflowError beyond the float range).
fn exp(x: f64) -> Result<f64, Error> {
    let value = x.exp();
    if value.is_infinite() && x.is_finite() {
        return Err(Error::Value("math range error".into()));
    }
    Ok(value)
}

/// Python's `max(a, b)`: the first argument unless the second is larger (NaN stays NaN).
fn py_max(a: f64, b: f64) -> f64 {
    if b > a {
        b
    } else {
        a
    }
}

/// Python's `min(a, b)`.
fn py_min(a: f64, b: f64) -> f64 {
    if b < a {
        b
    } else {
        a
    }
}

/// K(k) / K'(k), by Hilberg's approximation, which stays finite as k approaches 1.
fn k_ratio(k: f64) -> Result<f64, Error> {
    let k = py_min(py_max(k, 1e-12), 1.0 - 1e-15);
    let pi = std::f64::consts::PI;
    if k <= 0.7 {
        let kp = sqrt(1.0 - k * k)?;
        let s = sqrt(kp)?;
        return div(pi, ln(div(2.0 * (1.0 + s), 1.0 - s)?)?);
    }
    let s = sqrt(k)?;
    Ok(ln(div(2.0 * (1.0 + s), 1.0 - s)?)? / pi)
}

fn z01(u: f64) -> Result<f64, Error> {
    let pi = std::f64::consts::PI;
    let f = 6.0 + (2.0 * pi - 6.0) * exp(-pow(div(30.666, u)?, 0.7528)?)?;
    Ok(ETA0 / (2.0 * pi) * ln(div(f, u)? + sqrt(1.0 + pow(div(2.0, u)?, 2.0)?)?)?)
}

fn eeff(u: f64, er: f64) -> Result<f64, Error> {
    let u4 = pow(u, 4.0)?;
    let a = 1.0
        + ln(div(u4 + pow(u / 52.0, 2.0)?, u4 + 0.432)?)? / 49.0
        + ln(1.0 + pow(u / 18.1, 3.0)?)? / 18.7;
    let b = 0.564 * pow(div(er - 0.9, er + 3.0)?, 0.053)?;
    Ok((er + 1.0) / 2.0 + (er - 1.0) / 2.0 * pow(1.0 + div(10.0, u)?, -a * b)?)
}

/// `microstrip(width, height, er, thickness)`: (Z0 ohm, effective permittivity) of a surface
/// microstrip.
pub(crate) fn microstrip(
    width: f64,
    height: f64,
    er: f64,
    thickness: f64,
) -> Result<(f64, f64), Error> {
    let pi = std::f64::consts::PI;
    let u = div(width, height)?;
    let (u1, ur) = if thickness > 0.0 {
        let t = div(thickness, height)?;
        let coth = div(1.0, sqrt(6.517 * u)?.tanh())?;
        let du1 = t / pi * ln(1.0 + div(4.0 * std::f64::consts::E, t * pow(coth, 2.0)?)?)?;
        let dur = 0.5 * (1.0 + div(1.0, cosh(sqrt(er - 1.0)?)?)?) * du1;
        (u + du1, u + dur)
    } else {
        (u, u)
    };
    let effective = eeff(ur, er)? * pow(div(z01(u1)?, z01(ur)?)?, 2.0)?;
    Ok((div(z01(ur)?, sqrt(eeff(ur, er)?)?)?, effective))
}

/// `cpwg(width, gap, height, er)`: (Z0 ohm, effective permittivity) of a grounded coplanar
/// waveguide (ground both sides and below).
pub(crate) fn cpwg(width: f64, gap: f64, height: f64, er: f64) -> Result<(f64, f64), Error> {
    let pi = std::f64::consts::PI;
    let k = div(width, width + 2.0 * gap)?;
    let k1 = div(
        (pi * width / (4.0 * height)).tanh(),
        (pi * (width + 2.0 * gap) / (4.0 * height)).tanh(),
    )?;
    let (r, r1) = (k_ratio(k)?, k_ratio(k1)?);
    // `(1 + er * r1 / r) / (1 + r1 / r)`: er * r1 first, as Python evaluates it
    let effective = div(1.0 + div(er * r1, r)?, 1.0 + div(r1, r)?)?;
    Ok((div(div(60.0 * pi, sqrt(effective)?)?, r + r1)?, effective))
}

/// The impedance of a track of width `w` for `width_for`.
fn impedance_of(
    w: f64,
    height: f64,
    er: f64,
    thickness: f64,
    structure: &str,
    gap: f64,
) -> Result<f64, Error> {
    if structure == "microstrip" {
        Ok(microstrip(w, height, er, thickness)?.0)
    } else {
        Ok(cpwg(w, gap, height, er)?.0)
    }
}

/// `width_for(impedance, height, er, thickness, structure, gap)`: the track width giving
/// `impedance` ohm (bisection; impedance falls as the track widens).
pub(crate) fn width_for(
    impedance: f64,
    height: f64,
    er: f64,
    thickness: f64,
    structure: &str,
    gap: Option<f64>,
) -> Result<f64, Error> {
    if structure != "microstrip" && structure != "cpwg" {
        return Err(Error::Value("structure must be microstrip or cpwg".into()));
    }
    let gap_value = gap.unwrap_or(0.0);
    if structure == "cpwg" && gap_value == 0.0 {
        return Err(Error::Value(
            "cpwg needs the gap to the side grounds (gap_mm)".into(),
        ));
    }
    let z = |w: f64| impedance_of(w, height, er, thickness, structure, gap_value);
    let (mut lo, mut hi) = (height * 0.01, height * 40.0);
    // `not z(hi) <= impedance <= z(lo)`: Python evaluates z(lo) only when the first comparison holds,
    // and both again for the message
    let reachable = z(hi)? <= impedance && impedance <= z(lo)?;
    if !reachable {
        let (high, low) = (z(hi)?, z(lo)?);
        return Err(Error::Value(format!(
            "{} ohm is out of reach on this dielectric ({} to {} ohm)",
            py_float(impedance),
            py_fixed(high, 1),
            py_fixed(low, 1)
        )));
    }
    for _ in 0..100 {
        let mid = sqrt(lo * hi)?;
        if z(mid)? > impedance {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Ok((lo + hi) / 2.0)
}

/// The dielectric between a signal layer and its reference (`dielectric_under`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Dielectric {
    pub height: f64,
    pub epsilon_r: f64,
    pub copper_thickness: f64,
    pub reference_layer: String,
}

fn number(value: &Json) -> f64 {
    value.as_f64().unwrap_or(0.0)
}

/// Python's truthiness of a stackup number (`value or default`).
fn or_number(value: Option<&Json>, default: f64) -> f64 {
    match value.and_then(Json::as_f64) {
        Some(v) if v != 0.0 => v,
        _ => default,
    }
}

/// `dielectric_under(stackup_layers, signal_layer, reference_layer)`: height, epsilon_r
/// (thickness-weighted) and copper thickness between a signal layer and its reference, from the
/// layers `board_stackup_layers` reports.  The reference defaults to the nearest copper layer below
/// (above, for the bottom layer).
pub(crate) fn dielectric_under(
    layers: &[Json],
    signal_layer: &str,
    reference_layer: Option<&str>,
) -> Result<Dielectric, Error> {
    let copper: Vec<usize> = (0..layers.len())
        .filter(|&i| layers[i]["type"] == "copper")
        .collect();
    let names: Vec<String> = copper
        .iter()
        .map(|&i| layers[i]["layer"].as_str().unwrap_or_default().to_owned())
        .collect();
    let Some(position) = names.iter().position(|n| n == signal_layer) else {
        return Err(Error::Value(format!(
            "{signal_layer} is not a copper layer of this stackup ({})",
            py_list_str(&names)
        )));
    };
    let s = copper[position];
    let r = match reference_layer {
        None => match copper.iter().find(|&&i| i > s) {
            Some(&below) => below,
            None => *copper
                .iter()
                .filter(|&&i| i < s)
                .max()
                .ok_or_else(|| Error::Value("max() iterable argument is empty".into()))?,
        },
        Some(reference) => match names.iter().position(|n| n == reference) {
            Some(index) => copper[index],
            None => {
                return Err(Error::Value(format!(
                    "{reference} is not a copper layer of this stackup"
                )))
            }
        },
    };
    let (lo, hi) = if s <= r { (s, r) } else { (r, s) };
    let (mut height, mut weighted) = (0.0_f64, 0.0_f64);
    for layer in layers.iter().take(hi).skip(lo + 1) {
        if layer["type"] != "dielectric" {
            continue;
        }
        let fallback = vec![serde_json::json!({
            "thickness": layer["thickness"],
            "epsilon_r": layer.get("epsilon_r").cloned().unwrap_or(serde_json::json!(4.5)),
        })];
        let sublayers = layer
            .get("sublayers")
            .and_then(Json::as_array)
            .filter(|s| !s.is_empty())
            .unwrap_or(&fallback);
        for sub in sublayers {
            let thickness = number(&sub["thickness"]);
            height += thickness;
            weighted += thickness * or_number(sub.get("epsilon_r"), 4.5);
        }
    }
    if height <= 0.0 {
        return Err(Error::Value(
            "no dielectric between the signal and reference layers".into(),
        ));
    }
    Ok(Dielectric {
        height,
        epsilon_r: weighted / height,
        copper_thickness: or_number(layers[s].get("thickness"), 0.035),
        reference_layer: layers[r]["layer"].as_str().unwrap_or_default().to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Values from board_rf.py in the MCP interpreter.
    #[test]
    fn follows_python() {
        let (z, e) = microstrip(2.9, 1.51, 4.5, 0.035).unwrap();
        assert!((z - 50.0).abs() < 2.0, "{z}");
        assert!(e > 3.0 && e < 4.5);
        let (z, _) = cpwg(1.0, 0.2, 1.51, 4.5).unwrap();
        assert!(z > 30.0 && z < 80.0, "{z}");
        assert_eq!(
            width_for(50.0, 1.0, 4.5, 0.035, "stripline", None)
                .unwrap_err()
                .to_string(),
            "structure must be microstrip or cpwg"
        );
        assert_eq!(
            width_for(50.0, 1.0, 4.5, 0.035, "cpwg", Some(0.0))
                .unwrap_err()
                .to_string(),
            "cpwg needs the gap to the side grounds (gap_mm)"
        );
        assert!(width_for(1.0, 1.0, 4.5, 0.035, "microstrip", None)
            .unwrap_err()
            .to_string()
            .starts_with("1.0 ohm is out of reach on this dielectric ("));
        assert_eq!(microstrip(1.0, 1.0, 0.5, 0.035), Err(domain_error()));
        let layers = vec![
            json!({"type": "copper", "layer": "F.Cu", "thickness": 0.035}),
            json!({"type": "dielectric", "thickness": 1.51,
                   "sublayers": [{"thickness": 1.0, "epsilon_r": 4.0},
                                 {"thickness": 0.51, "epsilon_r": 0.0}]}),
            json!({"type": "copper", "layer": "B.Cu", "thickness": 0.0}),
        ];
        let d = dielectric_under(&layers, "B.Cu", None).unwrap();
        assert_eq!(d.reference_layer, "F.Cu");
        assert_eq!(d.copper_thickness, 0.035);
        assert_eq!(d.height, 1.51);
        assert_eq!(
            dielectric_under(&layers, "In1.Cu", None)
                .unwrap_err()
                .to_string(),
            "In1.Cu is not a copper layer of this stackup (['F.Cu', 'B.Cu'])"
        );
        assert_eq!(
            dielectric_under(&layers, "F.Cu", Some("F.Cu"))
                .unwrap_err()
                .to_string(),
            "no dielectric between the signal and reference layers"
        );
        assert_eq!(
            dielectric_under(&layers[..1], "F.Cu", None)
                .unwrap_err()
                .to_string(),
            "max() iterable argument is empty"
        );
    }
}
