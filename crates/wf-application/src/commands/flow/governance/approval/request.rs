//! Reading approval requests in the order `ApprovalStore` checks them, and
//! selecting and joining a listing.

use super::super::super::opts::{MAX_EXACT, Opts};
use super::super::super::term::Term;
use super::super::super::{FlowError, keys};
use super::super::{MAX_FIELD_BYTES, MAX_KEY_BYTES, key_size, native};
use super::{Approval, Command, Decision, Expiry, NewApproval, Status, Version};
use crate::routing::Partition;

const MAX_ASSIGNEES: usize = 1_000;
const MAX_LISTED: usize = 1_000;

fn rejected(text: String) -> FlowError {
    FlowError::rejected(text)
}

/// The approval's id, checked, and its key's size.
fn id(args: &Term) -> Result<Vec<u8>, FlowError> {
    let id = native(args, "id")?;
    if id.is_empty() {
        return Err(rejected(
            "ERR flow approval id must be a non-empty string".to_owned(),
        ));
    }
    let tag = keys::governance_prefix(&Partition::parse(&id), "a").len();
    key_size(tag.saturating_add(id.len()))?;
    Ok(id)
}

fn binary(opts: &Opts<'_>, key: &str, required: bool) -> Result<Option<Vec<u8>>, FlowError> {
    match opts.get(key) {
        None | Some(Term::Nil) if !required => Ok(None),
        Some(Term::Bin(value)) if !value.is_empty() && value.len() <= MAX_FIELD_BYTES => {
            Ok(Some(value.clone()))
        }
        Some(Term::Bin(value)) if !value.is_empty() => Err(rejected(format!(
            "ERR flow approval {key} must be at most {MAX_FIELD_BYTES} bytes"
        ))),
        _ => Err(rejected(format!(
            "ERR flow approval {key} must be a non-empty string"
        ))),
    }
}

fn required(opts: &Opts<'_>, key: &str) -> Result<Vec<u8>, FlowError> {
    binary(opts, key, true)?.ok_or_else(|| {
        rejected(format!(
            "ERR flow approval {key} must be a non-empty string"
        ))
    })
}

fn now_ms(opts: &Opts<'_>) -> Result<Option<u64>, FlowError> {
    match opts.get("now_ms") {
        None => Ok(None),
        Some(Term::Int(now)) => u64::try_from(*now)
            .ok()
            .filter(|now| *now <= MAX_EXACT)
            .map(Some)
            .ok_or_else(|| {
                rejected("ERR flow approval now_ms must be a non-negative integer".to_owned())
            }),
        Some(_) => Err(rejected(
            "ERR flow approval now_ms must be a non-negative integer".to_owned(),
        )),
    }
}

fn assignees(opts: &Opts<'_>) -> Result<Vec<Vec<u8>>, FlowError> {
    let refused =
        || rejected("ERR flow approval assignees must be a list of non-empty strings".to_owned());
    let items = match opts.get("assignees") {
        None => return Ok(Vec::new()),
        Some(Term::List(items)) => items,
        Some(_) => return Err(refused()),
    };
    let mut kept: Vec<Vec<u8>> = Vec::new();
    for (count, item) in items.iter().enumerate() {
        match item {
            Term::Bin(value)
                if !value.is_empty() && value.len() <= MAX_KEY_BYTES && count < MAX_ASSIGNEES =>
            {
                if !kept.contains(value) {
                    kept.push(value.clone());
                }
            }
            _ => return Err(refused()),
        }
    }
    Ok(kept)
}

fn policy_version(opts: &Opts<'_>) -> Result<Option<Version>, FlowError> {
    match opts.get("policy_version") {
        None | Some(Term::Nil) => Ok(None),
        Some(Term::Bin(text)) if !text.is_empty() && text.len() <= MAX_FIELD_BYTES => {
            Ok(Some(Version::Text(text.clone())))
        }
        Some(Term::Bin(text)) if !text.is_empty() => Err(rejected(format!(
            "ERR flow approval policy_version must be at most {MAX_FIELD_BYTES} bytes"
        ))),
        Some(Term::Int(number))
            if u64::try_from(*number).is_ok_and(|number| number <= MAX_EXACT) =>
        {
            Ok(u64::try_from(*number).ok().map(Version::Number))
        }
        Some(_) => Err(rejected(
            "ERR flow approval policy_version must be a non-empty string or non-negative integer"
                .to_owned(),
        )),
    }
}

