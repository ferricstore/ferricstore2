//! Actual leased replies, retained matching and separate workspace/field frontiers.

use super::*;
use wf_domain::workflow::batch::ApplyError;
use wf_domain::workflow::request::{RetainedMetadata, RetainedOutcome};
use wf_ports::log::RecordBatchApplyError::{Apply, Encoding};

fn check_atomic_postimage(
    operation: &IndexedDecision<'_>,
    outcome: &RetainedOutcome,
    payload: Option<&[u8]>,
) -> TestResult {
    let IndexedDecision::Accepted(batch) = operation else {
        return Err("atomic start rejected".into());
    };
    if batch.batch().items().len() != 1
        || batch.consumed_ordinals() != 1
        || batch.next_ordinal() != Some(1)
    {
        return Err("atomic cardinality or ordinal differs".into());
    }
    let item = &batch.batch().items()[0];
    let write = item.plan().write().ok_or("missing actual record")?;
    let record = &write.record;
    let lease = record.lease().ok_or("missing actual lease")?;
    if item.partition().as_bytes() != b"p"
        || write.expected_version.is_some()
        || item.plan().outcome() != Outcome::Changed
        || record.id().as_bytes() != b"i"
        || record.workflow_type() != b"r"
        || record.state() != b"running"
        || record.run_state() != Some(b"q".as_slice())
        || record.version() != 1
        || record.fence() != Fence::new(1)
        || record.attempts() != 1
        || record.created_at() != TimestampMillis::new(17)
        || record.updated_at() != TimestampMillis::new(17)
        || record.next_run_at() != Some(TimestampMillis::new(18))
        || lease.worker.as_slice() != b"w\0\xff"
        || lease.token.as_bytes() != b"w\0\xff:17:1"
        || lease.deadline != TimestampMillis::new(18)
        || record.payload() != payload
        || record.result().is_some()
        || record.error().is_some()
        || record.latest_event_witness() != (EventKind::Created, 1, Fence::new(1))
    {
        return Err("actual atomic postimage differs".into());
    }
    let event = item.plan().history().ok_or("missing actual Created")?;
    let metadata = batch.metadata().collect::<Vec<_>>();
    if event.event_id != "17-1"
        || event.kind != EventKind::Created
        || event.occurred_at != TimestampMillis::new(17)
        || event.version != 1
        || event.fence != Fence::new(1)
        || event.attempts != 1
        || event.state.as_slice() != b"running"
        || event.next_run_at != Some(TimestampMillis::new(18))
        || event.lease_deadline != TimestampMillis::new(18)
        || event.worker.as_deref() != Some(b"w\0\xff".as_slice())
        || event.retry.is_some()
        || metadata.len() != 1
        || metadata[0].priority != 0
        || metadata[0].state_enter_seq != 1_u128 << 32
    {
        return Err("actual Created history or ordering differs".into());
    }
    let deltas = batch.deltas().collect::<Vec<_>>();
    if deltas.len() != 1 || deltas[0].before.is_some() {
        return Err("atomic delta cardinality or predecessor differs".into());
    }
    let Some(IndexRows::Parallel { due }) = deltas[0].after else {
        return Err("actual parallel delta missing".into());
    };
    if due.priority != 0
        || due.due != TimestampMillis::new(18)
        || due.workflow_type != b"r"
        || due.state != b"q"
        || due.partition_key != b"p"
        || due.id != b"i"
        || outcome.scope() != &scope()?
        || outcome.request().as_bytes() != b"q"
        || outcome.intent().version() != 7
        || outcome.original_position() != log(1)
        || outcome.original_time() != TimestampMillis::new(17)
        || outcome.expires_at() != TimestampMillis::new(117)
        || outcome.status() != RetainedStatus::Accepted
        || outcome.response_version() != 4
    {
        return Err("actual index row or retained metadata differs".into());
    }
    Ok(())
}

