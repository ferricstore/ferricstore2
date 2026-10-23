use super::ManyItemKind::{Cancel, Complete, Create, Fail, Retry, Transition};
use super::ManyItemOptionsError::Id;
use super::{ManyItemKind, ManyItemOptionsError, ParsedManyItem, Raw};
use super::{optional_lease, required_lease, reserve_rows, validate_id};
use Raw::{Atom, Binary};

const CREATE_FIELDS: [&str; 19] = [
    "history_max_events",
    "history_hot_max_events",
    "max_active_ms",
    "retention_ttl_ms",
    "idempotent",
    "correlation_id",
    "root_flow_id",
    "parent_flow_id",
    "partition_key",
    "state_meta",
    "attributes",
    "value_refs",
    "values",
    "payload_ref",
    "payload",
    "priority",
    "run_at_ms",
    "state",
    "type",
];

const PAYLOAD_FIELDS: [&str; 10] = [
    "state_meta",
    "attributes_delete",
    "attributes_merge",
    "attributes",
    "override_values",
    "drop_values",
    "value_refs",
    "values",
    "payload_ref",
    "payload",
];

const CANCEL_REASON_FIELDS: [&str; 9] = [
    "state_meta",
    "attributes_delete",
    "attributes_merge",
    "attributes",
    "override_values",
    "drop_values",
    "value_refs",
    "values",
    "reason",
];

pub(super) fn parse<'a>(
    rows: &'a [(Raw<'a>, Raw<'a>)],
    kind: ManyItemKind,
) -> Result<ParsedManyItem<'a>, ManyItemOptionsError> {
    let terminal = matches!(kind, Complete | Retry | Fail);
    let needs_fence = !matches!(kind, Create);
    let mut selected = None;
    // Empty IDs and invalid leases reject after a complete key family is selected.
    for atom_family in [true, false] {
        let Some(Binary(id)) = field(rows, "id", atom_family) else {
            continue;
        };
        let fence = field(rows, "fencing_token", atom_family);
        let lease = field(rows, "lease_token", atom_family);
        if (needs_fence && fence.is_none()) || (terminal && lease.is_none()) {
            continue;
        }
        selected = Some((*id, fence, lease));
        break;
    }
    let (id, fence, lease) = selected.ok_or(Id)?;
    validate_id(id)?;
    let core_fence = if needs_fence {
        Some(fence.ok_or(Id)?)
    } else {
        None
    };
    let lease_token = if terminal {
        Some(required_lease(lease.ok_or(Id)?)?)
    } else {
        let lease = if needs_fence {
            option(rows, "lease_token")
        } else {
            None
        };
        optional_lease(lease)?;
        None
    };
    let groups = groups(kind);
    let mut count = usize::from(core_fence.is_some());
    for group in groups {
        for name in *group {
            count += usize::from(option(rows, name).is_some());
        }
    }
    let mut opts = reserve_rows(count)?;
    if let Some(fence) = core_fence {
        opts.push(("fencing_token", fence));
    }
    for group in groups {
        for &name in *group {
            if let Some(value) = option(rows, name) {
                opts.push((name, value));
            }
        }
    }
    Ok(ParsedManyItem {
        id,
        lease_token,
        opts,
    })
}

fn field<'a>(rows: &'a [(Raw<'a>, Raw<'a>)], name: &str, atom_family: bool) -> Option<&'a Raw<'a>> {
    rows.iter().find_map(|(key, value)| {
        let matches = if atom_family {
            matches!(key, Atom(atom) if *atom == name)
        } else {
            matches!(key, Binary(bytes) if *bytes == name.as_bytes())
        };
        matches.then_some(value)
    })
}

fn option<'a>(rows: &'a [(Raw<'a>, Raw<'a>)], name: &str) -> Option<&'a Raw<'a>> {
    field(rows, name, true).or_else(|| field(rows, name, false))
}

fn groups(kind: ManyItemKind) -> &'static [&'static [&'static str]] {
    match kind {
        Create => &[&CREATE_FIELDS],
        Complete => &[
            &["result_ref", "result"],
            &PAYLOAD_FIELDS,
            &["partition_key"],
        ],
        Retry => &[
            &["error_ref", "error"],
            &PAYLOAD_FIELDS,
            &["retry"],
            &["partition_key"],
        ],
        Fail => &[&["error_ref", "error"], &PAYLOAD_FIELDS, &["partition_key"]],
        Cancel => &[
            &["lease_token", "reason_ref"],
            &CANCEL_REASON_FIELDS,
            &["partition_key"],
        ],
        Transition => &[&PAYLOAD_FIELDS, &["lease_token", "partition_key"]],
    }
}
