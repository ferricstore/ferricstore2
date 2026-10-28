use super::{fixtures as f, *};

#[test]
fn step_codec_intent_separates_caller_fields_from_preparation() -> TestResult {
    let baseline = f::encoded(f::request()?)?;
    let intent = f::decoded(baseline.as_bytes())?.request().intent;
    for choice in 0..14 {
        let mut request = f::request()?;
        match choice {
            0 => request.scope = f::scope(b"other", b"g", b"z")?,
            1 => request.scope = f::scope(b"t", b"other", b"z")?,
            2 => request.scope = f::scope(b"t", b"g", b"other")?,
            3 => f::item_mut(&mut request)?.partition = PartitionId::new(b"other".to_vec())?,
            4 => f::item_mut(&mut request)?.command.id = RecordId::new(b"other".to_vec())?,
            5 => f::step_mut(&mut request)?.guard.token = LeaseToken::new(b"other".to_vec())?,
            6 => f::step_mut(&mut request)?.guard.fence = Fence::new(8),
            7 => f::step_mut(&mut request)?.from = b"other".to_vec(),
            8 => f::step_mut(&mut request)?.to = b"other".to_vec(),
            9 => f::step_mut(&mut request)?.worker = Some(Vec::new()),
            10 => f::step_mut(&mut request)?.worker = Some(b"worker".to_vec()),
            11 => f::step_mut(&mut request)?.lease_ms = 30_001,
            12 => f::step_mut(&mut request)?.payload = None,
            13 => f::step_mut(&mut request)?.payload = Some(Vec::new()),
            _ => return Err("fixture choice".into()),
        }
        let encoded = f::encoded(request)?;
        assert_ne!(f::decoded(encoded.as_bytes())?.request().intent, intent);
    }
    for choice in 0..4 {
        let mut request = f::request()?;
        match choice {
            0 => request.request = RequestId::new(b"other".to_vec())?,
            1 => request.stamp = f::clock(3)?,
            2 => f::item_mut(&mut request)?.command.now = TimestampMillis::new(3),
            3 => f::item_mut(&mut request)?.create_priority = 255,
            _ => return Err("fixture choice".into()),
        }
        let encoded = f::encoded(request)?;
        assert_eq!(f::decoded(encoded.as_bytes())?.request().intent, intent);
        assert_ne!(encoded.digest(), baseline.digest());
    }
    Ok(())
}
