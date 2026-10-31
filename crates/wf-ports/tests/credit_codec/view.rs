use super::{TestResult, framing};
use wf_domain::governance::credit::CreditLimits;
use wf_domain::governance::identified::{
    IdentifiedCreditDecision as D, IdentifiedCreditRejection as R, IdentifiedCreditReply,
};
use wf_domain::governance::view::CreditReplyView;
use wf_ports::log::{
    CodecError, decode_credit_owner, encode_credit_reply, with_public_credit_reply,
};

fn encoded(decision: D) -> TestResult<Vec<u8>> {
    let owner = if decision == D::Rejected(R::NotFound) {
        None
    } else {
        Some(decode_credit_owner(
            &framing::independent_owner(true),
            CreditLimits::default(),
            1024,
            16_384,
        )?)
    };
    let reply = IdentifiedCreditReply {
        decision,
        owner,
        reservation_ids: if matches!(decision, D::Spent { .. }) {
            vec![b"\xffb".to_vec(), b"\0a".to_vec()]
        } else {
            Vec::new()
        },
    };
    Ok(encode_credit_reply(&reply, 1024, 16_384)?)
}

#[test]
fn credit_public_reply_visits_every_source_shape_without_internal_owner_fields() -> TestResult {
    let cases = [
        (D::Granted { shard_id: 0 }, "lease"),
        (D::Renewed { shard_id: 0 }, "lease"),
        (D::Spent { shard_id: 0 }, "spent"),
        (D::Released, "owner"),
        (D::Owner, "owner"),
        (
            D::Cleaned {
                deleted: 0,
                pending: false,
            },
            "cleaned",
        ),
        (D::Rejected(R::NotFound), "rejected"),
    ];
    for (decision, expected) in cases {
        let raw = encoded(decision)?;
        let kind =
            with_public_credit_reply(
                &raw,
                CreditLimits::default(),
                1024,
                32_768,
                |view| match view {
                    CreditReplyView::Lease { owner, lease } => {
                        assert_eq!(
                            (owner.scope, owner.limit, owner.free),
                            (b"x".as_slice(), 3, 0)
                        );
                        assert_eq!(lease.shard_id, 0);
                        "lease"
                    }
                    CreditReplyView::Spent {
                        owner,
                        lease,
                        reservation_ids,
                    } => {
                        assert_eq!(owner.leases().len(), 1);
                        assert_eq!(
                            (lease.available, lease.in_use, lease.last_spend_at_ms),
                            (1, 2, Some(1001))
                        );
                        assert_eq!(reservation_ids, [b"\xffb".to_vec(), b"\0a".to_vec()]);
                        "spent"
                    }
                    CreditReplyView::Owner(owner) => {
                        assert_eq!((owner.epoch, owner.config_version), (1, 1));
                        "owner"
                    }
                    CreditReplyView::Cleaned { deleted, pending } => {
                        assert_eq!((deleted, pending), (0, false));
                        "cleaned"
                    }
                    CreditReplyView::Rejected(reason) => {
                        assert_eq!(reason, R::NotFound);
                        "rejected"
                    }
                },
            )?;
        assert_eq!(kind, expected);
    }
    Ok(())
}

#[test]
fn credit_public_reply_never_visits_incomplete_corrupt_or_limited_frames() -> TestResult {
    let raw = encoded(D::Spent { shard_id: 0 })?;
    assert!(with_public_credit_reply(&raw, CreditLimits::default(), 1024, 32_768, |_| ()).is_ok());
    let mut variants = (0..raw.len())
        .map(|end| raw[..end].to_vec())
        .collect::<Vec<_>>();
    let mut trailing = raw.clone();
    trailing.push(0);
    variants.push(trailing);
    let mut duplicate = raw.clone();
    let end = duplicate.len();
    duplicate[end - 2..].copy_from_slice(b"\xffb");
    variants.push(duplicate);
    for bad in variants {
        let mut visited = false;
        assert!(
            with_public_credit_reply(&bad, CreditLimits::default(), 1024, 32_768, |_| visited =
                true)
            .is_err()
        );
        assert!(!visited);
    }
    for (wire, owned, credit, expected) in [
        (
            raw.len() - 1,
            32_768,
            CreditLimits::default(),
            CodecError::ByteLimit,
        ),
        (1024, 1024, CreditLimits::default(), CodecError::OwnedLimit),
        (
            1024,
            32_768,
            CreditLimits::new(8, 8, 32_768, 1024)?,
            CodecError::WorkLimit,
        ),
    ] {
        let mut visited = false;
        assert_eq!(
            with_public_credit_reply(&raw, credit, wire, owned, |_| visited = true),
            Err(expected)
        );
        assert!(!visited);
    }
    Ok(())
}
