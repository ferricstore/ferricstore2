//! Generic original caller replies; source bytes and safety donors stay borrowed.

use wf_domain::policy::complete::{
    CompletePolicyEncodingError as EncodingError, OriginalEtfSafety,
    OriginalPolicyReadValue as ReadValue, OriginalPolicyReply as Reply,
    OriginalPolicyReplyContext as ReplyContext, OriginalPolicyReplyError as ReplyError,
    OriginalPolicyReplyRejection as ReplyRejection, OriginalPolicyReplyValue as ReplyValue,
    PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
    present_original_policy_get_reply as get_reply, present_original_policy_set_reply as set_reply,
};
use wf_domain::retention::ResolvedRetention;

use super::{TestResult, limits as profile};

mod cases;
mod owners;
mod refusals;
mod source;

#[test]
fn original_reply_preserves_set_get_origins_and_caller_defaults() -> TestResult {
    assert_set_get_type_boundary()?;
    assert_read_origins()?;
    assert_absent_defaults()?;
    Ok(())
}

#[test]
fn original_reply_distinguishes_nil_empty_and_opaque_state_selection() -> TestResult {
    let bytes = minimal(1)?;
    let limits = profile(1_000_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let all = accepted(set_reply(
        b"r",
        &bytes,
        None,
        context(),
        limits,
        &mut charge,
    ))?;
    assert_eq!(all.root().map_len(), Some(10));
    assert_eq!(field(all.root(), "states", &mut charge)?.map_len(), Some(0));
    assert!(all.root().atom_field("state", &mut charge)?.is_none());
    drop(all);
    for state in [&b""[..], &b"queued"[..], &b"\xff\0"[..]] {
        let selected = accepted(set_reply(
            b"r",
            &bytes,
            Some(state),
            context(),
            limits,
            &mut charge,
        ))?;
        let root = selected.root();
        assert_eq!(root.map_len(), Some(11));
        assert_eq!(selected.requested_state(), Some(state));
        assert_eq!(
            field(root, "state", &mut charge)?.binary_bytes(),
            Some(state)
        );
        assert_eq!(
            field(root, "mode", &mut charge)?.atom_bytes(),
            Some(&b"parallel"[..])
        );
        assert!(root.atom_field("states", &mut charge)?.is_none());
    }
    Ok(())
}

fn assert_set_get_type_boundary() -> TestResult {
    let bytes = minimal(7)?;
    let limits = profile(1_000_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let projected = accepted(set_reply(
        b"requested",
        &bytes,
        None,
        context(),
        limits,
        &mut charge,
    ))?;
    assert_eq!(projected.generation(), 7);
    assert_eq!(projected.requested_type(), b"requested");
    assert_eq!(
        field(projected.root(), "type", &mut charge)?.binary_bytes(),
        Some(&b"requested"[..])
    );
    drop(projected);
    assert!(matches!(
        get_reply(
            b"requested",
            ReadValue::Binary(&bytes),
            None,
            context(),
            limits,
            &mut charge
        )?,
        Err(ReplyRejection::Corrupt)
    ));
    let matched = accepted(get_reply(
        b"r",
        ReadValue::Binary(&bytes),
        None,
        context(),
        limits,
        &mut charge,
    ))?;
    assert_eq!(matched.generation(), 7);
    drop(matched);
    for policy in [map(&[])?, map(&[(atom("type")?, integer(7)?)])?] {
        let bytes = envelope(1, &policy)?;
        let value = accepted(set_reply(
            b"r",
            &bytes,
            None,
            context(),
            limits,
            &mut charge,
        ))?;
        assert_eq!(
            field(value.root(), "type", &mut charge)?.binary_bytes(),
            Some(&b"r"[..])
        );
        drop(value);
        assert!(matches!(
            get_reply(
                b"r",
                ReadValue::Binary(&bytes),
                None,
                context(),
                limits,
                &mut charge
            )?,
            Err(ReplyRejection::Corrupt)
        ));
    }
    Ok(())
}

fn assert_read_origins() -> TestResult {
    let limits = profile(1_000_000_000)?;
    let bad_atoms = vec!["duplicate"; 4_097];
    let poisoned = ReplyContext::new(
        OriginalEtfSafety::new(&bad_atoms, &[], None, &[]),
        defaults(),
        Some(b"unrelated"),
        Some((u32::MAX, &[u32::MAX], &[])),
        usize::MAX,
    );
    for (value, reason, body) in [
        (
            ReadValue::Unavailable,
            ReplyRejection::ShardUnavailable,
            &b"ERR flow policy shard not available"[..],
        ),
        (
            ReadValue::ReadFailed,
            ReplyRejection::ReadFailed,
            &b"ERR flow policy read failed"[..],
        ),
        (
            ReadValue::Other,
            ReplyRejection::Corrupt,
            &b"ERR flow policy is corrupt"[..],
        ),
    ] {
        let mut charge = PolicySchemaCharge::new(limits);
        let rejected = get_reply(b"r", value, None, poisoned, limits, &mut charge)?;
        assert!(matches!(rejected, Err(actual) if actual == reason));
        assert_eq!(reason.body(), body);
        assert_eq!((charge.g(), charge.p()), (128, 0));
    }
    for malformed in [&b""[..], &b"\x83"[..]] {
        let mut charge = PolicySchemaCharge::new(limits);
        assert!(matches!(
            set_reply(b"r", malformed, None, poisoned, limits, &mut charge)?,
            Err(ReplyRejection::AllocationFailed)
        ));
        assert_eq!(
            ReplyRejection::AllocationFailed.body(),
            b"ERR flow policy allocation failed"
        );
    }
    let oversized = vec![0; 263_169];
    let mut charge = PolicySchemaCharge::new(limits);
    assert!(matches!(
        set_reply(b"r", &oversized, None, poisoned, limits, &mut charge)?,
        Err(ReplyRejection::AllocationFailed)
    ));
    assert_eq!((charge.g(), charge.p()), (256, 0));
    let mut charge = PolicySchemaCharge::new(limits);
    assert!(matches!(
        get_reply(
            b"r",
            ReadValue::Binary(&oversized),
            None,
            poisoned,
            limits,
            &mut charge
        )?,
        Err(ReplyRejection::Corrupt)
    ));
    assert_eq!((charge.g(), charge.p()), (256, 0));
    let mut charge = PolicySchemaCharge::new(limits);
    assert!(matches!(
        set_reply(b"r", &[0, 0], None, poisoned, limits, &mut charge)?,
        Err(ReplyRejection::AllocationFailed)
    ));
    assert_eq!((charge.g(), charge.p()), (257, 0));
    let mut charge = PolicySchemaCharge::new(limits);
    let deferred = ReplyContext::new(safety(), defaults(), None, None, usize::MAX);
    assert!(matches!(
        set_reply(b"r", &[131, 0], None, deferred, limits, &mut charge)?,
        Err(ReplyRejection::AllocationFailed)
    ));
    assert_eq!(charge.p(), 0);
    Ok(())
}

fn assert_absent_defaults() -> TestResult {
    let limits = profile(1_000_000_000)?;
    let caller = ResolvedRetention {
        ttl_ms: 17,
        history_max_events: 23,
    };
    for state in [None, Some(&b""[..]), Some(&b"missing"[..])] {
        let mut charge = PolicySchemaCharge::new(limits);
        let supplied = ReplyContext::new(safety(), caller, None, None, 0);
        let value = accepted(get_reply(
            b"r",
            ReadValue::Absent,
            state,
            supplied,
            limits,
            &mut charge,
        ))?;
        assert_eq!(value.generation(), 0);
        assert_eq!(value.requested_state(), state);
        let root = value.root();
        let retention = field(root, "retention", &mut charge)?;
        assert_eq!(field(retention, "ttl_ms", &mut charge)?.integer(), Some(17));
        assert_eq!(
            field(retention, "history_max_events", &mut charge)?.integer(),
            Some(23)
        );
        assert!(
            retention
                .atom_field("history_hot_max_events", &mut charge)?
                .is_none()
        );
        assert_eq!(
            field(root, "version", &mut charge)?.atom_bytes(),
            Some(&b"nil"[..])
        );
        assert!(field(root, "indexed_attributes", &mut charge)?.is_empty_list());
        if state.is_none() {
            assert_eq!(field(root, "states", &mut charge)?.map_len(), Some(0));
            assert!(
                field(root, "states", &mut charge)?
                    .atom_field("missing", &mut charge)?
                    .is_none()
            );
        }
    }
    Ok(())
}

pub(super) fn accepted(
    value: Result<Result<Reply<'_>, ReplyRejection>, ReplyError>,
) -> TestResult<Reply<'_>> {
    Ok(value?.map_err(|reason| format!("unexpected reply rejection: {reason:?}"))?)
}

pub(super) fn field<'view, 'input>(
    value: ReplyValue<'view, 'input>,
    name: &str,
    charge: &mut PolicySchemaCharge,
) -> TestResult<ReplyValue<'view, 'input>> {
    Ok(value
        .atom_field(name, charge)?
        .ok_or_else(|| format!("missing field {name}"))?)
}

pub(super) const fn defaults() -> ResolvedRetention {
    ResolvedRetention {
        ttl_ms: 604_800_000,
        history_max_events: 100_000,
    }
}

pub(super) fn context() -> ReplyContext<'static> {
    ReplyContext::new(safety(), defaults(), None, None, 0)
}

