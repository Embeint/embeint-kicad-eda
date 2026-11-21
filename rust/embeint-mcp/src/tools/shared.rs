// SPDX-License-Identifier: GPL-3.0-or-later
//! The bundled skills (`live_skill_reference`; the kicad-skill:// resources are in
//! [`crate::resources`]) and `live_editor_grid`, which every editor answers.
//!
//! See [`super`] for the module contract.

use embeint_ipc::client::{self, Error};
use embeint_ipc::proto::{build, enum_name, int_field, list_field, message_field, with_field};
use prost_reflect::{DynamicMessage, Value};
use serde_json::{json, Value as Json};

use super::args::{opt, req, Arg, Args, Default, Param};
use super::{Ctx, Output};
use crate::resources::skills;

const C: &str = "kiapi.common.commands.";

/// board_client.LIMIT_MM: the largest coordinate or size in millimetres.
const LIMIT_MM: f64 = 10000.0;

/// board_client.GRID_STYLES and GRID_SNAPPING, in their order (the error lists them so).
const GRID_STYLES: [(&str, &str); 3] = [
    ("dots", "GST_DOTS"),
    ("lines", "GST_LINES"),
    ("small_cross", "GST_SMALL_CROSS"),
];
const GRID_SNAPPING: [(&str, &str); 3] = [
    ("always", "GSN_ALWAYS"),
    ("with_grid", "GSN_WITH_GRID"),
    ("never", "GSN_NEVER"),
];

pub fn params(tool: &str) -> Option<&'static [Param]> {
    Some(match tool {
        "live_skill_reference" => sig![
            opt("skill", Arg::Str, Default::Str("")),
            opt("file", Arg::Str, Default::Str("SKILL.md")),
        ],
        "live_editor_grid" => sig![
            req("document_spec", Arg::Dict),
            opt("size_mm", Arg::OptFloatOrListFloat, Default::None),
            opt("preset_index", Arg::OptInt, Default::None),
            opt("visible", Arg::OptBool, Default::None),
            opt("style", Arg::OptStr, Default::None),
            opt("snapping", Arg::OptStr, Default::None),
        ],
        _ => return None,
    })
}

pub async fn run(name: &str, args: &Args, ctx: &Ctx<'_>) -> Option<Result<Output, Error>> {
    params(name)?;
    Some(match name {
        "live_skill_reference" => {
            skills::read_skill(args.str("skill"), args.str("file")).map(Output::Dict)
        }
        _ => editor_grid(args, ctx).await.map(Output::Dict),
    })
}

/// `_nm(value)`: millimetres to nanometres, refusing what KiCad cannot hold.
fn nm(value: f64) -> Result<i64, Error> {
    if !value.is_finite() || value.abs() > LIMIT_MM {
        return Err(Error::Value(
            "Coordinates and sizes must be finite millimetres within +/-10000".into(),
        ));
    }
    // Python's round() rounds half to even
    Ok((value * 1_000_000.0).round_ties_even() as i64)
}

/// Python's `round(value, 6)`: correctly rounded to six decimals.
fn round6(value: f64) -> f64 {
    format!("{value:.6}").parse().unwrap_or(value)
}

/// `_mm(vector)`: [x, y] in millimetres, rounded to the nanometre.
fn mm(vector: &DynamicMessage) -> Json {
    let axis = |name| round6(int_field(vector, name) as f64 / 1e6);
    json!([axis("x_nm"), axis("y_nm")])
}

/// `GridStyle.Name(value).removeprefix("GST_").lower()`; protobuf's Name raises for a value the
/// enum does not define.
fn grid_enum(enum_name_: &str, prefix: &str, number: i64) -> Result<String, Error> {
    let full = format!("{C}{enum_name_}");
    let name = enum_name(&full, number);
    if name == number.to_string() {
        return Err(Error::Value(format!(
            "Enum {enum_name_} has no name defined for value {number}"
        )));
    }
    Ok(name.strip_prefix(prefix).unwrap_or(&name).to_lowercase())
}

/// board_client.grid_dict(settings).
fn grid_dict(settings: &DynamicMessage) -> Result<Json, Error> {
    let presets: Vec<Json> = list_field(settings, "presets")
        .iter()
        .filter_map(|preset| match preset {
            Value::Message(preset) => Some(mm(&message_field(preset, "size"))),
            _ => None,
        })
        .collect();
    let visible = settings
        .get_field_by_name("visible")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    Ok(json!({
        "size_mm": mm(&message_field(settings, "size")),
        "preset_index": int_field(settings, "preset_index"),
        "presets_mm": presets,
        "visible": visible,
        "style": grid_enum("GridStyle", "GST_", int_field(settings, "style"))?,
        "snapping": grid_enum("GridSnapping", "GSN_", int_field(settings, "snapping"))?,
        "origin_mm": mm(&message_field(settings, "origin")),
    }))
}

