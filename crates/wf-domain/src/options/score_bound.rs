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

/// Rejection-only baseline for the original captured binary parser.
///
/// # Errors
/// Returns the original invalid-float diagnosis for every input.
pub fn parse_original_score_bound(
    _value: &[u8],
) -> Result<OriginalScoreBound, OriginalScoreBoundError> {
    Err(OriginalScoreBoundError::Returned(INVALID_FLOAT))
}
