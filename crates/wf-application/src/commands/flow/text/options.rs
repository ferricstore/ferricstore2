//! Named options of FLOW text commands, as `NativeAstParser`'s
//! `@flow_option_specs`, `parse_flow_options/3`, `parse_flow_read_options/3`
//! and `parse_flow_signal_opts/2` read them: an option name in any case,
//! then its value coerced by the option's kind, with the same `ERR` texts.

use super::super::FlowError;
use super::super::term::Term;

/// How an option's value is read.
#[derive(Clone, Copy)]
pub(super) enum Kind {
    /// Kept as given.
    Binary,
    /// An integer, zero or more.
    NonNegative,
    /// An integer above zero.
    Positive,
    /// `1/true/yes/on` or `0/false/no/off`.
    Boolean,
    /// A partition; `GLOBAL` means none.
    Partition,
    /// A positive integer or `INFINITY`.
    Infinity,
    /// A reference of at most 4096 bytes, named in its refusal.
    Ref(&'static str),
}

/// Option name, typed-map key and kind.
pub(super) type Spec = &'static [(&'static str, &'static str, Kind)];

pub(super) fn syntax() -> FlowError {
    FlowError::rejected("ERR syntax error".to_owned())
}

pub(super) fn not_integer() -> FlowError {
    FlowError::rejected("ERR value is not an integer or out of range".to_owned())
}

/// The text of a word; numbers and booleans were turned into text already.
pub(super) fn word(term: &Term) -> Option<&[u8]> {
    match term {
        Term::Bin(bytes) => Some(bytes),
        _ => None,
    }
}

fn upper(term: &Term) -> Option<String> {
    word(term).map(|bytes| String::from_utf8_lossy(bytes).to_ascii_uppercase())
}

/// `parse_int/1`: a whole 64-bit integer.
pub(super) fn int(term: &Term) -> Option<i64> {
    std::str::from_utf8(word(term)?).ok()?.parse().ok()
}

/// `parse_bool/1`.
pub(super) fn boolean(term: &Term) -> Option<bool> {
    match upper(term)?.as_str() {
        "1" | "TRUE" | "YES" | "ON" => Some(true),
        "0" | "FALSE" | "NO" | "OFF" => Some(false),
        _ => None,
    }
}

/// The options read so far, as typed-map entries in the order given; a
/// repeated simple option keeps every value and the first one counts.
#[derive(Default)]
pub(super) struct Options(pub(super) Vec<(Vec<u8>, Term)>);

impl Options {
    pub(super) fn push(&mut self, key: &str, value: Term) {
        self.0.push((key.as_bytes().to_vec(), value));
    }

    /// `merge_map_opt/4`: the map under `key` gains `name`, replacing it.
    pub(super) fn merge(&mut self, key: &str, name: &Term, value: Term) -> Result<(), FlowError> {
        let name = word(name).ok_or_else(syntax)?.to_vec();
        let slot = self.0.iter_mut().find(|(entry, _)| entry == key.as_bytes());
        match slot {
            Some((_, Term::Map(entries))) => {
                entries.retain(|(existing, _)| *existing != name);
                entries.push((name, value));
            }
            _ => self.push(key, Term::Map(vec![(name, value)])),
        }
        Ok(())
    }

    /// `append_list_opt/3`: the list under `key` gains `value`.
    pub(super) fn append(&mut self, key: &str, value: Term) {
        match self.0.iter_mut().find(|(entry, _)| entry == key.as_bytes()) {
            Some((_, Term::List(items))) => items.push(value),
            _ => self.push(key, Term::List(vec![value])),
        }
    }

    pub(super) fn get(&self, key: &str) -> Option<&Term> {
        self.0
            .iter()
            .find(|(entry, _)| entry == key.as_bytes())
            .map(|(_, value)| value)
    }

