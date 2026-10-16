use super::*;
use sha2::{Digest, Sha256};
use wf_ports::log::{decode_prepared, encode_record_batch, prepare_record_batch};

const V5_FRAME: &str = "4657435000010100000001740000000167000000017a0000000171000525a2a227783574ca47f08fb9036bb53611b0cb7b509f7a81ab36a18cf02c31ec000000000000000100000000000000000001000000010000000170000000016900000000000000000101000000017200000001710000";
const V6_FRAME: &str = "4657435000010100000001740000000167000000017a00000001710006a5babedcc9c403578eb46ceeee55389f7f05dc3927076e09db38987bab787de1000000000000000100000000000000000001000000010000000170000000016900000000000000000101000000017200000001710000";
const V6_INTENT: &str = "a5babedcc9c403578eb46ceeee55389f7f05dc3927076e09db38987bab787de1";

fn minimal() -> TestResult<PreparedRequest> {
    let mut value = request(vec![item(b"i", create())?])?;
    value.stamp = clock(1)?;
    let PreparedOperation::Batch(items) = &mut value.operation else {
        return Err("not batch".into());
    };
    items[0].command.now = TimestampMillis::new(1);
    Ok(value)
}

#[test]
fn v6_literals_preserve_v5() -> TestResult {
    let canonical = hex(
        "57462d5245434f52442d424154434800563600000000017400000001000000017000000001690100000000017200000001710000",
    )?;
    assert_eq!(canonical.len(), 52);
    assert_eq!(Sha256::digest(&canonical).as_slice(), hex(V6_INTENT)?);
    assert!(matches!(
        encode_independent_record_batch(minimal()?, limits(), 1_658),
        Err(RecordBatchApplyError::Workspace {
            attempted: 1_659,
            maximum: 1_658
        })
    ));
    let frame = encode_independent_record_batch(minimal()?, limits(), 1_659)?;
    assert_eq!(frame.as_bytes(), hex(V6_FRAME)?);
    assert_eq!(frame.as_bytes().len(), 115);
    assert_eq!(
        frame.digest().as_bytes().as_slice(),
        hex("fb64416de4fcbb4a2fd2c3df99b6e3470dec8c448533e4f1d8135e7f5c3690ea")?
    );
    let decoded = prepare_independent_record_batch(frame.as_bytes(), limits(), WORKSPACE)?;
    assert_eq!(decoded.request().intent.version(), 6);
    assert_eq!(
        decoded.request().intent.digest().as_slice(),
        hex(V6_INTENT)?
    );
    assert_eq!(
        decoded.request().intent,
        ClientIntent::independent_record_batch(
            hex(V6_INTENT)?.try_into().map_err(|_| "digest length")?
        )
    );
    assert_eq!(decoded.state_key(0).ok_or("key absent")?.len(), 41);
    assert_eq!(
        encode_record_batch(minimal()?, limits(), WORKSPACE)?.as_bytes(),
        hex(V5_FRAME)?
    );
    assert!(matches!(
        prepare_record_batch(frame.as_bytes(), limits(), WORKSPACE),
        Err(RecordBatchApplyError::Encoding(
            CodecError::UnsupportedVersion
        ))
    ));
    assert!(matches!(
        prepare_independent_record_batch(&hex(V5_FRAME)?, limits(), WORKSPACE),
        Err(RecordBatchApplyError::Encoding(
            CodecError::UnsupportedVersion
        ))
    ));
    let mut forged = hex(V5_FRAME)?;
    forged[27..29].copy_from_slice(&6_u16.to_be_bytes());
    assert!(matches!(
        prepare_independent_record_batch(&forged, limits(), WORKSPACE),
        Err(RecordBatchApplyError::IntentMismatch)
    ));
    let mut wrong_root = frame.as_bytes().to_vec();
    wrong_root[6] = 6;
    assert!(matches!(
        decode_prepared(&wrong_root, limits()),
        Err(CodecError::UnsupportedTag)
    ));
    let mut unknown = frame.as_bytes().to_vec();
    unknown[27..29].copy_from_slice(&7_u16.to_be_bytes());
    assert!(matches!(
        decode_prepared(&unknown, limits()),
        Err(CodecError::UnsupportedVersion)
    ));
    assert_roundtrips()?;
    assert_unsupported_claims()?;
    Ok(())
}

fn assert_roundtrips() -> TestResult {
    for operation in eight_operations()? {
        let expected = request(vec![operation])?;
        let encoded = encode_independent_record_batch(expected.clone(), limits(), WORKSPACE)?;
        let decoded = prepare_independent_record_batch(encoded.as_bytes(), limits(), WORKSPACE)?;
        assert!(decoded.request().operation == expected.operation);
        assert_eq!(decoded.request().scope, expected.scope);
        assert_eq!(decoded.request().request, expected.request);
        let atomic = encode_record_batch(expected, limits(), WORKSPACE)?;
        assert_ne!(
            decode_prepared(atomic.as_bytes(), limits())?
                .request()
                .intent,
            decoded.request().intent
        );
    }
    let mut changed_time = minimal()?;
    changed_time.stamp = clock(99)?;
    let PreparedOperation::Batch(items) = &mut changed_time.operation else {
        return Err("not batch".into());
    };
    items[0].command.now = TimestampMillis::new(99);
    let encoded = encode_independent_record_batch(changed_time, limits(), WORKSPACE)?;
    assert_eq!(
        decode_prepared(encoded.as_bytes(), limits())?
            .request()
            .intent
            .digest()
            .as_slice(),
        hex(V6_INTENT)?
    );
    Ok(())
}

fn assert_unsupported_claims() -> TestResult {
    for reclaim in [false, true] {
        let claim = wf_domain::claim::Claim {
            expected_state: b"q".to_vec(),
            worker: b"w".to_vec(),
            token: LeaseToken::new(b"k".to_vec())?,
            lease_ms: 1,
        };
        let operation = if reclaim {
            Operation::Reclaim(claim)
        } else {
            Operation::Claim(claim)
        };
        assert!(matches!(
            encode_independent_record_batch(
                request(vec![item(b"i", operation)?])?,
                limits(),
                WORKSPACE
            ),
            Err(RecordBatchApplyError::Encoding(CodecError::UnsupportedTag))
        ));
    }
    Ok(())
}
