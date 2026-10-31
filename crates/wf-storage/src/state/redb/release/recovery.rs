//! Full binding-to-record and queue/completion closure under the recovery guard.
use super::super::{
    admission::{copy_bytes, total},
    store::RedbStateTransaction,
};
use super::read;
use crate::state::key::DecodedStateKey;
use redb::ReadableTable;
use wf_domain::workflow::{control::ControlState, request::RequestScope};
use wf_ports::{log::inspect_release_binding, state::StateTransactionError as E};
use wf_types::identity::{PartitionId, RecordId};

pub(in crate::state::redb) fn row(
    tx: &RedbStateTransaction<'_>,
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    key: &DecodedStateKey,
    bytes: &[u8],
    shards: u64,
    control: ControlState,
) -> Result<(), E> {
    let scope = tx.scope_bytes(key.part(2))?;
    let context = (shards, control.time().hlc().physical());
    match key.family() {
        20 => {
            let binding = inspect_release_binding(bytes, bytes.len()).map_err(read::codec_error)?;
            tx.temporary(total(key.part(3).len(), [key.part(4).len()])?)?;
            let partition = PartitionId::new(copy_bytes(key.part(3))?).map_err(|_| E::Corrupt)?;
            let id = RecordId::new(copy_bytes(key.part(4))?).map_err(|_| E::Corrupt)?;
            read::check_binding(tx, table, &scope, (&partition, &id), binding, context)?;
        }
        21 => {
            let meta = read::inspect_meta(bytes)?;
            for sequence in meta.head..=meta.tail {
                check_intent(tx, table, &scope, sequence, context)?;
            }
        }
        22 | 23 => {
            let meta = read::meta(tx, table, &scope)?.ok_or(E::Corrupt)?;
            let sequence = u64::try_from(key.number()).map_err(|_| E::Corrupt)?;
            if sequence < meta.head || sequence > meta.tail {
                return Err(E::Corrupt);
            }
            if key.family() == 22 {
                read::intent(bytes, context)?;
            } else {
                read::completion(bytes)?;
                check_intent(tx, table, &scope, sequence, context)?;
            }
        }
        _ => return Err(E::Corrupt),
    }
    Ok(())
}

fn check_intent(
    tx: &RedbStateTransaction<'_>,
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    scope: &RequestScope,
    sequence: u64,
    context: (u64, wf_types::version::TimestampMillis),
) -> Result<(), E> {
    read::read(
        tx,
        table,
        &read::queue_key(tx, scope, 22, Some(sequence))?,
        |bytes| read::intent(bytes, context).map(|_| ()),
    )?
    .ok_or(E::Corrupt)
}