fn check_atomic_response_corruption(
    operation: &IndexedDecision<'_>,
    outcome: &RetainedOutcome,
) -> TestResult {
    for case in 0..9 {
        let mut corrupt = outcome.response().to_vec();
        match case {
            0 => corrupt[38] ^= 1,
            1 => corrupt[53] = 2,
            2 => corrupt[62..70].copy_from_slice(&17_u64.to_be_bytes()),
            3 => corrupt[70..78].copy_from_slice(&16_u64.to_be_bytes()),
            4 => corrupt[78..86].copy_from_slice(&16_u64.to_be_bytes()),
            5 => corrupt[110] = 1,
            6 | 7 => {
                let offset = if case == 6 { 112 } else { 113 };
                corrupt[offset] = 1;
                let insertion = offset + 1;
                corrupt.splice(insertion..insertion, [0, 0, 0, 0]);
            }
            _ => corrupt.push(0),
        }
        let summary = atomic_start_retained_summary(
            outcome.intent(),
            4,
            &corrupt,
            log(1),
            TimestampMillis::new(17),
            limits().admission(),
        );
        let matching = atomic_start_response_matches(
            operation,
            &corrupt,
            log(1),
            TimestampMillis::new(17),
            limits().admission(),
        );
        if summary.is_ok() || matching.is_ok() {
            return Err(format!("corrupt atomic response accepted: {case}").into());
        }
    }
    Ok(())
}

#[test]
fn atomic_start_response_matches_actual_plan_and_option_presence() -> TestResult {
    let mut original = None;
    let mut previous = None;
    let options = [
        (None, 114),
        (Some(b"".as_slice()), 118),
        (Some(b"v".as_slice()), 119),
        (Some(b"x".as_slice()), 119),
    ];
    for (payload, size) in options {
        let plan = with_atomic_view(None, |gate, view| {
            let frame = encode_atomic_start(
                atomic_request(b"w\0\xff", payload, 17)?,
                limits(),
                WORKSPACE,
            )?;
            let RecordBatchInspection::Fresh(fresh) = inspect_atomic_start_point(
                prepare_atomic_start(frame.as_bytes(), limits(), WORKSPACE)?,
                log(1),
                gate,
                limits(),
                WORKSPACE,
            )?
            else {
                return Err("not fresh atomic start".into());
            };
            Ok(decide_fresh_atomic_start_touched(fresh, view, 0, 100)?)
        })?;
        let control = plan.control();
        let actual = control
            .next()
            .last_entry()
            .ok_or("missing actual control")?
            .log();
        assert_eq!(actual, log(1));
        assert_eq!(control.execution_time(), TimestampMillis::new(17));
        let RecordBatchApplyDecision::Fresh { outcome, .. } = plan.decision() else {
            return Err("not fresh result".into());
        };
        assert_eq!(outcome.original_position(), actual);
        assert_eq!(outcome.original_time(), control.execution_time());
        for wrong in [
            LogIdentity::new(2, 7, log(1).index()),
            LogIdentity::new(1, 8, log(1).index()),
        ] {
            assert_ne!(outcome.original_position(), wrong);
        }
        let (_, RecordBatchApplyDecision::Fresh { operation, outcome }) = plan.into_parts() else {
            return Err("fresh ownership lost".into());
        };
        // All preparation/query/policy owners ended; use the SAME moved Box/plan.
        check_atomic_postimage(&operation, &outcome, payload)?;
        assert_eq!(outcome.response().len(), size);
        assert_eq!(
            atomic_start_retained_summary(
                outcome.intent(),
                4,
                outcome.response(),
                log(1),
                TimestampMillis::new(17),
                limits().admission(),
            )?,
            (RetainedStatus::Accepted, Some(1))
        );
        assert!(atomic_start_response_matches(
            &operation,
            outcome.response(),
            log(1),
            TimestampMillis::new(17),
            limits().admission(),
        )?);
        for (earlier, _) in original.iter().chain(previous.iter()) {
            assert!(!atomic_start_response_matches(
                earlier,
                outcome.response(),
                log(1),
                TimestampMillis::new(17),
                limits().admission(),
            )?);
        }
        if original.is_none() {
            assert_eq!(outcome.response(), hex(ATOMIC_RESPONSE)?);
            assert_eq!(
                Sha256::digest(outcome.response()).as_slice(),
                hex("d3a1c14cd4d75c977864fffb47523dddeecedd4a408a2c616d5a4bbf60fad9bc")?
            );
            original = Some((operation, outcome));
        } else if payload == Some(b"v".as_slice()) {
            previous = Some((operation, outcome));
        }
    }
    Ok(())
}

