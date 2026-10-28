//! Source order payload through actual logical caller, codec9, storage and reopen.

use super::super::logical_codec as caller;
use super::*;
use wf_domain::value::{OriginalValueInput as I, OriginalValueNode as N};
use wf_domain::workflow::caller::LogicalMutationFieldValue as V;

#[test]
fn logical_caller_publishes_source_order_payload_and_hydrates_after_reopen() -> TestResult {
    // FlowValuePayloadTest source payload: %{order_id: 123, items: ["book", "pen"]}.
    // This uses the existing r/q/p physical harness, not its public get/claim setup.
    let nodes = [
        N::Atom {
            bytes: 0..8,
            utf8: true,
        },
        N::Integer(123),
        N::Atom {
            bytes: 8..13,
            utf8: true,
        },
        N::Binary { bytes: 13..17 },
        N::Binary { bytes: 17..20 },
        N::EmptyList,
        N::List {
            heads: 0..2,
            tail: 5,
        },
        N::Map { entries: 2..6 },
    ];
    let input = I {
        bytes: b"order_iditemsbookpen",
        nodes: &nodes,
        edges: &[3, 4, 0, 1, 2, 6],
        root: 7,
    };
    let logical = caller::prepared_create(
        b"order",
        V::Value {
            input,
            init_pid: None,
        },
    )?;
    let expected = b"FSV2\x02\x83t\0\0\0\x02w\x05itemsl\0\0\0\x02m\0\0\0\x04bookm\0\0\0\x03penjw\x08order_ida\x7b";
    assert!(
        matches!(&logical.command().operation, Operation::Create(value) if value.payload.as_deref() == Some(expected.as_slice()))
    );
    let directory = crate::fixtures::directory("logical-caller-history")?;
    let path = directory.join("state");
    let mut store = fresh(&path)?;
    apply(&mut store, &caller::request(logical, b"logical-order")?, 0)?;
    let scope = crate::fixtures::scope()?;
    let mut clock = clock(101)?;
    for reopen in [false, true] {
        if reopen {
            drop(store);
            store = RedbStateStore::open(
                &path,
                scope.group().clone(),
                scope.recovery().clone(),
                StateStoreLimits::default(),
            )?;
        }
        let mut request = selection(10, HistoryValueReturn::Enabled { max_bytes: 1_024 });
        request.decode_context.safe_atoms = Some(&["items", "order_id"]);
        let page = read(&store, &mut clock, b"order", request)?;
        assert_eq!(page.history.entries.len(), 1);
        let value = document(&page, 0, "payload")?;
        assert_eq!(value.encoded(), expected);
        assert!(matches!(value.root(), Some(N::Map { .. })));
        assert!(
            matches!(field(&page, 0, "payload_size")?, HistoryValue::StoredSize(bytes) if *bytes == expected.len())
        );
    }
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
