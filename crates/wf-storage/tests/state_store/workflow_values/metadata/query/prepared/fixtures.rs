//! Mechanical ETF fixture data and actual consumed native creation.
use super::*;
use wf_domain::{
    policy::complete::PolicySchemaLimits,
    value::{OriginalValueContext, OriginalValueLimits},
};
use wf_ports::{
    projection::query_row::{NativeQueryPreparationLimits, OriginalQueryRecordLimits},
    state::{
        WorkflowRecordPolicyReadLimits, WorkflowRecordPolicyReadPort,
        WorkflowRecordPolicyReadSnapshot,
    },
};

pub(super) fn context() -> OriginalValueContext<'static> {
    OriginalValueContext {
        safe_atoms: Some(&["nil"]),
        active_exports: None,
        local_node: None,
        local_functions: None,
    }
}
pub(super) fn preparation_limits() -> NativeQueryPreparationLimits {
    NativeQueryPreparationLimits {
        query: OriginalQueryRecordLimits {
            maximum_output_bytes: 64 * 1_048_576,
            maximum_work_units: 256 * 1_048_576,
            maximum_temporary_bytes: 64 * 1_048_576,
        },
        metadata: OriginalValueLimits {
            max_encoded_bytes: 1_048_576,
            max_nodes: 10_000,
            max_edges: 20_000,
            max_depth: 64,
            max_work_units: 64 * 1_048_576,
        },
        policy: PolicySchemaLimits::default(),
    }
}
pub(super) fn binary(value: &[u8]) -> TestResult<Vec<u8>> {
    let mut out = vec![109];
    out.extend_from_slice(&u32::try_from(value.len())?.to_be_bytes());
    out.extend_from_slice(value);
    Ok(out)
}
pub(super) fn map(rows: &[(&[u8], Vec<u8>)]) -> TestResult<Vec<u8>> {
    let mut out = vec![116];
    out.extend_from_slice(&u32::try_from(rows.len())?.to_be_bytes());
    for (key, value) in rows {
        out.extend_from_slice(&binary(key)?);
        out.extend_from_slice(value);
    }
    Ok(out)
}
pub(super) fn data(scope: bool, partition: &[u8]) -> TestResult<Vec<u8>> {
    let mut rows = vec![
        (&b"partition_key"[..], binary(partition)?),
        (&b"attributes"[..], map(&[(b"customer", vec![97, 42])])?),
        (
            &b"state_meta"[..],
            map(&[(b"q", map(&[(b"attempt", vec![97, 3])])?)])?,
        ),
        (&b"max_active_ms"[..], vec![97, 99]),
        (&b"history_hot_max_events"[..], vec![97, 99]),
        (
            &b"indexed_attributes"[..],
            vec![
                108, 0, 0, 0, 1, 109, 0, 0, 0, 6, b'f', b'o', b'r', b'g', b'e', b'd', 106,
            ],
        ),
        (&b"indexed_state_meta"[..], binary(b"forged")?),
    ];
    if scope {
        rows.push((b"scope_prefix", vec![119, 3, b'n', b'i', b'l']));
    }
    capture(&rows)
}
pub(super) fn capture(rows: &[(&[u8], Vec<u8>)]) -> TestResult<Vec<u8>> {
    let mut out = b"FSV2\x02\x83\x68\x03".to_vec();
    out.extend_from_slice(&binary(b"wf.record.metadata")?);
    out.extend_from_slice(&[97, 1]);
    out.extend_from_slice(&map(rows)?);
    Ok(out)
}
pub(super) fn setup(
    tag: &str,
    metadata: &[u8],
) -> TestResult<(
    std::path::PathBuf,
    wf_storage::state::RedbStateStore,
    WorkflowRecordPolicyReadSnapshot,
)> {
    let directory = crate::fixtures::directory(tag)?;
    let mut store = fresh(&directory.join("state"))?;
    apply(
        &mut store,
        &encode(&request(b"i", b"create", 100)?, &[Some(metadata)])?,
        0,
    )?;
    let captured = reread(&store, b"i")?;
    Ok((directory, store, captured))
}
pub(super) fn reread(
    store: &wf_storage::state::RedbStateStore,
    id: &[u8],
) -> TestResult<WorkflowRecordPolicyReadSnapshot> {
    Ok(store.read_workflow_record_with_policy(
        &crate::fixtures::scope()?,
        &PartitionId::new(b"p".to_vec())?,
        &RecordId::new(id.to_vec())?,
        Query::default(),
        WorkflowRecordPolicyReadLimits {
            record: limits(),
            schema: PolicySchemaLimits::default(),
        },
    )?)
}

pub(super) fn setup_indexed(
    tag: &str,
) -> TestResult<(std::path::PathBuf, wf_storage::state::RedbStateStore)> {
    use wf_domain::policy::complete::{
        CompletePolicyData, PolicySchemaCharge, StoredPolicyValue,
        prepare_complete_policy_value_v2, restore_complete_policy,
    };
    use wf_storage::state::{RedbStateStore, StateStoreGenesis, StateStoreLimits};
    let profile = PolicySchemaLimits::default();
    let mut charge = PolicySchemaCharge::new(profile);
    let policy = restore_complete_policy(
        1,
        CompletePolicyData {
            workflow_type: b"r".to_vec(),
            version: None,
            max_active_ms: Some(5000),
            retry: None,
            retention: None,
            states: std::collections::BTreeMap::new(),
            indexed_attributes: Some(vec![b"customer".to_vec(), b"region".to_vec()]),
            indexed_state_meta: Some(b"attempt".to_vec()),
            governance: None,
        },
        profile,
        &mut charge,
    )?;
    let donor = StoredPolicyValue::Complete(Box::new(prepare_complete_policy_value_v2(
        policy,
        profile,
        &mut charge,
    )?));
    let mut batch = request(b"i", b"create", 100)?;
    batch.policies = vec![crate::workflow_values::fixtures::witness(
        b"r",
        Some(&donor),
    )?];
    let StoredPolicyValue::Complete(policy) = donor else {
        return Err("complete donor".into());
    };
    let directory = crate::fixtures::directory(tag)?;
    let scope = crate::fixtures::scope()?;
    let placements = crate::fixtures::placements()?;
    let policies = [(scope.tenant().clone(), *policy)];
    let mut store = RedbStateStore::create_metadata_value_database(
        redb::Builder::new().create(directory.join("state"))?,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: crate::fixtures::clock(0)?,
            placements: &placements,
            policies: &policies,
        },
        crate::workflow_values::fixtures::defaults(),
        StateStoreLimits::default(),
    )?;
    apply(&mut store, &encode(&batch, &[Some(&data(true, b"p")?)])?, 0)?;
    Ok((directory, store))
}
