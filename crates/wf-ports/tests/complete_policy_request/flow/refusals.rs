//! Independently frozen parent primitives, late source caps and continuous owners.

use super::{
    AdmissionLimits, ApplyError, BTreeMap, ControlState, EncodingError, ExistingError, Gov, Input,
    KEY, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits, Raw, RawError, ReadError,
    ReadValue, Reason, Rejection, RetainedStatus, TestResult, accepted, apply, current, expected,
    frame, frame_with, k1_safety, profile, rejected, request, retained, safety, scope, stamp, view,
};
use wf_domain::policy::complete::{
    normalize_original_complete_policy_input, restore_complete_policy,
};
use wf_domain::workflow::request::RequestValueError;
use wf_domain::workflow::request::gate::GateView;
use wf_ports::log::prepare_original_flow_policy_replace_frame;
use wf_types::identity::RequestId;

pub(super) fn hex(value: &str) -> TestResult<Vec<u8>> {
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}

fn verifier_events() -> Vec<(u64, u64, bool)> {
    let mut raw = vec![
        128, 4, 2, 1, 128, 4, 1, 128, 4, 1, 128, 4, 1, 128, 4, 1, 2, 32, 8, 2, 8, 4, 4, 128, 4, 4,
        4, 128, 4, 4, 128, 35, 128, 4, 1, 128, 4, 14, 128, 4, 1, 128, 4, 1, 4, 4, 4,
    ];
    assert_eq!(raw.iter().sum::<u64>(), 1_723);
    raw.extend_from_slice(&[
        130, 208, 192, 128, 128, 129, 129, 129, 129, 128, 533, 149, 533, 315,
    ]);
    assert_eq!(raw.iter().sum::<u64>(), 4_683);
    raw.into_iter().map(|fee| (fee, 0, false)).collect()
}

fn reader_events() -> Vec<(u64, u64, bool)> {
    // Actual source lexical fields/blocks, independently reached in BOTH passes.
    let lex = [
        1, 128, 1, 1, 128, 1, 1, 14, 156, 128, 1, 1, 128, 1, 4, 128, 1, 1, 4, 146, 136, 128, 1, 4,
        1,
    ];
    assert_eq!(lex.iter().sum::<u64>(), 1_244);
    let mut events = [128, 1, 1, 128, 142, 132].map(|g| (g, 0, true)).to_vec();
    events.extend(lex.map(|g| (g, 0, true)));
    events.extend_from_slice(&[
        (0, 448, true),
        (320, 0, true),
        (132, 0, true),
        (128, 0, true),
    ]);
    events.extend(lex.map(|g| (g, 0, true)));
    events.extend([128, 128, 128, 156, 128, 128].map(|g| (g, 0, true)));
    events.extend_from_slice(&[
        (128, 0, true),
        (128, 0, true),
        (128, 0, true),
        (4, 0, true),
        (137, 0, true),
        (138, 0, true),
        (144, 0, true),
        (128, 0, true),
        (0, 128, true),
        (128, 0, true),
        (132, 0, true),
        (137, 0, true),
        (128, 0, true),
        (132, 0, true),
        (138, 0, true),
    ]);
    events.extend([128, 132, 136, 128, 128].map(|g| (g, 0, true)));
    assert_eq!(events.iter().map(|e| e.0).sum::<u64>(), 6_778);
    events
}

