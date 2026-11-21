//! Approvals, as `Governance.Approval` and `ApprovalStore` keep them: a
//! request names a Flow and a scope, and takes one terminal decision
//! (approved or rejected) unless it expires first. A decision attempted
//! after the expiry records it as expired and is refused.

use super::super::FlowError;
use super::super::effect::denial::Denial;
use super::super::term::{Term, map};
use super::{Change, Decided};

mod request;
mod stored;

pub use request::{List, parse, parse_list};

/// Where an approval is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Awaiting a decision.
    Pending,
    /// Approved.
    Approved,
    /// Rejected.
    Rejected,
    /// Expired before a decision.
    Expired,
}

impl Status {
    /// The status as Elixir names it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Expired => "expired",
        }
    }

    /// The status named `name`.
    #[must_use]
    pub fn parse(name: &[u8]) -> Option<Self> {
        [Self::Pending, Self::Approved, Self::Rejected, Self::Expired]
            .into_iter()
            .find(|status| status.name().as_bytes() == name)
    }
}

/// The policy version an approval names: text or a number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Version {
    /// Text.
    Text(Vec<u8>),
    /// A number.
    Number(u64),
}

/// When a request expires.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expiry {
    /// Never.
    Never,
    /// This long after the request.
    After(u64),
    /// At this time.
    At(u64),
}

/// One approval.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Approval {
    /// Its id.
    pub id: Vec<u8>,
    /// The Flow it is for.
    pub flow_id: Vec<u8>,
    /// Its scope.
    pub scope: Vec<u8>,
    /// Why it was requested.
    pub reason: Option<Vec<u8>>,
    /// Who requested it.
    pub requested_by: Option<Vec<u8>>,
    /// When.
    pub requested_at_ms: u64,
    /// Who may decide, in request order without repeats.
    pub assignees: Vec<Vec<u8>>,
    /// The policy hash it was requested under.
    pub policy_hash: Option<Vec<u8>>,
    /// The policy version it was requested under.
    pub policy_version: Option<Version>,
    /// When it expires.
    pub expires_at_ms: Option<u64>,
    /// Where it is.
    pub status: Status,
    /// Who decided.
    pub decided_by: Option<Vec<u8>>,
    /// When it was decided or found expired.
    pub decided_at_ms: Option<u64>,
    /// Why it was decided so.
    pub decision_reason: Option<Vec<u8>>,
}

/// A request for a new approval; its time and expiry are resolved when it
/// is decided, against the client's clock or the node's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewApproval {
    /// The approval, its time and expiry not yet set.
    pub approval: Approval,
    /// The client's clock.
    pub now_ms: Option<u64>,
    /// When it expires.
    pub expiry: Expiry,
}

/// A decision on a pending approval.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decision {
    /// The approval.
    pub id: Vec<u8>,
    /// Approve, or reject.
    pub approve: bool,
    /// Who decides.
    pub approver: Vec<u8>,
    /// Why.
    pub reason: Option<Vec<u8>>,
    /// The client's clock.
    pub now_ms: Option<u64>,
}

/// A command on one approval.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// `FLOW.APPROVAL.REQUEST`.
    Request(Box<NewApproval>),
    /// `FLOW.APPROVAL.APPROVE` or `REJECT`.
    Decide(Decision),
    /// `FLOW.APPROVAL.GET`.
    Get(Vec<u8>),
}

fn conflict(id: &[u8], message: Vec<u8>, status: Option<Status>) -> FlowError {
    let mut rest = vec![("approval_id", Some(id.to_vec()))];
    rest.extend(status.map(|status| ("status", Some(status.name().as_bytes().to_vec()))));
    FlowError::Governance(Box::new(Denial::new("GOVERNANCE_CONFLICT", message, rest)))
}

/// Stores `approval` with `reply`; an approval too large to encode is
/// refused instead.
fn stored(approval: &Approval, reply: Result<Term, FlowError>) -> Decided {
    match approval.encode() {
        Some(bytes) => (reply, Some(Change::Put(bytes))),
        None => (Err(FlowError::Unavailable), None),
    }
}