pub(super) const fn safety() -> OriginalEtfSafety<'static> {
    OriginalEtfSafety::new(
        &[
            "flow_policy_v1",
            "type",
            "generation",
            "states",
            "state",
            "version",
            "max_active_ms",
            "retry",
            "max_retries",
            "backoff",
            "kind",
            "fixed",
            "exponential",
            "base_ms",
            "max_ms",
            "jitter_pct",
            "exhausted_to",
            "retention",
            "ttl_ms",
            "history_max_events",
            "history_hot_max_events",
            "mode",
            "fifo",
            "parallel",
            "governance",
            "effects",
            "allowed",
            "denied",
            "indexed_attributes",
            "indexed_state_meta",
            "nil",
            "true",
            "false",
            "a",
            "b",
            "x",
            "y",
            "opaque",
            "__struct__",
            "Elixir.Dummy",
            "erlang",
            "length",
            "ok",
            "undefined",
            "node@host",
        ],
        &[("erlang", "length", 1)],
        None,
        &[],
    )
}

pub(super) fn minimal(generation: u64) -> TestResult<Vec<u8>> {
    envelope(
        generation,
        &map(&[(atom("states")?, map(&[])?), (atom("type")?, binary(b"r")?)])?,
    )
}

pub(super) fn envelope(generation: u64, policy: &[u8]) -> TestResult<Vec<u8>> {
    let mut output = vec![131];
    output.extend(tuple(&[
        atom("flow_policy_v1")?,
        integer(i128::from(generation))?,
        policy.to_vec(),
    ])?);
    Ok(output)
}

