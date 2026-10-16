use super::*;

#[test]
fn terminal_pipeline_keeps_slots_and_stops_after_unknown() -> TestResult {
    let scope = scope()?;
    let ids = [
        RequestId::new(b"x".to_vec())?,
        RequestId::new(b"y".to_vec())?,
        RequestId::new(b"z".to_vec())?,
        RequestId::new(b"w".to_vec())?,
    ];
    let fields = fields();
    let bad = [("fencing_token", MutationFieldValue::Integer(1))];
    let items = [
        item(&scope, &ids[0], b"i", &fields),
        item(&scope, &ids[1], b"bad", &bad),
        item(&scope, &ids[2], b"j", &fields),
        item(&scope, &ids[3], b"k", &fields),
    ];
    let mut port = probe();
    port.unknown_at = Some(1);
    let reply = submit_independent_mutations(
        IndependentMutationCall::TerminalPipeline { items: &items },
        &mut port,
        MutationCallLimits::default(),
    )?;
    assert_eq!(reply.items().len(), 4);
    assert!(!reply.is_scalar_ok());
    for (row, id) in reply.items().iter().zip(&ids) {
        assert_eq!(row.request(), id);
        assert_eq!(row.scope(), &scope);
    }
    assert!(matches!(
        reply.items()[0].status(),
        MutationItemStatus::Success { .. }
    ));
    assert!(matches!(
        reply.items()[1].status(),
        MutationItemStatus::LocalError(_)
    ));
    assert_eq!(reply.items()[1].intent(), None);
    assert!(matches!(
        reply.items()[2].status(),
        MutationItemStatus::Unknown(MutationUnknown::CommitUncertain)
    ));
    assert!(matches!(
        reply.items()[3].status(),
        MutationItemStatus::NotSubmitted(MutationNotSubmitted::PredecessorUnknown)
    ));
    assert_eq!(port.events, ["prepare", "submit", "prepare", "submit"]);
    assert_eq!(port.identities[0].1, ids[0]);
    assert_eq!(port.identities[1].1, ids[2]);
    let mut port = probe();
    let empty = submit_independent_mutations(
        IndependentMutationCall::TerminalPipeline { items: &[] },
        &mut port,
        MutationCallLimits::default(),
    )?;
    assert_eq!(empty.items(), []);
    assert!(!empty.is_scalar_ok());
    assert_eq!(port.events, [] as [&str; 0]);
    Ok(())
}

fn reply_modes_and_identity() -> TestResult {
    let scope = scope()?;
    let x = RequestId::new(b"x".to_vec())?;
    let y = RequestId::new(b"y".to_vec())?;
    let fields = fields();
    let items = [
        item(&scope, &x, b"i", &fields),
        item(&scope, &y, b"j", &fields),
    ];
    let mut first = probe();
    let a = submit_independent_mutations(
        many(&items, MutationReplyMode::Items),
        &mut first,
        MutationCallLimits::default(),
    )?;
    let mut second = probe();
    second.time = 2_001;
    let b = submit_independent_mutations(
        many(&items, MutationReplyMode::OkOnSuccess),
        &mut second,
        MutationCallLimits::default(),
    )?;
    assert!(!a.is_scalar_ok());
    assert!(b.is_scalar_ok());
    assert_ne!(first.frames[0], second.frames[0]);
    for index in 0..2 {
        assert_eq!(a.items()[index].intent(), b.items()[index].intent());
    }
    assert_eq!(
        a.items()[0]
            .intent()
            .ok_or("missing intent")?
            .digest()
            .as_slice(),
        hex("ad27fb04f2617b292c86a42ad3e31ef6346e62938f4586d81588f1c76ad58466")?
    );
    assert_eq!(first.identities[0].0, scope);
    assert_eq!(first.identities[0].1, x);
    assert_eq!(first.identities[1].1, y);
    let mut port = probe();
    port.replies = vec![(
        RetainedStatus::IndexedRejected(IndexRejection::Batch(BatchRejection::Domain(
            DomainError::StaleLease,
        ))),
        vec![2, 0, 4],
    )];
    let mixed = submit_independent_mutations(
        many(&items, MutationReplyMode::OkOnSuccess),
        &mut port,
        MutationCallLimits::default(),
    )?;
    assert!(!mixed.is_scalar_ok());
    assert!(matches!(
        mixed.items()[0].status(),
        MutationItemStatus::Rejected {
            reason: IndexRejection::Batch(BatchRejection::Domain(DomainError::StaleLease)),
            ..
        }
    ));
    assert!(matches!(
        mixed.items()[1].status(),
        MutationItemStatus::Success { .. }
    ));
    assert_eq!(port.frames.len(), 2);
    Ok(())
}

