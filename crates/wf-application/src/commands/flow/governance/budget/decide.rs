//! Deciding a budget command from the budget's stored form.

use super::super::super::FlowError;
use super::super::super::term::Term;
use super::super::{Change, Decided};
use super::{Budget, Command, rejected, request};

impl Command {
    /// The budget's scope.
    #[must_use]
    pub fn scope(&self) -> &[u8] {
        match self {
            Self::Reserve { scope, .. } | Self::Settle { scope, .. } | Self::Get(scope) => scope,
        }
    }

    /// Decides the command from the budget's stored form; a refusal after
    /// a window reset or a new budget still stores it, as Elixir does.
    pub fn decide(&self, stored: Option<&[u8]>, wall: u64) -> Decided {
        let existing = match stored.map(Budget::decode) {
            None => None,
            Some(Some(budget)) => Some(budget),
            Some(None) => return (Err(rejected("ERR flow budget record is corrupt")), None),
        };
        let (now, mut budget) = match self {
            Self::Get(_) => return (Ok(existing.map_or(Term::Nil, |budget| budget.view())), None),
            Self::Reserve {
                scope,
                now_ms,
                shape,
                ..
            } => {
                let now = now_ms.unwrap_or(wall);
                match existing.clone() {
                    Some(budget) => (now, budget),
                    None => match request::new_budget(scope, shape, now) {
                        Ok(budget) => (now, budget),
                        Err(error) => return (Err(error), None),
                    },
                }
            }
            Self::Settle { now_ms, .. } => match existing.clone() {
                Some(budget) => (now_ms.unwrap_or(wall), budget),
                None => return (Err(rejected("ERR flow budget not found")), None),
            },
        };
        if now < budget.window_start_ms {
            return (
                Err(rejected(
                    "ERR flow budget now_ms cannot precede window_start_ms",
                )),
                None,
            );
        }
        budget.roll(now);
        let result = match self {
            Self::Reserve {
                amount,
                reservation_id,
                ..
            } => budget.reserve(*amount, reservation_id.as_ref(), now),
            Self::Settle {
                reservation_id,
                actual_amount,
                usage,
                ..
            } => budget.settle_one(reservation_id, (*actual_amount, usage.as_ref()), now),
            Self::Get(_) => Err(FlowError::Unavailable),
        };
        let reply = result.map(|reservation| budget.reply(&reservation));
        if existing.as_ref() == Some(&budget) {
            return (reply, None);
        }
        match budget.encode() {
            Some(bytes) => (reply, Some(Change::Put(bytes))),
            None => (Err(FlowError::Unavailable), None),
        }
    }
}