/// `optional_expires_at_ms/2`: a timeout wins over an absolute time; both
/// are checked against the request's time when it is decided.
fn expiry(opts: &Opts<'_>) -> Result<Expiry, FlowError> {
    if let Some(timeout) = opts.get("timeout_ms") {
        return match timeout {
            Term::Int(timeout) if *timeout > 0 => {
                Ok(Expiry::After(u64::try_from(*timeout).unwrap_or(u64::MAX)))
            }
            _ => Err(rejected(
                "ERR flow approval timeout_ms must be a positive integer".to_owned(),
            )),
        };
    }
    match opts.get("expires_at_ms") {
        None | Some(Term::Nil) => Ok(Expiry::Never),
        Some(Term::Int(at)) if *at >= 0 => Ok(Expiry::At(u64::try_from(*at).unwrap_or(u64::MAX))),
        Some(_) => Err(rejected(
            "ERR flow approval expires_at_ms must be a non-negative integer".to_owned(),
        )),
    }
}

fn request(args: &Term) -> Result<Command, FlowError> {
    let id = id(args)?;
    let opts = Opts::new(
        "FLOW.APPROVAL.REQUEST",
        args,
        &[
            "id",
            "flow_id",
            "scope",
            "now_ms",
            "assignees",
            "reason",
            "requested_by",
            "policy_hash",
            "policy_version",
            "timeout_ms",
            "expires_at_ms",
            "deadline_ms",
        ],
    )?;
    let flow_id = required(&opts, "flow_id")?;
    let scope = required(&opts, "scope")?;
    for dimension in [&scope, &flow_id] {
        key_size(dimension.len())?;
    }
    let now_ms = now_ms(&opts)?;
    let assignees = assignees(&opts)?;
    let reason = binary(&opts, "reason", false)?;
    let requested_by = binary(&opts, "requested_by", false)?;
    let policy_hash = binary(&opts, "policy_hash", false)?;
    let policy_version = policy_version(&opts)?;
    let expiry = expiry(&opts)?;
    let approval = Approval {
        id,
        flow_id,
        scope,
        reason,
        requested_by,
        requested_at_ms: 0,
        assignees,
        policy_hash,
        policy_version,
        expires_at_ms: None,
        status: Status::Pending,
        decided_by: None,
        decided_at_ms: None,
        decision_reason: None,
    };
    Ok(Command::Request(Box::new(NewApproval {
        approval,
        now_ms,
        expiry,
    })))
}

/// Reads `FLOW.APPROVAL.REQUEST`, `APPROVE`, `REJECT` or `GET`.
///
/// # Errors
/// Elixir's text for an invalid option, or an unknown command.
pub fn parse(name: &str, args: &Term) -> Result<Command, FlowError> {
    let approve = match name {
        "FLOW.APPROVAL.REQUEST" => return request(args),
        "FLOW.APPROVAL.GET" => {
            let id = id(args)?;
            Opts::new(name, args, &["id", "deadline_ms"])?;
            return Ok(Command::Get(id));
        }
        "FLOW.APPROVAL.APPROVE" => true,
        "FLOW.APPROVAL.REJECT" => false,
        _ => {
            return Err(rejected(format!(
                "ERR {name} is not supported by this server"
            )));
        }
    };
    let id = id(args)?;
    let opts = Opts::new(
        name,
        args,
        &["id", "approver", "reason", "now_ms", "deadline_ms"],
    )?;
    let now_ms = now_ms(&opts)?;
    let approver = required(&opts, "approver")?;
    let reason = binary(&opts, "reason", false)?;
    Ok(Command::Decide(Decision {
        id,
        approve,
        approver,
        reason,
        now_ms,
    }))
}