fn fresh_events(present: bool) -> Vec<(u64, u64, bool)> {
    let mut events = [128, 128, 142, 129].map(|g| (g, 0, false)).to_vec();
    if present {
        events.extend(reader_events());
    }
    events.push((128, 0, false)); // Raw Nil CAS is still a reached guard.
    if present {
        events.push((130, 0, true));
    }
    events.extend([128, 128, 128, 1, 128].map(|g| (g, 0, false)));
    events.push((0, 4_096, false));
    events.extend([3_200, 896, 192].map(|g| (g, 0, false)));
    //15 source semantic roots/selectors +5 cells/4 paired edges, separately paid.
    events.extend(std::iter::repeat_n((128, 0, false), 24));
    events.extend([1, 133, 136, 133, 134, 134].map(|g| (g, 0, false)));
    events.extend([129, 129, 128, 179].map(|g| (g, 0, false)));
    events.extend([1, 130, 144, 130, 133, 136, 133, 134, 134].map(|g| (g, 0, false)));
    events.extend([142, 128, 128, 128, 128, 179, 128].map(|g| (g, 0, false)));
    assert_eq!(
        events.iter().map(|e| e.0).sum::<u64>(),
        if present { 18_708 } else { 11_800 }
    );
    events
}

fn events(present: bool) -> Vec<(u64, u64, bool)> {
    let mut events = verifier_events();
    events.extend([128, 134, 128, 129].map(|g| (g, 0, false))); // Existing Fresh gate519.
    events.extend(fresh_events(present));
    assert_eq!(
        events.iter().map(|e| e.0).sum::<u64>(),
        if present { 23_910 } else { 17_002 }
    );
    events
}

fn work(target: u64, maximum: u64, current: bool) -> RawError {
    let error = EncodingError::Schema(PolicySchemaError::WorkBytes {
        bytes: target,
        maximum,
    });
    if current {
        RawError::Current(ReadError::Encoding(error))
    } else {
        request(error)
    }
}

pub(super) fn assert_parent_events() -> TestResult {
    assert_events(false)?;
    Ok(())
}

pub(super) fn assert_present_current() -> TestResult {
    assert_events(true)?;
    Ok(())
}

fn assert_events(present: bool) -> TestResult {
    let scope = scope()?;
    let outcomes = BTreeMap::new();
    let marks = BTreeMap::new();
    let raw = current(1, b"r")?;
    let policies = if present {
        BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(raw.as_slice()))])
    } else {
        BTreeMap::new()
    };
    let gate = GateView {
        scope: &scope,
        control: ControlState::initial(stamp(0)?),
        outcomes: &outcomes,
    };
    let bytes = frame(Raw::Nil, Raw::Map(&[]))?;
    let untouched = bytes.clone();
    let (mut g, mut p) = (0_u64, 0_u64);
    for (delta, peak, current) in events(present) {
        let next_g = g + delta;
        let next_p = p.max(peak);
        let target = next_g + next_p;
        if target > g + p {
            let limits = profile(target - 1)?;
            let mut charge = PolicySchemaCharge::new(limits);
            assert_eq!(
                apply(
                    &view(gate, &policies, &marks, &marks, k1_safety()),
                    &bytes,
                    0,
                    limits,
                    AdmissionLimits::default(),
                    &mut charge
                )
                .err(),
                Some(work(target, target - 1, current))
            );
            assert_eq!((charge.g(), charge.p()), (g, p));
            assert_eq!(bytes, untouched);
            assert!(outcomes.is_empty());
        }
        (g, p) = (next_g, next_p);
    }
    assert_eq!(
        (g, p),
        if present {
            (23_910, 4_096)
        } else {
            (17_002, 4_096)
        }
    );
    let limits = profile(g + p)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let final_bytes = expected(if present { 2 } else { 1 }, b"r", &[])?;
    let plan = apply(
        &view(gate, &policies, &marks, &marks, k1_safety()),
        &bytes,
        0,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    accepted(
        &plan,
        u64::from(present),
        if present { 2 } else { 1 },
        KEY,
        &final_bytes,
    )?;
    assert_eq!((charge.g(), charge.p(), charge.total()), (g, p, g + p));
    Ok(())
}

