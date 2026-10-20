//! Exact actual ETF prefix, original head ID/empty capture and true RangeEnd.
use super::super::{TestResult, fixtures as f};
use std::path::Path;
use wf_domain::policy::complete::{
    OriginalPolicyReadValue as Read, OriginalPolicySetFreshView, PolicySchemaCharge,
    RawFlowPolicyValue as Raw,
};
use wf_domain::workflow::control::ControlState;
use wf_domain::workflow::request::{RetainedOutcome, RetainedStatus, gate::PointGateView};
use wf_ports::log::{
    OriginalPolicySetPointInspection, OriginalPolicySetRequestPlan,
    decide_original_policy_set_point_fresh_bounded,
    inspect_original_policy_set_point_frame_bounded,
    prepare_original_policy_set_point_frame_bounded,
};
use wf_ports::state::{
    GroupApplyStore, GroupApplyTransaction, GroupOriginalPolicyChanges,
    GroupOriginalPolicyTransaction, StateTransactionError,
};
use wf_storage::transaction::RecordBatchTransactionOutcome;
use wf_types::identity::RequestId;

#[test]
fn original_reindex_original_head_and_true_end_are_paid() -> TestResult {
    let dir = f::directory("original-consumed-prefix")?;
    let ample = f::limits();
    let s = f::scope()?;
    for fifo in [true, false] {
        let path = dir.join(if fifo {
            "original-some"
        } else {
            "original-none"
        });
        let before = fixture(&path, fifo)?;
        let mode = if fifo { "parallel" } else { "fifo" };
        let config = [(Raw::Atom("mode"), Raw::Atom(mode))];
        let states = [(Raw::Binary(b"q"), Raw::Map(&config))];
        let frame = f::public_frame(
            &s,
            &RequestId::new(b"q".to_vec())?,
            25,
            b"job",
            Raw::Map(&[(Raw::Atom("states"), Raw::Map(&states))]),
            Raw::List(&[]),
            &mut PolicySchemaCharge::new(ample.schema),
        )?;
        let (exact_c, end_steps, decoder) = prefix_limits(fifo);
        for (low, high) in [
            (
                (ample.maximum_read_steps, exact_c - 1),
                (ample.maximum_read_steps, exact_c),
            ),
            (
                (end_steps - 1, ample.maximum_adapter_bytes),
                (end_steps, ample.maximum_adapter_bytes),
            ),
        ] {
            let lo = commit_prefix(&path, &frame, fifo, low, false)?;
            let hi = commit_prefix(&path, &frame, fifo, high, false)?;
            assert!(
                lo.0 > 0 && lo.1 >= 1024,
                "existing validation/held peak did not run"
            );
            assert_eq!(hi, (lo.0 + decoder, lo.1));
            assert_eq!(f::cohort(&path)?, before);
        }
        commit_prefix(
            &path,
            &frame,
            fifo,
            (ample.maximum_read_steps, ample.maximum_adapter_bytes),
            true,
        )?;
        assert_eq!(f::cohort(&path)?, expected(&before, &frame, fifo)?);
        drop(f::open(&path, 64)?);
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn raw_policy(generation: u64, fifo: bool) -> TestResult<Vec<u8>> {
    let config = f::etf_map(&[(
        f::atom(b"mode")?,
        f::atom(if fifo { b"fifo" } else { b"parallel" })?,
    )])?;
    let states = f::etf_map(&[(f::binary(b"q")?, config)])?;
    f::etf_policy(generation, b"job", &[(b"states", &states)])
}

fn fixture(path: &Path, fifo: bool) -> TestResult<f::Cohort> {
    let s = f::scope()?;
    let caps = f::limits();
    let mut store = f::create(path, &[], 64)?;
    let mode = if fifo { "fifo" } else { "parallel" };
    let config = [(Raw::Atom("mode"), Raw::Atom(mode))];
    let states = [(Raw::Binary(b"q"), Raw::Map(&config))];
    let frame = f::public_frame(
        &s,
        &RequestId::new(b"initial".to_vec())?,
        0,
        b"job",
        Raw::Map(&[(Raw::Atom("states"), Raw::Map(&states))]),
        Raw::List(&[]),
        &mut PolicySchemaCharge::new(caps.schema),
    )?;
    let raw = raw_policy(1, fifo)?;
    assert_eq!(raw.len(), if fifo { 76 } else { 80 });
    let metadata = f::metadata(
        &s,
        b"initial",
        0,
        0,
        f::frame_intent(&frame, 0)?,
        RetainedStatus::Accepted,
    )?;
    let RecordBatchTransactionOutcome::Retained { outcome, .. } = f::apply(
        &mut store,
        &frame,
        0,
        caps,
        &mut PolicySchemaCharge::new(caps.schema),
    )?
    else {
        return Err("real live6 fixture did not retain".into());
    };
    assert_eq!(
        *outcome,
        RetainedOutcome::new(metadata, 1, raw.clone(), caps.codec.admission())?
    );
    let owner = f::owner(
        &s,
        b"p",
        f::data(b"i", b"job", b"q", 0, Some(0))?,
        0,
        1_u128 << 32,
    )?;
    f::seed(&mut store, &s, std::slice::from_ref(&owner), 1, 0)?;
    drop(store);
    let before = f::cohort(path)?;
    let rows = f::rows(&before)?;
    assert_eq!(
        rows.get(&f::point(&s, &owner)),
        Some(&f::record_wire(&s, &owner)?)
    );
    assert_eq!(
        rows.get(&f::member(&s, &owner)).map(Vec::as_slice),
        if fifo { Some([].as_slice()) } else { None }
    );
    assert_eq!(rows.get(&f::due(&s, &owner)?), Some(&vec![u8::from(fifo)]));
    assert_eq!(
        rows.get(&f::key(6, &[b"g", b"z", b"t", b"job"])),
        Some(&raw)
    );
    let (key, history) = f::history_wire(&s, &owner, &f::history(&owner))?;
    assert_eq!(rows.get(&key), Some(&history));
    assert_eq!(
        (
            f::point(&s, &owner).len(),
            f::record_wire(&s, &owner)?.len(),
            f::member(&s, &owner).len(),
            f::due(&s, &owner)?.len()
        ),
        (22, 131, 47, 39)
    );
    assert_eq!(
        &before[0].as_ref().ok_or("no metadata")?[b"manifest".as_slice()][..6],
        b"FWSS\0\x06"
    );
    // Independent returned/read ownership through End (including both grammar
    // key owners, full raw+temporary X, placement, canonical and actual heads).
    let r = 512
        + (512 + raw.len())
        + (65_536 + 256 * raw.len())
        + 278
        + 519
        + 513
        + 277
        + 269
        + if fifo { 2 * 519 } else { 0 };
    assert_eq!(r, if fifo { 88_986 } else { 88_976 });
    drop(f::open(path, 64)?);
    Ok(before)
}

fn prepare_plan(
    current: ControlState,
    frame: &[u8],
    old: &[u8],
    charge: &mut PolicySchemaCharge,
) -> TestResult<OriginalPolicySetRequestPlan<'static>> {
    let caps = f::limits();
    let s = f::scope()?;
    let verified = prepare_original_policy_set_point_frame_bounded(
        frame,
        caps.schema,
        caps.codec.admission(),
        charge,
        caps.workspace_bytes,
    )?;
    let inspected = inspect_original_policy_set_point_frame_bounded(
        PointGateView {
            scope: &s,
            control: current,
            retained_nonempty: true,
            outcome: None,
        },
        verified,
        f::log(2),
        caps.schema,
        caps.codec.admission(),
        charge,
        caps.workspace_bytes,
    )?;
    let OriginalPolicySetPointInspection::Fresh(fresh) = inspected else {
        return Err("consumed prefix was not Fresh".into());
    };
    Ok(decide_original_policy_set_point_fresh_bounded(
        *fresh,
        OriginalPolicySetFreshView {
            current: Read::Binary(old),
            active_migration: Ok(0),
            completed_migration: Ok(0),
            history_max_events_limit: Ok(64),
            outcome_retention_ms: Ok(1000),
        },
        caps.schema,
        caps.codec.admission(),
        charge,
        caps.workspace_bytes,
    )?)
}

fn commit_prefix(
    path: &Path,
    frame: &[u8],
    fifo: bool,
    caps: (usize, usize),
    accept: bool,
) -> TestResult<(u64, u64)> {
    let s = f::scope()?;
    let before = f::cohort(path)?;
    let mut store = f::open(path, 64)?;
    let ample = f::limits();
    // Actual preparation has its own completed C/step frontier, but ONE schema
    // account continues through constructor and the newly bounded consumed tx.
    let current = {
        let tx = store.begin_apply(
            s.group(),
            s.recovery(),
            ample.codec.admission(),
            ample.maximum_read_steps,
            ample.maximum_read_bytes,
            ample.maximum_adapter_bytes,
        )?;
        assert!(
            tx.outcome(s.tenant(), &RequestId::new(b"q".to_vec())?)?
                .is_none()
        );
        assert!(tx.retained_nonempty(s.tenant())?);
        tx.control()?
    };
    let old = raw_policy(1, fifo)?;
    let mut charge = PolicySchemaCharge::new(ample.schema);
    let plan = prepare_plan(current, frame, &old, &mut charge)?;
    let x = 65_536 + 256 * if fifo { 80 } else { 76 };
    let changes = GroupOriginalPolicyChanges::new(
        &s,
        current,
        Some(&old),
        plan,
        (ample.codec.admission(), ample.schema),
        &mut charge,
        x,
    )?;
    let prior = charge.g();
    let tx = store.begin_apply(
        s.group(),
        s.recovery(),
        ample.codec.admission(),
        caps.0,
        ample.maximum_read_bytes,
        caps.1,
    )?;
    let result = tx.commit_original_policy(changes, ample.schema, &mut charge);
    if accept {
        assert_eq!(result?, 6);
    } else {
        assert_eq!(result, Err(StateTransactionError::Limit));
    }
    let delta = charge.g().checked_sub(prior).ok_or("schema work reset")?;
    drop(store.begin_apply(
        s.group(),
        s.recovery(),
        ample.codec.admission(),
        100,
        ample.maximum_read_bytes,
        ample.maximum_adapter_bytes,
    )?);
    drop(store);
    if !accept {
        assert_eq!(f::cohort(path)?, before);
    }
    drop(f::open(path, 64)?);
    Ok((delta, charge.p()))
}

fn expected(before: &f::Cohort, frame: &[u8], fifo: bool) -> TestResult<f::Cohort> {
    let mut expected = before.clone();
    let s = f::scope()?;
    let owner = f::owner(
        &s,
        b"p",
        f::data(b"i", b"job", b"q", 0, Some(0))?,
        0,
        1_u128 << 32,
    )?;
    if fifo {
        assert_eq!(
            f::rows_mut(&mut expected)?.remove(&f::member(&s, &owner)),
            Some(Vec::new())
        );
    } else {
        assert!(
            f::rows_mut(&mut expected)?
                .insert(f::member(&s, &owner), Vec::new())
                .is_none()
        );
    }
    f::rows_mut(&mut expected)?.insert(f::due(&s, &owner)?, vec![u8::from(!fifo)]);
    let response = raw_policy(2, !fifo)?;
    f::rows_mut(&mut expected)?.insert(f::key(6, &[b"g", b"z", b"t", b"job"]), response.clone());
    let metadata = f::metadata(
        &s,
        b"q",
        2,
        25,
        f::frame_intent(frame, 2)?,
        RetainedStatus::Accepted,
    )?;
    f::finish(
        &mut expected,
        &s,
        frame,
        f::log(2),
        (f::clock(25)?, f::clock(25)?),
        Some((&metadata, 1, &response)),
    )?;
    f::upgrade6(
        &mut expected,
        &s,
        f::catalog(&s, [None, None], None, None, None)?,
    )?;
    Ok(expected)
}

fn prefix_limits(fifo: bool) -> (usize, usize, u64) {
    let x_old = 65_536 + 256 * if fifo { 76 } else { 80 };
    let x_new = 65_536 + 256 * if fifo { 80 } else { 76 };
    // Independent helper-ordered C ledger. Actual current raw+X are R.
    // Every measure/project call decodes separately; each project also owns
    // 1024+2*job3+256+2*q1=1288. Both indexed-field decoders are real repeats.
    let c: [usize; 21] = [
        13,
        x_new,
        536,
        x_old + x_new,
        x_old,
        x_old + 1288,
        x_new,
        x_new + 1288,
        0,
        0,
        if fifo { 685 } else { 689 },
        512,
        465,
        13,
        if fifo { 1283 } else { 569 },
        if fifo { 39 } else { 0 },
        if fifo { 1027 } else { 1026 },
        0,
        if fifo { 1101 } else { 562 },
        39,
        86,
    ];
    let prefix_c = c.into_iter().sum::<usize>() + 44;
    let steps: [usize; 18] = [
        1,
        0,
        1,
        0,
        0,
        0,
        2,
        3,
        8,
        0,
        3,
        1,
        if fifo { 3 } else { 2 },
        if fifo { 2 } else { 0 },
        0,
        if fifo { 1 } else { 0 },
        if fifo { 2 } else { 1 },
        2,
    ];
    let end_steps = steps.into_iter().sum::<usize>() + 3 + 3;
    let wire = 102 + 1 + 1 + 1 + 1 + if fifo { 80 } else { 76 };
    let exact_c = prefix_c + x_new + wire;
    assert_eq!(
        (prefix_c, end_steps, exact_c),
        if fifo {
            (607_459, 35, 693_661)
        } else {
            (605_146, 30, 690_320)
        }
    );
    // ONE existing full decoder on the literal sorted63-row canonical table
    // (emitter closure+nil): g130+8777+2*lex+784+1172+540+4803+796+128.
    // Lex35299/31123 includes every wirebyte,12terms and actual atom scans.
    let decoder =
        130 + 8777 + 2 * if fifo { 35_299 } else { 31_123 } + 784 + 1172 + 540 + 4803 + 796 + 128;
    assert_eq!(decoder, if fifo { 87_728 } else { 79_376 });
    (exact_c, end_steps, decoder)
}
