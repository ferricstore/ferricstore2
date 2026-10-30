use super::{fixtures as s, *};
use wf_domain::history::EventKind;
use wf_domain::workflow::active::{ActiveValueKind, ActiveValueRef};
use wf_ports::blob::WorkflowValueBindingKnowledge;
use wf_types::identity::{PartitionId, RecordId};

#[test]
fn named_claim_preserves_values_birth_and_complete_historical_snapshots() -> TestResult {
    for stored in [false, true] {
        for payload in [None, Some(b"".as_slice()), Some(b"FSV2\0\xff".as_slice())] {
            retained_case(stored, payload)?;
        }
    }
    Ok(())
}

fn retained_case(stored: bool, payload: Option<&[u8]>) -> TestResult {
    let dir = f::directory("named-claim-values")?;
    let path = dir.join("state");
    let mut store = s::fresh(&path, stored)?;
    s::create(&mut store, b"i", b"q", (0, 1000, 2000), payload)?;
    drop(store);
    let before = s::cohort(&path)?;
    let mut store = f::open(&path)?;
    let original = s::retained(s::apply(
        &mut store,
        &s::phase(b"claim", 2000, b"q", b"worker", 1)?,
        1,
    )?)?;
    s::assert_first_id(&original, b"i")?;
    let record = s::record(&mut store, b"i")?;
    assert_eq!(
        (
            record.record.version(),
            record.record.fence().get(),
            record.record.attempts()
        ),
        (2, 1, 0)
    );
    assert_eq!(record.record.payload(), payload);
    let lease = record.record.lease().ok_or("claimed lease")?;
    assert_eq!(lease.token.as_bytes(), b"worker:2000:1");
    assert_eq!(lease.deadline.get(), 32000);
    assert_eq!(record.ordering.state_enter_seq, 0);
    let history = s::history(&store, b"i")?;
    assert_eq!(history.entries.len(), 2);
    assert_eq!(history.entries[1].event.kind, EventKind::Claimed);
    assert_eq!(history.entries[1].event.event_id, "2000-2");
    let WorkflowValueBindingKnowledge::Known(Some(binding)) = &history.entries[1].values else {
        return Err("captured Claim binding".into());
    };
    assert_eq!(binding.lifecycle.birth.retention_ttl_ms(), 7000);
    assert_eq!(binding.lifecycle.effective_ttl_ms, None);
    assert_eq!(binding.lifecycle.terminal_deadline, None);
    assert_eq!(binding.refs.payload.is_some(), payload.is_some());
    if let Some(reference) = &binding.refs.payload {
        let ActiveValueRef::Versioned {
            partition,
            id,
            kind: ActiveValueKind::Payload,
            version: 1,
        } = reference
        else {
            return Err("actual original payload reference".into());
        };
        assert_eq!(partition.as_bytes(), b"p");
        assert_eq!(id.as_bytes(), b"i");
    }
    let current = store.workflow_value_binding(
        &f::scope()?,
        &PartitionId::new(b"p".to_vec())?,
        &RecordId::new(b"i".to_vec())?,
        None,
        v::read_limits(),
    )?;
    same_binding(&current, &history.entries[1].values)?;
    drop(store);
    let after = s::cohort(&path)?;
    for (key, value) in before[1]
        .iter()
        .filter(|(key, _)| matches!(key.get(6), Some(8 | 9)))
    {
        assert_eq!(after[1].get(key), Some(value));
    }
    assert_eq!(
        before[1]
            .keys()
            .filter(|key| matches!(key.get(6), Some(8 | 9)))
            .count(),
        after[1]
            .keys()
            .filter(|key| matches!(key.get(6), Some(8 | 9)))
            .count()
    );
    assert!(
        after[1]
            .iter()
            .filter(|(key, _)| key.get(6) == Some(&5))
            .all(|(_, value)| value.starts_with(b"FWHS\0\x01"))
    );
    let mut reopened = f::open(&path)?;
    assert_eq!(s::record(&mut reopened, b"i")?, record);
    let restored = s::history(&reopened, b"i")?;
    assert_eq!(restored.control, history.control);
    assert_eq!(restored.record_version, history.record_version);
    assert_eq!(restored.entries.len(), history.entries.len());
    for (actual, expected) in restored.entries.iter().zip(&history.entries) {
        assert_eq!(actual.event, expected.event);
        same_binding(&actual.values, &expected.values)?;
    }
    drop(reopened);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn same_binding(
    left: &WorkflowValueBindingKnowledge,
    right: &WorkflowValueBindingKnowledge,
) -> TestResult {
    let (
        WorkflowValueBindingKnowledge::Known(Some(left)),
        WorkflowValueBindingKnowledge::Known(Some(right)),
    ) = (left, right)
    else {
        return Err("known captured bindings".into());
    };
    assert!(left.refs == right.refs);
    assert!(left.lifecycle == right.lifecycle);
    Ok(())
}
