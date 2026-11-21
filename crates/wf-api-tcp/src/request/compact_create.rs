//! Compact `FLOW.CREATE_MANY` bodies, as Elixir's codec reads them.
//!
//! Tags: `0x90` (items `[id, payload]`, no partition), `0x96` (a batch
//! partition, `u32::MAX` length for none, then the same items) and `0x9E`
//! (items `[id, partition, payload]`). After the tag: the type, the state,
//! for `0x96` the partition, `now_ms`, `run_at_ms`, an independence byte
//! (`0` default, `1` false, `2` true), a return byte (`1` asks for a single
//! `OK` on success) and the item count, then the items.

use wf_application::commands::flow::Term;

use crate::codec::Reader;

use super::{CreateItem, CreateMany, RequestError};

/// Smallest item: an empty id and payload.
const MIN_ITEM_BYTES: usize = 8;

/// Parses a compact `FLOW.CREATE_MANY` body.
///
/// # Errors
/// Refuses an unknown tag or marker and a truncated or over-long body.
pub fn compact_create_many(body: &[u8]) -> Result<CreateMany, RequestError> {
    read(body)
        .ok_or_else(|| RequestError::new("ERR native compact FLOW.CREATE_MANY payload is invalid"))
}

fn read(body: &[u8]) -> Option<CreateMany> {
    let mut reader = Reader::new(body);
    let (batch_partition, item_partitions) = match reader.u8()? {
        0x90 => (false, false),
        0x96 => (true, false),
        0x9E => (false, true),
        _ => return None,
    };
    let flow_type = reader.bin()?.to_vec();
    let state = reader.bin()?.to_vec();
    let partition_key = if batch_partition {
        reader.optional_bin().ok()?.map(<[u8]>::to_vec)
    } else {
        None
    };
    let now_ms = reader.i64()?;
    let run_at_ms = reader.i64()?;
    let independent = match reader.u8()? {
        0 | 1 => false,
        2 => true,
        _ => return None,
    };
    let ok_on_success = match reader.u8()? {
        0 => false,
        1 => true,
        _ => return None,
    };
    let count = reader.count(MIN_ITEM_BYTES)?;
    let mut items = Vec::with_capacity(count);
    for _ in 0..count {
        let id = reader.bin()?.to_vec();
        let partition = if item_partitions {
            Some(reader.bin()?.to_vec())
        } else {
            None
        };
        items.push(CreateItem {
            id,
            partition_key: partition,
            payload: Some(reader.bin()?.to_vec()),
        });
    }
    reader.is_empty().then_some(CreateMany {
        flow_type,
        state,
        now_ms: Some(now_ms),
        run_at_ms: Some(run_at_ms),
        priority: 0,
        independent,
        ok_on_success,
        partition_key,
        items,
    })
}

fn entry(key: &str, value: Term) -> (Vec<u8>, Term) {
    (key.as_bytes().to_vec(), value)
}

impl CreateMany {
    /// The batch as the options of the generic command.
    #[must_use]
    pub fn options(&self) -> Term {
        let mut options = vec![
            entry("type", Term::Bin(self.flow_type.clone())),
            entry("state", Term::Bin(self.state.clone())),
            entry("independent", Term::Bool(self.independent)),
        ];
        if let Some(now) = self.now_ms {
            options.push(entry("now_ms", Term::Int(now)));
        }
        if let Some(at) = self.run_at_ms {
            options.push(entry("run_at_ms", Term::Int(at)));
        }
        if self.priority != 0 {
            options.push(entry("priority", Term::Int(self.priority)));
        }
        if self.ok_on_success {
            options.push(entry("return", Term::text("ok_on_success")));
        }
        if let Some(partition) = &self.partition_key {
            options.push(entry("partition_key", Term::Bin(partition.clone())));
        }
        let items = self.items.iter().map(item).collect();
        options.push(entry("items", Term::List(items)));
        Term::Map(options)
    }
}

/// An item as `[id]`, `[id, payload]` or `[id, partition, payload]`.
fn item(item: &CreateItem) -> Term {
    let mut fields = vec![Term::Bin(item.id.clone())];
    if let Some(partition) = &item.partition_key {
        fields.push(Term::Bin(partition.clone()));
    }
    match (&item.payload, &item.partition_key) {
        (Some(payload), _) => fields.push(Term::Bin(payload.clone())),
        (None, Some(_)) => fields.push(Term::Nil),
        (None, None) => {}
    }
    Term::List(fields)
}
