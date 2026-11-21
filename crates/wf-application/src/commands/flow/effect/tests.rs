//! Effect records, the ledger and request parsing, ported from Elixir's
//! governance store tests where the business rule is ours too.

use super::denial::decision_id;
use super::request::storage_key;
use super::store::{Effect, Event, Governance, Kind};
use super::{Command, LEDGER_MAX_BYTES, LEDGER_MAX_EVENTS, Status, parse};
use crate::commands::flow::FlowError;
use crate::commands::flow::term::{Term, map};

fn event(id: &str, at_ms: u64, message: Option<Vec<u8>>) -> Event {
    Event {
        id: id.as_bytes().to_vec(),
        kind: if message.is_some() {
            Kind::EffectDenied
        } else {
            Kind::Effect(Status::Reserved)
        },
        at_ms,
        effect_key: b"k".to_vec(),
        effect_type: b"email.send".to_vec(),
        policy_hash: b"h".to_vec(),
        denial: message
            .map(|message| [b"GOVERNANCE_EFFECT_DENIED".to_vec(), message, b"p".to_vec()]),
    }
}

fn governance() -> Governance {
    let mut governance = Governance::default();
    governance.effects.insert(
        b"send".to_vec(),
        Effect {
            effect_type: b"email.send".to_vec(),
            flow_type: b"t".to_vec(),
            state: b"running".to_vec(),
            status: Status::Confirmed,
            operation_digest: b"d".to_vec(),
            idempotency_key: None,
            policy_hash: b"h".to_vec(),
            created_at_ms: 1,
            updated_at_ms: 2,
            external_id: Some(b"x".to_vec()),
            error: None,
            reason: Some(b"r".to_vec()),
            latency_ms: Some(3),
        },
    );
    governance.ledger.push(event("a", 1, None));
    governance.ledger.push(event("b", 2, Some(b"m".to_vec())));
    governance.denials = 1;
    governance.credit = Some(super::store::Credit {
        scope: b"s".to_vec(),
        shard_id: 2,
        reservation_id: b"flr1:1:b:1".to_vec(),
    });
    governance
}

#[test]
fn decision_ids_are_elixirs() {
    // Captured from the Elixir server for Flow `ef-r2` in partition `ep-r2`.
    let id = |reason: &str, subject: &[u8]| String::from_utf8(decision_id(reason, subject));
    assert_eq!(
        id("idempotency_key_required", b"ef-r2:email.send").as_deref(),
        Ok("GIudSJzpGN5eQ_jgYaipVdOP3udnWGbBRL3nA2kCrSA")
    );
    assert_eq!(
        id("effect_ledger_reserved", b"ef-r2:mail").as_deref(),
        Ok("pryBDY4UrA64cEfQ28xlftptZ1SDo9lJ271fYZTVSNA")
    );
    assert_eq!(
        id("effect_terminal", b"mail").as_deref(),
        Ok("c8OSjqr5Rn2FDvQ81X-8WAsnw0Vto1fVYcAxoxTsHH8")
    );
    let key = storage_key(b"ef-r2", Some(b"ep-r2"), b"mail");
    assert_eq!(
        id("effect_conflict", &key).as_deref(),
        Ok("kb-18bReFVhzbFXvTuDI0CbVOSd3rJWSOhmgcFxB4Bc")
    );
}

#[test]
fn stored_records_round_trip_and_refuse_damage() -> Result<(), &'static str> {
    let stored = governance();
    let bytes = stored.encode().ok_or("encode")?;
    assert_eq!(Governance::decode(&bytes), Some(stored));
    // Trailing bytes, an unknown event kind or a missing field are damage.
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert_eq!(Governance::decode(&trailing), None);
    let mut bogus = Vec::new();
    Term::List(vec![
        Term::List(Vec::new()),
        Term::List(vec![map(vec![
            ("id", Term::text("e")),
            ("kind", Term::text("effect_bogus")),
            ("at_ms", Term::Int(1)),
            ("effect_key", Term::text("k")),
            ("effect_type", Term::text("t")),
            ("policy_hash", Term::text("h")),
        ])]),
        Term::Int(0),
        Term::Nil,
    ])
    .encode(&mut bogus)
    .ok_or("encode")?;
    assert_eq!(Governance::decode(&bogus), None);
    let mut missing = Vec::new();
    Term::List(vec![
        Term::List(vec![map(vec![("effect_key", Term::text("k"))])]),
        Term::List(Vec::new()),
        Term::Int(0),
    ])
    .encode(&mut missing)
    .ok_or("encode")?;
    assert_eq!(Governance::decode(&missing), None);
    Ok(())
}

