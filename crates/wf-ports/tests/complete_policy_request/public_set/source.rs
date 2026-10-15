//! Original strict-read, CAS, type, captured-C, content and late-migration order.

use super::{
    AdmissionLimits, ApplyError, BTreeMap, ControlState, DispatchDecision, ExistingError, GateView,
    OriginalEtfSafety, OriginalPolicySetMapOrders, PolicySchemaCharge, Raw, RawError, ReadError,
    ReadValue, RetainedStatus, TestResult, apply, digest, empty_orders, frame, hex, minimal,
    profile, retained, safety, scope, stamp, view,
};
use wf_domain::policy::complete::FlowPolicyInputRejection;
use wf_domain::policy::installation::PolicyError;

#[test]
fn public_set_parent_preserves_strict_current_cas_type_and_late_read_order() -> TestResult {
    assert_current_before_cas_and_type()?;
    assert_context_before_content_and_migrations()?;
    assert_original_writer_recipes()?;
    Ok(())
}

fn assert_current_before_cas_and_type() -> TestResult {
    let limits = profile(2_000_000_000)?;
    let namespace = scope()?;
    let outcomes = BTreeMap::new();
    let marks = BTreeMap::new();
    let gate = GateView {
        scope: &namespace,
        control: ControlState::initial(stamp(0)?),
        outcomes: &outcomes,
    };
    let orders = empty_orders();
    let guard = [Raw::Atom("expected_generation"), Raw::Integer(0)];
    let replace = [Raw::Atom("replace"), Raw::Atom("true")];
    let opts = [Raw::Tuple(&guard), Raw::Tuple(&replace)];
    let bytes = frame(Raw::List(&opts), &orders)?;
    for (value, error) in [
        (ReadValue::Other, ReadError::Corrupt),
        (ReadValue::Unavailable, ReadError::ShardUnavailable),
        (ReadValue::ReadFailed, ReadError::ReadFailed),
    ] {
        let policies = BTreeMap::from([(b"r".to_vec(), value)]);
        let mut charge = PolicySchemaCharge::new(limits);
        assert_eq!(
            apply(
                &view(gate, &policies, &marks, &marks, safety()),
                &bytes,
                &orders,
                0,
                limits,
                AdmissionLimits::default(),
                &mut charge
            )
            .err(),
            Some(RawError::Current(error))
        );
    }
    let mut wrong_type = minimal(1)?;
    *wrong_type.last_mut().ok_or("literal type")? = b'q';
    let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(&wrong_type))]);
    let mut charge = PolicySchemaCharge::new(limits);
    let plan = apply(
        &view(gate, &policies, &marks, &marks, safety()),
        &bytes,
        &orders,
        0,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    let out = retained(&plan)?;
    assert!(
        matches!(out.status(), RetainedStatus::PolicyRejected(reason)
        if reason.error() == PolicyError::StaleGeneration)
    );
    assert_eq!(out.response(), b"ERR stale flow policy generation");
    assert!(matches!(
        plan.decision(),
        DispatchDecision::Fresh((None, _))
    ));
    let unguarded = frame(Raw::List(&[Raw::Tuple(&replace)]), &orders)?;
    let mut charge = PolicySchemaCharge::new(limits);
    assert_eq!(
        apply(
            &view(gate, &policies, &marks, &marks, safety()),
            &unguarded,
            &orders,
            0,
            limits,
            AdmissionLimits::default(),
            &mut charge
        )
        .err(),
        Some(RawError::Current(ReadError::Corrupt))
    );
    Ok(())
}

