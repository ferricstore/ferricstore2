//! Independent product encoders must retain the existing public V1 golden bytes.
use super::*;
use wf_ports::log::{CodecError, prepare_public_claim_intent};

#[test]
fn streaming_port_matches_all_twelve_frozen_public_claim_vectors_and_exact_byte_limits()
-> TestResult {
    let tenant = TenantId::new(b"\0\xff".to_vec())?;
    let mut seen = 0;
    for row in include_str!("../../../../tests/intent/claim-due-v1-vectors.tsv")
        .lines()
        .filter(|row| !row.starts_with('#'))
    {
        let fields: Vec<_> = row.split('\t').collect();
        let caller = normalized(fixture_input(fields[0])?)?;
        let maximum = fields[1].len() / 2;
        let product = prepare_public_claim_intent(&tenant, &caller, maximum)?;
        let application = prepare_claim_due_intent(&tenant, &caller, IntentLimits::default())?;
        assert_eq!(
            product, application,
            "{} independent product encoders",
            fields[0]
        );
        assert_eq!(hex(product.digest()), fields[2], "{} golden", fields[0]);
        assert_eq!(
            prepare_public_claim_intent(&tenant, &caller, maximum - 1),
            Err(CodecError::ByteLimit)
        );
        seen += 1;
    }
    assert_eq!(seen, 12);
    Ok(())
}