#[test]
fn atomic_start_workspace_and_response_caps_are_independent() -> TestResult {
    let profile = CodecLimits::new(AdmissionLimits::new(8, 1_248, 1)?);
    // E129/H4/V48+N8=56; B1248 gives decoder4836/Fresh6532, not late2007.
    for (worker, now, workspace, length) in [
        (b"w\0\xff".as_slice(), 17, 6_531, 114),
        (b"w\0\xff".as_slice(), 17, 6_532, 114),
        (b"w".as_slice(), 0, 6_532, 109),
    ] {
        let result = with_atomic_view(None, |gate, view| {
            let frame =
                encode_atomic_start(atomic_request(worker, None, now)?, profile, WORKSPACE)?;
            assert_eq!(frame.as_bytes().len(), if now == 17 { 129 } else { 127 });
            let RecordBatchInspection::Fresh(fresh) = inspect_atomic_start_point(
                prepare_atomic_start(frame.as_bytes(), profile, workspace)?,
                log(1),
                gate,
                profile,
                workspace,
            )?
            else {
                return Err("workspace fixture not fresh".into());
            };
            Ok(decide_fresh_atomic_start_touched(fresh, view, 0, 100))
        })?;
        if workspace == 6_531 {
            assert_eq!(
                result.err(),
                Some(RecordBatchApplyError::Workspace {
                    attempted: 6_532,
                    maximum: 6_531,
                })
            );
            continue;
        }
        let (_, RecordBatchApplyDecision::Fresh { operation, outcome }) = result?.into_parts()
        else {
            return Err("exact workspace not fresh".into());
        };
        assert!(matches!(&operation, IndexedDecision::Accepted(_)));
        assert_eq!(outcome.response().len(), length);
        if now == 17 {
            assert_eq!(outcome.response(), hex(ATOMIC_RESPONSE)?);
            check_atomic_response_corruption(&operation, &outcome)?;
        }
        // These checks reuse the SAME actual response/plan, not Fresh under tiny B.
        let checks = if now == 17 {
            [
                (8, 114, None),
                (8, 113, Some(CodecError::ByteLimit)),
                (7, 4_096, Some(CodecError::ValueLimit)),
            ]
        } else {
            [
                (7, 4_096, None),
                (6, 4_096, Some(CodecError::ValueLimit)),
                (8, 4_096, None),
            ]
        };
        for (inline, budget, error) in checks {
            let cap = AdmissionLimits::new(inline, budget, 1)?;
            let summary = atomic_start_retained_summary(
                outcome.intent(),
                4,
                outcome.response(),
                log(1),
                TimestampMillis::new(now),
                cap,
            );
            let matching = atomic_start_response_matches(
                &operation,
                outcome.response(),
                log(1),
                TimestampMillis::new(now),
                cap,
            );
            if let Some(error) = error {
                assert_eq!(summary, Err(error));
                assert_eq!(matching, Err(error));
            } else {
                assert_eq!(summary?, (RetainedStatus::Accepted, Some(1)));
                assert!(matching?);
            }
        }
    }
    Ok(())
}