fn assert_context_before_content_and_migrations() -> TestResult {
    let limits = profile(2_000_000_000)?;
    let namespace = scope()?;
    let outcomes = BTreeMap::new();
    let empty = BTreeMap::new();
    let active = BTreeMap::from([(b"r".to_vec(), Err(ApplyError::PositionConflict))]);
    let done = BTreeMap::from([(b"r".to_vec(), Err(ApplyError::AllocationFailed))]);
    let gate = GateView {
        scope: &namespace,
        control: ControlState::initial(stamp(0)?),
        outcomes: &outcomes,
    };
    let patch = hex(
        "00000039040000000000000001000000190000002f0000003900000016020000000d6d61785f6163746976655f6d730000000a010000000000",
    )?;
    assert_eq!(patch.len(), 57);
    let orders =
        OriginalPolicySetMapOrders::new(digest(&patch), None, &[(0, &[0], &[0])], &[], &[]);
    let replace = [Raw::Atom("replace"), Raw::Atom("true")];
    let invalid = [Raw::Atom("max_active_ms"), Raw::Integer(0)];
    let opts = [Raw::Tuple(&replace), Raw::Tuple(&invalid)];
    let bytes = frame(Raw::List(&opts), &orders)?;
    let mut supplied = view(gate, &empty, &active, &done, safety());
    supplied.history_max_events_limit = Err(ApplyError::InvalidAuthority);
    let mut charge = PolicySchemaCharge::new(limits);
    assert_eq!(
        apply(
            &supplied,
            &bytes,
            &orders,
            0,
            limits,
            AdmissionLimits::default(),
            &mut charge
        )
        .err(),
        Some(operational(ApplyError::InvalidAuthority))
    );
    supplied.history_max_events_limit = Ok(17);
    let mut charge = PolicySchemaCharge::new(limits);
    let plan = apply(
        &supplied,
        &bytes,
        &orders,
        0,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    let out = retained(&plan)?;
    assert_eq!(
        out.status(),
        RetainedStatus::OriginalPolicySetRejected(FlowPolicyInputRejection::MaxActive)
    );
    assert_eq!(
        out.response(),
        b"ERR flow max_active_ms must be between 1 and 31536000000 or infinity"
    );
    supplied.outcome_retention_ms = Err(ApplyError::AllocationFailed);
    let mut charge = PolicySchemaCharge::new(limits);
    assert_eq!(
        apply(
            &supplied,
            &bytes,
            &orders,
            0,
            limits,
            AdmissionLimits::default(),
            &mut charge
        )
        .err(),
        Some(operational(ApplyError::AllocationFailed))
    );
    assert_late_migrations(gate, &empty, &active, &done)
}

fn assert_late_migrations(
    gate: GateView<'_>,
    policies: &BTreeMap<Vec<u8>, ReadValue<'_>>,
    active: &BTreeMap<Vec<u8>, Result<u64, ApplyError>>,
    done: &BTreeMap<Vec<u8>, Result<u64, ApplyError>>,
) -> TestResult {
    let limits = profile(2_000_000_000)?;
    let orders = empty_orders();
    let replace = [Raw::Atom("replace"), Raw::Atom("true")];
    let bytes = frame(Raw::List(&[Raw::Tuple(&replace)]), &orders)?;
    let mut supplied = view(gate, policies, active, done, safety());
    supplied.history_max_events_limit = Ok(17);
    let mut charge = PolicySchemaCharge::new(limits);
    assert_eq!(
        apply(
            &supplied,
            &bytes,
            &orders,
            0,
            limits,
            AdmissionLimits::default(),
            &mut charge
        )
        .err(),
        Some(operational(ApplyError::PositionConflict))
    );
    let empty = BTreeMap::new();
    supplied.active_migrations = &empty;
    let mut charge = PolicySchemaCharge::new(limits);
    assert_eq!(
        apply(
            &supplied,
            &bytes,
            &orders,
            0,
            limits,
            AdmissionLimits::default(),
            &mut charge
        )
        .err(),
        Some(operational(ApplyError::AllocationFailed))
    );
    Ok(())
}

fn operational(error: ApplyError) -> RawError {
    RawError::Replace(ExistingError::Apply(error))
}

fn assert_original_writer_recipes() -> TestResult {
    // Source246–317 supplies values/modes. These are serialized coherent-view
    // components; Task readiness, simultaneous apply and public get stay required.
    let pair = [
        (b"queued".as_slice(), "fifo"),
        (b"review".as_slice(), "parallel"),
    ];
    run_state_sequence(b"generation-concurrent-1", &pair, &[0, 1])?;
    run_state_sequence(b"generation-concurrent-1", &pair, &[1, 0])?;
    let names = (1..=12).map(|n| format!("state-{n}")).collect::<Vec<_>>();
    let patches = names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            (
                name.as_bytes(),
                if (index + 1) % 2 == 0 {
                    "fifo"
                } else {
                    "parallel"
                },
            )
        })
        .collect::<Vec<_>>();
    let forward = (0..12).collect::<Vec<_>>();
    let reverse = (0..12).rev().collect::<Vec<_>>();
    run_state_sequence(b"generation-concurrent-burst-1", &patches, &forward)?;
    run_state_sequence(b"generation-concurrent-burst-1", &patches, &reverse)
}

