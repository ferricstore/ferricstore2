//! Original GET ingress/read composition; no physical or cache adapter is run.

#![forbid(unsafe_code)]

use wf_domain::policy::complete::{
    CompletePolicyEncodingError as EncodingError, OriginalEtfSafety,
    OriginalPolicyGetRejection as Rejection, OriginalPolicyReadValue as ReadValue,
    OriginalPolicyReplyContext as Context, OriginalPolicyReplyError as ReplyError,
    PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits, RawFlowPolicyValue as Raw,
};
use wf_domain::policy::installation::PolicyLimits;
use wf_domain::retention::ResolvedRetention;
use wf_ports::state::{OriginalPolicyGetReader, read_original_policy_get_reply as get};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

struct RecordingPolicyReader<'input> {
    value: ReadValue<'input>,
    calls: Vec<(u32, Vec<u8>)>,
}

impl OriginalPolicyGetReader for RecordingPolicyReader<'_> {
    fn read_policy_value_without_cache_tracking<'read>(
        &'read mut self,
        shard: u32,
        key: &[u8],
    ) -> ReadValue<'read> {
        self.calls.push((shard, key.to_vec()));
        self.value
    }
}

#[test]
fn original_policy_get_validates_controls_before_one_allocator_read() -> TestResult {
    let too_long = vec![b'x'; 65_523];
    let bad_state = [Raw::Atom("state"), Raw::Integer(1)];
    let state_rows = [Raw::Tuple(&bad_state)];
    let malformed = [Raw::Tuple(&bad_state), Raw::Integer(0)];
    for (kind, opts, expected) in [
        (Raw::Integer(1), Raw::List(&[]), Rejection::Options),
        (Raw::Binary(b""), Raw::Binary(b""), Rejection::Options),
        (Raw::Binary(b""), Raw::List(&malformed), Rejection::Options),
        (Raw::Binary(b""), Raw::List(&[]), Rejection::Type),
        (
            Raw::Binary(&too_long),
            Raw::List(&state_rows),
            Rejection::State,
        ),
        (Raw::Binary(&too_long), Raw::List(&[]), Rejection::KeyBytes),
    ] {
        assert_rejected_without_read(kind, opts, expected)?;
    }

    let nil_first = [Raw::Atom("state"), Raw::Nil];
    let ignored_generation = [Raw::Atom("expected_generation"), Raw::Other];
    let ignored_replace = [Raw::Atom("replace"), Raw::Tuple(&[])];
    let ignored_value = [
        Raw::Atom("unknown"),
        Raw::ImproperList {
            prefix: &[],
            tail: &Raw::Other,
        },
    ];
    let opts = [
        Raw::Tuple(&nil_first),
        Raw::Tuple(&bad_state),
        Raw::Tuple(&ignored_generation),
        Raw::Tuple(&ignored_replace),
        Raw::Tuple(&ignored_value),
    ];
    assert_absence(b"r", Raw::List(&opts), None)?;
    let opaque_state = [Raw::Atom("state"), Raw::Binary(&[0xff, 0])];
    let rows = [Raw::Tuple(&opaque_state)];
    assert_absence(&[0xff, 0], Raw::List(&rows), Some(&[0xff, 0]))?;
    let empty_state = [Raw::Atom("state"), Raw::Binary(b"")];
    let rows = [Raw::Tuple(&empty_state)];
    assert_absence(b"r", Raw::List(&rows), Some(b""))?;
    let largest_type = vec![b'x'; 65_522];
    assert_absence(&largest_type, Raw::List(&[]), None)?;
    assert_read_failures_precede_unused_context()?;
    Ok(())
}

