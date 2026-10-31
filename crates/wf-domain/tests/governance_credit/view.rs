use super::*;
use wf_domain::governance::identified::{
    IdentifiedCreditDecision as D, IdentifiedCreditRejection as R, IdentifiedCreditReply,
};
use wf_domain::governance::view::{CreditLeaseView, CreditReplyView, project_credit_reply};

fn reply(fingerprint: Option<PreparedDigest>) -> TestResult<IdentifiedCreditReply> {
    let owner = fixtures::configured(b"\0scope\xff", 5, 7, fingerprint)?;
    let owner = fixtures::grant(owner, 2, 3, 1000, 1000)?;
    let owner = fixtures::spend(owner, 2, 2, 1001, &[b"a", b"b"])?;
    let mut data = owner.into_data();
    data.cleanup_head = 7;
    data.cleanup_tail = 8;
    let lease = data.leases.first_mut().ok_or("missing lease")?;
    lease.pending_reclaim = 1;
    lease.drain_rate_bits = 2.5_f64.to_bits();
    lease.reservation_page = 1;
    lease.reservation_page_fill = 2;
    Ok(IdentifiedCreditReply {
        decision: D::Owner,
        owner: Some(restore_credit_owner(data, CreditLimits::default())?),
        reservation_ids: Vec::new(),
    })
}

#[test]
fn credit_view_exposes_complete_source_values_and_borrows_original_scope() -> TestResult {
    let reply = reply(None)?;
    let CreditReplyView::Owner(view) = project_credit_reply(&reply)? else {
        return Err("expected public owner".into());
    };
    assert_eq!(view.scope, b"\0scope\xff");
    assert_eq!(
        view.scope.as_ptr(),
        reply
            .owner
            .as_ref()
            .ok_or("missing owner")?
            .view()
            .scope
            .as_ptr()
    );
    assert_eq!(
        (view.limit, view.free, view.epoch, view.config_version),
        (5, 2, 1, 7)
    );
    assert_eq!(view.policy_version_hash, None);
    let mut leases = view.leases();
    assert_eq!(leases.len(), 1);
    assert_eq!(
        leases.next_back(),
        Some(CreditLeaseView {
            shard_id: 2,
            epoch: 1,
            expires_at_ms: 2000,
            available: 1,
            in_use: 2,
            pending_reclaim: 1,
            drain_rate: 2.5,
            last_spend_at_ms: Some(1001),
        })
    );
    assert_eq!(leases.next(), None);
    Ok(())
}

#[test]
fn credit_view_policy_hash_uses_unpadded_url_alphabet_and_fixed_storage() -> TestResult {
    let vectors = [
        ([0; 32], "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"),
        ([255; 32], "__________________________________________8"),
        (
            std::array::from_fn(|i| u8::try_from(i).unwrap_or_default()),
            "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8",
        ),
    ];
    for (bytes, expected) in vectors {
        let reply = reply(Some(PreparedDigest::new(bytes)))?;
        let CreditReplyView::Owner(view) = project_credit_reply(&reply)? else {
            return Err("expected public owner".into());
        };
        let hash = view.policy_version_hash.ok_or("missing public hash")?;
        assert_eq!(hash.as_bytes(), expected.as_bytes());
        assert_eq!(hash.to_string(), expected);
        assert_eq!(std::mem::size_of_val(&hash), 43);
    }
    assert!(std::mem::size_of::<CreditReplyView<'_>>() <= 1024);
    Ok(())
}

#[test]
fn credit_view_shapes_preserve_ids_and_reject_inconsistent_neutral_replies() -> TestResult {
    let mut reply = reply(None)?;
    reply.decision = D::Spent { shard_id: 2 };
    reply.reservation_ids = vec![b"\xffb".to_vec(), b"\0a".to_vec()];
    let CreditReplyView::Spent {
        owner,
        lease,
        reservation_ids,
    } = project_credit_reply(&reply)?
    else {
        return Err("expected spent view".into());
    };
    assert_eq!(owner.scope, b"\0scope\xff");
    assert_eq!(lease.shard_id, 2);
    assert_eq!(reservation_ids.as_ptr(), reply.reservation_ids.as_ptr());
    assert_eq!(reservation_ids, [b"\xffb".to_vec(), b"\0a".to_vec()]);
    reply.decision = D::Owner;
    assert!(project_credit_reply(&reply).is_err());
    reply.reservation_ids.clear();
    reply.decision = D::Granted { shard_id: 1 };
    assert!(project_credit_reply(&reply).is_err());
    for decision in [D::Granted { shard_id: 2 }, D::Renewed { shard_id: 2 }] {
        reply.decision = decision;
        assert!(matches!(
            project_credit_reply(&reply)?,
            CreditReplyView::Lease { .. }
        ));
    }
    reply.decision = D::Cleaned {
        deleted: 256,
        pending: true,
    };
    assert!(matches!(
        project_credit_reply(&reply)?,
        CreditReplyView::Cleaned {
            deleted: 256,
            pending: true
        }
    ));
    reply.decision = D::Cleaned {
        deleted: 257,
        pending: true,
    };
    assert!(project_credit_reply(&reply).is_err());
    reply.decision = D::Cleaned {
        deleted: 0,
        pending: false,
    };
    assert!(project_credit_reply(&reply).is_err());
    reply.decision = D::Rejected(R::NotFound);
    assert!(project_credit_reply(&reply).is_err());
    reply.owner = None;
    assert!(matches!(
        project_credit_reply(&reply)?,
        CreditReplyView::Rejected(R::NotFound)
    ));
    reply.decision = D::Owner;
    assert!(project_credit_reply(&reply).is_err());
    Ok(())
}
