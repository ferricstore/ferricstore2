//! Deciding effect and ledger commands against a Flow's stored governance
//! records, as `Governance.Effect` and `Governance.Ledger` do.
//!
//! The shard hands over the Flow and its records; a write reports whether
//! it changed them, a refusal included (it appends a ledger event).

use super::super::FlowError;
use super::super::term::Term;
use super::denial::{Denial, decision_id, policy_hash};
use super::request::{Lease, LedgerList, Reserve, Update, storage_key};
use super::store::{Effect, Event, Governance, Kind, Owner};
use super::{Command, Status};

/// The Flow an effect command addresses, as stored.
#[derive(Clone, Copy, Debug)]
pub struct Flow<'a> {
    /// Its id.
    pub id: &'a [u8],
    /// Its partition's name.
    pub partition_key: &'a [u8],
    /// Its type.
    pub flow_type: &'a [u8],
    /// Its state.
    pub state: &'a [u8],
    /// Its lease token, when leased.
    pub lease_token: Option<&'a [u8]>,
    /// Its fencing token.
    pub fence: u64,
}

impl Flow<'_> {
    const fn owner(&self) -> Owner<'_> {
        Owner {
            id: self.id,
            partition_key: self.partition_key,
        }
    }
}

/// A write's reply and whether it changed the records.
pub type Decided = (Result<Term, FlowError>, bool);

fn rejected(text: &str) -> FlowError {
    FlowError::rejected(text.to_owned())
}

fn refused(denial: Denial) -> FlowError {
    FlowError::Governance(Box::new(denial))
}

/// `validate_flow_lease/2`.
fn leased(flow: &Flow<'_>, lease: &Result<Lease, String>) -> Result<(), FlowError> {
    let lease = lease.as_ref().map_err(|text| rejected(text))?;
    if flow.lease_token != Some(lease.token.as_slice()) {
        return Err(rejected("ERR flow lease token mismatch"));
    }
    if i64::try_from(flow.fence).ok() != Some(lease.fence) {
        return Err(rejected("ERR flow fencing token mismatch"));
    }
    Ok(())
}

/// Answers a read: an effect, or the Flow's ledger; a missing Flow has
/// neither.
#[must_use]
pub fn read(command: &Command, flow: Option<&Flow<'_>>, governance: Option<&Governance>) -> Term {
    let (Some(flow), Some(governance)) = (flow, governance) else {
        return match command {
            Command::Ledger(_) => Term::List(Vec::new()),
            _ => Term::Nil,
        };
    };
    match command {
        Command::Get(get) => governance
            .effects
            .get(&get.effect_key)
            .map_or(Term::Nil, |effect| {
                effect.view(flow.owner(), &get.effect_key, None)
            }),
        Command::Ledger(list) => ledger(list, flow, governance),
        Command::Reserve(_) | Command::Update(_) => Term::Nil,
    }
}

/// `Ledger.list/3`: the events in the window, oldest first unless `rev`.
fn ledger(list: &LedgerList, flow: &Flow<'_>, governance: &Governance) -> Term {
    let window = governance.ledger.iter().filter(|event| {
        event.at_ms >= list.from_ms && list.to_ms.is_none_or(|to| event.at_ms <= to)
    });
    let events: Vec<Term> = if list.rev {
        window
            .rev()
            .take(list.limit)
            .map(|event| event.view(flow.owner()))
            .collect()
    } else {
        window
            .take(list.limit)
            .map(|event| event.view(flow.owner()))
            .collect()
    };
    Term::List(events)
}

/// Decides a reservation or a terminal move on the Flow's records.
pub fn write(
    command: &Command,
    flow: Option<&Flow<'_>>,
    governance: &mut Governance,
    wall: u64,
) -> Decided {
    let Some(flow) = flow else {
        return (Err(rejected("ERR flow not found")), false);
    };
    match command {
        Command::Reserve(reserve) => self::reserve(reserve, flow, governance, wall),
        Command::Update(update) => self::update(update, flow, governance, wall),
        Command::Get(_) | Command::Ledger(_) => {
            (Ok(read(command, Some(flow), Some(governance))), false)
        }
    }
}

