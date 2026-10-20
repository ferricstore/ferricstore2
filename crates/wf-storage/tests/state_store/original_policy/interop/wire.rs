//! Literal ETF terms and full persisted cohorts, independent of product emitters.
use super::{TestResult, f};
use std::collections::BTreeMap;
use wf_domain::policy::RetryOverride;
use wf_domain::policy::complete::{
    CompleteCorePolicyPatchInput, CompleteCorePolicyPatchRequestInput, CompletePolicyData,
    CompletePolicyEncodingError as EncodingError, CompletePolicyValue, CompleteStatePolicyData,
    GovernanceData, OriginalPolicyReplyError, OriginalPolicyReplyValue, PolicySchemaCharge,
    RawFlowPolicyValue as Raw, prepare_complete_policy_value_v2, restore_complete_policy,
};
use wf_domain::policy::definition::{PolicyPatch, PolicyVersion, StateMode};
use wf_domain::retention::RetentionOverride;
use wf_domain::workflow::request::{ClientIntent, RetainedOutcome, RetainedStatus};
use wf_ports::log::{
    prepare_complete_core_policy_patch_frame, verify_complete_core_policy_patch_frame,
};
use wf_storage::{state::RedbStateStore, transaction::with_original_policy_get_reply};
use wf_types::identity::RequestId;

