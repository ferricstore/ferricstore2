use super::*;
pub(super) fn limits() -> L {
    L {
        maximum_output_bytes: 100_000,
        maximum_work_units: 20_000_000,
        maximum_temporary_bytes: 2_000_000,
    }
}
pub(super) fn key() -> TestResult<Vec<u8>> {
    wf_ports::blob::render_original_state_key(
        b"run-1",
        wf_ports::blob::OriginalValuePartition::Explicit(b"tenant-a"),
        65_535,
    )
    .map_err(|error| format!("{error:?}"))
}
pub(super) const RECORD: &[(I<'static, 'static>, I<'static, 'static>)] = &[
    (I::Atom("id"), I::Binary(b"run-1")),
    (I::Atom("type"), I::Binary(b"invoice")),
    (I::Atom("state"), I::Binary(b"queued")),
    (I::Atom("version"), I::Integer(7)),
    (I::Atom("priority"), I::Integer(-2)),
    (I::Atom("partition_key"), I::Binary(b"tenant-a")),
    (I::Atom("created_at_ms"), I::Integer(100)),
    (I::Atom("updated_at_ms"), I::Integer(200)),
    (I::Atom("next_run_at_ms"), I::Integer(300)),
    (I::Atom("lease_deadline_ms"), I::Nil),
    (I::Atom("attempts"), I::Integer(2)),
    (I::Atom("run_state"), I::Binary(b"ready")),
    (I::Atom("max_active_ms"), I::Integer(10_000)),
    (I::Atom("state_enter_seq"), I::Integer(42)),
    (I::Atom("history_max_events"), I::Integer(100_000)),
    (I::Atom("history_hot_max_events"), I::Integer(1_000)),
    (I::Atom("parent_flow_id"), I::Binary(b"parent")),
    (I::Atom("root_flow_id"), I::Binary(b"root")),
    (I::Atom("correlation_id"), I::Binary(b"correlation")),
    (
        I::Atom("attributes"),
        I::Map(&[
            (I::Binary(b"tier"), I::Binary(b"gold")),
            (
                I::Binary(b"labels"),
                I::List(&[I::Binary(b"urgent"), I::Binary(b"finance")]),
            ),
        ]),
    ),
    (
        I::Atom("indexed_attributes"),
        I::List(&[I::Binary(b"tier")]),
    ),
    (
        I::Atom("state_meta"),
        I::Map(&[
            (
                I::Binary(b"created"),
                I::Map(&[(I::Binary(b"source"), I::Binary(b"api"))]),
            ),
            (
                I::Binary(b"queued"),
                I::Map(&[
                    (I::Binary(b"worker"), I::Binary(b"worker-1")),
                    (I::Binary(b"score"), I::Float(1.5)),
                ]),
            ),
        ]),
    ),
    (I::Atom("indexed_state_meta"), I::Binary(b"worker")),
];
pub(super) const LOCATOR: &[(I<'static, 'static>, I<'static, 'static>)] = &[
    (I::Atom("flow_id"), I::Binary(b"run-1")),
    (I::Atom("kind"), I::Atom("state")),
    (I::Atom("version"), I::Integer(7)),
    (I::Atom("raft_index"), I::Integer(99)),
    (I::Atom("file_id"), I::Integer(3)),
    (I::Atom("offset"), I::Integer(128)),
    (I::Atom("value_size"), I::Integer(512)),
    (I::Atom("frame_size"), I::Nil),
    (I::Atom("checksum"), I::Binary(&[1; 32])),
    (I::Atom("expire_at_ms"), I::Nil),
    (I::Atom("segment_generation"), I::Integer(4)),
];
pub(super) fn locator() -> Locator<'static, 'static> {
    Locator {
        fields: I::Map(LOCATOR),
    }
}
pub(super) fn record() -> I<'static, 'static> {
    I::Map(RECORD)
}
pub(super) fn replace<'rows, 'leaf>(
    rows: &[(I<'rows, 'leaf>, I<'rows, 'leaf>)],
    key: &'leaf str,
    value: I<'rows, 'leaf>,
) -> Vec<(I<'rows, 'leaf>, I<'rows, 'leaf>)> {
    let mut found = false;
    let mut output: Vec<_> = rows
        .iter()
        .map(|&(name, old)| {
            let matched = matches!(name, I::Atom(s) if s == key);
            found |= matched;
            (name, if matched { value } else { old })
        })
        .collect();
    if !found {
        output.push((I::Atom(key), value));
    }
    output
}