pub(super) fn atom(name: &str) -> TestResult<Vec<u8>> {
    let bytes = name.as_bytes();
    let mut output = Vec::new();
    if let Ok(length) = u8::try_from(bytes.len()) {
        output.extend_from_slice(&[119, length]);
    } else {
        output.push(118);
        output.extend_from_slice(&u16::try_from(bytes.len())?.to_be_bytes());
    }
    output.extend_from_slice(bytes);
    Ok(output)
}

pub(super) fn binary(bytes: &[u8]) -> TestResult<Vec<u8>> {
    let mut output = vec![109];
    output.extend_from_slice(&u32::try_from(bytes.len())?.to_be_bytes());
    output.extend_from_slice(bytes);
    Ok(output)
}

pub(super) fn integer(number: i128) -> TestResult<Vec<u8>> {
    if let Ok(byte) = u8::try_from(number) {
        return Ok(vec![97, byte]);
    }
    if let Ok(number) = i32::try_from(number) {
        let mut output = vec![98];
        output.extend_from_slice(&number.to_be_bytes());
        return Ok(output);
    }
    let bytes = number.unsigned_abs().to_le_bytes();
    let length = bytes
        .iter()
        .rposition(|byte| *byte != 0)
        .map_or(0, |index| index + 1);
    big(number < 0, &bytes[..length])
}

pub(super) fn big(negative: bool, digits: &[u8]) -> TestResult<Vec<u8>> {
    let mut output = Vec::new();
    if let Ok(length) = u8::try_from(digits.len()) {
        output.extend_from_slice(&[110, length]);
    } else {
        output.push(111);
        output.extend_from_slice(&u32::try_from(digits.len())?.to_be_bytes());
    }
    output.push(u8::from(negative));
    output.extend_from_slice(digits);
    Ok(output)
}

pub(super) fn tuple(items: &[Vec<u8>]) -> TestResult<Vec<u8>> {
    let mut output = Vec::new();
    if let Ok(length) = u8::try_from(items.len()) {
        output.extend_from_slice(&[104, length]);
    } else {
        output.push(105);
        output.extend_from_slice(&u32::try_from(items.len())?.to_be_bytes());
    }
    for item in items {
        output.extend_from_slice(item);
    }
    Ok(output)
}

pub(super) fn map(fields: &[(Vec<u8>, Vec<u8>)]) -> TestResult<Vec<u8>> {
    let mut output = vec![116];
    output.extend_from_slice(&u32::try_from(fields.len())?.to_be_bytes());
    for (key, value) in fields {
        output.extend_from_slice(key);
        output.extend_from_slice(value);
    }
    Ok(output)
}

pub(super) fn list(heads: &[Vec<u8>], tail: Option<&[u8]>) -> TestResult<Vec<u8>> {
    if heads.is_empty() && tail.is_none() {
        return Ok(vec![106]);
    }
    let mut output = vec![108];
    output.extend_from_slice(&u32::try_from(heads.len())?.to_be_bytes());
    for head in heads {
        output.extend_from_slice(head);
    }
    output.extend_from_slice(tail.unwrap_or(&[106]));
    Ok(output)
}
