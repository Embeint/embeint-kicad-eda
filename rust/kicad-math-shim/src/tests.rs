// SPDX-License-Identifier: GPL-3.0-or-later
use super::{dispatch, run, UNIT};
use serde_json::{json, Value};

const ONE: &str = "3ff0000000000000";
const ZERO: &str = "0000000000000000";
const TWO: &str = "4000000000000000";

fn request(input: Value) -> String {
    json!({"unit":UNIT,"case":"t","input":input}).to_string()
}

fn identity() -> Value {
    json!([[ONE, ZERO, ZERO], [ZERO, ONE, ZERO], [ZERO, ZERO, ONE]])
}

fn refused(input: Value) -> String {
    let reply = dispatch(&request(input));
    assert_eq!(reply["error"]["kind"], "invalid_input", "{reply}");
    reply["error"]["message"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

#[test]
fn malformed_protocol_and_unknown_unit_fail() {
    for text in ["", "{}", "{\"unit\":\"x\",\"unit\":\"y\"}", "[]", "null"] {
        assert_eq!(dispatch(text)["error"]["kind"], "invalid_request");
    }
    assert_eq!(
        dispatch(&json!({"unit":"other","case":"x","input":{}}).to_string())["error"]["kind"],
        "unknown_unit"
    );
    assert_eq!(
        dispatch(&format!(
            r#"{{"unit":"{UNIT}","case":"x","input":{{"n":1e1000}}}}"#
        )),
        json!({"unit":null,"case":null,"error":{
            "kind":"invalid_request","message":"invalid JSON or duplicate key"}})
    );
}

#[test]
fn refusals_name_the_first_invalid_field_in_validation_order() {
    for (input, message) in [
        (json!({}), "expected op string"),
        (json!({"op":1}), "expected op string"),
        (json!({"op":"nope"}), "unknown operation"),
        (
            json!({"op":"matrix_default","x":1}),
            "unexpected or missing field",
        ),
        (json!({"op":"inverse"}), "unexpected or missing field"),
        (
            json!({"op":"inverse","matrix":[]}),
            "expected matrix: matrix",
        ),
        (
            json!({"op":"inverse","matrix":[[ONE,ONE,ONE],[ONE,ONE],[ONE,ONE,ONE]]}),
            "expected matrix: matrix",
        ),
        (
            json!({"op":"inverse","matrix":[[ONE,ONE,ONE],[ONE,ONE,"3FF0000000000000"],[ONE,ONE,ONE]]}),
            "expected double bit string: matrix",
        ),
        (
            json!({"op":"set_rotation","matrix":identity(),"angle":1.0}),
            "expected double bit string: angle",
        ),
        (
            json!({"op":"set_rotation","matrix":identity(),"angle":"+ff0000000000000"}),
            "expected double bit string: angle",
        ),
        (
            json!({"op":"set_rotation","matrix":identity(),"angle":"3ff000000000000"}),
            "expected double bit string: angle",
        ),
        (
            json!({"op":"set_scale","matrix":identity(),"scale":[ONE]}),
            "expected vector: scale",
        ),
        (
            json!({"op":"mul_vector3","matrix":identity(),"vector":[ONE,ONE]}),
            "expected vector: vector",
        ),
        (
            json!({"op":"words_to_double","hi":9223372036854775808_u64,"lo":0}),
            "integer out of range: hi",
        ),
        (
            json!({"op":"words_to_double","hi":0,"lo":-1}),
            "integer out of range: lo",
        ),
        (
            json!({"op":"words_to_double","hi":0,"lo":1.0}),
            "integer out of range: lo",
        ),
        (
            json!({"op":"to_double","value":1}),
            "expected int128 string: value",
        ),
        (
            json!({"op":"to_double","value":"-0"}),
            "expected int128 string: value",
        ),
        (
            json!({"op":"to_double","value":"01"}),
            "expected int128 string: value",
        ),
        (
            json!({"op":"to_double","value":"+1"}),
            "expected int128 string: value",
        ),
        (
            json!({"op":"to_double","value":""}),
            "expected int128 string: value",
        ),
        (
            json!({"op":"to_double","value":"170141183460469231731687303715884105728"}),
            "expected int128 string: value",
        ),
        (
            json!({"op":"cross_wide","a":[0,0,0],"b":[0,0]}),
            "expected vector: a",
        ),
        (
            json!({"op":"cross_wide","a":[0,0],"b":[0,"0"]}),
            "integer out of range: b",
        ),
        (
            json!({"op":"dot_wide","a":[i64::MIN,i64::MIN],"b":[i64::MIN,i64::MIN]}),
            "DotWide overflows KI_INT128",
        ),
    ] {
        assert_eq!(refused(input.clone()), message, "{input}");
    }
}

#[test]
fn integer_zero_tokens_match_the_reference_reader() {
    let text = request(json!({"op":"words_to_double","hi":0,"lo":0}));
    let reply = dispatch(&text);
    assert_eq!(reply["result"]["value"], ZERO);
    assert_eq!(dispatch(&text.replace("\"lo\":0", "\"lo\":-0")), reply);
    assert_eq!(dispatch(&text.replace("\"hi\":0", "\"hi\":-0")), reply);
}

#[test]
fn singular_inverse_transports_nonfinite_bits_losslessly() {
    let reply = run(
        &json!({"op":"inverse","matrix":[[ONE,TWO,ONE],[TWO,"4010000000000000",TWO],[ONE,ONE,ONE]]}),
    );
    let matrix = reply.unwrap_or_default()["matrix"].clone();
    // Determinant is +0, so 1.0 / det is +inf: nonzero cofactors become signed infinities
    // and zero cofactors 0 * inf = NaN. Exact bits depend on the platform's NaN sign; only the
    // non-NaN members are asserted here, the live oracle comparison checks all of them.
    assert_eq!(matrix[0][0], "7ff0000000000000");
    assert_eq!(matrix[0][1], "fff0000000000000");
    for row in matrix.as_array().into_iter().flatten() {
        for value in row.as_array().into_iter().flatten() {
            let text = value.as_str().unwrap_or_default();
            assert_eq!(text.len(), 16);
        }
    }
}

#[test]
fn replies_have_the_documented_shapes() {
    let cases = [
        (json!({"op":"matrix_default"}), "matrix"),
        (json!({"op":"get_scale","matrix":identity()}), "vector"),
        (json!({"op":"determinant","matrix":identity()}), "value"),
        (json!({"op":"stream","matrix":identity()}), "text"),
    ];
    for (input, key) in cases {
        let result = run(&input).unwrap_or_default();
        assert!(result.get(key).is_some(), "{input} -> {result}");
    }
    assert_eq!(
        run(&json!({"op":"stream","matrix":identity()})).unwrap_or_default()["text"],
        "| 1 0 0 |\n| 0 1 0 |\n| 0 0 1 |\n"
    );
    assert_eq!(
        run(&json!({"op":"compare","matrix":identity(),"other":identity()})).unwrap_or_default(),
        json!({"equal":true,"not_equal":false})
    );
    assert_eq!(
        run(&json!({"op":"cross_wide","a":[i64::MIN,i64::MIN],"b":[i64::MAX,i64::MIN]}))
            .unwrap_or_default(),
        json!({"value":"170141183460469231722463931679029329920","double":"47e0000000000000"})
    );
    assert_eq!(
        run(&json!({"op":"to_double","value":"-170141183460469231731687303715884105728"}))
            .unwrap_or_default(),
        json!({"value":"c7e0000000000000","via_words":"c7e0000000000000"})
    );
}

#[test]
fn reply_identities_are_preserved_for_results_and_refusals() {
    let ok = dispatch(&request(json!({"op":"matrix_default"})));
    assert_eq!(ok["unit"], UNIT);
    assert_eq!(ok["case"], "t");
    let bad = dispatch(&request(json!({"op":"matrix_default","extra":true})));
    assert_eq!(bad["unit"], UNIT);
    assert_eq!(bad["case"], "t");
}

#[test]
fn integer_matrices_and_explicit_stream_state_reach_the_port() {
    let qa = json!([[1, 1, 1], [2, 2, 2], [3, 3, 3]]);
    assert_eq!(
        run(&json!({"op":"mul_vector3","type":"int","matrix":qa,"vector":[5,5,5]}))
            .unwrap_or_default(),
        json!({"vector":[15,30,45]})
    );
    assert_eq!(
        run(&json!({"op":"mul_scalar","type":"int","scalar_type":"int","matrix":qa,"scalar":5}))
            .unwrap_or_default(),
        json!({"matrix":[[5,5,5],[10,10,10],[15,15,15]]})
    );
    let eighth = "3fc0000000000000";
    let reply = run(
        &json!({"op":"stream","matrix":[[eighth,ZERO,ZERO],[ZERO,eighth,ZERO],
        [ZERO,ZERO,eighth]],"state":{"flags":["fixed"],"precision":2,"width":0,"fill":" "}}),
    );
    assert_eq!(
        reply.unwrap_or_default(),
        json!({"text":"| 0.12 0.00 0.00 |\n| 0.00 0.12 0.00 |\n| 0.00 0.00 0.12 |\n",
            "width_after":0})
    );
    // Without a state the reply keeps the original fresh-stream shape.
    assert_eq!(
        run(&json!({"op":"stream","matrix":identity()})).unwrap_or_default(),
        json!({"text":"| 1 0 0 |\n| 0 1 0 |\n| 0 0 1 |\n"})
    );
    for (input, message) in [
        (
            json!({"op":"determinant","type":"int","matrix":[[i32::MAX,0,0],[0,2,0],[0,0,1]]}),
            "MATRIX3x3<int>::Determinant overflows int",
        ),
        (
            json!({"op":"inverse","type":"int","matrix":[[1,2,3],[2,4,6],[1,1,1]]}),
            "operator*( MATRIX3x3<int>, double ) converts a value outside int",
        ),
        (
            json!({"op":"transpose","type":"float","matrix":identity()}),
            "expected type: double or int",
        ),
        (
            json!({"op":"stream","matrix":identity(),"state":{"flags":["fixed","fixed"],
                "precision":2,"width":0,"fill":" "}}),
            "expected stream state",
        ),
        (
            json!({"op":"stream","matrix":identity(),"state":{"flags":[],
                "precision":513,"width":0,"fill":" "}}),
            "integer out of range: precision",
        ),
    ] {
        assert_eq!(refused(input.clone()), message, "{input}");
    }
}