/// The ledger event of an effect reaching `status`, with Elixir's id.
fn effect_event(flow: &Flow<'_>, effect_key: &[u8], effect: &Effect) -> Event {
    let mut subject = flow.id.to_vec();
    subject.push(b':');
    subject.extend_from_slice(effect_key);
    let at_ms = match effect.status {
        Status::Reserved => effect.created_at_ms,
        _ => effect.updated_at_ms,
    };
    Event {
        id: decision_id(&format!("effect_ledger_{}", effect.status.name()), &subject),
        kind: Kind::Effect(effect.status),
        at_ms,
        effect_key: effect_key.to_vec(),
        effect_type: effect.effect_type.clone(),
        policy_hash: effect.policy_hash.clone(),
        denial: None,
    }
}

fn reserve(reserve: &Reserve, flow: &Flow<'_>, governance: &mut Governance, wall: u64) -> Decided {
    if let Err(error) = leased(flow, &reserve.lease) {
        return (Err(error), false);
    }
    let fields = match &reserve.fields {
        Ok(fields) => fields,
        Err(text) => return (Err(rejected(text)), false),
    };
    let now = fields.now_ms.unwrap_or(wall);
    if let Some(existing) = governance.effects.get(&reserve.effect_key) {
        if existing.operation_digest == fields.operation_digest {
            let view = existing.view(flow.owner(), &reserve.effect_key, Some("already_reserved"));
            return (Ok(view), false);
        }
        let key = storage_key(flow.id, Some(flow.partition_key), &reserve.effect_key);
        let denial = Denial::new(
            "GOVERNANCE_CONFLICT",
            b"Effect key already exists with a different operation_digest".to_vec(),
            vec![
                ("policy", Some(b"effect_idempotency".to_vec())),
                ("effect_key", Some(reserve.effect_key.clone())),
                ("status", Some(existing.status.name().as_bytes().to_vec())),
                ("decision_id", Some(decision_id("effect_conflict", &key))),
            ],
        );
        return (Err(refused(denial)), false);
    }
    let hash = policy_hash();
    if fields.idempotency_key.as_ref().is_none_or(Vec::is_empty) {
        return deny(reserve, flow, governance, (now, hash));
    }
    let effect = Effect {
        effect_type: reserve.effect_type.clone(),
        flow_type: flow.flow_type.to_vec(),
        state: flow.state.to_vec(),
        status: Status::Reserved,
        operation_digest: fields.operation_digest.clone(),
        idempotency_key: fields.idempotency_key.clone(),
        policy_hash: hash,
        created_at_ms: now,
        updated_at_ms: now,
        external_id: None,
        error: None,
        reason: None,
        latency_ms: None,
    };
    let view = effect.view(flow.owner(), &reserve.effect_key, Some("reserved"));
    let event = effect_event(flow, &reserve.effect_key, &effect);
    governance
        .effects
        .insert(reserve.effect_key.clone(), effect);
    match governance.append(event) {
        Ok(()) => (Ok(view), true),
        Err(text) => (Err(rejected(&text)), true),
    }
}

