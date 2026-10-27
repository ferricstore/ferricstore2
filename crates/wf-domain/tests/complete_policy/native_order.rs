//! Reached native binary order derives from actual current and merged maps.

use super::{TestResult, limits};
use wf_domain::policy::complete::{
    CompletePolicyEncodingError as Error, CompletePolicyReplaceApplyError as ApplyError,
    CompletePolicyRequestError, FlowPolicyInputRejection as Reason, GovernanceField,
    GovernancePolicyInputRejection as Gov, GovernanceRuleKind as Rule,
    OriginalCompletePolicyReplaceApplyError as RawError, OriginalPolicyReadValue,
    OriginalPolicySetFrameInput, OriginalPolicySetFresh, OriginalPolicySetFreshView,
    OriginalPolicySetMapOrders, PolicySchemaCharge, RawFlowPolicyValue as Raw,
    prepare_original_policy_set_frame_bytes, prepare_original_policy_set_point_fresh,
    prepare_original_policy_set_request, restore_original_policy_set_frame_header,
    scan_original_policy_set_frame,
};
use wf_domain::policy::installation::PolicyError;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::request::{PolicyRejection, RequestScope, RetainedStatus};
use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};
use wf_types::version::{AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity};

// Independently captured OTP28.3.1 optimized64LE maps.to_list order for s01..s33.
// This literal predates the new native-order implementation; ETF is key-sorted.
const HAMT33: [usize; 33] = [
    0, 2, 21, 18, 5, 22, 15, 26, 4, 20, 6, 32, 31, 27, 23, 10, 9, 19, 30, 14, 17, 1, 13, 29, 8, 24,
    3, 25, 11, 7, 16, 12, 28,
];

fn atom(name: &str) -> TestResult<Vec<u8>> {
    let mut out = vec![119, u8::try_from(name.len())?];
    out.extend_from_slice(name.as_bytes());
    Ok(out)
}

fn binary(value: &[u8]) -> TestResult<Vec<u8>> {
    let mut out = vec![109];
    out.extend_from_slice(&u32::try_from(value.len())?.to_be_bytes());
    out.extend_from_slice(value);
    Ok(out)
}

fn map(rows: &[(Vec<u8>, Vec<u8>)]) -> TestResult<Vec<u8>> {
    let mut out = vec![116];
    out.extend_from_slice(&u32::try_from(rows.len())?.to_be_bytes());
    for (key, value) in rows {
        out.extend_from_slice(key);
        out.extend_from_slice(value);
    }
    Ok(out)
}

fn policy(fields: &[(Vec<u8>, Vec<u8>)]) -> TestResult<Vec<u8>> {
    let mut out = vec![131, 104, 3];
    out.extend_from_slice(&atom("flow_policy_v1")?);
    out.extend_from_slice(&[97, 1]);
    out.extend_from_slice(&map(fields)?);
    Ok(out)
}

fn names(count: usize) -> Vec<Vec<u8>> {
    (1..=count)
        .map(|n| format!("s{n:02}").into_bytes())
        .collect()
}

fn state_policy(names: &[Vec<u8>], wrong: (usize, usize)) -> TestResult<Vec<u8>> {
    let rows = names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let config = if i == wrong.0 {
                map(&[(atom("mode")?, vec![97, 7])])?
            } else if i == wrong.1 {
                map(&[(atom("max_active_ms")?, vec![97, 1])])?
            } else {
                map(&[])?
            };
            Ok((binary(name)?, config))
        })
        .collect::<TestResult<Vec<_>>>()?;
    policy(&[
        (atom("states")?, map(&rows)?),
        (atom("type")?, binary(b"r")?),
    ])
}

fn scope() -> TestResult<RequestScope> {
    Ok(RequestScope::new(
        TenantId::new(b"t".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"z".to_vec())?,
    ))
}

