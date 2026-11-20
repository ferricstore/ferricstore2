//! Request bodies the Python SDK sends for the queue workflow.

use wf_api_tcp::frame::FLAG_CUSTOM_PAYLOAD;
use wf_api_tcp::request::{self, ClaimShape, Partitions};

use super::{CLAIM_DUE, COMPLETE, COMPLETE_MANY, CREATE, CREATE_MANY, TestResult};

fn typed_bin(out: &mut Vec<u8>, bytes: &[u8]) -> TestResult {
    out.push(4);
    out.extend_from_slice(&u32::try_from(bytes.len())?.to_be_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

/// A typed map whose values are all binaries.
fn typed_map(entries: &[(&str, &[u8])]) -> TestResult<Vec<u8>> {
    let mut out = vec![6];
    out.extend_from_slice(&u32::try_from(entries.len())?.to_be_bytes());
    for (key, value) in entries {
        out.extend_from_slice(&u32::try_from(key.len())?.to_be_bytes());
        out.extend_from_slice(key.as_bytes());
        typed_bin(&mut out, value)?;
    }
    Ok(out)
}

#[test]
fn parses_the_sdk_create_many_request() -> TestResult {
    let parsed = request::create_many(CREATE_MANY)?;
    assert!(parsed.flow_type.starts_with(b"dbos_python_sdk_bench:"));
    assert_eq!(parsed.state, b"queued");
    assert!(parsed.independent && parsed.ok_on_success);
    assert_eq!(
        (parsed.priority, parsed.partition_key.as_deref()),
        (0, None)
    );
    assert_eq!(parsed.items.len(), 500);
    let first = parsed.items.first().ok_or("missing first item")?;
    assert!(first.id.ends_with(b":flow:1"));
    assert_eq!(first.partition_key, None);
    assert_eq!(first.payload.as_deref(), Some(b"".as_slice()));
    let second = parsed.items.get(1).ok_or("missing second item")?;
    assert!(second.id.ends_with(b":flow:33"));
    Ok(())
}

#[test]
fn parses_the_sdk_single_create_request() -> TestResult {
    let parsed = request::create(CREATE)?;
    assert!(parsed.id.ends_with(b":0"));
    assert!(parsed.flow_type.starts_with(b"sc:"));
    assert_eq!(parsed.state, b"queued");
    assert_eq!(parsed.payload.as_deref(), Some(b"".as_slice()));
    assert!(parsed.run_at_ms.is_some());
    assert_eq!((parsed.partition_key, parsed.priority), (None, 0));
    Ok(())
}

#[test]
fn parses_the_sdk_compact_claim_due_request() -> TestResult {
    let parsed = request::claim_due(FLAG_CUSTOM_PAYLOAD, CLAIM_DUE)?;
    assert_eq!(parsed.state.as_deref(), Some(b"queued".as_slice()));
    assert!(parsed.worker.ends_with(b":worker:0"));
    assert_eq!(
        (
            parsed.lease_ms,
            parsed.limit,
            parsed.block_ms,
            parsed.priority
        ),
        (30_000, 500, None, Some(0))
    );
    assert!(!parsed.reclaim_expired);
    assert_eq!(parsed.shape, ClaimShape::JobsAttributes);
    assert_eq!(parsed.partitions, Partitions::Any);
    Ok(())
}

#[test]
fn parses_the_sdk_compact_complete_many_request() -> TestResult {
    let parsed = request::complete_many(FLAG_CUSTOM_PAYLOAD, COMPLETE_MANY)?;
    assert_eq!(parsed.partition_key, None);
    assert!(!parsed.independent && !parsed.ok_on_success && !parsed.terminal_local_only);
    assert_eq!(parsed.items.len(), 500);
    let first = parsed.items.first().ok_or("missing first item")?;
    assert!(first.id.ends_with(b":flow:11681"));
    assert_eq!(
        first.partition_key.as_deref(),
        Some(b"__flow_auto__:131".as_slice())
    );
    assert!(first.lease_token.ends_with(b":1794694374921:1"));
    assert_eq!(first.fencing_token, 1);
    Ok(())
}

#[test]
fn parses_the_sdk_single_complete_request() -> TestResult {
    let parsed = request::complete(COMPLETE)?;
    assert!(parsed.id.ends_with(b":0"));
    assert_eq!(
        parsed.partition_key.as_deref(),
        Some(b"__flow_auto__:241".as_slice())
    );
    assert!(parsed.lease_token.starts_with(b"w:") && parsed.lease_token.ends_with(b":1"));
    assert_eq!(parsed.fencing_token, 1);
    Ok(())
}

#[test]
fn unsupported_options_are_refused_by_name() -> TestResult {
    let create_many = typed_map(&[("type", b"t"), ("state", b"q"), ("attributes", b"x")])?;
    let error = request::create_many(&create_many)
        .err()
        .ok_or("accepted attributes")?;
    assert!(error.message().contains("attributes"), "{error}");
    let create = typed_map(&[
        ("id", b"a"),
        ("type", b"t"),
        ("state", b"q"),
        ("parent", b"p"),
    ])?;
    let error = request::create(&create).err().ok_or("accepted parent")?;
    assert!(error.message().contains("parent"), "{error}");
    let complete = typed_map(&[("id", b"a"), ("lease_token", b"l"), ("result", b"r")])?;
    let error = request::complete(&complete)
        .err()
        .ok_or("accepted result")?;
    assert!(error.message().contains("result"), "{error}");
    Ok(())
}

#[test]
fn compact_requests_require_the_custom_payload_flag_and_known_tags() -> TestResult {
    assert!(request::claim_due(0, CLAIM_DUE).is_err());
    assert!(request::complete_many(0, COMPLETE_MANY).is_err());
    let mut wrong_tag = CLAIM_DUE.to_vec();
    *wrong_tag.first_mut().ok_or("empty fixture")? = 0x92;
    assert!(request::claim_due(FLAG_CUSTOM_PAYLOAD, &wrong_tag).is_err());
    let mut trailing = COMPLETE_MANY.to_vec();
    trailing.push(0);
    assert!(request::complete_many(FLAG_CUSTOM_PAYLOAD, &trailing).is_err());
    Ok(())
}