pub(super) fn assert_aggregate_and_expiry() -> TestResult {
    let limits = profile(2_000_000_000)?;
    let scope = scope()?;
    let outcomes = BTreeMap::new();
    let empty = BTreeMap::new();
    let marks = BTreeMap::new();
    let gate = GateView {
        scope: &scope,
        control: ControlState::initial(stamp(0)?),
        outcomes: &outcomes,
    };
    let suffix = vec![b'r'; 100];
    let mut key = b"f:{f}:policy:".to_vec();
    key.extend_from_slice(&suffix);
    let bytes = frame_with(&key, Raw::Nil, Raw::Map(&[]), b"q", 0)?;
    let final_bytes = expected(1, &suffix, &[])?;
    assert_eq!((bytes.len(), key.len(), final_bytes.len()), (286, 113, 150));
    assert_eq!(key.len() + 2 * final_bytes.len() + 4, 417);
    for cap in [416, 417] {
        let mut charge = PolicySchemaCharge::new(limits);
        let result = apply(
            &view(gate, &empty, &marks, &marks, safety()),
            &bytes,
            0,
            limits,
            AdmissionLimits::new(1_048_576, cap, 1_000)?,
            &mut charge,
        );
        if cap == 416 {
            assert_eq!(result.err(), Some(request(EncodingError::ByteLimit)));
            assert_eq!((charge.g(), charge.p()), (17_755, 4_096));
        } else {
            accepted(&result?, 0, 1, &key, &final_bytes)?;
            assert_eq!((charge.g(), charge.p()), (18_289, 4_096));
        }
    }
    for (time, window) in [
        (0, Ok(0)),
        (1, Ok(u64::MAX)),
        (0, Err(ApplyError::PositionConflict)),
    ] {
        let bytes = frame_with(KEY, Raw::Nil, Raw::Map(&[]), b"q", time)?;
        let mut supplied = view(gate, &empty, &marks, &marks, safety());
        supplied.outcome_retention_ms = window;
        let mut charge = PolicySchemaCharge::new(limits);
        let expected = match window {
            Err(e) => RawError::Replace(ExistingError::Apply(e)),
            Ok(_) => RawError::Replace(ExistingError::Outcome(RequestValueError::InvalidExpiry)),
        };
        assert_eq!(
            apply(
                &supplied,
                &bytes,
                0,
                limits,
                AdmissionLimits::default(),
                &mut charge
            )
            .err(),
            Some(expected)
        );
    }
    Ok(())
}

pub(super) fn assert_continuous() -> TestResult {
    assert_held(false)?;
    assert_held(true)?;
    assert_small_seed()?;
    assert_producer_peak()?;
    Ok(())
}

