//! `FLOW.SPAWN_CHILDREN` and child groups (ADR-014 step 8), as
//! `MutationAttrs.spawn_children_attrs/3`, `ChildPolicy` and the Elixir state
//! machine's group bookkeeping read and update them.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use super::FlowError;
mod group;

pub use group::{ChildGroup, ChildResult, Wait};

use super::create::{self, CreateFlow, MaxActive};
use super::opts::Opts;
use super::term::Term;

/// The longest new child group id.
const MAX_GROUP_ID_BYTES: usize = 128;

/// A spawn request.
#[derive(Clone, Debug, PartialEq)]
pub struct SpawnChildren {
    /// Parent Flow id.
    pub parent_id: Vec<u8>,
    /// Parent partition.
    pub partition_key: Vec<u8>,
    /// Group id.
    pub group_id: Vec<u8>,
    /// The new group, without children yet.
    pub group: ChildGroup,
    /// Expected parent state.
    pub from_state: Option<Vec<u8>>,
    /// State the parent waits in.
    pub wait_state: Option<Vec<u8>>,
    /// Parent lease token.
    pub lease_token: Option<Vec<u8>>,
    /// Parent fence.
    pub fence: u64,
    /// Client clock.
    pub now_ms: Option<u64>,
    /// The children to create; each defaults to the parent's partition.
    pub children: Vec<CreateFlow>,
}

fn policy(
    opts: &Opts<'_>,
    key: &str,
    default: &str,
    allowed: &[&str],
) -> Result<String, FlowError> {
    let value = match opts.get(key) {
        None | Some(Term::Nil) => default.to_owned(),
        Some(Term::Bin(text)) => String::from_utf8_lossy(text)
            .to_lowercase()
            .replace('-', "_"),
        Some(_) => String::new(),
    };
    if allowed.contains(&value.as_str()) {
        Ok(value)
    } else {
        Err(FlowError::flow(key, "has unsupported value"))
    }
}

fn exhaust_to(opts: &Opts<'_>) -> Result<(Vec<u8>, Vec<u8>), FlowError> {
    let pick = |term: Option<&Term>| match term {
        Some(Term::Bin(state)) if !state.is_empty() => Some(state.clone()),
        _ => None,
    };
    let (success, failure) = match opts.get("exhaust_to") {
        Some(exhaust) => (pick(exhaust.get(b"success")), pick(exhaust.get(b"failure"))),
        None => (pick(opts.get("success")), pick(opts.get("failure"))),
    };
    let (success, failure) = success.zip(failure).ok_or_else(|| {
        FlowError::rejected(
            "ERR flow exhaust_to must include success and failure states".to_owned(),
        )
    })?;
    bounded_state(&success)?;
    bounded_state(&failure)?;
    Ok((success, failure))
}

/// A state a group moves its parent into is a new state, held to 64 bytes
/// as Elixir holds it (audit gap G33).
fn bounded_state(state: &[u8]) -> Result<(), FlowError> {
    if state.len() > 64 {
        return Err(FlowError::flow("state", "is longer than 64 bytes"));
    }
    Ok(())
}

fn child(item: &Term, opts: &Opts<'_>, parent_partition: &[u8]) -> Result<CreateFlow, FlowError> {
    let Term::Map(entries) = item else {
        return Err(FlowError::rejected(
            "ERR native flow item is invalid".to_owned(),
        ));
    };
    let mut entries = entries.clone();
    // Values given for the whole spawn go to every child; a child's own
    // value of the same name wins.
    if let Some(Term::Map(shared)) = opts.get("values") {
        let mut merged = shared.clone();
        if let Some((_, Term::Map(own))) = entries.iter().find(|(key, _)| key == b"values") {
            merged.retain(|(name, _)| !own.iter().any(|(mine, _)| mine == name));
            merged.extend(own.iter().cloned());
        }
        entries.retain(|(key, _)| key != b"values");
        entries.push((b"values".to_vec(), Term::Map(merged)));
    }
    if !entries.iter().any(|(key, _)| key == b"partition_key") {
        entries.push((
            b"partition_key".to_vec(),
            Term::Bin(parent_partition.to_vec()),
        ));
    }
    if let Some(now) = opts.get("now_ms") {
        entries.push((b"now_ms".to_vec(), now.clone()));
    }
    create::parse(&Term::Map(entries))
}

/// The request hash: SHA-256 of the group policy, its wait state and the
/// create fields Elixir compares (`@flow_child_group_child_fields` and the
/// payload), children in id and partition order, as lowercase hex.
fn request_hash(
    (group_id, wait_state): (&[u8], Option<&[u8]>),
    group: &ChildGroup,
    children: &[CreateFlow],
) -> Vec<u8> {
    let mut hasher = Sha256::new();
    let mut part = |bytes: &[u8]| {
        hasher.update(u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_be_bytes());
        hasher.update(bytes);
    };
    let number =
        |value: Option<u64>| value.map_or_else(Vec::new, |value| value.to_be_bytes().to_vec());
    part(group_id);
    part(wait_state.unwrap_or_default());
    part(group.wait.as_str().as_bytes());
    part(&[u8::from(group.fail_parent), u8::from(group.cancel_children)]);
    part(&group.success);
    part(&group.failure);
    let mut ordered: Vec<&CreateFlow> = children.iter().collect();
    ordered.sort_by(|left, right| {
        (&left.id, &left.partition_key).cmp(&(&right.id, &right.partition_key))
    });
    for child in ordered {
        part(&child.id);
        part(&child.flow_type);
        part(&child.state);
        part(child.partition_key.as_deref().unwrap_or_default());
        part(&number(child.run_at_ms));
        part(&[child.priority]);
        part(&number(child.retention_ttl_ms));
        part(&match child.max_active_ms {
            None => Vec::new(),
            Some(MaxActive::Infinity) => b"infinity".to_vec(),
            Some(MaxActive::Ms(ms)) => ms.to_be_bytes().to_vec(),
        });
        part(&number(child.history_hot_max_events));
        part(&number(child.history_max_events));
        part(child.correlation_id.as_deref().unwrap_or_default());
        part(child.payload_ref.as_deref().unwrap_or_default());
        part(child.payload.as_deref().unwrap_or_default());
    }
    super::keys::digest(&hasher.finalize())
}

