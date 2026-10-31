use super::{BTreeMap, LimitData, StoredPolicyValue, TestResult, limits};
use wf_domain::policy::complete::{
    CompletePolicyData, GovernanceData, PolicySchemaCharge, prepare_complete_policy_value_v2,
    restore_complete_policy,
};

pub(super) fn governance(rule: LimitData) -> GovernanceData {
    GovernanceData {
        mode: None,
        effects: None,
        audit: None,
        limits: Some(BTreeMap::from([(b"running".to_vec(), rule)])),
        budgets: None,
        circuits: None,
        approvals: None,
    }
}

pub(super) fn typed(data: CompletePolicyData, generation: u64) -> TestResult<StoredPolicyValue> {
    let schema = limits(128 * 1024 * 1024)?;
    let mut charge = PolicySchemaCharge::new(schema);
    let candidate = restore_complete_policy(generation, data, schema, &mut charge)?;
    Ok(StoredPolicyValue::Complete(Box::new(
        prepare_complete_policy_value_v2(candidate, schema, &mut charge)?,
    )))
}

pub(super) fn atom(name: &str) -> TestResult<Vec<u8>> {
    let mut bytes = vec![119, u8::try_from(name.len())?];
    bytes.extend_from_slice(name.as_bytes());
    Ok(bytes)
}

pub(super) fn binary(value: &[u8]) -> TestResult<Vec<u8>> {
    let mut bytes = vec![109];
    bytes.extend_from_slice(&u32::try_from(value.len())?.to_be_bytes());
    bytes.extend_from_slice(value);
    Ok(bytes)
}

pub(super) fn map(rows: &[(Vec<u8>, Vec<u8>)]) -> TestResult<Vec<u8>> {
    let mut bytes = vec![116];
    bytes.extend_from_slice(&u32::try_from(rows.len())?.to_be_bytes());
    for (key, value) in rows {
        bytes.extend_from_slice(key);
        bytes.extend_from_slice(value);
    }
    Ok(bytes)
}

pub(super) fn envelope(generation: u8, fields: &[(Vec<u8>, Vec<u8>)]) -> TestResult<Vec<u8>> {
    let mut bytes = vec![131, 104, 3];
    bytes.extend_from_slice(&atom("flow_policy_v1")?);
    bytes.extend_from_slice(&[97, generation]);
    bytes.extend_from_slice(&map(fields)?);
    Ok(bytes)
}

pub(super) fn running(fields: &[(Vec<u8>, Vec<u8>)]) -> TestResult<Vec<u8>> {
    map(&[(
        atom("limits")?,
        map(&[(binary(b"running")?, map(fields)?)])?,
    )])
}

pub(super) fn base_rule() -> TestResult<Vec<u8>> {
    running(&[
        (atom("limit")?, vec![97, 8]),
        (atom("lease_size")?, vec![97, 7]),
        (atom("enforcement")?, atom("approximate_global")?),
    ])
}
