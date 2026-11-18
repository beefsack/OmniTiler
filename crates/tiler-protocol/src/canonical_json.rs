//! Canonical JSON number and retained-value writer owned by the protocol
//! crate. Behavior mirrors the core transitional sizing writers so the live
//! wire codec no longer depends on core sizing helpers for floats or raw
//! JSON. Core keeps its originals until the legacy sizing path is removed.

use crate::world_protocol::push_json_string;
use tiler_core::connection::RawSettingValue;

/// Shortest round-trip spelling for one finite binary64 value: Ryu digits
/// with a plain (never scientific) expansion. `-0.0` spells `"0"`, matching
/// the digest and the protocol number gate.
///
/// Finite precondition: callers must prove finiteness first (every canonical
/// writer below is checked and only calls this on the finite
/// leg). The `debug_assert` guards the precondition in debug builds; the
/// checked wrappers make it unreachable for non-finite input in all builds.
#[must_use]
pub(crate) fn encode_finite_json_number(value: f64) -> String {
    debug_assert!(value.is_finite(), "finite precondition");
    if value == 0.0 {
        return "0".into();
    }
    let mut buffer = ryu::Buffer::new();
    let mut spelling = buffer.format_finite(value).to_owned();
    if !spelling.contains(['e', 'E']) {
        if spelling.ends_with(".0") {
            spelling.truncate(spelling.len() - 2);
        }
        return spelling;
    }
    let (mantissa, exp) = spelling.split_once(['e', 'E']).expect("ryu exponent");
    let exp: i32 = exp.parse().expect("ryu exponent parses");
    let negative = mantissa.starts_with('-');
    let unsigned = mantissa.trim_start_matches('-');
    let dot = unsigned.find('.').unwrap_or(unsigned.len()) as i32;
    let digits: String = unsigned.chars().filter(|c| *c != '.').collect();
    let point = dot + exp;
    let mut expanded = if point <= 0 {
        format!("0.{}{}", "0".repeat((-point) as usize), digits)
    } else if point as usize >= digits.len() {
        format!("{}{}", digits, "0".repeat(point as usize - digits.len()))
    } else {
        format!(
            "{}.{}",
            &digits[..point as usize],
            &digits[point as usize..]
        )
    };
    if expanded.contains('.') {
        while expanded.ends_with('0') {
            expanded.pop();
        }
        if expanded.ends_with('.') {
            expanded.pop();
        }
    }
    if negative {
        expanded.insert(0, '-');
    }
    expanded
}

/// Checked-canonical failure: a non-finite binary64 reached a canonical
/// writer. Callers grade the row to a narrow hold or refuse the report;
/// writers never normalize to `null` and never panic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NonFiniteNumber;

/// Canonical JSON encoding for one JSON number: the finite shortest
/// round-trip spelling. Non-finite values (constructible only through the
/// public typed carriers, never from wire JSON) are a checked error: the
/// writer emits nothing, so a non-finite typed fact can never normalize
/// to `null` on the wire.
pub(crate) fn push_json_number(out: &mut String, value: f64) -> Result<(), NonFiniteNumber> {
    if value.is_finite() {
        out.push_str(&encode_finite_json_number(value));
        Ok(())
    } else {
        Err(NonFiniteNumber)
    }
}

/// Exact encoded width of [`push_json_number`] without encoding: `None`
/// when the value has no canonical spelling (non-finite). Pre-validation
/// estimators fall back to the `null` width (4) as a pure size bound, never
/// as an encoding.
#[must_use]
pub(crate) fn json_number_len(value: f64) -> Option<usize> {
    if value.is_finite() {
        Some(encode_finite_json_number(value).len())
    } else {
        None
    }
}

/// Canonical JSON encoding for one JSON-agnostic held-screen field value:
/// `null` and bools literally, finite numbers in the shared shortest
/// round-trip spelling, strings through the canonical escaping of
/// [`push_json_string`], arrays and objects in stored order (nested objects
/// iterate in stored order, so the spelling stays deterministic). A
/// non-finite number anywhere in the value is a checked error (see
/// [`push_json_number`]), never a silent `null`. Atomic: a non-finite value
/// anywhere inside leaves `out` untouched.
pub(crate) fn push_raw_setting_value(
    out: &mut String,
    value: &RawSettingValue,
) -> Result<(), NonFiniteNumber> {
    let mut staged = String::new();
    push_raw_setting_value_inner(&mut staged, value)?;
    out.push_str(&staged);
    Ok(())
}