/// The group's policies, before its children are added.
fn empty_group(opts: &Opts<'_>) -> Result<ChildGroup, FlowError> {
    let wait = match policy(opts, "wait", "all", &["all", "any", "none"])?.as_str() {
        "any" => Wait::Any,
        "none" => Wait::None,
        _ => Wait::All,
    };
    let fail_parent = policy(
        opts,
        "on_child_failed",
        "fail_parent",
        &["fail_parent", "ignore"],
    )? == "fail_parent";
    let cancel_children = policy(
        opts,
        "on_parent_closed",
        "cancel_children",
        &["cancel_children", "abandon_children"],
    )? == "cancel_children";
    let (success, failure) = exhaust_to(opts)?;
    Ok(ChildGroup {
        wait,
        fail_parent,
        cancel_children,
        success,
        failure,
        request_hash: Vec::new(),
        children: BTreeMap::new(),
        child_partitions: BTreeMap::new(),
        results: BTreeMap::new(),
        resolved: (wait == Wait::None).then(|| b"success".to_vec()),
    })
}

/// Reads `FLOW.SPAWN_CHILDREN`.
///
/// # Errors
/// The Elixir server's refusal text for an invalid option or child.
pub fn parse(args: &Term) -> Result<SpawnChildren, FlowError> {
    let opts = Opts::new(
        "FLOW.SPAWN_CHILDREN",
        args,
        &[
            "id",
            "children",
            "partition_key",
            "group_id",
            "wait",
            "on_child_failed",
            "on_parent_closed",
            "exhaust_to",
            "success",
            "failure",
            "from_state",
            "wait_state",
            "lease_token",
            "fencing_token",
            "now_ms",
            "max_active_ms",
            "values",
        ],
    )?;
    let parent_id = match opts.get("id") {
        Some(Term::Bin(id)) if !id.is_empty() => id.clone(),
        _ => return Err(FlowError::flow("id", "must be a non-empty string")),
    };
    let items = match opts.get("children") {
        Some(Term::List(items)) if !items.is_empty() => items,
        _ => {
            return Err(FlowError::rejected(
                "ERR flow children must be a non-empty list".to_owned(),
            ));
        }
    };
    let partition_key = create::partition_key(&opts)?
        .ok_or_else(|| FlowError::flow("partition_key", "is required"))?;
    let group_id = opts.required_binary("group_id")?;
    // A new group's id is bounded as Elixir bounds it (audit gap G33).
    if group_id.len() > MAX_GROUP_ID_BYTES {
        return Err(FlowError::flow("group_id", "is longer than 128 bytes"));
    }
    if group_id.starts_with(b"__") {
        return Err(FlowError::rejected(
            "ERR flow group_id is reserved".to_owned(),
        ));
    }
    let group = empty_group(&opts)?;
    let from_state = opts.binary_or_nil("from_state")?;
    // An empty wait state is a missing one, as in Elixir (audit gap G19).
    let wait_state = opts
        .binary_or_nil("wait_state")?
        .filter(|state| !state.is_empty());
    wait_state.as_deref().map_or(Ok(()), bounded_state)?;
    let lease_token = match opts.get("lease_token") {
        None | Some(Term::Nil) => None,
        Some(Term::Bin(token)) if !token.is_empty() => Some(token.clone()),
        Some(_) => return Err(FlowError::flow("lease_token", "must be a non-empty string")),
    };
    let fence = opts
        .non_neg_integer("fencing_token")?
        .ok_or_else(|| FlowError::flow("fencing_token", "is required"))?;
    let children = items
        .iter()
        .map(|item| child(item, &opts, &partition_key))
        .collect::<Result<Vec<_>, _>>()?;
    if children.iter().any(|child| child.id == parent_id) {
        return Err(FlowError::rejected(
            "ERR flow child id must differ from parent id".to_owned(),
        ));
    }
    let mut seen = std::collections::HashSet::new();
    if !children.iter().all(|child| seen.insert(child.id.clone())) {
        return Err(FlowError::rejected(
            "ERR flow duplicate id in batch".to_owned(),
        ));
    }
    let mut group = group;
    group.request_hash = request_hash((&group_id, wait_state.as_deref()), &group, &children);
    for child in &children {
        group.children.insert(child.id.clone(), b"running".to_vec());
        group.child_partitions.insert(
            child.id.clone(),
            child
                .partition_key
                .clone()
                .unwrap_or_else(|| partition_key.clone()),
        );
    }
    Ok(SpawnChildren {
        parent_id,
        partition_key,
        group_id,
        group,
        from_state,
        wait_state,
        lease_token,
        fence,
        now_ms: opts.non_neg_integer("now_ms")?,
        children,
    })
}
