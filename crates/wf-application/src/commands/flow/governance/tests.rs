//! Approvals' stored form, ported from Elixir's governance store tests:
//! a record with trailing bytes or an invalid shape is refused as corrupt.

use super::approval::{Approval, Command, Status, Version};
use super::{Change, parse};
use crate::commands::flow::FlowError;
use crate::commands::flow::term::{Term, map};

fn approval() -> Approval {
    Approval {
        id: b"a1".to_vec(),
        flow_id: b"f1".to_vec(),
        scope: b"s".to_vec(),
        reason: Some(b"why".to_vec()),
        requested_by: None,
        requested_at_ms: 10,
        assignees: vec![b"ann".to_vec()],
        policy_hash: None,
        policy_version: Some(Version::Number(7)),
        expires_at_ms: Some(20),
        status: Status::Pending,
        decided_by: None,
        decided_at_ms: None,
        decision_reason: None,
    }
}

#[test]
fn stored_approvals_round_trip_and_refuse_damage() -> Result<(), &'static str> {
    let stored = approval();
    let bytes = stored.encode().ok_or("encode")?;
    assert_eq!(Approval::decode(&bytes), Some(stored));
    let mut trailing = bytes;
    trailing.push(0);
    assert_eq!(Approval::decode(&trailing), None);
    for (key, value) in [
        ("status", Term::text("bogus")),
        ("assignees", Term::List(vec![Term::Int(1)])),
        ("requested_at_ms", Term::Int(-1)),
    ] {
        let Term::Map(mut entries) = approval().view() else {
            return Err("view is not a map");
        };
        entries.retain(|(held, _)| held.as_slice() != key.as_bytes());
        entries.push((key.as_bytes().to_vec(), value));
        let mut damaged = Vec::new();
        Term::Map(entries).encode(&mut damaged).ok_or("encode")?;
        assert_eq!(Approval::decode(&damaged), None, "{key}");
    }
    Ok(())
}

#[test]
fn a_corrupt_record_is_refused_and_left_alone() -> Result<(), FlowError> {
    let command = parse("FLOW.APPROVAL.GET", &map(vec![("id", Term::text("a1"))]))?;
    let (reply, change) = command.decide(Some(b"\x00"), 0);
    assert_eq!(
        reply,
        Err(FlowError::rejected(
            "ERR flow approval record is corrupt".to_owned()
        ))
    );
    assert_eq!(change, None);
    let decide = parse(
        "FLOW.APPROVAL.APPROVE",
        &map(vec![
            ("id", Term::text("a1")),
            ("approver", Term::text("x")),
        ]),
    )?;
    let stored = approval().encode().ok_or(FlowError::Unavailable)?;
    let (reply, change) = decide.decide(Some(&stored), 15);
    assert_eq!(
        reply.map(|view| view.get(b"status").cloned()),
        Ok(Some(Term::text("approved")))
    );
    assert!(matches!(change, Some(Change::Put(_))));
    Ok(())
}

#[test]
fn the_id_is_checked_by_the_native_layer_then_the_store() {
    assert_eq!(
        parse("FLOW.APPROVAL.GET", &map(vec![("id", Term::Int(5))])).err(),
        Some(FlowError::BadRequest(
            "ERR native field id must be binary".to_owned()
        ))
    );
    assert_eq!(
        parse("FLOW.APPROVAL.GET", &map(vec![("id", Term::text(""))])).err(),
        Some(FlowError::rejected(
            "ERR flow approval id must be a non-empty string".to_owned()
        ))
    );
    assert!(matches!(
        parse("FLOW.APPROVAL.GET", &map(vec![("id", Term::text("a"))])),
        Ok(super::Command::Approval(Command::Get(_)))
    ));
}