fn push_raw_setting_value_inner(
    out: &mut String,
    value: &RawSettingValue,
) -> Result<(), NonFiniteNumber> {
    match value {
        RawSettingValue::Null => out.push_str("null"),
        RawSettingValue::Bool(set) => out.push_str(if *set { "true" } else { "false" }),
        RawSettingValue::Number(number) => push_json_number(out, *number)?,
        RawSettingValue::String(text) => push_json_string(out, text),
        RawSettingValue::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                push_raw_setting_value_inner(out, item)?;
            }
            out.push(']');
        }
        RawSettingValue::Object(entries) => {
            out.push('{');
            for (i, (key, item)) in entries.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                push_json_string(out, key);
                out.push(':');
                push_raw_setting_value_inner(out, item)?;
            }
            out.push('}');
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world_admission_rows::json_string_len;

    fn encoded_number(value: f64) -> String {
        let mut out = String::new();
        push_json_number(&mut out, value).expect("finite test scalar");
        out
    }

    fn encoded_string(value: &str) -> String {
        let mut out = String::new();
        push_json_string(&mut out, value);
        out
    }

    #[test]
    fn nested_nonfinite_leaves_output_untouched() {
        let value = RawSettingValue::Object(vec![
            ("ok".to_owned(), RawSettingValue::Number(1.5)),
            (
                "bad".to_owned(),
                RawSettingValue::Array(vec![
                    RawSettingValue::Number(0.0),
                    RawSettingValue::Number(f64::INFINITY),
                ]),
            ),
        ]);
        let mut out = String::from("prefix");
        assert!(push_raw_setting_value(&mut out, &value).is_err());
        assert_eq!(out, "prefix");
    }

    #[test]
    #[allow(
        clippy::excessive_precision,
        reason = "the tie witnesses intentionally carry full decimal precision; clippy's shorter suggestion does not round-trip"
    )]
    fn halfway_ties_use_shortest_even_digits() {
        assert_eq!(
            encode_finite_json_number(1000000000000000.25),
            "1000000000000000.2"
        );
        assert_eq!(
            encode_finite_json_number(1000000000000000.75),
            "1000000000000000.8"
        );
        for text in ["1000000000000000.2", "1000000000000000.8"] {
            let parsed: f64 = text.parse().expect("parses");
            assert_eq!(encoded_number(parsed), text);
        }
    }

    #[test]
    fn signed_zero_spells_zero() {
        assert_eq!(encode_finite_json_number(-0.0), "0");
        assert_eq!(encoded_number(-0.0), "0");
        assert_eq!(json_number_len(-0.0), Some(1));
    }

    #[test]
    fn small_large_and_boundary_exponents_expand_plainly() {
        assert_eq!(encode_finite_json_number(1e21), "1000000000000000000000");
        assert_eq!(encode_finite_json_number(1.5e-7), "0.00000015");
        assert_eq!(encode_finite_json_number(0.000001), "0.000001");
        assert_eq!(encode_finite_json_number(1.25), "1.25");
        assert_eq!(encode_finite_json_number(-1.0), "-1");
        let smallest = encode_finite_json_number(5e-324);
        assert_eq!(smallest.len(), 326);
        assert!(smallest.starts_with("0.000"));
        assert!(smallest.ends_with('5'));
        let max = encode_finite_json_number(f64::MAX);
        assert_eq!(max.len(), 309);
        assert!(max.starts_with("17976931348623157"));
        assert!(!max.contains(['e', 'E', '.', '+', '-']));
    }

    #[test]
    fn non_finite_scalars_are_checked_errors_never_null() {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut out = String::new();
            assert_eq!(push_json_number(&mut out, value), Err(NonFiniteNumber));
            assert!(out.is_empty(), "no silent null normalization");
            assert_eq!(json_number_len(value), None);
            for held in [
                RawSettingValue::Number(value),
                RawSettingValue::Array(vec![
                    RawSettingValue::Number(1.0),
                    RawSettingValue::Number(value),
                ]),
                RawSettingValue::Object(vec![("scale".to_owned(), RawSettingValue::Number(value))]),
            ] {
                let mut raw = String::new();
                assert_eq!(
                    push_raw_setting_value(&mut raw, &held),
                    Err(NonFiniteNumber)
                );
                assert!(raw.is_empty(), "no silent null normalization");
            }
        }
    }

    #[test]
    #[allow(
        clippy::excessive_precision,
        reason = "the tie witness intentionally carries full decimal precision"
    )]
    fn number_len_matches_encoding() {
        for value in [
            0.0,
            -0.0,
            1.0,
            -1.0,
            1.25,
            1.5,
            100.0,
            0.1 + 0.2,
            1e21,
            1.5e-7,
            5e-324,
            f64::MAX,
            f64::MIN_POSITIVE,
            1000000000000000.25,
        ] {
            assert_eq!(
                json_number_len(value).expect("finite test scalar"),
                encode_finite_json_number(value).len(),
                "{value}"
            );
            assert_eq!(encoded_number(value), encode_finite_json_number(value));
        }
    }

    #[test]
    fn strings_preserve_multibyte_and_escape_c0() {
        assert_eq!(
            encoded_string("M\u{fc}ller-\u{b5}-\u{65e5}\u{672c}\u{8a9e}-\u{1f39b}"),
            "\"M\u{fc}ller-\u{b5}-\u{65e5}\u{672c}\u{8a9e}-\u{1f39b}\""
        );
        assert_eq!(
            encoded_string("quote\"back\\slash\0\u{1}\u{1f}newline\n"),
            "\"quote\\\"back\\\\slash\\u0000\\u0001\\u001fnewline\\u000a\""
        );
        assert_eq!(encoded_string(""), "\"\"");
        for text in [
            "M\u{fc}ller-\u{b5}-\u{65e5}\u{672c}\u{8a9e}-\u{1f39b}",
            "quote\"back\\slash\0\u{1f}\n",
            "",
            "plain",
        ] {
            assert_eq!(
                json_string_len(text),
                encoded_string(text).len(),
                "{text:?}"
            );
        }
    }

    #[test]
    fn string_len_counts_utf8_bytes_at_identity_bound() {
        let exact = format!("{}\u{e9}", "a".repeat(254));
        assert_eq!(exact.len(), 256);
        assert_eq!(json_string_len(&exact), 258);
        let over = format!("{}\u{e9}", "a".repeat(255));
        assert_eq!(over.len(), 257);
        assert_eq!(json_string_len(&over), 259);
        assert_eq!(encoded_string(&over).len(), json_string_len(&over));
    }
}
