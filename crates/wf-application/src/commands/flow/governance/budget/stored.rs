//! A budget's stored form: its summary, its unsettled and settled
//! reservations (each with its window), and the generated-id count.

use std::collections::BTreeMap;

use super::super::super::term::{Term, map};
use super::{Budget, Reservation, Status};

fn int(term: &Term, key: &str) -> Option<u64> {
    match term.get(key.as_bytes()) {
        Some(Term::Int(value)) => u64::try_from(*value).ok(),
        _ => None,
    }
}

fn text(term: &Term, key: &str) -> Option<Vec<u8>> {
    match term.get(key.as_bytes()) {
        Some(Term::Bin(value)) => Some(value.clone()),
        _ => None,
    }
}

fn reservation_term(reservation: &Reservation) -> Term {
    let mut entries = reservation.view();
    entries.push(("window_start_ms", Term::int(reservation.window_start_ms)));
    map(entries)
}

fn reservation(term: &Term) -> Option<Reservation> {
    let status = match text(term, "status")?.as_slice() {
        b"reserved" => Status::Reserved,
        b"committed" => Status::Committed,
        b"released" => Status::Released,
        _ => return None,
    };
    let optional = |key: &str| match term.get(key.as_bytes()) {
        None | Some(Term::Nil) => Some(None),
        Some(Term::Int(value)) => u64::try_from(*value).ok().map(Some),
        Some(_) => None,
    };
    Some(Reservation {
        id: text(term, "reservation_id")?,
        amount: int(term, "reserved_amount")?,
        actual_amount: optional("actual_amount")?,
        status,
        usage: term
            .get(b"usage")
            .filter(|usage| **usage != Term::Nil)
            .cloned(),
        overage_amount: int(term, "overage_amount")?,
        reserved_at_ms: int(term, "reserved_at_ms")?,
        settled_at_ms: optional("settled_at_ms")?,
        window_start_ms: int(term, "window_start_ms")?,
    })
}

fn reservations(term: &Term, key: &str) -> Option<Vec<Reservation>> {
    match term.get(key.as_bytes()) {
        Some(Term::List(items)) => items.iter().map(reservation).collect(),
        _ => None,
    }
}

impl Budget {
    /// The stored form.
    #[must_use]
    pub fn encode(&self) -> Option<Vec<u8>> {
        let mut entries = self.summary();
        entries.extend([
            (
                "reservations",
                Term::List(self.reservations.values().map(reservation_term).collect()),
            ),
            (
                "settled",
                Term::List(self.settled.iter().map(reservation_term).collect()),
            ),
            ("generated", Term::int(self.generated)),
        ]);
        let mut out = Vec::new();
        map(entries).encode(&mut out)?;
        Some(out)
    }

    /// Reads the stored form; `None` when it is not a budget.
    #[must_use]
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let Some((term, [])) = Term::decode(bytes) else {
            return None;
        };
        let active: BTreeMap<Vec<u8>, Reservation> = reservations(&term, "reservations")?
            .into_iter()
            .map(|reservation| (reservation.id.clone(), reservation))
            .collect();
        if active
            .values()
            .any(|reservation| reservation.status != Status::Reserved)
        {
            return None;
        }
        Some(Self {
            scope: text(&term, "scope")?,
            limit: int(&term, "limit")?,
            window_ms: int(&term, "window_ms").filter(|window| *window > 0)?,
            window_start_ms: int(&term, "window_start_ms")?,
            used: int(&term, "used")?,
            reservations: active,
            settled: reservations(&term, "settled")?,
            generated: int(&term, "generated")?,
        })
    }
}