/// One of a fixed set of names, as `GRID_STYLES[style]` after the membership check.
fn choice(
    given: &str,
    choices: &[(&str, &str)],
    enum_name_: &str,
    what: &str,
) -> Result<Value, Error> {
    let Some((_, value)) = choices.iter().find(|(name, _)| *name == given) else {
        let names: Vec<String> = choices
            .iter()
            .map(|(name, _)| format!("'{name}'"))
            .collect();
        return Err(Error::Value(format!(
            "{what} must be one of [{}]",
            names.join(", ")
        )));
    };
    let number = embeint_ipc::proto::pool()
        .get_enum_by_name(&format!("{C}{enum_name_}"))
        .and_then(|e| e.get_value_by_name(value))
        .map_or(0, |v| v.number());
    Ok(Value::EnumNumber(number))
}

/// server.py's live_editor_grid: the document by its type, then BoardTools.editor_grid.
async fn editor_grid(args: &Args, ctx: &Ctx<'_>) -> Result<Json, Error> {
    let doc = client::editor_document(args.get("document_spec"))?;
    let changes = ["size_mm", "preset_index", "visible", "style", "snapping"];
    if changes.iter().all(|name| args.is_null(name)) {
        let request = with_field(
            build(&format!("{C}GetGridSettings"), json!({})),
            "document",
            doc,
        );
        let settings = ctx
            .client
            .lock()
            .await
            .send(&request, &format!("{C}GridSettings"))
            .await?;
        return grid_dict(&settings);
    }
    // The request is filled in Python's order, so the first invalid setting is the one reported
    let mut request = with_field(
        build(&format!("{C}SetGridSettings"), json!({})),
        "document",
        doc,
    );
    match args.get("size_mm") {
        Json::Null => {}
        size => {
            let (sx, sy) = match size {
                Json::Array(items) => match items.as_slice() {
                    [x, y] => (x.as_f64().unwrap_or(0.0), y.as_f64().unwrap_or(0.0)),
                    // `sx, sy = size_mm` (Python 3.12's wording)
                    [_, _, ..] => {
                        return Err(Error::Value(
                            "too many values to unpack (expected 2)".into(),
                        ))
                    }
                    short => {
                        return Err(Error::Value(format!(
                            "not enough values to unpack (expected 2, got {})",
                            short.len()
                        )))
                    }
                },
                number => {
                    let value = number.as_f64().unwrap_or(0.0);
                    (value, value)
                }
            };
            if sx <= 0.0 || sy <= 0.0 {
                return Err(Error::Value("grid size must be positive".into()));
            }
            let size = build(
                "kiapi.common.types.Vector2",
                json!({"x_nm": nm(sx)?, "y_nm": nm(sy)?}),
            );
            request = with_field(request, "size", size);
        }
    }
    match args.get("preset_index") {
        Json::Null => {}
        index => {
            // An int32 field: protobuf refuses a value beyond its range
            let number = index
                .as_i64()
                .and_then(|value| i32::try_from(value).ok())
                .ok_or_else(|| Error::Value(format!("Value out of range: {index}")))?;
            request.set_field_by_name("preset_index", Value::I32(number));
        }
    }
    if let Some(visible) = args.opt_bool("visible") {
        request.set_field_by_name("visible", Value::Bool(visible));
    }
    if let Some(style) = args.opt_str("style") {
        let value = choice(&style, &GRID_STYLES, "GridStyle", "style")?;
        request.set_field_by_name("style", value);
    }
    if let Some(snapping) = args.opt_str("snapping") {
        let value = choice(&snapping, &GRID_SNAPPING, "GridSnapping", "snapping")?;
        request.set_field_by_name("snapping", value);
    }
    let settings = ctx
        .client
        .lock()
        .await
        .send(&request, &format!("{C}GridSettings"))
        .await?;
    grid_dict(&settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_as_python() {
        assert_eq!(nm(1.27).unwrap(), 1_270_000);
        assert!(nm(10000.5).is_err());
        assert_eq!(round6(1_270_000.0 / 1e6), 1.27);
        assert_eq!(round6(-1.0 / 1e6), -0.000001);
    }

    #[test]
    fn grid_names() {
        assert_eq!(grid_enum("GridStyle", "GST_", 3).unwrap(), "small_cross");
        assert_eq!(grid_enum("GridSnapping", "GSN_", 0).unwrap(), "unknown");
        assert_eq!(
            grid_enum("GridStyle", "GST_", 9).unwrap_err(),
            Error::Value("Enum GridStyle has no name defined for value 9".into())
        );
        assert_eq!(
            choice("grid", &GRID_STYLES, "GridStyle", "style").unwrap_err(),
            Error::Value("style must be one of ['dots', 'lines', 'small_cross']".into())
        );
    }
}