pub(super) fn integer(number: u64) -> Vec<u8> {
    if let Ok(byte) = u8::try_from(number) {
        vec![97, byte]
    } else if let Ok(number) = i32::try_from(number) {
        let mut out = vec![98];
        out.extend_from_slice(&number.to_be_bytes());
        out
    } else {
        let bytes = number.to_le_bytes();
        let length = bytes
            .iter()
            .rposition(|&byte| byte != 0)
            .map_or(1, |i| i + 1);
        let mut out = vec![110, u8::try_from(length).unwrap_or(8), 0];
        out.extend_from_slice(&bytes[..length]);
        out
    }
}
pub(super) fn fields(rows: &[(&[u8], Vec<u8>)]) -> TestResult<Vec<u8>> {
    let rows = rows
        .iter()
        .map(|(key, value)| Ok((f::atom(key)?, value.clone())))
        .collect::<TestResult<Vec<_>>>()?;
    f::etf_map(&rows)
}
pub(super) fn rich_wire(generation: u64, retry: u64, mode: &[u8]) -> TestResult<Vec<u8>> {
    let empty = f::etf_map(&[])?;
    let state = fields(&[
        (b"governance", empty.clone()),
        (b"mode", f::atom(mode)?),
        (b"retention", empty.clone()),
        (b"retry", fields(&[(b"max_retries", integer(3))])?),
    ])?;
    f::etf_policy(
        generation,
        b"job",
        &[
            (b"governance", &empty),
            (b"indexed_attributes", &[106]),
            (b"max_active_ms", &integer(1000)),
            (
                b"retention",
                &fields(&[
                    (b"history_max_events", integer(64)),
                    (b"ttl_ms", integer(100)),
                ])?,
            ),
            (
                b"retry",
                &fields(&[
                    (b"exhausted_to", f::binary(b"dead")?),
                    (b"max_retries", integer(retry)),
                ])?,
            ),
            (b"states", &f::etf_map(&[(f::binary(b"queued")?, state)])?),
            (b"version", &integer(7)),
        ],
    )
}
fn governance() -> GovernanceData {
    GovernanceData {
        mode: None,
        effects: None,
        audit: None,
        limits: None,
        budgets: None,
        circuits: None,
        approvals: None,
    }
}
pub(super) fn parallel_value(kind: &[u8]) -> TestResult<CompletePolicyValue> {
    let schema = f::limits().schema;
    let mut charge = PolicySchemaCharge::new(schema);
    let data = CompletePolicyData {
        workflow_type: kind.to_vec(),
        version: None,
        max_active_ms: None,
        retry: None,
        retention: None,
        indexed_attributes: None,
        indexed_state_meta: None,
        governance: None,
        states: BTreeMap::from([(
            b"queued".to_vec(),
            CompleteStatePolicyData {
                mode: Some(StateMode::Parallel),
                retry: None,
                retention: None,
                governance: None,
            },
        )]),
    };
    Ok(prepare_complete_policy_value_v2(
        restore_complete_policy(1, data, schema, &mut charge)?,
        schema,
        &mut charge,
    )?)
}
pub(super) fn rich_value() -> TestResult<CompletePolicyValue> {
    let schema = f::limits().schema;
    let data = CompletePolicyData {
        workflow_type: b"job".to_vec(),
        version: Some(PolicyVersion::Number(7)),
        max_active_ms: Some(1000),
        retry: Some(RetryOverride {
            max_retries: Some(7),
            exhausted_to: Some(b"dead".to_vec()),
            ..RetryOverride::default()
        }),
        retention: Some(RetentionOverride {
            ttl_ms: Some(100),
            history_max_events: Some(64),
        }),
        states: BTreeMap::from([(
            b"queued".to_vec(),
            CompleteStatePolicyData {
                mode: Some(StateMode::Parallel),
                retry: Some(RetryOverride {
                    max_retries: Some(3),
                    ..RetryOverride::default()
                }),
                retention: Some(RetentionOverride::default()),
                governance: Some(governance()),
            },
        )]),
        indexed_attributes: Some(Vec::new()),
        indexed_state_meta: None,
        governance: Some(governance()),
    };
    let mut charge = PolicySchemaCharge::new(schema);
    let candidate = restore_complete_policy(1, data, schema, &mut charge)?;
    Ok(prepare_complete_policy_value_v2(
        candidate,
        schema,
        &mut charge,
    )?)
}
pub(super) fn simple_wire(generation: u64, mode: &[u8], version: &[u8]) -> TestResult<Vec<u8>> {
    let state = fields(&[(b"mode", f::atom(mode)?)])?;
    f::etf_policy(
        generation,
        b"job",
        &[
            (b"states", &f::etf_map(&[(f::binary(b"queued")?, state)])?),
            (b"version", version),
        ],
    )
}
pub(super) fn opaque_wire(generation: u64, mode: &[u8]) -> TestResult<Vec<u8>> {
    let wrapper = vec![108, 0, 0, 0, 0, 106];
    let state = f::etf_map(&[
        (f::atom(b"mode")?, f::atom(mode)?),
        (f::binary(b"opaque")?, wrapper.clone()),
    ])?;
    let mut big = vec![110, 19, 0, 1];
    big.extend_from_slice(&[0; 15]);
    big.extend_from_slice(&[1, 0, 0]);
    let map = f::etf_map(&[
        (f::binary(b"unknown")?, f::binary(b"\0\xffopaque")?),
        (integer(7), wrapper),
        (f::atom(b"version")?, big),
        (
            f::atom(b"states")?,
            f::etf_map(&[(f::binary(b"queued")?, state)])?,
        ),
        (f::atom(b"type")?, f::binary(b"job")?),
    ])?;
    let mut out = vec![131, 104, 3];
    out.extend_from_slice(&f::atom(b"flow_policy_v1")?);
    out.extend_from_slice(&integer(generation));
    out.extend_from_slice(&map);
    Ok(out)
}
pub(super) fn core_frame(
    request: &[u8],
    now: u64,
    expected: Option<u64>,
    patch: PolicyPatch,
) -> TestResult<Vec<u8>> {
    let caps = f::limits();
    Ok(prepare_complete_core_policy_patch_frame(
        CompleteCorePolicyPatchRequestInput::new(
            f::scope()?,
            RequestId::new(request.to_vec())?,
            f::clock(now)?,
            CompleteCorePolicyPatchInput {
                workflow_type: b"job".to_vec(),
                expected_generation: expected,
                patch,
            },
        ),
        caps.schema,
        caps.codec.admission(),
        &mut PolicySchemaCharge::new(caps.schema),
    )?
    .as_bytes()
    .to_vec())
}
pub(super) fn core_intent(frame: &[u8], at: u64) -> TestResult<ClientIntent> {
    let caps = f::limits();
    Ok(verify_complete_core_policy_patch_frame(
        frame,
        caps.schema,
        caps.codec.admission(),
        &mut PolicySchemaCharge::new(caps.schema),
    )?
    .header(f::log(at))
    .intent)
}
pub(super) fn accepted(
    expected: &mut f::Cohort,
    frame: &[u8],
    request: &[u8],
    position: (u64, u64),
    intent: ClientIntent,
    response: (u16, &[u8]),
) -> TestResult<RetainedOutcome> {
    let (at, now) = position;
    let (version, body) = response;
    let scope = f::scope()?;
    f::rows_mut(expected)?.insert(f::key(6, &[b"g", b"z", b"t", b"job"]), body.to_vec());
    let metadata = f::metadata(&scope, request, at, now, intent, RetainedStatus::Accepted)?;
    f::finish(
        expected,
        &scope,
        frame,
        f::log(at),
        (f::clock(now)?, f::clock(now)?),
        Some((&metadata, version, body)),
    )?;
    f::upgrade6(
        expected,
        &scope,
        f::catalog(&scope, [None, None], None, None, None)?,
    )?;
    let outcome = RetainedOutcome::new(
        metadata,
        version,
        body.to_vec(),
        f::limits().codec.admission(),
    )?;
    assert_outcome_wire(&outcome)?;
    Ok(outcome)
}
fn assert_outcome_wire(outcome: &RetainedOutcome) -> TestResult {
    use wf_storage::state::value::{encode_outcome_profiled, scan_outcome_profiled};
    let scope = outcome.scope();
    let caps = f::limits();
    let mut literal = b"FWSO\0\x01".to_vec();
    for field in [
        scope.tenant().as_bytes(),
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
        outcome.request().as_bytes(),
    ] {
        f::raw(&mut literal, field)?;
    }
    literal.extend_from_slice(&outcome.intent().version().to_be_bytes());
    literal.extend_from_slice(outcome.intent().digest());
    for number in [
        outcome.original_position().term(),
        outcome.original_position().leader_node(),
        outcome.original_position().index().get(),
        outcome.original_time().get(),
        outcome.expires_at().get(),
    ] {
        literal.extend_from_slice(&number.to_be_bytes());
    }
    literal.extend_from_slice(&outcome.response_version().to_be_bytes());
    f::raw(&mut literal, outcome.response())?;
    let fields = scope.tenant().as_bytes().len()
        + scope.group().as_bytes().len()
        + scope.recovery().as_bytes().len()
        + outcome.request().as_bytes().len()
        + outcome.response().len();
    assert_eq!(literal.len(), 102 + fields);
    let owner = if outcome.response().first() == Some(&131) {
        65_536 + 256 * outcome.response().len()
    } else {
        0
    };
    let mut charge = PolicySchemaCharge::new(caps.schema);
    assert_eq!(
        encode_outcome_profiled(
            outcome,
            literal.len(),
            caps.codec.admission(),
            caps.schema,
            &mut charge,
            owner
        )?,
        literal
    );
    let scanned = scan_outcome_profiled(
        &literal,
        scope,
        outcome.request(),
        caps.codec.admission(),
        literal.len(),
        (caps.schema, &mut charge),
        owner,
    )?;
    assert_eq!(scanned.owned_bytes(), 512 + fields);
    let retained = scanned.into_owned(512 + fields)?;
    drop(literal);
    assert_eq!(retained, *outcome);
    Ok(())
}