fn invoke(
    current: &[u8],
    opts: Raw<'_>,
    patch_records: &[(u32, &[u32], &[u32])],
    history: Result<u32, wf_domain::workflow::batch::ApplyError>,
) -> TestResult<OriginalPolicySetFresh> {
    let profile = limits(2_000_000_000)?;
    let mut setup = PolicySchemaCharge::new(profile);
    let request =
        prepare_original_policy_set_request(&Raw::Binary(b"r"), &opts, profile, &mut setup)?
            .map_err(|_| "invalid front fixture")?;
    // Only PATCH ranks are a mathematical frame donor. No CURRENT/MERGED ranks
    // or current bytes are supplied to the new source-order reducer.
    let orders = OriginalPolicySetMapOrders::new([0; 32], None, patch_records, &[], &[]);
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    let input = OriginalPolicySetFrameInput::new(
        scope()?,
        RequestId::new(b"q".to_vec())?,
        stamp,
        &request,
        &orders,
    );
    let (bytes, _, _, _) = prepare_original_policy_set_frame_bytes(
        input,
        profile,
        AdmissionLimits::default(),
        &mut setup,
    )?;
    let scanned =
        scan_original_policy_set_frame(&bytes, profile, AdmissionLimits::default(), &mut setup)?;
    let header = restore_original_policy_set_frame_header(
        scanned,
        [0; 32],
        [0; 32],
        &orders,
        profile,
        AdmissionLimits::default(),
        &mut setup,
    )?;
    let intent = header.intent();
    let view = OriginalPolicySetFreshView {
        current: OriginalPolicyReadValue::Binary(current),
        active_migration: Ok(0),
        completed_migration: Ok(0),
        history_max_events_limit: history,
        outcome_retention_ms: Ok(1000),
    };
    let mut charge = PolicySchemaCharge::new(profile);
    let (fresh, old, _, _) = prepare_original_policy_set_point_fresh(
        view,
        header,
        (
            intent,
            LogIdentity::new(1, 7, AppliedPosition::new(0)),
            stamp.wall(),
        ),
        profile,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    assert_eq!(old, Some(current));
    Ok(fresh)
}

fn rejected(result: &OriginalPolicySetFresh, reason: Reason, body: &[u8]) {
    assert!(result.0.is_none());
    assert_eq!(
        result.1.status(),
        RetainedStatus::OriginalPolicySetRejected(reason)
    );
    assert_eq!(result.1.response(), body);
}

fn state_failure(
    current: &[u8],
    opts: Raw<'_>,
    records: &[(u32, &[u32], &[u32])],
    mode: bool,
) -> TestResult {
    let (reason, body) = if mode {
        (
            Reason::StateMode,
            b"ERR flow state mode must be parallel or fifo".as_slice(),
        )
    } else {
        (
            Reason::StateMaxActive,
            b"ERR flow max_active_ms is type-level only".as_slice(),
        )
    };
    rejected(&invoke(current, opts, records, Ok(64))?, reason, body);
    Ok(())
}

#[test]
fn binary_named_maps_cross_32_33_and_shrink() -> TestResult {
    let empty = [(0, &[][..], &[][..])];
    for count in [31, 32, 33] {
        let order = if count == 33 {
            HAMT33.to_vec()
        } else {
            (0..count).collect()
        };
        let names = names(count);
        for adjacent in order.windows(2) {
            for swap in [false, true] {
                let wrong = (adjacent[usize::from(swap)], adjacent[usize::from(!swap)]);
                state_failure(&state_policy(&names, wrong)?, Raw::List(&[]), &empty, !swap)?;
            }
        }
    }
    let pair = [
        Raw::Atom("states"),
        Raw::Map(&[(Raw::Binary(b"s33"), Raw::Map(&[]))]),
    ];
    let add = [Raw::Tuple(&pair)];
    let added = [
        (0, &[0][..], &[0][..]),
        (1, &[0][..], &[0][..]),
        (2, &[][..], &[][..]),
    ];
    state_failure(
        &state_policy(&names(32), (1, 2))?,
        Raw::List(&add),
        &added,
        false,
    )?;
    let remove = [(Raw::Binary(b"s33"), Raw::Nil)];
    let pair = [Raw::Atom("states"), Raw::Map(&remove)];
    let opts = [Raw::Tuple(&pair)];
    let removed = [(0, &[0][..], &[0][..]), (1, &[0][..], &[0][..])];
    state_failure(
        &state_policy(&names(33), (1, 2))?,
        Raw::List(&opts),
        &removed,
        true,
    )?;
    assert_update_and_list_last()
}

fn assert_update_and_list_last() -> TestResult {
    let changed = [(Raw::Atom("mode"), Raw::Atom("parallel"))];
    let states = [(Raw::Binary(b"s02"), Raw::Map(&changed))];
    let pair = [Raw::Atom("states"), Raw::Map(&states)];
    let rows = [Raw::Tuple(&pair)];
    let records = [
        (0, &[0][..], &[0][..]),
        (1, &[0][..], &[0][..]),
        (2, &[0][..], &[0][..]),
    ];
    state_failure(
        &state_policy(&names(32), (1, 2))?,
        Raw::List(&rows),
        &records,
        false,
    )?;

    let earlier = [(Raw::Atom("mode"), Raw::Integer(7))];
    let later = [(Raw::Atom("mode"), Raw::Atom("fifo"))];
    let pairs = [
        [Raw::Binary(b"s32"), Raw::Map(&earlier)],
        [Raw::Binary(b"s32"), Raw::Map(&later)],
    ];
    let entries = pairs
        .iter()
        .map(|pair| Raw::Tuple(pair))
        .collect::<Vec<_>>();
    let pair = [Raw::Atom("states"), Raw::List(&entries)];
    let opts = [Raw::Tuple(&pair)];
    // CURRENT31 + incoming physical2 duplicates = raw33, finalUNIQUE32.
    // Only the last s32 value survives. Native traversal must remain flat32,
    // encounter poisoned s02 before s03, and not use staging/raw-count33 HAMT.
    let current = state_policy(&names(31), (1, 2))?;
    state_failure(&current, Raw::List(&opts), &records, true)?;
    assert_last_value(Raw::List(&opts), &records)
}

fn assert_last_value(opts: Raw<'_>, records: &[(u32, &[u32], &[u32])]) -> TestResult {
    let empty = [(atom("states")?, map(&[])?), (atom("type")?, binary(b"r")?)];
    let current = policy(&empty)?;
    let result = invoke(&current, opts, records, Ok(64))?;
    assert_eq!(result.1.status(), RetainedStatus::Accepted);
    let fifo = map(&[(atom("mode")?, atom("fifo")?)])?;
    let states = map(&[(binary(b"s32")?, fifo)])?;
    let mut expected = policy(&[(atom("states")?, states), (atom("type")?, binary(b"r")?)])?;
    expected[20] = 2;
    let install = result.0.ok_or("last-value install")?;
    assert_eq!(install.encoded(), expected);
    assert_eq!(result.1.response(), expected);
    Ok(())
}

fn rule_policy(
    kind: Rule,
    count: usize,
    wrong: (usize, usize),
    mixed: bool,
) -> TestResult<Vec<u8>> {
    let valid = match kind {
        Rule::Limits => map(&[(atom("limit")?, vec![97, 1])])?,
        Rule::Budgets => map(&[
            (atom("limit")?, vec![97, 1]),
            (atom("window_ms")?, vec![97, 1]),
        ])?,
        Rule::Circuits => map(&[
            (atom("failure_threshold")?, vec![97, 1]),
            (atom("open_ms")?, vec![97, 1]),
        ])?,
    };
    let rows = names(count)
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let key = if mixed && i == 0 {
                atom("nil")?
            } else {
                binary(name)?
            };
            Ok((
                key,
                if i == wrong.0 {
                    vec![97, 0]
                } else if i == wrong.1 {
                    map(&[])?
                } else {
                    valid.clone()
                },
            ))
        })
        .collect::<TestResult<Vec<_>>>()?;
    let name = match kind {
        Rule::Limits => "limits",
        Rule::Budgets => "budgets",
        Rule::Circuits => "circuits",
    };
    let governance = map(&[(atom(name)?, map(&rows)?)])?;
    policy(&[
        (atom("governance")?, governance),
        (atom("states")?, map(&[])?),
        (atom("type")?, binary(b"r")?),
    ])
}

