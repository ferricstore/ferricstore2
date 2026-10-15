//! Conditional server-captured merge semantics, without runtime or gate proof.

use std::collections::BTreeMap;

use wf_domain::policy::complete::{
    CompletePolicyEncodingError as EncodingError, CompletePolicyReplaceApplyError as ExistingError,
    CompletePolicyRequestError as RequestError, FlowPolicyInputRejection as Reason,
    OriginalCompletePolicyReplaceApplyError as RawError,
    OriginalCompletePolicyReplaceApplyView as RawView, OriginalEtfSafety,
    OriginalPolicyReadValue as ReadValue, OriginalPolicySetApplyView, OriginalPolicySetFrameInput,
    OriginalPolicySetFresh, OriginalPolicySetMapOrders, OriginalPolicySetPatchView,
    PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits, RawFlowPolicyValue as Raw,
    prepare_original_policy_set_frame_bytes, prepare_original_policy_set_fresh,
    prepare_original_policy_set_request, restore_original_policy_set_frame_header,
    scan_original_policy_set_frame,
};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::batch::ApplyError;
use wf_domain::workflow::control::ControlState;
use wf_domain::workflow::request::gate::GateView;
use wf_domain::workflow::request::{RequestScope, RetainedStatus};
use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};
use wf_types::version::{AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity};

use super::{TestResult, limits as profile};

mod cases;
mod current;
mod order;
mod owners;
mod refusals;

#[test]
fn original_public_merge_preserves_exact_and_equivalent_key_semantics() -> TestResult {
    assert_exact_class()?;
    assert_fallback_class()?;
    assert_mathematical_state_keys()?;
    Ok(())
}

fn assert_exact_class() -> TestResult {
    let both = [
        (Raw::Atom("max_active_ms"), Raw::Integer(1_000)),
        (Raw::Binary(b"max_active_ms"), Raw::Integer(3_000)),
        (Raw::Atom("states"), Raw::Map(&[])),
        (Raw::Atom("type"), Raw::Binary(b"r")),
    ];
    let stored = current::envelope(1, Raw::Map(&both))?;
    for (key, number, expected) in [
        (Raw::Binary(b"max_active_ms"), 2_000, 1_000),
        (Raw::Atom("max_active_ms"), 4_000, 4_000),
    ] {
        let pair = [key, Raw::Integer(number)];
        // Public options require atom keys; a nested state policy exercises the
        // binary spelling separately below. Top binary input is already-patched
        // source only, so this public fixture uses ordinary atom options.
        if !matches!(key, Raw::Atom(_)) {
            continue;
        }
        let opts = [Raw::Tuple(&pair)];
        let wanted = cases::policy_with_max(expected)?;
        let result = invoke(Some(&stored), Raw::List(&opts), 1_000_000)?;
        accepted(&result, 1, 2, &wanted)?;
    }
    cases::assert_nested_exact_classes()
}

fn assert_fallback_class() -> TestResult {
    for stored_key in [Raw::Atom("max_active_ms"), Raw::Binary(b"max_active_ms")] {
        let fields = [
            (stored_key, Raw::Integer(1_000)),
            (Raw::Atom("states"), Raw::Map(&[])),
            (Raw::Atom("type"), Raw::Binary(b"r")),
        ];
        let stored = current::envelope(1, Raw::Map(&fields))?;
        let pair = [Raw::Atom("max_active_ms"), Raw::Integer(2_000)];
        let opts = [Raw::Tuple(&pair)];
        let wanted = cases::policy_with_max(2_000)?;
        accepted(
            &invoke(Some(&stored), Raw::List(&opts), 1_000_000)?,
            1,
            2,
            &wanted,
        )?;
    }
    Ok(())
}

