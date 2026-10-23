use super::ManyItemKind::{Cancel, Complete, Create, Fail, Retry, Transition};
use super::ManyItemOptionsError::{Id, LeaseToken, Options};
use super::{ManyItemKind, ManyItemOptionsError, ParsedManyItem, Raw};
use super::{optional_lease, required_lease, reserve_rows, validate_id};
use Raw::{Atom, Binary, List, Map, Nil, Tuple};

pub(super) fn parse<'a>(
    item: &'a Raw<'a>,
    kind: ManyItemKind,
) -> Result<ParsedManyItem<'a>, ManyItemOptionsError> {
    match item {
        Binary(id) if matches!(kind, Create) => fixed_result(id, None, &[]),
        Map(rows) => super::map::parse(rows, kind),
        Tuple(parts) => tuple(parts, kind),
        _ => Err(Id),
    }
}

fn tuple<'a>(
    parts: &'a [Raw<'a>],
    kind: ManyItemKind,
) -> Result<ParsedManyItem<'a>, ManyItemOptionsError> {
    match parts {
        [Binary(id), List(rows)] => keyword_result(id, None, rows, kind),
        [Binary(id), lease @ Binary(_), List(rows)] if matches!(kind, Complete | Retry | Fail) => {
            keyword_result(id, Some(lease), rows, kind)
        }
        _ => match kind {
            Create => create_flat(parts),
            Complete | Retry | Fail => leased_flat(parts),
            Cancel => cancel_flat(parts),
            Transition => transition_flat(parts),
        },
    }
}

fn keyword_result<'a>(
    id: &'a [u8],
    explicit_lease: Option<&'a Raw<'a>>,
    rows: &'a [Raw<'a>],
    kind: ManyItemKind,
) -> Result<ParsedManyItem<'a>, ManyItemOptionsError> {
    validate_id(id)?;
    let lease_token = if let Some(lease) = explicit_lease {
        let lease = required_lease(lease)?;
        keyword_lease(rows)?;
        Some(lease)
    } else {
        let lease = keyword_lease(rows)?;
        if matches!(kind, Complete | Retry | Fail) {
            Some(required_lease(lease.ok_or(LeaseToken)?)?)
        } else {
            optional_lease(lease)?;
            None
        }
    };
    let opts = copy_keyword(rows)?;
    Ok(ParsedManyItem {
        id,
        lease_token,
        opts,
    })
}

fn keyword_lease<'a>(raw_rows: &'a [Raw<'a>]) -> Result<Option<&'a Raw<'a>>, ManyItemOptionsError> {
    let mut lease = None;
    for row in raw_rows {
        let Tuple([key, value]) = row else {
            return Err(Options);
        };
        if atom_name(key)? == "lease_token" && lease.is_none() {
            lease = Some(value);
        }
    }
    Ok(lease)
}

fn copy_keyword<'a>(
    raw_rows: &'a [Raw<'a>],
) -> Result<Vec<(&'a str, &'a Raw<'a>)>, ManyItemOptionsError> {
    // The immutable list has already passed keyword_lease's complete validation.
    let mut rows = reserve_rows(raw_rows.len())?;
    for row in raw_rows {
        match row {
            Tuple([Atom(name), value]) => rows.push((name, value)),
            Tuple([Nil, value]) => rows.push(("nil", value)),
            _ => {}
        }
    }
    Ok(rows)
}

fn atom_name<'a>(raw_key: &Raw<'a>) -> Result<&'a str, ManyItemOptionsError> {
    match raw_key {
        Nil => Ok("nil"),
        Atom(name) => Ok(name),
        _ => Err(Options),
    }
}