#[test]
fn binary_hash_collisions_and_governance_named_order() -> TestResult {
    let empty = [(0, &[][..], &[][..])];
    assert_full_hash_collision_order(&empty)?;
    for kind in [Rule::Limits, Rule::Budgets, Rule::Circuits] {
        let singular = match kind {
            Rule::Limits => "limit",
            Rule::Budgets => "budget",
            Rule::Circuits => "circuit",
        };
        let body = format!("ERR flow governance {singular} rule must be a map or keyword list");
        rejected(
            &invoke(
                &rule_policy(kind, 33, (2, 1), false)?,
                Raw::List(&[]),
                &empty,
                Ok(64),
            )?,
            Reason::Governance(Gov::RuleShape(kind)),
            body.as_bytes(),
        );
        let (reason, body) = if kind == Rule::Circuits {
            (
                Gov::Positive(GovernanceField::FailureThreshold),
                b"ERR flow governance failure_threshold must be a positive integer".as_slice(),
            )
        } else {
            (
                Gov::Limit,
                b"ERR flow governance limit must be a non-negative integer".as_slice(),
            )
        };
        rejected(
            &invoke(
                &rule_policy(kind, 33, (1, 2), false)?,
                Raw::List(&[]),
                &empty,
                Ok(64),
            )?,
            Reason::Governance(reason),
            body,
        );
    }
    // Small-map binary comparison reaches the first differing byte, including
    // NUL/0xff. This is not represented as a full64-bit hash collision proof.
    let common = [b"prefix\0".to_vec(), b"prefix\xff".to_vec()];
    state_failure(
        &state_policy(&common, (0, 1))?,
        Raw::List(&[]),
        &empty,
        true,
    )
}

