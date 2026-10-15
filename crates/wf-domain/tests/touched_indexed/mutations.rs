use super::*;

#[test]
fn touched_indexed_matches_complete_mutation_plans() -> Result<(), Box<dyn Error>> {
    for fifo_mode in [false, true] {
        let a = create(b"a")?;
        let b = create(b"b")?;
        with_views(&[], &[&a, &b], fifo_mode, |full, touched| {
            let decision = decide(touched, &[&a, &b], order(20, 4))?;
            assert_eq!(decision, complete(full, &[&a, &b], order(20, 4))?);
            let IndexedDecision::Accepted(plan) = decision else {
                return Err("create rejected".into());
            };
            assert_eq!(
                plan.metadata()
                    .map(|meta| meta.state_enter_seq)
                    .collect::<Vec<_>>(),
                [85_899_345_924, 85_899_345_925]
            );
            assert_eq!(plan.consumed_ordinals(), 2);
            assert_eq!(plan.next_ordinal(), Some(6));
            for delta in plan.deltas() {
                assert!(delta.before.is_none());
                assert_eq!(
                    matches!(delta.after, Some(IndexRows::Fifo { .. })),
                    fifo_mode
                );
            }
            assert_eq!(
                plan.batch().items()[0]
                    .plan()
                    .history()
                    .ok_or("history absent")?
                    .kind,
                EventKind::Created
            );
            Ok(())
        })?;
        let old = record(b"a")?;
        let command = transition(b"a")?;
        let row = ClaimRecord {
            priority: if fifo_mode { 0 } else { 2 },
            ..entry(&old, 77)
        };
        with_views(&[row], &[&command], fifo_mode, |full, touched| {
            let decision = decide(touched, &[&command], order(20, 4))?;
            assert_eq!(decision, complete(full, &[&command], order(20, 4))?);
            let IndexedDecision::Accepted(plan) = decision else {
                return Err("transition rejected".into());
            };
            let metadata = plan.metadata().next().ok_or("metadata absent")?;
            assert_eq!(metadata.priority, row.priority);
            assert_eq!(metadata.state_enter_seq, 85_899_345_924);
            let changed = plan.batch().items()[0].plan();
            assert_eq!(
                changed.history().ok_or("history absent")?.kind,
                EventKind::Transitioned
            );
            assert_eq!(
                changed.write().ok_or("write absent")?.record.payload(),
                Some(b"new".as_slice())
            );
            assert_eq!(old.version(), 1);
            Ok(())
        })?;
    }
    Ok(())
}

#[test]
fn touched_indexed_preserves_order_and_query_lifetimes() -> Result<(), Box<dyn Error>> {
    let old = record(b"a")?;
    let command = transition(b"a")?;
    let row = entry(&old, 77);
    let plan = with_views(&[row], &[&command], false, |full, touched| {
        let result = decide(touched, &[&command], order(20, 4))?;
        assert_eq!(result, complete(full, &[&command], order(20, 4))?);
        Ok(result)
    })?;
    drop(command);
    // All query key buffers, map descriptors and local installed modes are gone.
    let IndexedDecision::Accepted(plan) = plan else {
        return Err("transition rejected".into());
    };
    let before = plan
        .deltas()
        .next()
        .ok_or("delta absent")?
        .before
        .ok_or("before absent")?;
    assert!(
        matches!(before, IndexRows::Parallel { due } if due.id == b"a" && due.partition_key == b"p")
    );
    assert_eq!(old.state(), b"queued");
    assert_eq!(
        plan.metadata()
            .next()
            .ok_or("metadata absent")?
            .state_enter_seq,
        85_899_345_924
    );
    let a = create(b"a")?;
    let b = create(b"b")?;
    with_views(&[], &[&a, &b], false, |_, touched| {
        let IndexedDecision::Accepted(last) = decide(touched, &[&a], order(u64::MAX, u32::MAX))?
        else {
            return Err("last ordinal rejected".into());
        };
        assert_eq!(
            last.metadata()
                .next()
                .ok_or("metadata absent")?
                .state_enter_seq,
            79_228_162_514_264_337_593_543_950_335
        );
        assert_eq!(last.next_ordinal(), None);
        assert_eq!(
            decide(touched, &[&a, &b], order(20, u32::MAX)),
            Err(ApplyError::InvalidAuthority)
        );
        let unknown = BTreeMap::new();
        let duplicate = decide(
            TouchedIndexedGroupView {
                policies: &unknown,
                ..touched
            },
            &[&a, &a],
            order(20, 0),
        )?;
        assert_eq!(
            duplicate,
            IndexedDecision::Rejected(IndexRejection::Batch(BatchRejection::DuplicateId))
        );
        let mut invalid = a.clone();
        let Operation::Create(create) = &mut invalid.operation else {
            return Err("not create".into());
        };
        create.workflow_type.clear();
        let invalid = decide(
            TouchedIndexedGroupView {
                policies: &unknown,
                ..touched
            },
            &[&invalid],
            order(20, 0),
        )?;
        assert!(matches!(
            invalid,
            IndexedDecision::Rejected(IndexRejection::Batch(BatchRejection::Domain(_)))
        ));
        Ok(())
    })?;
    Ok(())
}