fn run_state_sequence(kind: &[u8], patches: &[(&[u8], &str)], order: &[usize]) -> TestResult {
    let namespace = scope()?;
    let mut outcomes = BTreeMap::new();
    let mut control = ControlState::initial(stamp(0)?);
    let mut current = Vec::new();
    let mut seen = Vec::new();
    for (ordinal, &index) in order.iter().enumerate() {
        let previous = seen.clone();
        let patch = *patches.get(index).ok_or("source writer index")?;
        seen.push(patch);
        seen.sort_by_key(|&(state, _)| state);
        let gate = GateView {
            scope: &namespace,
            control,
            outcomes: &outcomes,
        };
        let (next, outcome, next_control) = state_step(
            kind,
            patch,
            &current,
            (&previous, &seen),
            gate,
            u64::try_from(ordinal)?,
        )?;
        current = next;
        control = next_control;
        let request = super::RequestId::new(format!("writer-{ordinal}").into_bytes())?;
        outcomes.insert(request, outcome);
    }
    let (expected, _) = expected_policy(kind, &seen, u64::try_from(patches.len())?)?;
    assert_eq!(current, expected);
    assert_eq!(seen.len(), patches.len());
    assert_eq!(outcomes.len(), patches.len());
    Ok(())
}

type SourceState<'a> = (&'a [u8], &'a str);
type SourceMapOrders = Vec<(u32, Vec<u32>)>;

fn state_step(
    kind: &[u8],
    patch: (&[u8], &str),
    current: &[u8],
    states: (&[SourceState<'_>], &[SourceState<'_>]),
    gate: GateView<'_>,
    ordinal: u64,
) -> TestResult<(Vec<u8>, super::RetainedOutcome, ControlState)> {
    let limits = profile(2_000_000_000)?;
    let (expected, _) = expected_policy(kind, states.1, ordinal + 1)?;
    let current_metadata = if current.is_empty() {
        Vec::new()
    } else {
        let (before, metadata) = expected_policy(kind, states.0, ordinal)?;
        assert_eq!(current, before);
        metadata
    };
    let records = current_metadata
        .iter()
        .map(|(id, rows)| (*id, rows.as_slice(), rows.as_slice()))
        .collect::<Vec<_>>();
    let canonical = canonical_state_patch(patch.0, patch.1)?;
    let root_rows = if current.is_empty() {
        vec![0_u32]
    } else {
        vec![0_u32, 1]
    };
    let (state_forward, state_inverse) = merged_state_order(states.0, patch, states.1)?;
    let merged = [
        (0, root_rows.as_slice(), root_rows.as_slice()),
        (1, state_forward.as_slice(), state_inverse.as_slice()),
    ];
    let orders = OriginalPolicySetMapOrders::new(
        digest(&canonical),
        (!current.is_empty()).then_some(current),
        &[(0, &[0], &[0]), (1, &[0], &[0]), (2, &[0], &[0])],
        &records,
        &merged,
    );
    let mode = [Raw::Atom("mode"), Raw::Atom(patch.1)];
    let mode_opts = [Raw::Tuple(&mode)];
    let state_patch = [(Raw::Binary(patch.0), Raw::List(&mode_opts))];
    let state_pair = [Raw::Atom("states"), Raw::Map(&state_patch)];
    let opts = [Raw::Tuple(&state_pair)];
    let request = format!("writer-{ordinal}").into_bytes();
    let bytes = super::frame_for_type(kind, Raw::List(&opts), &orders, &request, ordinal)?;
    let policies = if current.is_empty() {
        BTreeMap::new()
    } else {
        BTreeMap::from([(kind.to_vec(), ReadValue::Binary(current))])
    };
    let marks = BTreeMap::new();
    let mut charge = PolicySchemaCharge::new(limits);
    let plan = apply(
        &view(gate, &policies, &marks, &marks, sequence_safety()),
        &bytes,
        &orders,
        ordinal,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    let (_, _, control, decision) = plan.into_parts();
    let DispatchDecision::Fresh((Some(install), outcome)) = decision else {
        return Err("source writer must install its merged state".into());
    };
    let mut key = b"f:{f}:policy:".to_vec();
    key.extend_from_slice(kind);
    assert_eq!(install.policy_key(), key);
    assert_eq!(install.stored_generation(), ordinal);
    assert_eq!(install.generation(), ordinal + 1);
    assert_eq!(install.encoded(), expected);
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    assert_eq!(outcome.response(), expected);
    assert_ne!(install.encoded().as_ptr(), outcome.response().as_ptr());
    Ok((install.encoded().to_vec(), outcome, control.next()))
}

fn merged_state_order(
    before: &[(&[u8], &str)],
    patch: (&[u8], &str),
    after: &[(&[u8], &str)],
) -> TestResult<(Vec<u32>, Vec<u32>)> {
    // OTP small binary-key maps traverse sorted names. Overlay logical rows
    // preserve the old canonical rows, followed by this newly inserted key.
    let logical = before
        .iter()
        .copied()
        .chain(std::iter::once(patch))
        .collect::<Vec<_>>();
    let mut forward = Vec::with_capacity(after.len());
    let mut inverse = vec![0; after.len()];
    for (source, &(name, _)) in after.iter().enumerate() {
        let rank = logical
            .iter()
            .position(|&(prior, _)| prior == name)
            .ok_or("source state rank")?;
        forward.push(u32::try_from(rank)?);
        inverse[rank] = u32::try_from(source)?;
    }
    Ok((forward, inverse))
}

const fn sequence_safety() -> OriginalEtfSafety<'static> {
    OriginalEtfSafety::new(
        &[
            "flow_policy_v1",
            "type",
            "states",
            "mode",
            "fifo",
            "parallel",
        ],
        &[],
        None,
        &[],
    )
}