fn assert_held(seeded: bool) -> TestResult {
    let seed_g = if seeded { 13_726 } else { 0 };
    let peak = if seeded { 6_912 } else { 4_096 };
    let target = seed_g + 3 * 17_002 + peak;
    let limits = profile(target - 1)?;
    let mut charge = PolicySchemaCharge::new(limits);
    // This actual OLD snapshot Other input is NOT transported by the new frame.
    let leaves = [Raw::Nil, Raw::Nil];
    let unknown = [(Raw::Other, Raw::List(&leaves))];
    let seed = if seeded {
        Some(
            normalize_original_complete_policy_input(
                &Raw::Binary(b"r"),
                &Raw::Map(&unknown),
                17,
                limits,
                &mut charge,
            )?
            .map_err(|r| format!("seed {r:?}"))?,
        )
    } else {
        None
    };
    if seeded {
        assert_eq!((charge.g(), charge.p()), (13_726, 6_912));
    }
    let scope = scope()?;
    let outcomes = BTreeMap::new();
    let empty = BTreeMap::new();
    let marks = BTreeMap::new();
    let gate = GateView {
        scope: &scope,
        control: ControlState::initial(stamp(0)?),
        outcomes: &outcomes,
    };
    let bytes = frame(Raw::Nil, Raw::Map(&[]))?;
    let final_bytes = expected(1, b"r", &[])?;
    let supplied = view(gate, &empty, &marks, &marks, safety());
    let first = apply(
        &supplied,
        &bytes,
        0,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    let first_pointer = retained(&first)?.response().as_ptr();
    accepted(&first, 0, 1, KEY, &final_bytes)?;
    assert_eq!((charge.g(), charge.p()), (seed_g + 17_002, peak));
    let second = apply(
        &supplied,
        &bytes,
        0,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    accepted(&first, 0, 1, KEY, &final_bytes)?;
    accepted(&second, 0, 1, KEY, &final_bytes)?;
    assert_ne!(first_pointer, retained(&second)?.response().as_ptr());
    assert_eq!((charge.g(), charge.p()), (seed_g + 34_004, peak));
    assert_eq!(
        apply(
            &supplied,
            &bytes,
            0,
            limits,
            AdmissionLimits::default(),
            &mut charge
        )
        .err(),
        Some(work(target, target - 1, false))
    );
    assert_eq!((charge.g(), charge.p()), (seed_g + 50_878, peak));
    accepted(&first, 0, 1, KEY, &final_bytes)?;
    accepted(&second, 0, 1, KEY, &final_bytes)?;
    assert_eq!(retained(&first)?.response().as_ptr(), first_pointer);
    if let Some(seed) = seed {
        let mut snapshot = vec![131];
        snapshot.extend_from_slice(&super::cases::policy(b"r", &[])?);
        assert_eq!(seed, snapshot);
    }
    assert!(matches!(leaves, [Raw::Nil, Raw::Nil]));
    Ok(())
}

fn assert_small_seed() -> TestResult {
    let limits = profile(12_393)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let mut data = super::super::minimal_data();
    data.indexed_attributes = Some(vec![b"x".to_vec()]);
    let seed = restore_complete_policy(1, data, limits, &mut charge)?;
    assert_eq!((charge.g(), charge.p()), (1_928, 64));
    let scope = scope()?;
    let outcomes = BTreeMap::new();
    let empty = BTreeMap::new();
    let marks = BTreeMap::new();
    let gate = GateView {
        scope: &scope,
        control: ControlState::initial(stamp(0)?),
        outcomes: &outcomes,
    };
    let bytes = frame(Raw::Nil, Raw::Map(&[]))?;
    assert_eq!(
        apply(
            &view(gate, &empty, &marks, &marks, safety()),
            &bytes,
            0,
            limits,
            AdmissionLimits::default(),
            &mut charge
        )
        .err(),
        Some(work(12_394, 12_393, false))
    );
    assert_eq!((charge.g(), charge.p()), (8_298, 64));
    assert_eq!(seed.data().indexed_attributes, Some(vec![b"x".to_vec()]));
    Ok(())
}

fn assert_producer_peak() -> TestResult {
    let limits = profile(16_938)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let input = Input::new(
        scope()?,
        RequestId::new(b"q".to_vec())?,
        stamp(0)?,
        KEY,
        Raw::Nil,
        Raw::Map(&[]),
    );
    let bytes = prepare_original_flow_policy_replace_frame(
        input,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    assert_eq!((charge.g(), charge.p()), (6_473, 272));
    let scope = scope()?;
    let outcomes = BTreeMap::new();
    let empty = BTreeMap::new();
    let marks = BTreeMap::new();
    let gate = GateView {
        scope: &scope,
        control: ControlState::initial(stamp(0)?),
        outcomes: &outcomes,
    };
    let copy = bytes.as_bytes().to_vec();
    let pointer = bytes.as_bytes().as_ptr();
    assert_eq!(
        apply(
            &view(gate, &empty, &marks, &marks, safety()),
            bytes.as_bytes(),
            0,
            limits,
            AdmissionLimits::default(),
            &mut charge
        )
        .err(),
        Some(work(16_939, 16_938, false))
    );
    assert_eq!((charge.g(), charge.p()), (12_843, 272));
    assert_eq!(bytes.as_bytes(), copy);
    assert_eq!(bytes.as_bytes().as_ptr(), pointer);
    Ok(())
}

pub(super) fn assert_large_source_errors(
    gate: GateView<'_>,
    limits: PolicySchemaLimits,
) -> TestResult {
    let names = (1..=600)
        .map(|i| format!("{}{i}", "e".repeat(2_000)).into_bytes())
        .collect::<Vec<_>>();
    assert_eq!(names.iter().map(Vec::len).sum::<usize>(), 1_201_692);
    let values = names
        .iter()
        .map(|name| Raw::Binary(name))
        .collect::<Vec<_>>();
    let effects = [(Raw::Atom("allowed"), Raw::List(&values))];
    let governance = [(Raw::Atom("effects"), Raw::Map(&effects))];
    let empty = BTreeMap::new();
    let failed = BTreeMap::from([(b"r".to_vec(), Err(ApplyError::PositionConflict))]);
    for poison_states in [false, true] {
        let mut patch = vec![(Raw::Atom("governance"), Raw::Map(&governance))];
        if poison_states {
            patch.push((Raw::Atom("states"), Raw::Integer(7)));
        }
        let bytes = frame(Raw::Nil, Raw::Map(&patch))?;
        assert!(bytes.len() > 1_048_576);
        assert!(bytes.len() < 16 * 1_024 * 1_024);
        let mut charge = PolicySchemaCharge::new(limits);
        let plan = apply(
            &view(gate, &empty, &failed, &failed, safety()),
            &bytes,
            0,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        rejected(
            &plan,
            RetainedStatus::OriginalFlowPolicyReplaceRejected(Rejection::Input(
                Reason::Governance(Gov::PolicyBytes),
            )),
            b"ERR flow governance policy exceeds 1048576-byte durable limit",
            &bytes,
        )?;
    }
    assert_snapshot_caps(gate, limits)?;
    Ok(())
}

fn assert_snapshot_caps(gate: GateView<'_>, limits: PolicySchemaLimits) -> TestResult {
    let empty = BTreeMap::new();
    let marks = BTreeMap::new();
    let failed = BTreeMap::from([(b"r".to_vec(), Err(ApplyError::PositionConflict))]);
    let version = vec![b'x'; 262_144];
    for bad_states in [false, true] {
        let mut rows = vec![(Raw::Atom("version"), Raw::Binary(&version))];
        if bad_states {
            rows.insert(0, (Raw::Atom("states"), Raw::Nil));
        }
        let bytes = frame(Raw::Nil, Raw::Map(&rows))?;
        let mut charge = PolicySchemaCharge::new(limits);
        let plan = apply(
            &view(gate, &empty, &failed, &failed, safety()),
            &bytes,
            0,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        let (reason, body) = if bad_states {
            (
                Reason::StatesShape,
                b"ERR flow policy states must be a map or keyword list".as_slice(),
            )
        } else {
            (
                Reason::SnapshotBytes,
                b"ERR flow policy snapshot exceeds 262144 bytes".as_slice(),
            )
        };
        rejected(
            &plan,
            RetainedStatus::OriginalFlowPolicyReplaceRejected(Rejection::Input(reason)),
            body,
            &bytes,
        )?;
    }
    let key = b"f:{f}:policy:boundary";
    for width in [262_092, 262_093] {
        let version = vec![b'x'; width];
        let rows = [(Raw::Atom("version"), Raw::Binary(&version))];
        let bytes = frame_with(key, Raw::Nil, Raw::Map(&rows), b"q", 0)?;
        let final_bytes = expected(
            1,
            b"boundary",
            &[("version", super::cases::binary(&version)?)],
        )?;
        assert_eq!(final_bytes.len(), width + 72);
        let mut charge = PolicySchemaCharge::new(limits);
        let plan = apply(
            &view(gate, &empty, &marks, &marks, safety()),
            &bytes,
            0,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        if width == 262_092 {
            accepted(&plan, 0, 1, key, &final_bytes)?;
        } else {
            rejected(
                &plan,
                RetainedStatus::OriginalFlowPolicyReplaceRejected(Rejection::Input(
                    Reason::SnapshotBytes,
                )),
                b"ERR flow policy snapshot exceeds 262144 bytes",
                &bytes,
            )?;
        }
    }
    Ok(())
}
