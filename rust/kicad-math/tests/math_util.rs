// SPDX-License-Identifier: GPL-3.0-or-later
use kicad_math::math_util::{
    format_printf, kimath_log_debug, kimath_log_overflow, printf_f, rescale_i32,
    rescale_i64_int128, rounded_hypot_wide, LogArgument, LogEvent, MathUtilError, RecordingLog,
    WxAssertCall, WxLogCall, PINNED_LOG, UNIT,
};

#[test]
fn hypot_rounds_and_preserves_both_unsigned_wraps() {
    assert_eq!(rounded_hypot_wide(3, 4), 5);
    assert_eq!(rounded_hypot_wide(1, 1), 1);
    assert_eq!(rounded_hypot_wide(2, 2), 3);
    assert_eq!(
        rounded_hypot_wide(1 << 63, 1 << 63),
        13_043_817_825_332_782_212
    );
    assert_eq!(rounded_hypot_wide(u64::MAX, (1 << 32) - 1), u64::MAX);
    assert_eq!(rounded_hypot_wide(u64::MAX, 1 << 32), 0);
    assert_eq!(
        rounded_hypot_wide(u64::MAX, u64::MAX),
        18_446_744_073_709_551_614
    );
    assert_eq!(rounded_hypot_wide(0, 0), 0);
}

#[test]
fn rescale_negative_halfways_odd_divisors_and_amplification() {
    for (n, v, d, expected) in [
        (1, 1, 2, 1),
        (-1, 1, 2, -1),
        (1, 1, -2, -1),
        (-1, 1, -2, 1),
        (1, -4, 3, -1),
        (1, 4, -3, -1),
        (7, 20, 2, 70),
    ] {
        assert_eq!(rescale_i32(n, v, d), Ok(expected));
        assert_eq!(
            rescale_i64_int128(n.into(), v.into(), d.into()),
            Ok(expected.into())
        );
    }
}

#[test]
fn signed_narrowing_is_observable_not_saturated_or_refused() {
    assert_eq!(rescale_i32(i32::MIN, -1, 1), Ok(i32::MIN));
    assert_eq!(rescale_i32(i32::MIN, i32::MIN, 1), Ok(0));
    assert_eq!(rescale_i64_int128(i64::MIN, -1, 1), Ok(i64::MIN));
    assert_eq!(rescale_i64_int128(i64::MIN, i64::MIN, 1), Ok(0));
    assert_eq!(rescale_i64_int128(i64::MIN, 1, i64::MIN), Ok(1));
}

#[test]
fn undefined_divisions_and_missing_host_effects_are_typed() {
    assert_eq!(rescale_i32(0, 0, 0), Err(MathUtilError::ZeroDenominator));
    assert_eq!(
        rescale_i64_int128(i64::MIN, i64::MIN, 0),
        Err(MathUtilError::ZeroDenominator)
    );
    // The production host defers at the first wx call each body makes.
    assert_eq!(
        kimath_log_debug(&PINNED_LOG, "%lld", &[]),
        Err(MathUtilError::Unported {
            upstream: "wxLog::IsLevelEnabled",
            unit: UNIT
        })
    );
    assert_eq!(
        kimath_log_overflow(&PINNED_LOG, f64::INFINITY, "i"),
        Err(MathUtilError::Unported {
            upstream: "wxOnAssert",
            unit: UNIT
        })
    );
}

#[test]
fn overflow_formats_the_value_and_type_name_and_returns_after_the_assertion() {
    let log = RecordingLog::default();
    let cases: [(f64, &str, &str); 11] = [
        (f64::INFINITY, "i", "inf to i"),
        (f64::NEG_INFINITY, "l", "-inf to l"),
        (f64::NAN, "i", "nan to i"),
        (-f64::NAN, "i", "-nan to i"),
        (-0.0, "i", "-0.000000 to i"),
        (2147483647.5, "i", "2147483647.500000 to i"),
        (0.0078125, "j", "0.007812 to j"),
        (0.0234375, "j", "0.023438 to j"),
        (-1e-10, "x", "-0.000000 to x"),
        (1e22, "a\0b", "10000000000000000000000.000000 to a"),
        (1.0, "\u{e9}", "1.000000 to "),
    ];
    for (value, name, text) in cases {
        assert_eq!(kimath_log_overflow(&log, value, name), Ok(()));
        assert_eq!(
            log.events().last(),
            Some(&LogEvent::Assert(WxAssertCall {
                line: 58,
                function: "kimathLogOverflow",
                condition: "\"Assert failure\"",
                message: format!("\n\nOverflow converting value {text}."),
            }))
        );
    }
    assert_eq!(log.events().len(), cases.len());
    assert_eq!(printf_f(f64::MAX).len(), 309 + 7);
    assert_eq!(printf_f(5e-324), "0.000000");
}

#[test]
fn debug_logs_a_warning_only_when_the_debug_level_is_enabled() {
    let args = [
        LogArgument::Signed(i64::MIN),
        LogArgument::Signed(-1),
        LogArgument::Signed(i64::MAX),
        LogArgument::Signed(7),
    ];
    let format = "Overflow in rescale (%lld * %lld + %lld) / %lld";
    let log = RecordingLog::default();
    assert_eq!(kimath_log_debug(&log, format, &args), Ok(()));
    assert_eq!(
        log.events(),
        [LogEvent::Log(WxLogCall {
            level: 2,
            message: "Overflow in rescale (-9223372036854775808 * -1 + 9223372036854775807) / 7"
                .into(),
            line: 48,
            function: "kimathLogDebug",
            component: "",
        })]
    );
    // Below wxLOG_Debug, or with logging disabled, nothing is formatted, so a missing
    // argument is never read either.
    for log in [RecordingLog::new(5, true), RecordingLog::new(10000, false)] {
        assert_eq!(kimath_log_debug(&log, "%lld %f", &[]), Ok(()));
        assert_eq!(log.events(), []);
    }
    assert_eq!(
        kimath_log_debug(&RecordingLog::new(6, true), "%lld", &[]),
        Err(MathUtilError::UndefinedFormat)
    );
    assert_eq!(
        format_printf(
            "%% %f %lld",
            &[LogArgument::Float(0.5), LogArgument::Signed(3)]
        ),
        Ok("% 0.500000 3".into())
    );
    assert_eq!(
        format_printf("%f", &[LogArgument::Signed(3)]),
        Err(MathUtilError::UndefinedFormat)
    );
    assert_eq!(
        format_printf("%d", &[LogArgument::Signed(3)]),
        Err(MathUtilError::UndefinedFormat)
    );
    // Excess arguments are evaluated but ignored.
    assert_eq!(
        format_printf("x", &[LogArgument::Signed(3)]),
        Ok("x".into())
    );
}