#[test]
fn original_policy_get_preserves_pinned_allocator_and_type_binding() -> TestResult {
    // Source getter observations only: original SETs, routed-replica deletion,
    // native shard hashing, setup and durable reads are still adapter duties.
    for (kind, generation, active) in [
        (&b"generation-canonical-get-1-0"[..], 1, 1_000),
        (&b"generation-always-on-1"[..], 2, 2_000),
        (&b"generation-cas-1"[..], 2, 2_000),
    ] {
        let bytes = entry(kind, generation, Some(active))?;
        let mut reader = recording(ReadValue::Binary(&bytes));
        let limits = profile(100_000_000)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let reply = get(
            &mut reader,
            &Raw::Binary(kind),
            &Raw::List(&[]),
            context(0),
            limits,
            &mut charge,
        )?
        .map_err(|_| "valid source-value getter refused")?;
        assert_eq!(reply.generation(), u64::from(generation));
        let maximum = reply
            .root()
            .atom_field("max_active_ms", &mut charge)?
            .ok_or("max_active_ms field")?;
        assert_eq!(maximum.integer(), Some(i128::from(active)));
        drop(reply);
        assert_eq!(reader.calls, vec![(0, key(kind))]);
    }

    let kind = b"generation-mismatched-read-1";
    let bytes = entry(b"foreign-policy-type", 1, Some(9_000))?;
    let mut reader = recording(ReadValue::Binary(&bytes));
    let limits = profile(100_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let Err(reason) = get(
        &mut reader,
        &Raw::Binary(kind),
        &Raw::List(&[]),
        context(0),
        limits,
        &mut charge,
    )?
    else {
        return Err("foreign stored type accepted".into());
    };
    assert_eq!(reason, Rejection::Corrupt);
    assert_eq!(reason.as_bytes(), b"ERR flow policy is corrupt");
    assert_eq!(reader.calls, vec![(0, key(kind))]);
    assert_pinned_stored_selection()?;
    Ok(())
}

#[test]
fn original_policy_get_prepays_read_and_releases_temporary_key() -> TestResult {
    let bytes = entry(b"r", 1, None)?;
    assert_eq!(bytes.len(), 51);
    // Frozen before SUT: ingress G1180/P142; existing GET G18785/P2492.
    // Its final128 yields joined19965/2492 and exact total22457. The key
    // belongs only to ingress/read; recording allocations are external scope.
    for (maximum, attempted, spent, peak, calls) in [
        (1_321, 1_322, 1_038, 142, 0),
        (1_322, 1_450, 1_180, 142, 1),
        (22_456, 22_457, 19_837, 2_492, 1),
    ] {
        let limits = profile(maximum)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let mut reader = recording(ReadValue::Binary(&bytes));
        let Err(error) = get(
            &mut reader,
            &Raw::Binary(b"r"),
            &Raw::List(&[]),
            minimal_context(),
            limits,
            &mut charge,
        ) else {
            return Err("tight joined account succeeded".into());
        };
        assert_eq!(error, work(attempted, maximum));
        assert_eq!(charge.g(), spent);
        assert_eq!(charge.p(), peak);
        assert_eq!(reader.calls.len(), calls);
        if calls != 0 {
            assert_eq!(reader.calls, vec![(0, key(b"r"))]);
        }
    }
    let limits = profile(22_457)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let mut reader = recording(ReadValue::Binary(&bytes));
    let reply = get(
        &mut reader,
        &Raw::Binary(b"r"),
        &Raw::List(&[]),
        minimal_context(),
        limits,
        &mut charge,
    )?
    .map_err(|_| "exact joined account refused")?;
    assert_eq!(charge.g(), 19_965);
    assert_eq!(charge.p(), 2_492);
    assert_eq!(reply.generation(), 1);
    assert_eq!(reply.root().map_len(), Some(10));
    drop(reply);
    assert_eq!(reader.calls, vec![(0, key(b"r"))]);
    Ok(())
}

fn assert_rejected_without_read(kind: Raw<'_>, opts: Raw<'_>, expected: Rejection) -> TestResult {
    let mut reader = recording(ReadValue::Unavailable);
    let limits = profile(100_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let Err(reason) = get(
        &mut reader,
        &kind,
        &opts,
        context(usize::MAX),
        limits,
        &mut charge,
    )?
    else {
        return Err("invalid public GET input accepted".into());
    };
    assert_eq!(reason, expected);
    assert_eq!(reader.calls, Vec::<(u32, Vec<u8>)>::new());
    Ok(())
}

fn assert_absence(kind: &[u8], opts: Raw<'_>, state: Option<&[u8]>) -> TestResult {
    let mut reader = recording(ReadValue::Absent);
    let limits = profile(100_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let reply = get(
        &mut reader,
        &Raw::Binary(kind),
        &opts,
        context(0),
        limits,
        &mut charge,
    )?
    .map_err(|_| "valid absent public GET refused")?;
    assert_eq!(reply.generation(), 0);
    assert_eq!(reply.requested_type(), kind);
    assert_eq!(reply.requested_state(), state);
    assert_eq!(
        reply.root().map_len(),
        Some(if state.is_some() { 11 } else { 10 })
    );
    drop(reply);
    assert_eq!(reader.calls, vec![(0, key(kind))]);
    Ok(())
}

fn assert_read_failures_precede_unused_context() -> TestResult {
    for (value, expected) in [
        (ReadValue::Other, Rejection::Corrupt),
        (ReadValue::Unavailable, Rejection::ShardUnavailable),
        (ReadValue::ReadFailed, Rejection::ReadFailed),
        (ReadValue::Binary(&[131]), Rejection::Corrupt),
    ] {
        let mut reader = recording(value);
        let limits = profile(100_000_000)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let Err(reason) = get(
            &mut reader,
            &Raw::Binary(b"r"),
            &Raw::List(&[]),
            context(usize::MAX),
            limits,
            &mut charge,
        )?
        else {
            return Err("invalid read result accepted".into());
        };
        assert_eq!(reason, expected);
        assert_eq!(reader.calls, vec![(0, key(b"r"))]);
    }
    Ok(())
}

fn assert_pinned_stored_selection() -> TestResult {
    let bytes = selected_entry()?;
    let mut reader = recording(ReadValue::Binary(&bytes));
    let pair = [Raw::Atom("state"), Raw::Binary(b"charge_card")];
    let opts = [Raw::Tuple(&pair)];
    let limits = profile(100_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let reply = get(
        &mut reader,
        &Raw::Binary(b"flow-policy"),
        &Raw::List(&opts),
        selected_context(),
        limits,
        &mut charge,
    )?
    .map_err(|_| "pinned stored selected recipe refused")?;
    assert_eq!(reply.requested_state(), Some(&b"charge_card"[..]));
    let root = reply.root();
    let retry = root.atom_field("retry", &mut charge)?.ok_or("retry")?;
    let backoff = retry.atom_field("backoff", &mut charge)?.ok_or("backoff")?;
    let retention = root
        .atom_field("retention", &mut charge)?
        .ok_or("retention")?;
    assert_eq!(
        root.atom_field("max_active_ms", &mut charge)?
            .ok_or("max_active_ms")?
            .integer(),
        Some(120_000)
    );
    assert_eq!(
        retry
            .atom_field("max_retries", &mut charge)?
            .ok_or("max_retries")?
            .integer(),
        Some(2)
    );
    assert_eq!(
        backoff
            .atom_field("base_ms", &mut charge)?
            .ok_or("base_ms")?
            .integer(),
        Some(10_000)
    );
    assert_eq!(
        retry
            .atom_field("exhausted_to", &mut charge)?
            .ok_or("exhausted_to")?
            .binary_bytes(),
        Some(&b"payment_failed"[..])
    );
    assert_eq!(
        retention
            .atom_field("ttl_ms", &mut charge)?
            .ok_or("ttl_ms")?
            .integer(),
        Some(30_000)
    );
    assert!(
        retention
            .atom_field("history_hot_max_events", &mut charge)?
            .is_none()
    );
    assert_eq!(
        retention
            .atom_field("history_max_events", &mut charge)?
            .ok_or("history_max_events")?
            .integer(),
        Some(256)
    );
    drop(reply);
    assert_eq!(reader.calls, vec![(0, key(b"flow-policy"))]);
    Ok(())
}

fn recording(value: ReadValue<'_>) -> RecordingPolicyReader<'_> {
    RecordingPolicyReader {
        value,
        calls: Vec::new(),
    }
}

const fn context(additional_live: usize) -> Context<'static> {
    Context::new(
        OriginalEtfSafety::new(
            &["flow_policy_v1", "max_active_ms", "states", "type"],
            &[],
            None,
            &[],
        ),
        defaults(),
        None,
        None,
        additional_live,
    )
}

const fn minimal_context() -> Context<'static> {
    Context::new(
        OriginalEtfSafety::new(&["flow_policy_v1", "states", "type"], &[], None, &[]),
        defaults(),
        None,
        None,
        0,
    )
}

const fn selected_context() -> Context<'static> {
    Context::new(
        OriginalEtfSafety::new(
            &[
                "backoff",
                "base_ms",
                "exhausted_to",
                "fixed",
                "flow_policy_v1",
                "history_max_events",
                "jitter_pct",
                "kind",
                "max_active_ms",
                "max_ms",
                "max_retries",
                "retention",
                "retry",
                "states",
                "ttl_ms",
                "type",
            ],
            &[],
            None,
            &[],
        ),
        defaults(),
        None,
        None,
        0,
    )
}

const fn defaults() -> ResolvedRetention {
    ResolvedRetention {
        ttl_ms: 53,
        history_max_events: 17,
    }
}

fn profile(maximum: u64) -> TestResult<PolicySchemaLimits> {
    Ok(PolicySchemaLimits::new(PolicyLimits::default(), maximum)?)
}

fn key(kind: &[u8]) -> Vec<u8> {
    let mut key = b"f:{f}:policy:".to_vec();
    key.extend_from_slice(kind);
    key
}

fn entry(kind: &[u8], generation: u8, maximum: Option<i32>) -> TestResult<Vec<u8>> {
    let mut bytes = b"\x83h\x03w\x0eflow_policy_v1a".to_vec();
    bytes.push(generation);
    bytes.push(116);
    bytes.extend_from_slice(&(if maximum.is_some() { 3_u32 } else { 2_u32 }).to_be_bytes());
    if let Some(maximum) = maximum {
        bytes.extend_from_slice(b"w\x0dmax_active_msb");
        bytes.extend_from_slice(&maximum.to_be_bytes());
    }
    bytes.extend_from_slice(b"w\x06statest\x00\x00\x00\x00w\x04typem");
    bytes.extend_from_slice(&u32::try_from(kind.len())?.to_be_bytes());
    bytes.extend_from_slice(kind);
    Ok(bytes)
}

fn selected_entry() -> TestResult<Vec<u8>> {
    let backoff = map(&[
        (atom("kind")?, atom("fixed")?),
        (atom("base_ms")?, integer(10_000)),
        (atom("max_ms")?, integer(30_000)),
        (atom("jitter_pct")?, integer(0)),
    ])?;
    let retry = map(&[
        (atom("max_retries")?, integer(5)),
        (atom("backoff")?, backoff),
        (atom("exhausted_to")?, binary(b"failed")?),
    ])?;
    let retention = map(&[
        (atom("ttl_ms")?, integer(60_000)),
        (atom("history_max_events")?, integer(512)),
    ])?;
    let state = map(&[
        (
            atom("retry")?,
            map(&[
                (atom("max_retries")?, integer(2)),
                (atom("exhausted_to")?, binary(b"payment_failed")?),
            ])?,
        ),
        (
            atom("retention")?,
            map(&[
                (atom("ttl_ms")?, integer(30_000)),
                (atom("history_max_events")?, integer(256)),
            ])?,
        ),
    ])?;
    let policy = map(&[
        (atom("type")?, binary(b"flow-policy")?),
        (atom("max_active_ms")?, integer(120_000)),
        (atom("retry")?, retry),
        (atom("retention")?, retention),
        (atom("states")?, map(&[(binary(b"charge_card")?, state)])?),
    ])?;
    let mut bytes = b"\x83h\x03w\x0eflow_policy_v1a\x01".to_vec();
    bytes.extend_from_slice(&policy);
    Ok(bytes)
}

fn atom(value: &str) -> TestResult<Vec<u8>> {
    let mut bytes = vec![119, u8::try_from(value.len())?];
    bytes.extend_from_slice(value.as_bytes());
    Ok(bytes)
}

fn binary(value: &[u8]) -> TestResult<Vec<u8>> {
    let mut bytes = vec![109];
    bytes.extend_from_slice(&u32::try_from(value.len())?.to_be_bytes());
    bytes.extend_from_slice(value);
    Ok(bytes)
}

fn integer(value: i32) -> Vec<u8> {
    let mut bytes = vec![98];
    bytes.extend_from_slice(&value.to_be_bytes());
    bytes
}

fn map(rows: &[(Vec<u8>, Vec<u8>)]) -> TestResult<Vec<u8>> {
    let mut bytes = vec![116];
    bytes.extend_from_slice(&u32::try_from(rows.len())?.to_be_bytes());
    for (key, value) in rows {
        bytes.extend_from_slice(key);
        bytes.extend_from_slice(value);
    }
    Ok(bytes)
}

const fn work(bytes: u64, maximum: u64) -> ReplyError {
    ReplyError::Encoding(EncodingError::Schema(PolicySchemaError::WorkBytes {
        bytes,
        maximum,
    }))
}
