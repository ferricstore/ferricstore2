//! JSON text of a [`Term`], as Elixir's Jason writes it for history fields
//! (ADR-014): maps in key order as given, binaries as strings, nil as
//! `null`, floats in their shortest round-trip form.

use std::fmt::Write as _;

mod decode;

pub use decode::decode;

use super::term::Term;

fn string(out: &mut String, bytes: &[u8]) {
    out.push('"');
    for character in String::from_utf8_lossy(bytes).chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            control if u32::from(control) < 0x20 => {
                // Writing to a String cannot fail.
                let _written = write!(out, "\\u{:04x}", u32::from(control));
            }
            other => out.push(other),
        }
    }
    out.push('"');
}

fn value(out: &mut String, term: &Term) {
    match term {
        Term::Nil => out.push_str("null"),
        Term::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
        Term::Int(value) => out.push_str(&value.to_string()),
        Term::Float(value) => {
            let text = format!("{value:?}");
            out.push_str(&text);
        }
        Term::Bin(bytes) => string(out, bytes),
        Term::List(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                value(out, item);
            }
            out.push(']');
        }
        Term::Map(entries) => {
            out.push('{');
            for (index, (key, entry)) in entries.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                string(out, key);
                out.push(':');
                value(out, entry);
            }
            out.push('}');
        }
    }
}

/// The JSON text of `term`.
#[must_use]
pub fn encode(term: &Term) -> Vec<u8> {
    let mut out = String::new();
    value(&mut out, term);
    out.into_bytes()
}