#[test]
fn stored_circuits_round_trip_and_refuse_damage() -> Result<(), FlowError> {
    let command = parse(
        "FLOW.CIRCUIT.OPEN",
        &map(vec![
            ("scope", Term::text("c")),
            ("window_ms", Term::Int(10)),
            ("now_ms", Term::Int(5)),
        ]),
    )?;
    let (reply, change) = command.decide(None, 0);
    let Some(Change::Put(bytes)) = change else {
        return Err(FlowError::Unavailable);
    };
    let stored = super::circuit::Circuit::decode(&bytes).ok_or(FlowError::Unavailable)?;
    assert_eq!(Ok(stored.view()), reply);
    let mut trailing = bytes;
    trailing.push(0);
    assert_eq!(super::circuit::Circuit::decode(&trailing), None);
    let get = parse("FLOW.CIRCUIT.GET", &map(vec![("scope", Term::text("c"))]))?;
    assert_eq!(
        get.decide(Some(&trailing), 0).0,
        Err(FlowError::rejected(
            "ERR flow circuit record is corrupt".to_owned()
        ))
    );
    Ok(())
}

#[test]
fn policy_version_fingerprints_are_elixirs() {
    use crate::routing::base64url_nopad;
    // Captured from the Elixir server's `policy_version_hash`.
    for (version, expected) in [
        (
            Term::text("pv1"),
            "mvjJaFOrYFE14c6NPWj3-0MhidqTGTqRVR0_vDpP4vU",
        ),
        (Term::Int(7), "DkKNcZoMDC2l16Duf-rTdbJKU3ZmpNKz1IsFovkce6U"),
        (
            Term::Int(70_000),
            "hUEWlrXw7heLoCLP_Bg5Pr0zvPQXAi4jv0xf3s5zpi0",
        ),
        (
            Term::Int(9_000_000_000_000),
            "eL4JmqkiVptXy5qN8dDtkAIDoOX3Oi4MLQhshTOPtUg",
        ),
    ] {
        let digest = super::limit::fingerprint(&version).map(|digest| base64url_nopad(&digest));
        assert_eq!(digest.as_deref(), Some(expected), "{version:?}");
    }
}

#[test]
fn stored_limits_round_trip_and_refuse_damage() -> Result<(), &'static str> {
    let owner = super::limit::Owner::new(b"s", 5, 1, super::limit::fingerprint(&Term::Int(1)));
    let bytes = owner.encode().ok_or("encode")?;
    assert_eq!(super::limit::Owner::decode(&bytes), Some(owner));
    let mut trailing = bytes;
    trailing.push(0);
    assert_eq!(super::limit::Owner::decode(&trailing), None);
    Ok(())
}

#[test]
fn a_budget_holds_at_most_4096_active_reservations() -> Result<(), &'static str> {
    // Elixir's `flow_governance_exhaustive_review_test.exs`: "budget active
    // reservations are bounded without breaking idempotent retries".
    use super::budget::{Budget, Command as BudgetCommand, Reservation, Status as Held};
    let reservations = (1..=4_096_u64)
        .map(|index| {
            let id = format!("reservation-{index}").into_bytes();
            let held = Reservation {
                id: id.clone(),
                amount: 1,
                actual_amount: None,
                status: Held::Reserved,
                usage: None,
                overage_amount: 0,
                reserved_at_ms: 0,
                settled_at_ms: None,
                window_start_ms: 0,
            };
            (id, held)
        })
        .collect();
    let budget = Budget {
        scope: b"scope".to_vec(),
        limit: 4_097,
        window_ms: 1_000,
        window_start_ms: 0,
        used: 4_096,
        reservations,
        settled: Vec::new(),
        generated: 0,
    };
    let stored = budget.encode().ok_or("the budget does not encode")?;
    let reserve = |id: &str| BudgetCommand::Reserve {
        scope: b"scope".to_vec(),
        amount: 1,
        now_ms: Some(1),
        reservation_id: Some(id.as_bytes().to_vec()),
        shape: (None, None),
    };
    let (retried, _) = reserve("reservation-1").decide(Some(&stored), 1);
    let retried = retried.map_err(|_| "a retry was refused")?;
    assert_eq!(retried.get(b"used"), Some(&Term::Int(4_096)));
    let (overflow, _) = reserve("overflow").decide(Some(&stored), 1);
    assert_eq!(
        overflow.err().map(|error| error.to_string()),
        Some("ERR flow budget has too many active reservations".to_owned())
    );
    Ok(())
}