fn assert_mathematical_state_keys() -> TestResult {
    let one = [Raw::Integer(1), Raw::Integer(2)];
    let alias = [Raw::Integer(1)];
    let tail = Raw::List(&one[1..]);
    for (old, incoming) in [
        (
            Raw::Integer(1),
            Raw::BigInteger {
                negative: false,
                little_endian: &[1, 0],
            },
        ),
        (
            Raw::BigInteger {
                negative: true,
                little_endian: &[0, 0],
            },
            Raw::Integer(0),
        ),
        (Raw::Nil, Raw::Atom("nil")),
        (
            Raw::List(&one),
            Raw::ImproperList {
                prefix: &alias,
                tail: &tail,
            },
        ),
        (Raw::List(&[]), Raw::List(&[])),
        (Raw::Tuple(&one), Raw::Tuple(&one)),
        (Raw::Map(&[]), Raw::Map(&[])),
    ] {
        let config = [(Raw::Atom("mode"), Raw::Atom("fifo"))];
        let state = [(old, Raw::Map(&config))];
        let fields = [
            (Raw::Atom("states"), Raw::Map(&state)),
            (Raw::Atom("type"), Raw::Binary(b"r")),
        ];
        let stored = current::envelope(1, Raw::Map(&fields))?;
        let remove = [(incoming, Raw::Nil)];
        let pair = [Raw::Atom("states"), Raw::Map(&remove)];
        let opts = [Raw::Tuple(&pair)];
        let wanted = current::minimal(2)?;
        accepted(
            &invoke(Some(&stored), Raw::List(&opts), 1_000_000)?,
            1,
            2,
            &wanted,
        )?;
    }
    // [] is not the nil atom: deleting atom nil must leave the invalid [] key.
    let bad = [(Raw::List(&[]), Raw::Map(&[]))];
    let fields = [
        (Raw::Atom("states"), Raw::Map(&bad)),
        (Raw::Atom("type"), Raw::Binary(b"r")),
    ];
    let stored = current::envelope(1, Raw::Map(&fields))?;
    let remove = [(Raw::Nil, Raw::Nil)];
    let pair = [Raw::Atom("states"), Raw::Map(&remove)];
    let opts = [Raw::Tuple(&pair)];
    rejected(
        &invoke(Some(&stored), Raw::List(&opts), 1_000_000)?,
        Reason::StateName,
        b"ERR flow policy state must be a non-empty string",
    );
    Ok(())
}

fn scope() -> TestResult<RequestScope> {
    Ok(RequestScope::new(
        TenantId::new(b"t".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"z".to_vec())?,
    ))
}

fn stamp() -> TestResult<CommittedTime> {
    Ok(CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?)
}

const fn safety() -> OriginalEtfSafety<'static> {
    OriginalEtfSafety::new(
        &[
            "flow_policy_v1",
            "type",
            "states",
            "version",
            "max_active_ms",
            "retry",
            "max_retries",
            "max_attempts",
            "backoff",
            "kind",
            "fixed",
            "base_ms",
            "max_ms",
            "jitter_pct",
            "exhausted_to",
            "retention",
            "ttl_ms",
            "history_max_events",
            "history_hot_max_events",
            "mode",
            "fifo",
            "parallel",
            "governance",
            "effects",
            "allowed",
            "denied",
            "limits",
            "limit",
            "enforcement",
            "strict_global",
            "budgets",
            "circuits",
            "approvals",
            "assignees",
            "nil",
            "true",
            "false",
            "a",
            "b",
            "x",
            "y",
            "__struct__",
            "Elixir.Dummy",
            "indexed_attributes",
            "indexed_state_meta",
            "error_classes",
            "window_ms",
        ],
        &[],
        None,
        &[],
    )
}