fn atomic_original(profile: CodecLimits) -> TestResult<RecordBatchApplyPlan<'static>> {
    let plan = with_atomic_view(None, |gate, view| {
        let frame = encode_atomic_start(atomic_request(b"w\0\xff", None, 17)?, profile, WORKSPACE)?;
        let RecordBatchInspection::Fresh(fresh) = inspect_atomic_start_point(
            prepare_atomic_start(frame.as_bytes(), profile, WORKSPACE)?,
            log(1),
            gate,
            profile,
            WORKSPACE,
        )?
        else {
            return Err("original is not Fresh".into());
        };
        Ok(decide_fresh_atomic_start_touched(fresh, view, 0, 100)?)
    })?;
    let control = plan.control();
    assert_eq!(
        control
            .next()
            .last_entry()
            .ok_or("missing original log")?
            .log(),
        log(1)
    );
    assert_eq!(control.execution_time(), TimestampMillis::new(17));
    let RecordBatchApplyDecision::Fresh { outcome, .. } = plan.decision() else {
        return Err("original result is not Fresh".into());
    };
    assert_eq!(
        outcome.original_position(),
        control.next().last_entry().ok_or("log")?.log()
    );
    assert_eq!(outcome.status(), Accepted);
    assert_eq!(outcome.original_time(), control.execution_time());
    Ok(plan)
}

#[test]
fn atomic_start_point_replay_validates_original_response() -> TestResult {
    let profile = CodecLimits::new(AdmissionLimits::new(8, 1_248, 1)?);
    let plan = atomic_original(profile)?;
    let control = plan.control();
    let (
        _,
        RecordBatchApplyDecision::Fresh {
            outcome: original, ..
        },
    ) = plan.into_parts()
    else {
        return Err("original ownership lost".into());
    };
    let later = encode_atomic_start(atomic_request(b"w\0\xff", None, 18)?, profile, WORKSPACE)?;
    let query = scope()?;
    for case in 0..5 {
        let altered = if (1..=3).contains(&case) {
            let mut response = original.response().to_vec();
            if case == 1 {
                response[0] ^= 1;
            }
            let status = match case {
                3 => IndexedRejected(IndexRejection::DirectRunningFifoEntry),
                _ => original.status(),
            };
            Some(RetainedOutcome::new(
                RetainedMetadata {
                    scope: original.scope().clone(),
                    request: original.request().clone(),
                    intent: original.intent(),
                    original_position: original.original_position(),
                    original_time: original.original_time(),
                    expires_at: original.expires_at(),
                    status,
                },
                if case == 2 { 1 } else { 4 },
                response,
                profile.admission(),
            )?)
        } else {
            None
        };
        let retained = altered.as_ref().unwrap_or(original.as_ref());
        let inline = if case == 4 { 7 } else { 8 };
        let current = CodecLimits::new(AdmissionLimits::new(inline, 1_248, 1)?);
        let result = inspect_atomic_start_point(
            prepare_atomic_start(later.as_bytes(), current, WORKSPACE)?,
            log(2),
            PointGateView {
                scope: &query,
                control: control.next(),
                retained_nonempty: true,
                outcome: Some(retained),
            },
            current,
            WORKSPACE,
        );
        let error = match case {
            1 => Some(Encoding(CodecError::Malformed)),
            2 => Some(Encoding(CodecError::UnsupportedVersion)),
            3 => Some(Apply(ApplyError::InvalidAuthority)),
            4 => Some(Encoding(CodecError::ValueLimit)),
            _ => None,
        };
        if let Some(error) = error {
            assert_eq!(result.err(), Some(error), "{case}");
            continue;
        }
        let RecordBatchInspection::Complete(replay) = result? else {
            return Err("retained point requested current authority".into());
        };
        let RecordBatchApplyDecision::Replay(saved) = replay.decision() else {
            return Err("not original Replay".into());
        };
        assert!(std::ptr::eq(*saved, original.as_ref()));
        assert_eq!(
            replay
                .control()
                .next()
                .last_entry()
                .ok_or("replay log absent")?
                .log(),
            log(2)
        );
        assert_eq!(replay.control().execution_time(), TimestampMillis::new(18));
        assert_eq!(saved.original_position(), log(1));
        assert_eq!(saved.original_time(), TimestampMillis::new(17));
        assert_eq!(saved.expires_at(), TimestampMillis::new(117));
        assert_eq!(saved.response(), hex(ATOMIC_RESPONSE)?);
        assert_eq!(&saved.response()[38..46], b"w\0\xff:17:1");
    }
    Ok(())
}
