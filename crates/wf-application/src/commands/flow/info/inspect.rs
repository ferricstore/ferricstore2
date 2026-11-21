//! The texts Elixir orders attribute values by: `inspect/1` for the value
//! lists and `Attributes.index_value/1` for picking a candidate index.

use super::super::term::Term;
use crate::routing::base64url_nopad;

/// Whether `String.printable?/1` holds for `c`.
const fn printable(c: char) -> bool {
    matches!(
        c,
        '\n' | '\r' | '\t' | '\u{0B}' | '\u{08}' | '\u{0C}' | '\u{1B}' | '\u{7F}' | '\u{07}'
            | ' '..='~'
            | '\u{A0}'..='\u{D7FF}'
            | '\u{E000}'..='\u{FFFD}'
            | '\u{10000}'..='\u{10FFFF}'
    )
}

/// A printable string as `inspect/1` quotes it.
fn quoted(text: &str) -> Vec<u8> {
    let mut out = String::with_capacity(text.len().saturating_add(2));
    out.push('"');
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0B}' => out.push_str("\\v"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0C}' => out.push_str("\\f"),
            '\u{1B}' => out.push_str("\\e"),
            '\u{7F}' => out.push_str("\\d"),
            '\u{07}' => out.push_str("\\a"),
            '#' if chars.peek() == Some(&'{') => out.push_str("\\#"),
            c => out.push(c),
        }
    }
    out.push('"');
    out.into_bytes()
}

/// A binary that is not printable text, as `inspect/1` lists its bytes.
fn bytes(raw: &[u8]) -> Vec<u8> {
    let listed: Vec<String> = raw.iter().map(u8::to_string).collect();
    format!("<<{}>>", listed.join(", ")).into_bytes()
}

/// `inspect/1` of an attribute value.
pub(super) fn inspect(value: &Term) -> Vec<u8> {
    match value {
        Term::Bin(raw) => match std::str::from_utf8(raw) {
            Ok(text) if text.chars().all(printable) => quoted(text),
            _ => bytes(raw),
        },
        Term::Int(number) => number.to_string().into_bytes(),
        Term::Float(number) => format!("{number:?}").into_bytes(),
        Term::Bool(true) => b"true".to_vec(),
        Term::Bool(false) => b"false".to_vec(),
        Term::Nil => b"nil".to_vec(),
        Term::List(_) | Term::Map(_) => Vec::new(),
    }
}

/// `Attributes.index_value/1` of a scalar attribute value.
pub(super) fn index_value(value: &Term) -> Vec<u8> {
    match value {
        Term::Bin(raw) => format!("s64:{}", base64url_nopad(raw)).into_bytes(),
        Term::Int(number) => format!("i:{number}").into_bytes(),
        Term::Float(number) => format!("f:{number:?}").into_bytes(),
        Term::Bool(true) => b"b:1".to_vec(),
        Term::Bool(false) => b"b:0".to_vec(),
        Term::Nil => b"n:".to_vec(),
        Term::List(_) | Term::Map(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::{Term, inspect};

    #[test]
    fn values_order_as_elixir_inspects_them() {
        let mut values = [
            Term::text("acme\0north"),
            Term::text("gamma"),
            Term::Int(7),
            Term::text("beta"),
            Term::Bool(true),
            Term::Nil,
        ];
        values.sort_by_key(inspect);
        assert_eq!(
            values,
            [
                Term::text("beta"),
                Term::text("gamma"),
                Term::Int(7),
                Term::text("acme\0north"),
                Term::Nil,
                Term::Bool(true),
            ]
        );
        assert_eq!(inspect(&Term::text("a\"b#{c}")), b"\"a\\\"b\\#{c}\"");
        assert_eq!(inspect(&Term::text("\0x")), b"<<0, 120>>");
    }
}