fn corrupt_and_denied_replies() -> TestResult {
    let scope = scope()?;
    let ids = [
        RequestId::new(b"x".to_vec())?,
        RequestId::new(b"y".to_vec())?,
        RequestId::new(b"z".to_vec())?,
    ];
    let fields = fields();
    let items = [
        item(&scope, &ids[0], b"i", &fields),
        item(&scope, &ids[1], b"j", &fields),
        item(&scope, &ids[2], b"k", &fields),
    ];
    let stale = RetainedStatus::IndexedRejected(IndexRejection::Batch(BatchRejection::Domain(
        DomainError::StaleLease,
    )));
    let mut port = probe();
    port.replies = vec![
        (stale, vec![2, 0, 4]),
        (RetainedStatus::Accepted, vec![0, 0]),
    ];
    let reply = submit_independent_mutations(
        many(&items, MutationReplyMode::OkOnSuccess),
        &mut port,
        MutationCallLimits::default(),
    )?;
    assert!(!reply.is_scalar_ok());
    assert!(matches!(
        reply.items()[0].status(),
        MutationItemStatus::Rejected { .. }
    ));
    assert!(matches!(
        reply.items()[1].status(),
        MutationItemStatus::Unknown(MutationUnknown::InvalidReply)
    ));
    assert!(matches!(
        reply.items()[2].status(),
        MutationItemStatus::NotSubmitted(MutationNotSubmitted::PredecessorUnknown)
    ));
    assert_eq!(port.frames.len(), 2);
    let mut port = probe();
    port.foreign_intent = true;
    let reply = submit_independent_mutations(
        many(&items, MutationReplyMode::Items),
        &mut port,
        MutationCallLimits::default(),
    )?;
    assert!(matches!(
        reply.items()[0].status(),
        MutationItemStatus::Unknown(MutationUnknown::InvalidReply)
    ));
    assert_eq!(port.frames.len(), 1);
    let mut port = probe();
    port.denied_at = Some(0);
    let reply = submit_independent_mutations(
        many(&items[..1], MutationReplyMode::Items),
        &mut port,
        MutationCallLimits::default(),
    )?;
    assert!(matches!(
        reply.items()[0].status(),
        MutationItemStatus::NotSubmitted(MutationNotSubmitted::InvalidScope)
    ));
    assert_eq!(port.events, ["prepare", "submit"]);
    assert_eq!(port.frames, [] as [Vec<u8>; 0]);
    let mut port = probe();
    port.prepare_failure_at = Some(1);
    let reply = submit_independent_mutations(
        many(&items[..2], MutationReplyMode::Items),
        &mut port,
        MutationCallLimits::default(),
    )?;
    assert!(matches!(
        reply.items()[0].status(),
        MutationItemStatus::Success { .. }
    ));
    assert!(matches!(
        reply.items()[1].status(),
        MutationItemStatus::NotSubmitted(MutationNotSubmitted::Preparation)
    ));
    assert_eq!(port.frames.len(), 1);
    Ok(())
}

#[test]
fn scalar_projection_validates_every_reply_and_preserves_identity() -> TestResult {
    normalization::generic_vectors()?;
    reply_modes_and_identity()?;
    corrupt_and_denied_replies()
}
