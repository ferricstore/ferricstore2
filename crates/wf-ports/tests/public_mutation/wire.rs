use super::*;
use wf_ports::log::{decode_prepared, prepare_independent_record_batch, prepare_record_batch};

#[test]
fn public_mutation_v1_matches_generic_and_independent_vectors() -> TestResult {
    let fields = fields();
    let caller = caller(&fields, Some(b"p"))?;
    let scope = scope()?;
    let request = RequestId::new(b"x".to_vec())?;
    let intent = prepare_public_mutation_intent(scope.tenant(), &caller, 387)?;
    assert_eq!(intent.version(), 1);
    assert_eq!(
        intent.digest().as_slice(),
        hex("ad27fb04f2617b292c86a42ad3e31ef6346e62938f4586d81588f1c76ad58466")?
    );
    assert!(prepare_public_mutation_intent(scope.tenant(), &caller, 386).is_err());
    assert_eq!(
        measure_public_mutation(&scope, &request, &caller, limits())?,
        117
    );
    let (bytes, actual) = encoded(2_000, &fields)?;
    assert_eq!(actual, intent);
    assert_eq!(bytes.as_bytes(), hex(FRAME)?);
    assert_eq!(
        bytes.digest().as_bytes().as_slice(),
        hex("be4b1be622c9f5ec0e810c7f4a09295c17e3de7083a1e4a8118e9c5e9ecbebf2")?
    );
    let decoded = prepare_public_mutation(bytes.as_bytes(), limits(), WORKSPACE)?;
    assert_eq!(decoded.request().intent, intent);
    assert_eq!(decoded.request().scope, scope);
    assert_eq!(decoded.request().request, request);
    let items = decoded
        .request()
        .operation
        .mutation_items()
        .ok_or("no public item")?;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].partition.as_bytes(), b"p");
    assert_eq!(items[0].command.id.as_bytes(), b"i");
    assert_eq!(items[0].command.now, TimestampMillis::new(2_000));
    assert_eq!(decoded.state_key(0).ok_or("state key")?.len(), 41);
    assert!(prepare_record_batch(bytes.as_bytes(), limits(), WORKSPACE).is_err());
    assert!(prepare_independent_record_batch(bytes.as_bytes(), limits(), WORKSPACE).is_err());
    for end in 0..bytes.as_bytes().len() {
        assert!(prepare_public_mutation(&bytes.as_bytes()[..end], limits(), WORKSPACE).is_err());
    }
    for offset in [6, 79, 80, 81, 92, 101] {
        let mut bad = bytes.as_bytes().to_vec();
        bad[offset] = 255;
        assert!(prepare_public_mutation(&bad, limits(), WORKSPACE).is_err());
    }
    for tag in [2, 3, 7] {
        let mut bad = bytes.as_bytes().to_vec();
        bad[101] = tag;
        assert!(prepare_public_mutation(&bad, limits(), WORKSPACE).is_err());
    }
    let mut trailing = bytes.as_bytes().to_vec();
    trailing.push(0);
    assert!(prepare_public_mutation(&trailing, limits(), WORKSPACE).is_err());
    assert!(decode_prepared(&trailing, limits()).is_err());
    assert_presence_and_time()?;
    assert_prospective_decode(bytes.as_bytes())
}

fn assert_presence_and_time() -> TestResult {
    let fields = fields();
    let (first, intent) = encoded(2_000, &fields)?;
    let (later, retry_intent) = encoded(2_001, &fields)?;
    assert_eq!(intent, retry_intent);
    assert_ne!(first.digest(), later.digest());
    let with_now = [
        ("lease_token", MutationFieldValue::Bytes(b"l")),
        ("fencing_token", MutationFieldValue::Integer(1)),
        ("now_ms", MutationFieldValue::Integer(0)),
    ];
    let (bytes, explicit) = encoded(2_000, &with_now)?;
    assert_ne!(intent, explicit);
    assert_eq!(bytes.as_bytes().len(), 125);
    assert_eq!(
        prepare_public_mutation(bytes.as_bytes(), limits(), WORKSPACE)?
            .request()
            .operation
            .mutation_items()
            .ok_or("item")?[0]
            .command
            .now,
        TimestampMillis::new(0)
    );
    let with_empty = [
        ("lease_token", MutationFieldValue::Bytes(b"l")),
        ("fencing_token", MutationFieldValue::Integer(1)),
        ("payload", MutationFieldValue::Bytes(b"")),
    ];
    assert_ne!(encoded(2_000, &with_empty)?.1, intent);
    let scope = scope()?;
    let auto = caller(&fields, None)?;
    let (bytes, auto_intent) = encode_public_mutation(
        &scope,
        &RequestId::new(b"x".to_vec())?,
        &auto,
        clock(2_000)?,
        limits(),
        WORKSPACE,
    )?;
    let decoded = prepare_public_mutation(bytes.as_bytes(), limits(), WORKSPACE)?;
    assert_eq!(
        decoded
            .request()
            .operation
            .mutation_items()
            .ok_or("auto item")?[0]
            .partition
            .as_bytes(),
        b"__flow_auto__:113"
    );
    assert_ne!(auto_intent, intent);
    let literal = caller(&fields, Some(b"__flow_auto__:113"))?;
    assert_ne!(
        prepare_public_mutation_intent(scope.tenant(), &literal, 4_096)?,
        auto_intent
    );
    Ok(())
}

fn assert_prospective_decode(bytes: &[u8]) -> TestResult {
    assert!(matches!(
        prepare_public_mutation(bytes, limits(), 16_780_755),
        Err(RecordBatchApplyError::Workspace {
            attempted: 16_780_756,
            maximum: 16_780_755
        })
    ));
    assert!(prepare_public_mutation(bytes, limits(), 16_780_756).is_ok());
    assert!(
        matches!(decide_missing(bytes, 33_558_419), Err(error) if error.downcast_ref::<RecordBatchApplyError>() == Some(&RecordBatchApplyError::Workspace { attempted: 33_558_420, maximum: 33_558_419 }))
    );
    assert_eq!(
        retained(&decide_missing(bytes, 33_558_420)?)?.response(),
        [2, 0, 0]
    );
    Ok(())
}