fn assert_full_hash_collision_order(empty: &[(u32, &[u32], &[u32])]) -> TestResult {
    // ARCH D2-FIXTURE1: exactly two modular inversions, independently forwarded
    // by Root/PERF against pinned OTP28.3.1. Both full64-bit hashes are ZERO.
    // These are not a shared nibble/prefix, birthday search, or SUT hash call.
    let first = [
        0x66, 0x95, 0xb9, 0x74, 0x1d, 0x40, 0x8e, 0xf0, 0xd2, 0x26, 0x7d, 0x0e, 0x14, 0xdc, 0x90,
        0x65,
    ];
    let second = [
        0xe6, 0x13, 0xc3, 0xec, 0x92, 0xd6, 0x9b, 0xc9, 0x62, 0xd7, 0x4a, 0x65, 0x90, 0x8a, 0xdd,
        0xcd,
    ];
    assert!(first < second);
    let mut keys = names(31);
    keys.extend([first.to_vec(), second.to_vec()]);
    // The nonzero existing s01..s31 hashes preserve their independent relative
    // order, then fullhash0's descending actual-byte tie is second,first.
    let mut order = HAMT33.into_iter().filter(|&i| i < 31).collect::<Vec<_>>();
    order.extend([32, 31]);
    assert_eq!(order.len(), 33);
    for pair in order.windows(2) {
        for swap in [false, true] {
            let wrong = if swap {
                (pair[1], pair[0])
            } else {
                (pair[0], pair[1])
            };
            state_failure(&state_policy(&keys, wrong)?, Raw::List(&[]), empty, !swap)?;
        }
    }
    Ok(())
}

#[test]
fn actual_order_is_lazy_and_never_invents_atom_identity() -> TestResult {
    let empty = [(0, &[][..], &[][..])];
    let states = map(&[(atom("nil")?, map(&[])?), (binary(b"a")?, map(&[])?)])?;
    let fields = [
        (atom("states")?, states.clone()),
        (atom("type")?, binary(b"r")?),
    ];
    let current = policy(&fields)?;
    let error = invoke(&current, Raw::List(&[]), &empty, Ok(64))
        .err()
        .ok_or("mixed map accepted")?;
    let actual = error
        .downcast::<RawError>()
        .map_err(|_| "wrong error class")?;
    assert_eq!(
        *actual,
        RawError::Replace(ApplyError::Request(CompletePolicyRequestError::Encoding(
            Error::UnsupportedSourceOrder
        )))
    );
    let expected = [Raw::Atom("expected_generation"), Raw::Integer(0)];
    let result = invoke(
        &current,
        Raw::List(&[Raw::Tuple(&expected)]),
        &empty,
        Err(wf_domain::workflow::batch::ApplyError::InvalidAuthority),
    )?;
    assert!(result.0.is_none());
    assert_eq!(
        result.1.status(),
        RetainedStatus::PolicyRejected(PolicyRejection::new(PolicyError::StaleGeneration)?)
    );
    assert_eq!(result.1.response(), b"ERR stale flow policy generation");
    let replace = [Raw::Atom("replace"), Raw::Atom("true")];
    let result = invoke(&current, Raw::List(&[Raw::Tuple(&replace)]), &empty, Ok(64))?;
    assert_eq!(result.1.status(), RetainedStatus::Accepted);
    let mut expected = policy(&[(atom("states")?, map(&[])?), (atom("type")?, binary(b"r")?)])?;
    expected[20] = 2;
    assert_eq!(result.0.ok_or("replacement install")?.encoded(), expected);
    assert_earlier_and_count(&states, &empty)
}

fn assert_earlier_and_count(states: &[u8], empty: &[(u32, &[u32], &[u32])]) -> TestResult {
    let fields = [
        (atom("max_active_ms")?, vec![97, 0]),
        (atom("states")?, states.to_vec()),
        (atom("type")?, binary(b"r")?),
    ];
    rejected(
        &invoke(&policy(&fields)?, Raw::List(&[]), empty, Ok(64))?,
        Reason::MaxActive,
        b"ERR flow max_active_ms must be between 1 and 31536000000 or infinity",
    );
    for (kind, plural) in [
        (Rule::Limits, "limits"),
        (Rule::Budgets, "budgets"),
        (Rule::Circuits, "circuits"),
    ] {
        let current = rule_policy(kind, 1001, (usize::MAX, usize::MAX), true)?;
        let body = format!("ERR flow governance {plural} policy must contain at most 1000 rules");
        rejected(
            &invoke(&current, Raw::List(&[]), empty, Ok(64))?,
            Reason::Governance(Gov::RuleCount(kind)),
            body.as_bytes(),
        );
    }
    Ok(())
}
