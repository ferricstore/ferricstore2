use super::*;

pub(super) const FRAME: &str = "4657435000010100000001740000000167000000017a0000000171000a9f2dd1d154730c5c8f52552831bc851ffb090859391283f5934bcb4ea25e277200000000000000020000000000000000000200000001000000017000000001690000000000000000020c000000056c65617365000000000000000700000009617574686f72697a6500000006636861726765000000000000007530010000000200ff";
pub(super) const INTENT: &str = "9f2dd1d154730c5c8f52552831bc851ffb090859391283f5934bcb4ea25e2772";
pub(super) const DIGEST: &str = "fe30c3b100ebb1fd27b78db8a16b811bc86fa509a8f4849bcbd54f373f6d35e3";
pub(super) const ISOLATION: &str = "46575343000100000001740000000167000000017a";
pub(super) const KEY: &str =
    "4657534b00010000001546575343000100000001740000000167000000017a00000001700000000169";

pub(super) fn bytes(text: &str) -> TestResult<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return Err("odd golden hex".into());
    }
    let mut result = Vec::new();
    for pair in text.as_bytes().as_chunks::<2>().0 {
        result.push(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?);
    }
    Ok(result)
}

pub(super) fn digest(text: &str) -> TestResult<[u8; 32]> {
    Ok(bytes(text)?.try_into().map_err(|_| "golden digest width")?)
}

pub(super) fn limits() -> TestResult<CodecLimits> {
    Ok(CodecLimits::new(AdmissionLimits::new(32, 8192, 4)?))
}

pub(super) fn clock(now: u64) -> TestResult<CommittedTime> {
    Ok(CommittedTime::new(HlcTimestamp::new(now, 0)?, now)?)
}

pub(super) fn scope(tenant: &[u8], group: &[u8], recovery: &[u8]) -> TestResult<RequestScope> {
    Ok(RequestScope::new(
        TenantId::new(tenant.to_vec())?,
        GroupId::new(group.to_vec())?,
        RecoveryId::new(recovery.to_vec())?,
    ))
}

pub(super) fn request() -> TestResult<PreparedRequest> {
    Ok(PreparedRequest {
        scope: scope(b"t", b"g", b"z")?,
        request: RequestId::new(b"q".to_vec())?,
        intent: ClientIntent::new(1, [99; 32])?,
        stamp: clock(2)?,
        operation: PreparedOperation::Batch(vec![PreparedBatchItem {
            partition: PartitionId::new(b"p".to_vec())?,
            command: CommittedCommand {
                id: RecordId::new(b"i".to_vec())?,
                now: TimestampMillis::new(2),
                operation: Operation::StepContinue(StepContinue {
                    guard: LeaseGuard {
                        token: LeaseToken::new(b"lease".to_vec())?,
                        fence: Fence::new(7),
                    },
                    from: b"authorize".to_vec(),
                    to: b"charge".to_vec(),
                    worker: None,
                    lease_ms: 30_000,
                    payload: Some(vec![0, 255]),
                }),
            },
            create_priority: 0,
        }]),
    })
}

pub(super) fn item_mut(request: &mut PreparedRequest) -> TestResult<&mut PreparedBatchItem> {
    let PreparedOperation::Batch(items) = &mut request.operation else {
        return Err("fixture batch".into());
    };
    let [item] = items.as_mut_slice() else {
        return Err("fixture singleton".into());
    };
    Ok(item)
}

pub(super) fn step_mut(request: &mut PreparedRequest) -> TestResult<&mut StepContinue> {
    let Operation::StepContinue(step) = &mut item_mut(request)?.command.operation else {
        return Err("fixture step".into());
    };
    Ok(step)
}

pub(super) fn encoded(request: PreparedRequest) -> TestResult<PreparedBytes> {
    let result = encode_step_continue(request, limits()?, WORKSPACE);
    assert!(
        result.is_ok(),
        "healthy singleton Step encoding: {:?}",
        result.as_ref().err()
    );
    Ok(result?)
}

pub(super) fn decoded(bytes: &[u8]) -> TestResult<DecodedRequest<'_>> {
    let result = prepare_step_continue(bytes, limits()?, WORKSPACE);
    assert!(
        result.is_ok(),
        "healthy singleton Step preparation: {:?}",
        result.as_ref().err()
    );
    Ok(result?)
}
