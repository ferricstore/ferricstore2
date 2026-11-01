//! Same-live revision interleaving and corruption; no new source-parity credit.

use wf_domain::workflow::{Operation, Transition};
use wf_ports::state::{
    StateTransactionError as Error, WorkflowRecordReadPort, WorkflowRecordReadQuery as Query,
};
use wf_types::{
    identity::{PartitionId, RecordId},
    version::Fence,
};

use super::{read, row};
use crate::state::value;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
mod fixtures;
mod policy;
use fixtures::fixture_data::*;

#[test]
fn native_record_query_pins_core_companion_and_control_across_real_publication() -> TestResult {
    let (directory, mut store, scope) = setup("pinned")?;
    let p = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    let mut context = read::open(&store, &scope, &p, &id, Query::default(), limits())?;
    let before = context.control(&scope)?;
    publish(
        &mut store,
        &scope,
        101,
        Operation::Transition(Transition {
            from: b"q".to_vec(),
            to: b"next".to_vec(),
            fence: Fence::new(0),
            lease_token: None,
            run_at: None,
            payload: None,
        }),
        8,
    )?;
    let old = row::capture(&mut context, &scope, &p, &id, None)?.ok_or("pinned capture")?;
    assert_eq!(old.record.record.version(), 1);
    assert_eq!(old.record.record.state(), b"q");
    assert_eq!(old.metadata.last(), Some(&7));
    assert_eq!(context.control(&scope)?, before);
    let new = store.read_workflow_record(&scope, &p, &id, Query::default(), limits())?;
    assert_ne!(new.control, before);
    let new = new.capture.ok_or("new capture")?;
    assert_eq!(new.record.record.version(), 2);
    assert_eq!(new.record.record.state(), b"next");
    assert_eq!(new.metadata.last(), Some(&8));
    assert!(old.values.lifecycle == new.values.lifecycle);
    drop((context, store));
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn native_record_query_rejects_missing_live_core_or_companion_and_fences() -> TestResult {
    for (family, history) in [(1_u8, false), (10, false), (5, true), (11, true)] {
        let (directory, store, scope) = setup(&format!("missing-{family}"))?;
        let before = rows(&store, false)?;
        let (key, _) = before
            .iter()
            .find(|(key, _)| key.get(6) == Some(&family))
            .ok_or("physical family")?;
        let p = PartitionId::new(b"p".to_vec())?;
        let id = RecordId::new(b"i".to_vec())?;
        let history_bytes = before
            .iter()
            .find(|(key, _)| key.get(6) == Some(&5))
            .map(|(_, bytes)| bytes)
            .ok_or("history row")?;
        let history_row = value::inspect_history_snapshot(
            history_bytes,
            &scope,
            &p,
            &id,
            wf_domain::workflow::admission::AdmissionLimits::default(),
            1_048_576,
        )?;
        let event = std::str::from_utf8(history_row.event_id())?.to_owned();
        rewrite(&store, false, key, None)?;
        let query = Query {
            history: if history { Some(&event) } else { None },
            expected_control: None,
        };
        let result = store.read_workflow_record(&scope, &p, &id, query, limits());
        assert!(
            matches!(result, Err(Error::Corrupt)),
            "family {family}: {result:?}"
        );
        assert!(matches!(
            store.read_workflow_record(&scope, &p, &id, Query::default(), limits()),
            Err(Error::Unavailable)
        ));
        drop(store);
        std::fs::remove_dir_all(directory)?;
    }
    Ok(())
}

#[test]
fn native_record_query_rejects_resealed_wrong_core_version_digest_and_birth() -> TestResult {
    for fault in 0..4 {
        let (directory, store, scope) = setup(&format!("resealed-{fault}"))?;
        let before = rows(&store, false)?;
        let (key, bytes) = before
            .iter()
            .find(|(key, _)| key.get(6) == Some(&10))
            .ok_or("companion")?;
        let mut data = value::decode_metadata_binding(bytes, defaults())?;
        match fault {
            0 => data.version += 1,
            1 => data.core_digest[0] ^= 1,
            2 => data.binding.lifecycle.incarnation += 1,
            _ => data.binding.versions[0] = None,
        }
        let changed = value::encode_metadata_binding(&data, 1_048_576)?;
        rewrite(&store, false, key, Some(&changed))?;
        let p = PartitionId::new(b"p".to_vec())?;
        let id = RecordId::new(b"i".to_vec())?;
        let result = store.read_workflow_record(&scope, &p, &id, Query::default(), limits());
        assert!(
            matches!(result, Err(Error::Corrupt)),
            "fault {fault}: {result:?}"
        );
        drop(store);
        std::fs::remove_dir_all(directory)?;
    }
    Ok(())
}

#[test]
fn native_record_query_rejects_foreign_point_with_a_resealed_matching_core_digest() -> TestResult {
    use sha2::{Digest, Sha256};
    use wf_domain::workflow::{admission::AdmissionLimits, request::RequestScope};
    use wf_types::identity::TenantId;
    for foreign_partition in [false, true] {
        let (directory, store, scope) = setup(&format!("foreign-{foreign_partition}"))?;
        let before = rows(&store, false)?;
        let (core_key, core) = before
            .iter()
            .find(|(key, _)| key.get(6) == Some(&1))
            .ok_or("core")?;
        let (binding_key, binding) = before
            .iter()
            .find(|(key, _)| key.get(6) == Some(&10))
            .ok_or("binding")?;
        let p = PartitionId::new(b"p".to_vec())?;
        let id = RecordId::new(b"i".to_vec())?;
        let owned = value::scan_record_snapshot(
            core,
            &scope,
            &p,
            &id,
            AdmissionLimits::default(),
            1_048_576,
        )?
        .into_owned(1_048_576)?;
        let selected_scope = if foreign_partition {
            scope.clone()
        } else {
            RequestScope::new(
                TenantId::new(b"other".to_vec())?,
                scope.group().clone(),
                scope.recovery().clone(),
            )
        };
        let selected_partition = if foreign_partition {
            PartitionId::new(b"other".to_vec())?
        } else {
            p.clone()
        };
        let changed = value::encode_record_snapshot(
            &selected_scope,
            &selected_partition,
            &owned.record,
            owned.ordering,
            AdmissionLimits::default(),
            1_048_576,
        )?;
        let mut companion = value::decode_metadata_binding(binding, defaults())?;
        companion.core_digest = Sha256::digest(&changed).into();
        let encoded = value::encode_metadata_binding(&companion, 1_048_576)?;
        rewrite(&store, false, core_key, Some(&changed))?;
        rewrite(&store, false, binding_key, Some(&encoded))?;
        let result = store.read_workflow_record(&scope, &p, &id, Query::default(), limits());
        assert!(matches!(result, Err(Error::Corrupt)), "{result:?}");
        drop(store);
        std::fs::remove_dir_all(directory)?;
    }
    Ok(())
}

#[test]
fn native_record_query_requires_actual_manifest_and_control_in_its_read_revision() -> TestResult {
    for metadata in [false, true] {
        let (directory, store, scope) = setup(&format!("authority-{metadata}"))?;
        let key = if metadata {
            b"manifest".to_vec()
        } else {
            crate::state::key::encode_control_key(scope.group(), scope.recovery(), 2048)?
        };
        rewrite(&store, metadata, &key, None)?;
        let result = store.read_workflow_record(
            &scope,
            &PartitionId::new(b"p".to_vec())?,
            &RecordId::new(b"i".to_vec())?,
            Query::default(),
            limits(),
        );
        assert!(matches!(result, Err(Error::Corrupt)), "{result:?}");
        drop(store);
        std::fs::remove_dir_all(directory)?;
    }
    Ok(())
}

#[test]
fn native_record_query_checks_control_before_materializing_a_corrupt_record() -> TestResult {
    let (directory, store, scope) = setup("control-before-core")?;
    let p = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    let before = rows(&store, false)?;
    let key = before
        .keys()
        .find(|key| key.get(6) == Some(&1))
        .ok_or("core")?;
    rewrite(&store, false, key, Some(b"invalid core"))?;
    let old = Query {
        history: None,
        expected_control: Some(wf_domain::workflow::control::ControlState::initial(
            wf_types::version::CommittedTime::new(wf_types::version::HlcTimestamp::new(0, 0)?, 0)?,
        )),
    };
    assert!(matches!(
        store.read_workflow_record(&scope, &p, &id, old, limits()),
        Err(Error::KnownAborted)
    ));
    assert!(matches!(
        store.read_workflow_record(&scope, &p, &id, Query::default(), limits()),
        Err(Error::Corrupt)
    ));
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
