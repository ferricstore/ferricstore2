use super::*;
use std::{collections::BTreeMap, path::Path};
use wf_domain::workflow::request::RetainedOutcome;
use wf_ports::log::PreparedBytes;
use wf_storage::transaction::RecordBatchTransactionLimits;
use wf_types::identity::RequestId;

pub(super) fn seed(path: &Path) -> TestResult {
    drop(catalog::create(path)?);
    for (index, name) in [b"a", b"b", b"c"].into_iter().enumerate() {
        catalog::lease(path, name, u64::try_from(index)?)?;
    }
    Ok(())
}

pub(super) fn frame(id: &[u8], request: Request) -> TestResult<PreparedBytes> {
    Ok(encode_catalog_ack_request(
        &common::scope()?,
        &RequestId::new(id.to_vec())?,
        common::clock(1001)?,
        request,
        common::driver_limits().codec,
        16 * common::MIB,
    )?)
}

pub(super) fn run(
    path: &Path,
    id: &[u8],
    request: Request,
    index: u64,
    limits: RecordBatchTransactionLimits,
) -> TestResult<RecordBatchTransactionOutcome> {
    let frame = frame(id, request)?;
    let mut store = common::open(path)?;
    Ok(apply_catalog_ack_transaction(
        &mut store,
        &common::scope()?,
        frame.as_bytes(),
        common::log(index),
        limits,
    )?)
}

pub(super) fn retained(
    path: &Path,
    id: &[u8],
    request: Request,
    index: u64,
) -> TestResult<Box<RetainedOutcome>> {
    let result = run(path, id, request, index, common::driver_limits())?;
    let RecordBatchTransactionOutcome::Retained { outcome, control } = result else {
        return Err("missing retained ACK".into());
    };
    assert_eq!(control.position(), common::log(index).index());
    Ok(outcome)
}

pub(super) fn reply(outcome: &RetainedOutcome) -> TestResult<Reply> {
    assert_eq!(
        (outcome.intent().version(), outcome.response_version()),
        (13, 11)
    );
    Ok(decode_catalog_ack_reply(outcome.response(), 1024, 1024)?)
}

pub(super) fn owners(path: &Path) -> TestResult<BTreeMap<Vec<u8>, Vec<u8>>> {
    Ok(common::raw_rows(path)?
        .into_iter()
        .filter(|(key, _)| key.get(6).is_some_and(|family| (12..=15).contains(family)))
        .collect())
}

pub(super) fn entries(path: &Path, count: u64) -> TestResult {
    use redb::TableDefinition;
    let (meta, _) = catalog::row(path, 16)?;
    let (mut key, _) = catalog::row(path, 17)?;
    let at = key.len() - 8;
    let mut value = b"FWCI\0\x01\0\0\0\x0eorphan".to_vec();
    value.extend_from_slice(&0_u64.to_be_bytes());
    let end = value.len() - 8;
    let database = redb::Builder::new().open(path)?;
    let tx = database.begin_write()?;
    {
        let mut table = tx.open_table(TableDefinition::<&[u8], &[u8]>::new("wf_state_rows_v1"))?;
        for sequence in 1..=count {
            key[at..].copy_from_slice(&sequence.to_be_bytes());
            value[end..].copy_from_slice(&sequence.to_be_bytes());
            table.insert(key.as_slice(), value.as_slice())?;
        }
        table.insert(meta.as_slice(), catalog::meta(1, count).as_slice())?;
    }
    tx.commit()?;
    Ok(())
}
