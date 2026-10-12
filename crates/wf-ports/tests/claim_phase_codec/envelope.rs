//! The envelope borrows actual decoded query leaves and immutable external bytes.

use wf_domain::claim::PartitionFilter;
use wf_domain::claim::apply::ClaimStates;
use wf_ports::log::{CodecLimits, decode_prepared, encode_prepared};
use wf_types::version::{AppliedPosition, LogIdentity};

use super::{TestResult, request};

#[test]
fn decoded_claim_envelope_binds_header_physical_filters_and_original_bytes() -> TestResult {
    let expected = request("claim_rich")?;
    let encoded = encode_prepared(&expected, CodecLimits::default())?;
    let decoded = decode_prepared(encoded.as_bytes(), CodecLimits::default())?;
    let log = LogIdentity::new(7, 9, AppliedPosition::new(1));
    let envelope = decoded
        .claim_envelope(log)
        .ok_or("missing claim envelope")?;
    assert_eq!(envelope.header.scope, &expected.scope);
    assert_eq!(envelope.header.request, &expected.request);
    assert_eq!(envelope.header.intent, expected.intent);
    assert_eq!(envelope.header.stamp, expected.stamp);
    assert_eq!(envelope.header.identity.log(), log);
    assert_eq!(envelope.header.identity.prepared(), encoded.digest());
    assert_eq!(
        envelope.header.encoded_entry.as_ptr(),
        encoded.as_bytes().as_ptr()
    );
    assert_eq!(
        envelope.header.encoded_isolation_scope,
        decoded.isolation_scope()
    );
    assert_eq!(envelope.query.workflow_type, [b't', 255]);
    assert_eq!(envelope.query.worker, [b'w', 255]);
    assert_eq!((envelope.query.lease_ms, envelope.query.limit), (7, 5));
    assert_eq!((envelope.scan_budget, envelope.head_budget), (9, 11));
    assert_eq!((envelope.token_version, envelope.first_ordinal), (1, 7));
    assert_eq!(envelope.query.priority, Some(2));
    assert!(envelope.resume.is_none());
    let ClaimStates::Only(states) = envelope.query.states else {
        return Err("wrong physical states".into());
    };
    let PartitionFilter::Only(partitions) = envelope.query.partitions else {
        return Err("wrong partitions".into());
    };
    assert_eq!(states, [b"waiting".as_slice(), b"running".as_slice()]);
    assert_eq!(partitions, [[0, 255].as_slice(), b"p".as_slice()]);
    assert_eq!(envelope.query.exclude_states, [b"waiting".as_slice()]);
    for leaf in states
        .iter()
        .chain(partitions)
        .chain(envelope.query.exclude_states)
    {
        assert!(
            encoded
                .as_bytes()
                .windows(leaf.len())
                .any(|slice| slice.as_ptr() == leaf.as_ptr() && slice == *leaf)
        );
    }
    assert!(decoded.state_key(0).is_none());
    assert!(decoded.value_bytes(0).is_none());
    assert!(decoded.policy_envelope(log).is_none());
    let expected = request("claim_any")?;
    let encoded = encode_prepared(&expected, CodecLimits::default())?;
    let decoded = decode_prepared(encoded.as_bytes(), CodecLimits::default())?;
    let envelope = decoded.claim_envelope(log).ok_or("missing any envelope")?;
    assert_eq!(envelope.query.states, ClaimStates::Any);
    assert_eq!(envelope.query.partitions, PartitionFilter::Any);
    for name in ["claim_states_empty", "claim_partitions_empty"] {
        let encoded = encode_prepared(&request(name)?, CodecLimits::default())?;
        let decoded = decode_prepared(encoded.as_bytes(), CodecLimits::default())?;
        let envelope = decoded.claim_envelope(log).ok_or("missing empty filter")?;
        if name == "claim_states_empty" {
            assert_eq!(envelope.query.states, ClaimStates::Only(&[]));
        } else {
            assert_eq!(envelope.query.partitions, PartitionFilter::Only(&[]));
        }
    }
    Ok(())
}
