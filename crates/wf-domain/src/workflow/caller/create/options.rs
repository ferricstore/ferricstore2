//! Original keyword validation and first-occurrence lookup, including atom aliases.
use super::I;
use super::budget::{Failure, source};
pub(super) fn atom<'a>(value: I<'_, 'a>) -> Option<&'a str> {
    match value {
        I::Atom(s) => Some(s),
        I::Nil => Some("nil"),
        I::Boolean(true) => Some("true"),
        I::Boolean(false) => Some("false"),
        _ => None,
    }
}
pub(super) fn nil(value: I<'_, '_>) -> bool {
    matches!(value, I::Nil | I::Atom("nil"))
}
pub(super) fn rows<'r, 'a>(value: I<'r, 'a>) -> Result<&'r [I<'r, 'a>], Failure> {
    let I::List(rows) = value else {
        return Err(source("ERR flow opts must be a keyword list"));
    };
    for &row in rows {
        if !matches!(row,I::Tuple(parts) if parts.len()==2 && atom(parts[0]).is_some()) {
            return Err(source("ERR flow opts must be a keyword list"));
        }
    }
    if get(rows, "return").is_some() {
        return Err(source("ERR flow return option is not supported"));
    }
    Ok(rows)
}
pub(super) fn get<'r, 'a>(rows: &[I<'r, 'a>], name: &str) -> Option<I<'r, 'a>> {
    rows.iter().find_map(|row| match row {
        I::Tuple(parts) if parts.len() == 2 && atom(parts[0]) == Some(name) => Some(parts[1]),
        _ => None,
    })
}
pub(super) fn binary<'a>(
    value: Option<I<'_, 'a>>,
    key: &'static str,
    required: bool,
) -> Result<&'a [u8], Failure> {
    match value {
        Some(I::Binary(b)) if !b.is_empty() => Ok(b),
        None if required => Err(source(format!("ERR flow {key} is required"))),
        _ => Err(source(format!("ERR flow {key} must be a non-empty string"))),
    }
}
pub(super) fn reference<'r, 'a>(
    rows: &[I<'r, 'a>],
    key: &'static str,
) -> Result<Option<I<'r, 'a>>, Failure> {
    match get(rows, key) {
        None => Ok(None),
        Some(v) if nil(v) => Ok(None),
        Some(I::Binary(b)) if b.len() <= 4096 => Ok(Some(I::Binary(b))),
        Some(I::Binary(_)) => Err(source(format!("ERR flow {key} too large (max 4096 bytes)"))),
        _ => Err(source(format!("ERR flow {key} must be a string"))),
    }
}