#[test]
fn the_ledger_keeps_the_newest_events_within_its_byte_budget() -> Result<(), String> {
    let mut governance = Governance::default();
    for sequence in 1..=18_u64 {
        let message = vec![b'm'; 60_000];
        governance.append(event(
            &format!("e{sequence}"),
            2_000 + sequence,
            Some(message),
        ))?;
    }
    // The stored form is the ledger plus an empty effect list and a count.
    let stored = governance.encode().ok_or("encode")?;
    assert!(stored.len() <= LEDGER_MAX_BYTES + 32, "{}", stored.len());
    assert!(governance.ledger.len() < 18);
    assert_eq!(
        governance.ledger.last().map(|event| event.at_ms),
        Some(2_018)
    );
    let mut many = Governance::default();
    for sequence in 0..1_005_u64 {
        many.append(event(&format!("n{sequence}"), sequence, None))?;
    }
    assert_eq!(many.ledger.len(), LEDGER_MAX_EVENTS);
    assert_eq!(many.ledger.first().map(|event| event.at_ms), Some(5));
    Ok(())
}

#[test]
fn the_ledger_orders_by_time_and_keeps_an_id_once() -> Result<(), String> {
    let mut governance = Governance::default();
    governance.append(event("late", 10, None))?;
    governance.append(event("early", 5, None))?;
    governance.append(event("tie", 10, None))?;
    let order: Vec<&[u8]> = governance
        .ledger
        .iter()
        .map(|event| event.id.as_slice())
        .collect();
    assert_eq!(order, [b"early".as_slice(), b"late", b"tie"]);
    governance.append(event("late", 10, None))?;
    assert_eq!(governance.ledger.len(), 3);
    assert_eq!(
        governance.append(event("late", 11, None)),
        Err("ERR flow governance ledger event id already exists".to_owned())
    );
    Ok(())
}

fn args(entries: Vec<(&str, Term)>) -> Term {
    map(entries)
}

#[test]
fn requests_are_checked_in_elixirs_order() -> Result<(), FlowError> {
    let lease_missing = parse(
        "FLOW.EFFECT.RESERVE",
        &args(vec![
            ("id", Term::text("f")),
            ("effect_key", Term::text("k")),
            ("effect_type", Term::text("t")),
        ]),
    )?;
    let Command::Reserve(reserve) = lease_missing else {
        return Err(FlowError::Unavailable);
    };
    // Both problems are kept: the shard reports the lease first.
    assert_eq!(
        reserve.lease.err().as_deref(),
        Some("ERR flow lease_token must be a non-empty string")
    );
    assert!(reserve.fields.is_err());
    assert_eq!(
        parse(
            "FLOW.EFFECT.GET",
            &args(vec![("id", Term::text("f")), ("effect_key", Term::Int(5))])
        )
        .err(),
        Some(FlowError::BadRequest(
            "ERR native field effect_key must be binary".to_owned()
        ))
    );
    assert_eq!(
        parse(
            "FLOW.EFFECT.GET",
            &args(vec![
                ("id", Term::text("f")),
                ("effect_key", Term::text(""))
            ])
        )
        .err(),
        Some(FlowError::rejected(
            "ERR flow effect_key must be a non-empty string of at most 65535 bytes".to_owned()
        ))
    );
    for (key, value, text) in [
        (
            "limit",
            Term::Int(0),
            "ERR flow governance ledger limit must be a positive integer",
        ),
        (
            "rev",
            Term::text("yes"),
            "ERR flow governance ledger rev must be boolean",
        ),
        (
            "from_ms",
            Term::Int(-1),
            "ERR flow governance ledger from_ms must be a non-negative integer",
        ),
    ] {
        assert_eq!(
            parse(
                "FLOW.GOVERNANCE.LEDGER",
                &args(vec![("id", Term::text("f")), (key, value)])
            )
            .err(),
            Some(FlowError::rejected(text.to_owned()))
        );
    }
    let Command::Ledger(list) = parse(
        "FLOW.GOVERNANCE.LEDGER",
        &args(vec![("id", Term::text("f")), ("limit", Term::Int(5_000))]),
    )?
    else {
        return Err(FlowError::Unavailable);
    };
    assert_eq!(list.limit, LEDGER_MAX_EVENTS);
    Ok(())
}