/// `FLOW.APPROVAL.LIST`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct List {
    /// Most approvals listed.
    pub limit: usize,
    /// Only this status.
    pub status: Option<Status>,
    /// Only these scopes.
    pub scopes: Option<Vec<Vec<u8>>>,
    /// Only this Flow's.
    pub flow_id: Option<Vec<u8>>,
}

/// Reads `FLOW.APPROVAL.LIST`.
///
/// # Errors
/// Elixir's text for an invalid option.
pub fn parse_list(args: &Term) -> Result<List, FlowError> {
    let opts = Opts::new(
        "FLOW.APPROVAL.LIST",
        args,
        &[
            "limit",
            "status",
            "scope",
            "partition_key",
            "flow_id",
            "deadline_ms",
        ],
    )?;
    let limit = match opts.get("limit") {
        None => 100,
        Some(Term::Int(limit)) if *limit > 0 => usize::try_from(*limit)
            .unwrap_or(usize::MAX)
            .min(MAX_LISTED),
        Some(_) => {
            return Err(rejected(
                "ERR flow approval limit must be a positive integer".to_owned(),
            ));
        }
    };
    let status = match opts.get("status") {
        None | Some(Term::Nil) => None,
        Some(Term::Bin(name)) if Status::parse(name).is_some() => Status::parse(name),
        Some(_) => {
            return Err(rejected(
                "ERR flow approval status must be pending, approved, rejected, or expired"
                    .to_owned(),
            ));
        }
    };
    let scopes = match (opts.get("scope"), opts.get("partition_key")) {
        (Some(Term::Bin(scope)), _) if !scope.is_empty() => Some(vec![scope.clone()]),
        (None | Some(Term::Nil), Some(Term::Bin(partition))) if !partition.is_empty() => {
            let mut prefixed = b"partition:".to_vec();
            prefixed.extend_from_slice(partition);
            Some(vec![partition.clone(), prefixed])
        }
        (None | Some(Term::Nil), None | Some(Term::Nil)) => None,
        _ => {
            return Err(rejected(
                "ERR flow approval scope must be a non-empty string".to_owned(),
            ));
        }
    };
    let flow_id = match opts.get("flow_id") {
        None | Some(Term::Nil) => None,
        Some(Term::Bin(flow_id)) if !flow_id.is_empty() => Some(flow_id.clone()),
        Some(_) => {
            return Err(rejected(
                "ERR flow approval flow_id must be a non-empty string".to_owned(),
            ));
        }
    };
    Ok(List {
        limit,
        status,
        scopes,
        flow_id,
    })
}

impl List {
    fn matches(&self, approval: &Approval) -> bool {
        self.status.is_none_or(|status| approval.status == status)
            && self
                .scopes
                .as_ref()
                .is_none_or(|scopes| scopes.contains(&approval.scope))
            && self
                .flow_id
                .as_ref()
                .is_none_or(|flow_id| *flow_id == approval.flow_id)
    }

    /// One shard's matching approvals, keyed newest request first.
    #[must_use]
    pub fn select<'a>(&self, records: impl Iterator<Item = &'a [u8]>) -> Vec<(Vec<u8>, Term)> {
        records
            .filter_map(Approval::decode)
            .filter(|approval| self.matches(approval))
            .map(|approval| {
                let mut key = (!approval.requested_at_ms).to_be_bytes().to_vec();
                key.extend_from_slice(&approval.id);
                (key, approval.view())
            })
            .collect()
    }

    /// The newest requests first, cut to the limit.
    #[must_use]
    pub fn join(&self, mut parts: Vec<(Vec<u8>, Term)>) -> Term {
        parts.sort_by(|left, right| left.0.cmp(&right.0));
        Term::List(
            parts
                .into_iter()
                .take(self.limit)
                .map(|(_, view)| view)
                .collect(),
        )
    }
}
