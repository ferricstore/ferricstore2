use super::{TestResult, framing};
use sha2::{Digest, Sha256};
use wf_domain::{
    governance::{
        credit::CreditLimits,
        identified::{IdentifiedCreditDecision as D, IdentifiedCreditReply},
    },
    workflow::{admission::AdmissionLimits, request::ClientIntent},
};
use wf_ports::log::{
    CodecError, credit_cleanup_intent, credit_cleanup_retained_summary, decode_credit_owner,
    encode_credit_reply,
};

#[test]
fn cleanup_intent_matches_independent_canonical_body_without_header_inputs() -> TestResult {
    let mut hash = Sha256::new();
    hash.update(b"wf-governing-credit-intent-v1\0");
    hash.update([0, 0, 0, 1, b'x']);
    hash.update(4_u64.to_be_bytes());
    hash.update([0, 6]);
    assert_eq!(
        credit_cleanup_intent(b"x", 4)?,
        ClientIntent::governance_credit(hash.finalize().into())
    );
    assert!(credit_cleanup_intent(b"", 4).is_err());
    assert!(credit_cleanup_intent(b"x", 0).is_err());
    assert!(credit_cleanup_intent(&vec![0; 65536], 4).is_err());
    Ok(())
}
#[test]
fn cleanup_summary_validates_complete_original_owner_and_only_cleanup_decisions() -> TestResult {
    let owner = decode_credit_owner(
        &framing::independent_owner(false),
        CreditLimits::default(),
        1024,
        16384,
    )?;
    let reply = IdentifiedCreditReply {
        decision: D::Cleaned {
            deleted: 0,
            pending: false,
        },
        owner: Some(owner),
        reservation_ids: Vec::new(),
    };
    let encoded = encode_credit_reply(&reply, 1024, 16384)?;
    let intent = ClientIntent::governance_credit([1; 32]);
    let summarize = |bytes: &[u8]| {
        credit_cleanup_retained_summary(
            intent,
            10,
            bytes,
            CreditLimits::default(),
            AdmissionLimits::default(),
        )
    };
    assert_eq!(summarize(&encoded)?, (0, false));
    for end in 0..encoded.len() {
        assert!(summarize(&encoded[..end]).is_err());
    }
    let mut trailing = encoded.clone();
    trailing.push(0);
    assert!(summarize(&trailing).is_err());
    assert!(
        credit_cleanup_retained_summary(
            intent,
            9,
            &encoded,
            CreditLimits::default(),
            AdmissionLimits::default()
        )
        .is_err()
    );
    let mut other = reply;
    other.decision = D::Owner;
    assert_eq!(
        summarize(&encode_credit_reply(&other, 1024, 16384)?),
        Err(CodecError::UnsupportedTag)
    );
    Ok(())
}
