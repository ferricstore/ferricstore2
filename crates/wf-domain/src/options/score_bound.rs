//! Pure score-bound values at the original captured binary boundary.

/// An original score bound; numeric parser outputs must be finite.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OriginalScoreBound {
    /// The exact negative-infinity sentinel.
    NegInf,
    /// The exact positive-infinity sentinel.
    PosInf,
    /// A score included in the bound.
    Inclusive(f64),
    /// A score excluded from the bound.
    Exclusive(f64),
}

/// The original returned-error or raised-argument outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OriginalScoreBoundError {
    /// The original static business-error message.
    Returned(&'static str),
    /// The source raised an argument error during numeric conversion.
    RaisedArgumentError,
}

const INVALID_FLOAT: &str = "ERR min or max is not a float";

/// Parses a captured binary using the original decimal-prefix grammar.
///
/// # Errors
/// Returns the source invalid-float diagnosis or raised-argument classification.
pub fn parse_original_score_bound(
    value: &[u8],
) -> Result<OriginalScoreBound, OriginalScoreBoundError> {
    if value == b"-inf" {
        return Ok(OriginalScoreBound::NegInf);
    }
    if value == b"+inf" {
        return Ok(OriginalScoreBound::PosInf);
    }

    let (exclusive, number) = match value.strip_prefix(b"(") {
        Some(rest) => (true, rest),
        None => (false, value),
    };
    let (end, complete_exponent) = number_prefix(number);
    if end == 0 {
        return Err(OriginalScoreBoundError::Returned(INVALID_FLOAT));
    }

    let negative = number.first() == Some(&b'-');
    let start = usize::from(matches!(number.first(), Some(b'+' | b'-')));
    let parsed = std::str::from_utf8(&number[start..end])
        .ok()
        .and_then(|prefix| prefix.parse::<f64>().ok());
    let score = match parsed {
        Some(score) if score.is_finite() => {
            if negative {
                -score
            } else {
                score
            }
        }
        _ => {
            return Err(if complete_exponent {
                OriginalScoreBoundError::Returned(INVALID_FLOAT)
            } else {
                OriginalScoreBoundError::RaisedArgumentError
            });
        }
    };
    if end != number.len() {
        return Err(OriginalScoreBoundError::Returned(INVALID_FLOAT));
    }
    Ok(if exclusive {
        OriginalScoreBound::Exclusive(score)
    } else {
        OriginalScoreBound::Inclusive(score)
    })
}

fn number_prefix(value: &[u8]) -> (usize, bool) {
    let mut end = usize::from(matches!(value.first(), Some(b'+' | b'-')));
    let digits_start = end;
    while value.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    if end == digits_start {
        return (0, false);
    }

    if value.get(end) == Some(&b'.') && value.get(end + 1).is_some_and(u8::is_ascii_digit) {
        end += 1;
        while value.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
    }

    let mut complete_exponent = false;
    if matches!(value.get(end), Some(b'e' | b'E')) {
        let mut exponent_digits = end + 1;
        if matches!(value.get(exponent_digits), Some(b'+' | b'-')) {
            exponent_digits += 1;
        }
        if value.get(exponent_digits).is_some_and(u8::is_ascii_digit) {
            end = exponent_digits + 1;
            while value.get(end).is_some_and(u8::is_ascii_digit) {
                end += 1;
            }
            complete_exponent = true;
        }
    }
    (end, complete_exponent)
}