    pub(super) fn term(self) -> Term {
        Term::Map(self.0)
    }
}

/// `parse_flow_option/3`: one spec option and its value.
fn spec_option(
    options: &mut Options,
    spec: Spec,
    name: &Term,
    value: &Term,
) -> Result<(), FlowError> {
    let upper = upper(name).ok_or_else(syntax)?;
    let (_, key, kind) = spec
        .iter()
        .find(|(wire, _, _)| *wire == upper)
        .ok_or_else(syntax)?;
    let coerced = match kind {
        Kind::Boolean => Term::Bool(
            boolean(value)
                .ok_or_else(|| FlowError::rejected(format!("ERR flow {key} must be a boolean")))?,
        ),
        Kind::Partition if upper_is(value, "GLOBAL") => return Ok(()),
        Kind::Binary | Kind::Partition => value.clone(),
        Kind::NonNegative => Term::Int(int(value).filter(|int| *int >= 0).ok_or_else(not_integer)?),
        Kind::Infinity if upper_is(value, "INFINITY") => Term::text("infinity"),
        Kind::Positive | Kind::Infinity => {
            Term::Int(int(value).filter(|int| *int > 0).ok_or_else(|| {
                FlowError::rejected(format!("ERR flow {key} must be a positive integer"))
            })?)
        }
        Kind::Ref(label) => {
            if word(value).is_some_and(|bytes| bytes.len() > 4_096) {
                return Err(FlowError::rejected(format!(
                    "ERR flow {label} too large (max 4096 bytes)"
                )));
            }
            value.clone()
        }
    };
    options.push(key, coerced);
    Ok(())
}

pub(super) fn upper_is(term: &Term, expected: &str) -> bool {
    upper(term).is_some_and(|text| text == expected)
}

/// `parse_flow_options/3`: attributes and `STATE_META` pairs, then spec
/// options.
pub(super) fn general(args: &[Term], spec: Spec) -> Result<Options, FlowError> {
    let mut options = Options::default();
    let mut rest = args;
    while let Some((name, tail)) = rest.split_first() {
        let token = upper(name).ok_or_else(syntax)?;
        rest = match (token.as_str(), tail) {
            ("ATTRIBUTE", [key, value, more @ ..]) => {
                options.merge("attributes", key, value.clone())?;
                more
            }
            ("ATTRIBUTE_MERGE", [key, value, more @ ..]) => {
                options.merge("attributes_merge", key, value.clone())?;
                more
            }
            ("ATTRIBUTE_DELETE", [key, more @ ..]) => {
                options.append("attributes_delete", key.clone());
                more
            }
            ("STATE_META", [key, value, more @ ..]) => {
                options.merge("state_meta", key, value.clone())?;
                more
            }
            (_, [value, more @ ..]) => {
                spec_option(&mut options, spec, name, value)?;
                more
            }
            _ => return Err(syntax()),
        };
    }
    Ok(options)
}

/// `parse_flow_read_options/3`: `FULL`, `NOPAYLOAD`, `PAYLOAD [MAXBYTES n]`,
/// `VALUE`, `PARTITIONS n ...` and attribute filters, then spec options.
pub(super) fn read(args: &[Term], spec: Spec) -> Result<Options, FlowError> {
    let mut options = Options::default();
    let mut rest = args;
    while let Some((name, tail)) = rest.split_first() {
        let token = upper(name).ok_or_else(syntax)?;
        rest = match (token.as_str(), tail) {
            ("FULL", [value, more @ ..]) if boolean(value).is_some() => {
                options.push("full", Term::Bool(boolean(value) == Some(true)));
                more
            }
            ("FULL", more) => {
                options.push("full", Term::Bool(true));
                more
            }
            ("NOPAYLOAD", more) => {
                options.push("payload", Term::Bool(false));
                more
            }
            ("PAYLOAD", [marker, bytes, more @ ..]) if upper_is(marker, "MAXBYTES") => {
                let max = int(bytes).filter(|max| *max >= 0).ok_or_else(not_integer)?;
                options.push("payload_max_bytes", Term::Int(max));
                options.push("payload", Term::Bool(true));
                more
            }
            ("PAYLOAD", more) => {
                options.push("payload", Term::Bool(true));
                more
            }
            ("VALUE", [value, more @ ..]) => {
                options.append("values", value.clone());
                more
            }
            ("PARTITIONS", [count, more @ ..]) => {
                let count = int(count)
                    .and_then(|count| usize::try_from(count).ok())
                    .filter(|count| *count > 0)
                    .ok_or_else(|| {
                        FlowError::rejected(
                            "ERR flow partition_keys must be a non-empty list".to_owned(),
                        )
                    })?;
                let (partitions, after) = more.split_at_checked(count).ok_or_else(|| {
                    FlowError::rejected("ERR flow partition_keys count mismatch".to_owned())
                })?;
                options.push("partition_keys", Term::List(partitions.to_vec()));
                after
            }
            ("ATTRIBUTE", [key, value, more @ ..]) => {
                options.merge("attributes", key, value.clone())?;
                more
            }
            ("ATTRIBUTE_MERGE", [key, value, more @ ..]) => {
                options.merge("attributes_merge", key, value.clone())?;
                more
            }
            ("ATTRIBUTE_DELETE", [key, more @ ..]) => {
                options.append("attributes_delete", key.clone());
                more
            }
            (_, [value, more @ ..]) => {
                spec_option(&mut options, spec, name, value)?;
                more
            }
            _ => return Err(syntax()),
        };
    }
    Ok(options)
}

/// `parse_flow_signal_opts/2`: named values and drops, then signal options.
pub(super) fn signal(args: &[Term]) -> Result<Options, FlowError> {
    let mut options = Options::default();
    let mut rest = args;
    while let Some((name, tail)) = rest.split_first() {
        let token = upper(name).ok_or_else(syntax)?;
        rest = match (token.as_str(), tail) {
            ("VALUE", [key, value, more @ ..]) => {
                options.merge("values", key, value.clone())?;
                more
            }
            ("VALUE_REF", [key, value, more @ ..]) => {
                options.merge("value_refs", key, value.clone())?;
                more
            }
            ("DROP_VALUE", [key, more @ ..]) => {
                options.append("drop_values", key.clone());
                more
            }
            ("OVERRIDE_VALUE", [key, more @ ..]) => {
                options.append("override_values", key.clone());
                more
            }
            (_, [value, more @ ..]) => {
                spec_option(&mut options, super::specs::SIGNAL, name, value)?;
                more
            }
            _ => return Err(syntax()),
        };
    }
    Ok(options)
}