/// The default policy's refusal of a reservation without an idempotency
/// key, recorded in the ledger under a numbered event id.
fn deny(
    reserve: &Reserve,
    flow: &Flow<'_>,
    governance: &mut Governance,
    (now, hash): (u64, Vec<u8>),
) -> Decided {
    let mut message = b"Effect ".to_vec();
    message.extend_from_slice(&reserve.effect_type);
    message.extend_from_slice(b" requires an idempotency_key");
    let mut subject = flow.id.to_vec();
    subject.push(b':');
    subject.extend_from_slice(&reserve.effect_type);
    let scope = reserve
        .governance_scope
        .clone()
        .unwrap_or_else(|| flow.partition_key.to_vec());
    let denial = Denial::new(
        "GOVERNANCE_EFFECT_DENIED",
        message,
        vec![
            ("policy", Some(b"idempotency_key_required".to_vec())),
            ("scope", Some(scope)),
            ("type", Some(flow.flow_type.to_vec())),
            ("state", Some(flow.state.to_vec())),
            ("effect_type", Some(reserve.effect_type.clone())),
            ("policy_hash", Some(hash.clone())),
            ("policy_version", None),
            ("enforcement", Some(b"strict_local".to_vec())),
            (
                "decision_id",
                Some(decision_id("idempotency_key_required", &subject)),
            ),
        ],
    );
    governance.denials = governance.denials.saturating_add(1);
    let mut numbered = flow.id.to_vec();
    numbered.extend_from_slice(format!(":{}", governance.denials).as_bytes());
    let digest = decision_id("effect_denied", &numbered);
    let mut id = format!("{now}:").into_bytes();
    id.extend_from_slice(digest.get(..11).unwrap_or(&digest));
    let event = Event {
        id,
        kind: Kind::EffectDenied,
        at_ms: now,
        effect_key: reserve.effect_key.clone(),
        effect_type: reserve.effect_type.clone(),
        policy_hash: hash,
        denial: Some([
            denial.code().to_vec(),
            denial.message().to_vec(),
            denial.policy().to_vec(),
        ]),
    };
    // Elixir answers the refusal whether or not its ledger write succeeds.
    let _appended = governance.append(event);
    (Err(refused(denial)), true)
}

fn update(update: &Update, flow: &Flow<'_>, governance: &mut Governance, wall: u64) -> Decided {
    if let Err(error) = leased(flow, &update.lease) {
        return (Err(error), false);
    }
    let now = match &update.now_ms {
        Ok(now) => now.unwrap_or(wall),
        Err(text) => return (Err(rejected(text)), false),
    };
    let Some(effect) = governance.effects.get_mut(&update.effect_key) else {
        return (Err(rejected("ERR flow effect not found")), false);
    };
    if effect.status == update.status {
        let view = effect.view(flow.owner(), &update.effect_key, Some("already_applied"));
        return (Ok(view), false);
    }
    if effect.status != Status::Reserved {
        let denial = Denial::new(
            "GOVERNANCE_CONFLICT",
            b"Effect is already terminal".to_vec(),
            vec![
                ("policy", Some(b"effect_lifecycle".to_vec())),
                ("effect_key", Some(update.effect_key.clone())),
                (
                    "current_status",
                    Some(effect.status.name().as_bytes().to_vec()),
                ),
                (
                    "requested_status",
                    Some(update.status.name().as_bytes().to_vec()),
                ),
                (
                    "decision_id",
                    Some(decision_id("effect_terminal", &update.effect_key)),
                ),
            ],
        );
        return (Err(refused(denial)), false);
    }
    if now < effect.updated_at_ms {
        return (
            Err(rejected(
                "ERR flow effect now_ms cannot precede updated_at_ms",
            )),
            false,
        );
    }
    effect.status = update.status;
    effect.updated_at_ms = now;
    for (slot, value) in [
        (&mut effect.external_id, &update.external_id),
        (&mut effect.error, &update.error),
        (&mut effect.reason, &update.reason),
    ] {
        if value.is_some() {
            slot.clone_from(value);
        }
    }
    if update.latency_ms.is_some() {
        effect.latency_ms = update.latency_ms;
    }
    let view = effect.view(flow.owner(), &update.effect_key, Some(update.status.name()));
    let event = effect_event(flow, &update.effect_key, effect);
    match governance.append(event) {
        Ok(()) => (Ok(view), true),
        Err(text) => (Err(rejected(&text)), true),
    }
}