fn invoke(
    current: Option<&[u8]>,
    opts: Raw<'_>,
    history: u32,
) -> TestResult<OriginalPolicySetFresh> {
    let limits = profile(2_000_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    invoke_for_type(b"r", current, opts, history, limits, &mut charge)
}

fn invoke_for_type(
    kind: &[u8],
    current: Option<&[u8]>,
    opts: Raw<'_>,
    history: u32,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult<OriginalPolicySetFresh> {
    let merged = [(0, &[][..], &[][..])];
    invoke_captured(
        (kind, current, history),
        opts,
        &[],
        &merged,
        (limits, charge),
    )
}

fn invoke_captured(
    supplied: (&[u8], Option<&[u8]>, u32),
    opts: Raw<'_>,
    current_records: &[(u32, &[u32], &[u32])],
    merged_records: &[(u32, &[u32], &[u32])],
    account: (PolicySchemaLimits, &mut PolicySchemaCharge),
) -> TestResult<OriginalPolicySetFresh> {
    let (kind, current, history) = supplied;
    let fixtures = profile(2_000_000_000)?;
    let mut setup = PolicySchemaCharge::new(fixtures);
    let prepared =
        prepare_original_policy_set_request(&Raw::Binary(kind), &opts, fixtures, &mut setup)?
            .map_err(|_| "invalid public fixture")?;
    let mut patch = Vec::new();
    order::patch_records(prepared.patch(), &mut patch)?;
    let borrowed = patch
        .iter()
        .map(|(id, p, q)| (*id, p.as_slice(), q.as_slice()))
        .collect::<Vec<_>>();
    let orders = OriginalPolicySetMapOrders::new(
        [0; 32],
        current,
        &borrowed,
        current_records,
        merged_records,
    );
    invoke_prepared(kind, current, &prepared, &orders, history, account)
}

fn invoke_prepared(
    kind: &[u8],
    current: Option<&[u8]>,
    prepared: &wf_domain::policy::complete::OriginalPolicySetRequest<'_>,
    orders: &OriginalPolicySetMapOrders<'_>,
    history: u32,
    account: (PolicySchemaLimits, &mut PolicySchemaCharge),
) -> TestResult<OriginalPolicySetFresh> {
    invoke_checked(
        (kind, current, history, safety()),
        prepared,
        orders,
        account,
    )
}

fn invoke_checked(
    supplied: (&[u8], Option<&[u8]>, u32, OriginalEtfSafety<'_>),
    prepared: &wf_domain::policy::complete::OriginalPolicySetRequest<'_>,
    orders: &OriginalPolicySetMapOrders<'_>,
    account: (PolicySchemaLimits, &mut PolicySchemaCharge),
) -> TestResult<OriginalPolicySetFresh> {
    let (kind, current, history, etf_safety) = supplied;
    let (limits, charge) = account;
    // Isolated Fresh component: setup is a separately charged mathematical
    // header fixture. Root's ports tests thread the complete account and SHA.
    let fixtures = profile(2_000_000_000)?;
    let mut setup = PolicySchemaCharge::new(fixtures);
    let input = OriginalPolicySetFrameInput::new(
        scope()?,
        RequestId::new(b"q".to_vec())?,
        stamp()?,
        prepared,
        orders,
    );
    let (bytes, _, _, _) = prepare_original_policy_set_frame_bytes(
        input,
        fixtures,
        AdmissionLimits::default(),
        &mut setup,
    )?;
    let scanned =
        scan_original_policy_set_frame(&bytes, fixtures, AdmissionLimits::default(), &mut setup)?;
    let header = restore_original_policy_set_frame_header(
        scanned,
        [0; 32],
        [0; 32],
        orders,
        fixtures,
        AdmissionLimits::default(),
        &mut setup,
    )?;
    let intent = header.intent();
    let namespace = scope()?;
    let outcomes = BTreeMap::new();
    let policies = current.map_or_else(BTreeMap::new, |bytes| {
        BTreeMap::from([(kind.to_vec(), ReadValue::Binary(bytes))])
    });
    let marks = BTreeMap::new();
    let base = RawView {
        gate: GateView {
            scope: &namespace,
            control: ControlState::initial(stamp()?),
            outcomes: &outcomes,
        },
        revision: 7,
        policies: &policies,
        active_migrations: &marks,
        completed_migrations: &marks,
        history_max_events_limit: Ok(history),
        outcome_retention_ms: Ok(100),
        safety: etf_safety,
    };
    Ok(prepare_original_policy_set_fresh(
        OriginalPolicySetApplyView::new(base, orders),
        header,
        (
            intent,
            LogIdentity::new(1, 7, AppliedPosition::new(0)),
            stamp()?.wall(),
        ),
        limits,
        AdmissionLimits::default(),
        charge,
    )?)
}

fn accepted(
    result: &OriginalPolicySetFresh,
    stored: u64,
    generation: u64,
    expected: &[u8],
) -> TestResult {
    let (Some(install), outcome) = result else {
        return Err("expected owned install".into());
    };
    assert_eq!(install.stored_generation(), stored);
    assert_eq!(install.generation(), generation);
    assert_eq!(install.encoded(), expected);
    assert_eq!(outcome.response(), expected);
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    assert_ne!(install.encoded().as_ptr(), outcome.response().as_ptr());
    Ok(())
}

fn rejected(result: &OriginalPolicySetFresh, reason: Reason, body: &[u8]) {
    assert!(result.0.is_none());
    assert_eq!(
        result.1.status(),
        RetainedStatus::OriginalPolicySetRejected(reason)
    );
    assert_eq!(result.1.response(), body);
}
