// SPDX-License-Identifier: GPL-3.0-or-later
//! IEC 60062 spelling, including the reference's six-digit `{:g}` conversion.

fn general(value: f64) -> String {
    if value.is_nan() {
        return if value.is_sign_negative() {
            "-nan"
        } else {
            "nan"
        }
        .into();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-inf"
        } else {
            "inf"
        }
        .into();
    }
    let text = format!("{value:.5e}");
    let Some((mantissa, exponent)) = text.split_once('e') else {
        return text;
    };
    let Ok(exp) = exponent.parse::<i32>() else {
        return text;
    };
    let sign = if value.is_sign_negative() { "-" } else { "" };
    let digits: String = mantissa
        .trim_start_matches('-')
        .chars()
        .filter(|c| *c != '.')
        .collect();
    if !(-4..6).contains(&exp) {
        let mantissa = mantissa.trim_end_matches('0').trim_end_matches('.');
        return format!(
            "{mantissa}e{}{abs:02}",
            if exp < 0 { '-' } else { '+' },
            abs = exp.abs()
        );
    }
    let point = exp + 1;
    let mut result = sign.to_owned();
    if point <= 0 {
        result.push_str("0.");
        result.extend(std::iter::repeat_n('0', (-point) as usize));
        result.push_str(&digits);
    } else {
        for (i, digit) in digits.chars().enumerate() {
            if i == point as usize {
                result.push('.');
            }
            result.push(digit);
        }
    }
    if result.contains('.') {
        result.truncate(result.trim_end_matches('0').trim_end_matches('.').len());
    }
    result
}

/// The rounding epsilon in the calculator is unrelated to this conversion.
pub fn str_value(mut value: f64) -> String {
    let digit = super::eseries::power10(libm::floor(libm::log10(value)) - 2.0);
    value = libm::round(value / digit) * digit;
    let units = [
        (1e12, 'T'),
        (1e9, 'G'),
        (1e6, 'M'),
        (1e3, 'K'),
        (1.0, 'R'),
        (1e-3, 'L'),
    ];
    let (scale, unit) = units
        .iter()
        .copied()
        .find(|(scale, _)| value >= *scale)
        .unwrap_or((1e-3, 'L'));
    let mut text = general(value / scale);
    if let Some(point) = text.find('.') {
        text.replace_range(point..point + 1, &unit.to_string());
    } else {
        text.push(unit);
    }
    text
}