fn expected_policy(
    kind: &[u8],
    states: &[(&[u8], &str)],
    generation: u64,
) -> TestResult<(Vec<u8>, SourceMapOrders)> {
    let configs = states
        .iter()
        .map(|&(_, mode)| [(Raw::Atom("mode"), Raw::Atom(mode))])
        .collect::<Vec<_>>();
    let rows = states
        .iter()
        .zip(&configs)
        .map(|(&(name, _), config)| (Raw::Binary(name), Raw::Map(config)))
        .collect::<Vec<_>>();
    let policy = [
        (Raw::Atom("states"), Raw::Map(&rows)),
        (Raw::Atom("type"), Raw::Binary(kind)),
    ];
    let mut bytes = vec![131, 104, 3];
    let mut next_span = 1;
    let mut maps = Vec::new();
    expected_term(
        Raw::Atom("flow_policy_v1"),
        &mut bytes,
        &mut next_span,
        &mut maps,
    )?;
    expected_term(
        Raw::Integer(i128::from(generation)),
        &mut bytes,
        &mut next_span,
        &mut maps,
    )?;
    expected_term(Raw::Map(&policy), &mut bytes, &mut next_span, &mut maps)?;
    Ok((bytes, maps))
}

fn expected_term(
    value: Raw<'_>,
    bytes: &mut Vec<u8>,
    next: &mut u32,
    maps: &mut SourceMapOrders,
) -> TestResult {
    let id = *next;
    *next += 1;
    match value {
        Raw::Atom(name) => {
            bytes.extend_from_slice(&[119, u8::try_from(name.len())?]);
            bytes.extend_from_slice(name.as_bytes());
        }
        Raw::Binary(value) => {
            bytes.push(109);
            bytes.extend_from_slice(&u32::try_from(value.len())?.to_be_bytes());
            bytes.extend_from_slice(value);
        }
        Raw::Integer(number) => bytes.extend_from_slice(&[97, u8::try_from(number)?]),
        Raw::Map(rows) => {
            maps.push((id, (0..u32::try_from(rows.len())?).collect()));
            bytes.push(116);
            bytes.extend_from_slice(&u32::try_from(rows.len())?.to_be_bytes());
            for &(key, value) in rows {
                expected_term(key, bytes, next, maps)?;
                expected_term(value, bytes, next, maps)?;
            }
        }
        _ => return Err("unsupported independent source fixture".into()),
    }
    Ok(())
}

fn canonical_state_patch(state: &[u8], mode: &str) -> TestResult<Vec<u8>> {
    let config = canonical_pair(
        2,
        &canonical_text(2, b"mode")?,
        &canonical_text(2, mode.as_bytes())?,
    )?;
    let states = canonical_pair(1, &canonical_text(3, state)?, &config)?;
    canonical_pair(0, &canonical_text(2, b"states")?, &states)
}

fn canonical_text(tag: u8, bytes: &[u8]) -> TestResult<Vec<u8>> {
    let mut out = u32::try_from(9 + bytes.len())?.to_be_bytes().to_vec();
    out.push(tag);
    out.extend_from_slice(&u32::try_from(bytes.len())?.to_be_bytes());
    out.extend_from_slice(bytes);
    Ok(out)
}

fn canonical_pair(id: u32, key: &[u8], value: &[u8]) -> TestResult<Vec<u8>> {
    let length = 25 + key.len() + value.len();
    let mut out = u32::try_from(length)?.to_be_bytes().to_vec();
    out.push(4);
    out.extend_from_slice(&id.to_be_bytes());
    out.extend_from_slice(&1_u32.to_be_bytes());
    for offset in [25, 25 + key.len(), length] {
        out.extend_from_slice(&u32::try_from(offset)?.to_be_bytes());
    }
    out.extend_from_slice(key);
    out.extend_from_slice(value);
    Ok(out)
}