fn text(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

impl Command {
    /// The approval's id.
    #[must_use]
    pub fn id(&self) -> &[u8] {
        match self {
            Self::Request(new) => &new.approval.id,
            Self::Decide(decision) => &decision.id,
            Self::Get(id) => id,
        }
    }

    /// Decides the command from the approval's stored form.
    pub fn decide(&self, stored: Option<&[u8]>, wall: u64) -> Decided {
        let existing = match stored.map(Approval::decode) {
            None => None,
            Some(Some(approval)) => Some(approval),
            Some(None) => {
                return (
                    Err(FlowError::rejected(
                        "ERR flow approval record is corrupt".to_owned(),
                    )),
                    None,
                );
            }
        };
        match self {
            Self::Get(_) => (
                Ok(existing.map_or(Term::Nil, |approval| approval.view())),
                None,
            ),
            Self::Request(new) => request(new, existing.as_ref(), wall),
            Self::Decide(decision) => match existing {
                None => (
                    Err(FlowError::rejected(
                        "ERR flow approval not found".to_owned(),
                    )),
                    None,
                ),
                Some(approval) => decide(decision, approval, wall),
            },
        }
    }
}

fn request(new: &NewApproval, existing: Option<&Approval>, wall: u64) -> Decided {
    let now = new.now_ms.unwrap_or(wall);
    let expires_at_ms = match new.expiry {
        Expiry::Never => None,
        Expiry::After(timeout) => match now
            .checked_add(timeout)
            .filter(|at| *at <= super::super::MAX_EXACT)
        {
            Some(at) => Some(at),
            None => {
                return (
                    Err(FlowError::rejected(
                        "ERR flow approval timeout_ms must be a positive integer".to_owned(),
                    )),
                    None,
                );
            }
        },
        Expiry::At(at) if at > now && at <= super::super::MAX_EXACT => Some(at),
        Expiry::At(_) => {
            return (
                Err(FlowError::rejected(
                    "ERR flow approval expires_at_ms must be greater than now_ms".to_owned(),
                )),
                None,
            );
        }
    };
    if existing.is_some() {
        let error = conflict(&new.approval.id, b"Approval already exists".to_vec(), None);
        return (Err(error), None);
    }
    let approval = Approval {
        requested_at_ms: now,
        expires_at_ms,
        ..new.approval.clone()
    };
    let reply = Ok(approval.view());
    stored(&approval, reply)
}

fn decide(decision: &Decision, mut approval: Approval, wall: u64) -> Decided {
    let now = decision.now_ms.unwrap_or(wall);
    if approval.status != Status::Pending {
        let message = text(&[
            b"Approval ",
            &approval.id,
            b" already has terminal status ",
            approval.status.name().as_bytes(),
        ]);
        return (
            Err(conflict(&approval.id, message, Some(approval.status))),
            None,
        );
    }
    if now < approval.requested_at_ms {
        return (
            Err(FlowError::rejected(
                "ERR flow approval now_ms cannot precede requested_at_ms".to_owned(),
            )),
            None,
        );
    }
    if approval.expires_at_ms.is_some_and(|at| now >= at) {
        approval.status = Status::Expired;
        approval.decided_at_ms = Some(now);
        let message = text(&[
            b"Approval ",
            &approval.id,
            b" expired before it was decided",
        ]);
        let error = conflict(&approval.id, message, Some(Status::Expired));
        return stored(&approval, Err(error));
    }
    approval.status = if decision.approve {
        Status::Approved
    } else {
        Status::Rejected
    };
    approval.decided_by = Some(decision.approver.clone());
    approval.decided_at_ms = Some(now);
    approval.decision_reason.clone_from(&decision.reason);
    let reply = Ok(approval.view());
    stored(&approval, reply)
}

impl Approval {
    /// The approval as Elixir replies with it.
    #[must_use]
    pub fn view(&self) -> Term {
        let version = match &self.policy_version {
            None => Term::Nil,
            Some(Version::Text(text)) => Term::Bin(text.clone()),
            Some(Version::Number(number)) => Term::int(*number),
        };
        map(vec![
            ("id", Term::Bin(self.id.clone())),
            ("flow_id", Term::Bin(self.flow_id.clone())),
            ("scope", Term::Bin(self.scope.clone())),
            ("reason", Term::optional_bin(self.reason.as_deref())),
            (
                "requested_by",
                Term::optional_bin(self.requested_by.as_deref()),
            ),
            ("requested_at_ms", Term::int(self.requested_at_ms)),
            (
                "assignees",
                Term::List(self.assignees.iter().cloned().map(Term::Bin).collect()),
            ),
            (
                "policy_hash",
                Term::optional_bin(self.policy_hash.as_deref()),
            ),
            ("policy_version", version),
            ("expires_at_ms", Term::optional_int(self.expires_at_ms)),
            ("status", Term::text(self.status.name())),
            ("decided_by", Term::optional_bin(self.decided_by.as_deref())),
            ("decided_at_ms", Term::optional_int(self.decided_at_ms)),
            (
                "decision_reason",
                Term::optional_bin(self.decision_reason.as_deref()),
            ),
        ])
    }
}
