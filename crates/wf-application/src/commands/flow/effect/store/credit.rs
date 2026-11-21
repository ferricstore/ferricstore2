//! The limit credit a Flow holds, and its stored form.

use super::super::super::term::Term;

/// A limit credit a governed claim gave a Flow; released when the Flow
/// leaves `running` (ADR-014 step 15).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Credit {
    /// The limit's scope.
    pub scope: Vec<u8>,
    /// The lease's shard id.
    pub shard_id: u64,
    /// The credit's reservation id.
    pub reservation_id: Vec<u8>,
}

/// The stored form: scope, shard id and reservation id, or nil.
pub(super) fn term(credit: Option<&Credit>) -> Term {
    credit.map_or(Term::Nil, |credit| {
        Term::List(vec![
            Term::Bin(credit.scope.clone()),
            Term::int(credit.shard_id),
            Term::Bin(credit.reservation_id.clone()),
        ])
    })
}

/// A stored credit that is neither nil nor a credit.
pub(super) struct Damaged;

/// Reads the stored form.
pub(super) fn decode(term: &Term) -> Result<Option<Credit>, Damaged> {
    let parts = match term {
        Term::Nil => return Ok(None),
        Term::List(parts) => parts,
        _ => return Err(Damaged),
    };
    let [
        Term::Bin(scope),
        Term::Int(shard_id),
        Term::Bin(reservation_id),
    ] = parts.as_slice()
    else {
        return Err(Damaged);
    };
    Ok(Some(Credit {
        scope: scope.clone(),
        shard_id: u64::try_from(*shard_id).map_err(|_| Damaged)?,
        reservation_id: reservation_id.clone(),
    }))
}
