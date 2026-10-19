//! Deterministic answer rules (no Bevy, no expression evaluation).
//!
//! Supported answer types:
//! * **Integer**: stage `answer: {mode: exact, value: 12}` where `value` is a
//!   JSON/YAML integer that fits `i64`. The player's input must be an integer:
//!   optional `+`/`-`, then digits. `12.0`, `1e3`, `1 2` are malformed; a
//!   number that does not fit `i64` is out of range. Compared numerically.
//! * **Decimal**: `exact` with a non-integer `value` (e.g. `2.5`), or
//!   `answer: {mode: tolerance, value: 3.14, tolerance: 0.01}`. Input syntax:
//!   optional sign, digits, optionally `.` and digits (no `.5`, `5.`, exponents,
//!   `inf` or `nan`). Equality policy: with no tolerance, the input and the
//!   expected value are equal when they parse to the same `f64`, so `2.5`,
//!   `2.50` and `+2.5` match (no epsilon); with a tolerance the answer is
//!   correct when `|input - value| <= tolerance` (tolerance finite and >= 0).
//!
//! Other modes (`expression`, `multiple_choice`, `custom`, generators) are not
//! supported in v1. Input is trimmed; empty input is an error, never "wrong".

use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub enum AnswerSpec {
    Integer(i64),
    /// `tolerance` of 0 means exact `f64` equality.
    Decimal {
        value: f64,
        tolerance: f64,
    },
}

/// Why an input could not be judged. It never solves a challenge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum AnswerError {
    #[error("Type an answer first.")]
    Empty,
    #[error("Enter a number, for example 12 or 3.5.")]
    Malformed,
    #[error("That number is out of range.")]
    OutOfRange,
}

impl AnswerSpec {
    /// Reads `math.answer`. `Ok(None)` means a valid but unsupported mode.
    pub fn from_stage(answer: &Value) -> Result<Option<Self>, String> {
        let mode = answer
            .get("mode")
            .and_then(Value::as_str)
            .ok_or("answer.mode must be a string")?;
        let value = answer.get("value");
        match mode {
            "exact" => {
                let Some(Value::Number(number)) = value else {
                    return Err("answer.value must be a number for mode 'exact'".to_string());
                };
                if number.is_i64() {
                    Ok(Some(Self::Integer(number.as_i64().unwrap_or_default())))
                } else if number.is_u64() {
                    Err("answer.value does not fit a 64-bit integer".to_string())
                } else {
                    let value = number.as_f64().filter(|v| v.is_finite());
                    Ok(Some(Self::Decimal {
                        value: value.ok_or("answer.value must be finite")?,
                        tolerance: 0.0,
                    }))
                }
            }
            "tolerance" => {
                let value = value
                    .and_then(Value::as_f64)
                    .filter(|v| v.is_finite())
                    .ok_or("answer.value must be a finite number for mode 'tolerance'")?;
                let tolerance = answer
                    .get("tolerance")
                    .and_then(Value::as_f64)
                    .filter(|t| t.is_finite() && *t >= 0.0)
                    .ok_or("answer.tolerance must be a finite number >= 0")?;
                Ok(Some(Self::Decimal { value, tolerance }))
            }
            _ => Ok(None),
        }
    }

    /// `Ok(true)` correct, `Ok(false)` a well-formed wrong answer, `Err` input
    /// that cannot be judged.
    pub fn check(&self, input: &str) -> Result<bool, AnswerError> {
        match self {
            Self::Integer(expected) => Ok(parse_integer(input)? == *expected),
            Self::Decimal { value, tolerance } => {
                let given = parse_decimal(input)?;
                Ok(if *tolerance == 0.0 {
                    given == *value
                } else {
                    (given - value).abs() <= *tolerance
                })
            }
        }
    }
}

fn split_sign(text: &str) -> &str {
    text.strip_prefix(['+', '-']).unwrap_or(text)
}

pub fn parse_integer(input: &str) -> Result<i64, AnswerError> {
    let text = input.trim();
    if text.is_empty() {
        return Err(AnswerError::Empty);
    }
    let digits = split_sign(text);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(AnswerError::Malformed);
    }
    text.parse::<i64>().map_err(|_| AnswerError::OutOfRange)
}

pub fn parse_decimal(input: &str) -> Result<f64, AnswerError> {
    let text = input.trim();
    if text.is_empty() {
        return Err(AnswerError::Empty);
    }
    let body = split_sign(text);
    let (whole, fraction) = match body.split_once('.') {
        Some((w, f)) => (w, Some(f)),
        None => (body, None),
    };
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(whole) || fraction.is_some_and(|f| !digits(f)) {
        return Err(AnswerError::Malformed);
    }
    let value: f64 = text.parse().map_err(|_| AnswerError::Malformed)?;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(AnswerError::OutOfRange)
    }
}