pub(super) fn get(
    store: &mut RedbStateStore,
    generation: u64,
    mode: &[u8],
    fields: (Option<i128>, i128, i128),
    big: bool,
) -> TestResult {
    let caps = f::limits();
    let mut charge = PolicySchemaCharge::new(caps.schema);
    let account = std::ptr::from_ref(&charge);
    let mut visits = 0;
    with_original_policy_get_reply(
        store,
        &f::scope()?,
        &Raw::Binary(b"job"),
        &Raw::List(&[]),
        caps,
        &mut charge,
        |reply, charge| {
            assert!(std::ptr::eq(account, std::ptr::from_ref(&*charge)));
            visits += 1;
            let reply =
                reply.map_err(|_| OriginalPolicyReplyError::Encoding(EncodingError::Malformed))?;
            assert_eq!(reply.generation(), generation);
            let root = reply.root();
            assert_eq!(root.map_len(), Some(10));
            assert_eq!(
                root.atom_field("type", charge)?
                    .and_then(OriginalPolicyReplyValue::binary_bytes),
                Some(b"job".as_slice())
            );
            assert_eq!(
                root.atom_field("max_active_ms", charge)?
                    .and_then(OriginalPolicyReplyValue::integer),
                fields.0
            );
            let version = root
                .atom_field("version", charge)?
                .ok_or(EncodingError::Malformed)?;
            if big {
                assert_eq!(version.integer(), None);
                let (negative, magnitude) =
                    version.big_integer().ok_or(EncodingError::Malformed)?;
                assert!(!negative);
                assert_eq!(magnitude[0], 1);
                assert_eq!(magnitude[16], 1);
                assert!(magnitude[1..16].iter().all(|&n| n == 0));
                assert!(magnitude[17..].iter().all(|&n| n == 0));
            } else {
                assert_eq!(version.integer(), Some(fields.1));
            }
            let retry = root
                .atom_field("retry", charge)?
                .ok_or(EncodingError::Malformed)?;
            assert_eq!(
                retry
                    .atom_field("max_retries", charge)?
                    .and_then(OriginalPolicyReplyValue::integer),
                Some(fields.2)
            );
            let states = root
                .atom_field("states", charge)?
                .ok_or(EncodingError::Malformed)?;
            assert_eq!(states.map_len(), Some(1));
            let mut rows = states.map_iter().ok_or(EncodingError::Malformed)?;
            let (name, state) = rows.next(charge)?.ok_or(EncodingError::Malformed)?;
            assert_eq!(name.binary_bytes(), Some(b"queued".as_slice()));
            assert_eq!(
                state
                    .atom_field("mode", charge)?
                    .and_then(OriginalPolicyReplyValue::atom_bytes),
                Some(mode)
            );
            assert!(rows.next(charge)?.is_none());
            Ok(())
        },
    )?;
    assert_eq!(visits, 1);
    Ok(())
}

mod expanded;
pub(super) fn expanded(dir: &std::path::Path) -> TestResult {
    expanded::run(dir)
}