fn create_flat<'a>(parts: &'a [Raw<'a>]) -> Result<ParsedManyItem<'a>, ManyItemOptionsError> {
    match parts {
        [Atom("id"), Binary(id), Atom("payload_ref"), value] => {
            fixed_result(id, None, &[("payload_ref", value)])
        }
        [Atom("id"), Binary(id), Atom("payload"), value] => {
            fixed_result(id, None, &[("payload", value)])
        }
        [
            Atom("id"),
            Binary(id),
            Atom("partition_key"),
            partition,
            Atom("payload"),
            value,
        ] => fixed_result(
            id,
            None,
            &[("partition_key", partition), ("payload", value)],
        ),
        [
            Atom("id"),
            Binary(id),
            Atom("partition_key"),
            partition,
            Atom("payload_ref"),
            value,
        ] => fixed_result(
            id,
            None,
            &[("partition_key", partition), ("payload_ref", value)],
        ),
        _ => Err(Id),
    }
}

fn leased_flat<'a>(parts: &'a [Raw<'a>]) -> Result<ParsedManyItem<'a>, ManyItemOptionsError> {
    match parts {
        [
            Atom("id"),
            Binary(id),
            Atom("lease_token"),
            lease,
            Atom("fencing_token"),
            fence,
        ] => fixed_result(id, Some(lease), &[("fencing_token", fence)]),
        [
            Atom("id"),
            Binary(id),
            Atom("partition_key"),
            partition,
            Atom("lease_token"),
            lease,
            Atom("fencing_token"),
            fence,
        ] => fixed_result(
            id,
            Some(lease),
            &[("partition_key", partition), ("fencing_token", fence)],
        ),
        _ => Err(Id),
    }
}

fn cancel_flat<'a>(parts: &'a [Raw<'a>]) -> Result<ParsedManyItem<'a>, ManyItemOptionsError> {
    match parts {
        [Atom("id"), Binary(id), Atom("fencing_token"), fence] => {
            fixed_result(id, None, &[("fencing_token", fence)])
        }
        [
            Atom("id"),
            Binary(id),
            Atom("partition_key"),
            partition,
            Atom("fencing_token"),
            fence,
        ] => fixed_result(
            id,
            None,
            &[("partition_key", partition), ("fencing_token", fence)],
        ),
        _ => Err(Id),
    }
}

fn transition_flat<'a>(parts: &'a [Raw<'a>]) -> Result<ParsedManyItem<'a>, ManyItemOptionsError> {
    match parts {
        [
            Atom("id"),
            Binary(id),
            Atom("fencing_token"),
            fence,
            Atom("lease_token"),
            lease,
        ] => {
            let rows = [("fencing_token", fence), ("lease_token", lease)];
            let count = if matches!(lease, Nil | Atom("nil")) {
                1
            } else {
                2
            };
            fixed_result(id, None, &rows[..count])
        }
        [
            Atom("id"),
            Binary(id),
            Atom("partition_key"),
            partition,
            Atom("fencing_token"),
            fence,
            Atom("lease_token"),
            lease,
        ] => {
            let rows = [
                ("partition_key", partition),
                ("fencing_token", fence),
                ("lease_token", lease),
            ];
            let count = if matches!(lease, Nil | Atom("nil")) {
                2
            } else {
                3
            };
            fixed_result(id, None, &rows[..count])
        }
        _ => Err(Id),
    }
}

fn fixed_result<'a>(
    id: &'a [u8],
    explicit_lease: Option<&'a Raw<'a>>,
    typed_rows: &[(&'a str, &'a Raw<'a>)],
) -> Result<ParsedManyItem<'a>, ManyItemOptionsError> {
    validate_id(id)?;
    let lease_token = if let Some(lease) = explicit_lease {
        Some(required_lease(lease)?)
    } else {
        optional_lease(
            typed_rows
                .iter()
                .find_map(|(key, value)| (*key == "lease_token").then_some(*value)),
        )?;
        None
    };
    let mut opts = reserve_rows(typed_rows.len())?;
    for &row in typed_rows {
        opts.push(row);
    }
    Ok(ParsedManyItem {
        id,
        lease_token,
        opts,
    })
}
