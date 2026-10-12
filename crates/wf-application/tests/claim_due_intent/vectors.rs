use super::{IntentLimits, TenantId, TestResult, fixture_input, hex, normalized};
use wf_application::commands::prepare_claim_due_intent;
use wf_application::error::PreparationError;

#[test]
fn public_claim_matches_independent_byte_hash_vectors() -> TestResult {
    let tenant = TenantId::new(b"\0\xff".to_vec())?;
    let mut seen = 0;
    for row in include_str!("../../../../tests/intent/claim-due-v1-vectors.tsv")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let fields: Vec<_> = row.split('\t').collect();
        assert_eq!(fields.len(), 5);
        let request = normalized(fixture_input(fields[0])?)?;
        let bytes = fields[1].len() / 2;
        let values = fields[3].parse::<usize>()?;
        let depth = fields[4].parse()?;
        let optional = [
            request.priority().is_some(),
            request.payload_max_bytes().is_some(),
            request.governance_limit_scope().is_some(),
            request.governance_shard_id().is_some(),
            request.requested_now_ms().is_some(),
        ]
        .into_iter()
        .filter(|present| *present)
        .count();
        assert_eq!(
            values,
            45 + 2 * optional
                + request.states().1.len()
                + request.partitions().1.len()
                + request.values().1.len()
        );
        let exact = IntentLimits::new(bytes, values, depth)?;
        let prepared = prepare_claim_due_intent(&tenant, &request, exact)?;
        assert_eq!(prepared.version(), 1);
        assert_eq!(hex(prepared.digest()), fields[2], "{} digest", fields[0]);
        for (limits, error) in [
            (
                IntentLimits::new(bytes - 1, values, depth)?,
                PreparationError::ByteLimit,
            ),
            (
                IntentLimits::new(bytes, values - 1, depth)?,
                PreparationError::ValueLimit,
            ),
            (
                IntentLimits::new(bytes, values, depth - 1)?,
                PreparationError::DepthLimit,
            ),
        ] {
            assert_eq!(
                prepare_claim_due_intent(&tenant, &request, limits),
                Err(error),
                "{} exact boundary",
                fields[0]
            );
        }
        seen += 1;
    }
    assert_eq!(seen, 12);
    Ok(())
}
