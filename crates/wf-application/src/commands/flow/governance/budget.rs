//! Budgets, as `Governance.Budget` and `BudgetStore` keep them: a scope's
//! fixed-window allowance. A reservation holds part of the window's limit
//! until it is committed (with the amount actually used, which may run
//! over) or released. A window that has passed resets the usage and keeps
//! the unsettled reservations; settling one from an earlier window leaves
//! the current usage alone. A refusal after a reset still stores it.

use std::collections::BTreeMap;

use super::super::FlowError;
use super::super::MAX_EXACT;
use super::super::effect::denial::{Denial, Field};
use super::super::term::{Term, map};

mod decide;
mod request;
mod stored;

pub use request::{List, parse, parse_list};

const SETTLED_KEPT: usize = 128;
const MAX_ACTIVE: usize = 4_096;

/// Where a reservation is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Holding part of the limit.
    Reserved,
    /// Settled with the amount used.
    Committed,
    /// Settled with nothing used.
    Released,
}

impl Status {
    /// The status as Elixir names it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Reserved => "reserved",
            Self::Committed => "committed",
            Self::Released => "released",
        }
    }
}

/// One reservation.
#[derive(Clone, Debug, PartialEq)]
pub struct Reservation {
    /// Its id.
    pub id: Vec<u8>,
    /// The amount held.
    pub amount: u64,
    /// The amount used, once settled.
    pub actual_amount: Option<u64>,
    /// Where it is.
    pub status: Status,
    /// What the commit reported using.
    pub usage: Option<Term>,
    /// How far the commit ran over.
    pub overage_amount: u64,
    /// When it was made.
    pub reserved_at_ms: u64,
    /// When it was settled.
    pub settled_at_ms: Option<u64>,
    /// The window it was made in.
    pub window_start_ms: u64,
}

/// One budget.
#[derive(Clone, Debug, PartialEq)]
pub struct Budget {
    /// Its scope.
    pub scope: Vec<u8>,
    /// The allowance per window.
    pub limit: u64,
    /// The window's length.
    pub window_ms: u64,
    /// When the current window started.
    pub window_start_ms: u64,
    /// Used in the current window.
    pub used: u64,
    /// Unsettled reservations by id.
    pub reservations: BTreeMap<Vec<u8>, Reservation>,
    /// The current window's settled reservations, newest first.
    pub settled: Vec<Reservation>,
    /// Reservation ids generated so far, numbering the next.
    pub generated: u64,
}

/// A command on one budget.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// `FLOW.BUDGET.RESERVE`.
    Reserve {
        /// The scope.
        scope: Vec<u8>,
        /// The amount.
        amount: u64,
        /// The client's clock.
        now_ms: Option<u64>,
        /// The reservation's id, or one is generated.
        reservation_id: Option<Vec<u8>>,
        /// A new budget's limit and window, as given (checked on creation).
        shape: (Option<Term>, Option<Term>),
    },
    /// `FLOW.BUDGET.COMMIT` with the amount used, or `RELEASE`.
    Settle {
        /// The scope.
        scope: Vec<u8>,
        /// The reservation.
        reservation_id: Vec<u8>,
        /// The amount used; `None` releases.
        actual_amount: Option<u64>,
        /// What was used.
        usage: Option<Term>,
        /// The client's clock.
        now_ms: Option<u64>,
    },
    /// `FLOW.BUDGET.GET`.
    Get(Vec<u8>),
}

fn rejected(text: &str) -> FlowError {
    FlowError::rejected(text.to_owned())
}

impl Reservation {
    fn view(&self) -> Vec<(&'static str, Term)> {
        vec![
            ("reservation_id", Term::Bin(self.id.clone())),
            ("reserved_amount", Term::int(self.amount)),
            ("actual_amount", Term::optional_int(self.actual_amount)),
            ("status", Term::text(self.status.name())),
            ("usage", self.usage.clone().unwrap_or(Term::Nil)),
            ("overage_amount", Term::int(self.overage_amount)),
            ("reserved_at_ms", Term::int(self.reserved_at_ms)),
            ("settled_at_ms", Term::optional_int(self.settled_at_ms)),
        ]
    }
}

impl Budget {
    fn summary(&self) -> Vec<(&'static str, Term)> {
        let count = self.reservations.len().saturating_add(self.settled.len());
        vec![
            ("scope", Term::Bin(self.scope.clone())),
            ("limit", Term::int(self.limit)),
            ("window_ms", Term::int(self.window_ms)),
            ("window_start_ms", Term::int(self.window_start_ms)),
            ("used", Term::int(self.used)),
            ("remaining", Term::int(self.limit.saturating_sub(self.used))),
            ("over_budget", Term::Bool(self.used > self.limit)),
            (
                "reservations_count",
                Term::int(u64::try_from(count).unwrap_or(u64::MAX)),
            ),
        ]
    }

    /// The budget as `FLOW.BUDGET.GET` replies with it.
    #[must_use]
    pub fn view(&self) -> Term {
        map(self.summary())
    }

    fn reply(&self, reservation: &Reservation) -> Term {
        let mut entries = self.summary();
        entries.extend(reservation.view());
        map(entries)
    }

    /// `maybe_reset/2`.
    fn roll(&mut self, now: u64) {
        if now.saturating_sub(self.window_start_ms) >= self.window_ms {
            self.used = 0;
            self.window_start_ms = now;
            self.settled.clear();
        }
    }

    fn settle(&mut self, reservation: Reservation) {
        self.reservations.remove(&reservation.id);
        self.settled.retain(|held| held.id != reservation.id);
        self.settled.insert(0, reservation);
        self.settled.truncate(SETTLED_KEPT);
    }

    /// The next generated id: 16 bytes of a digest over the scope and a
    /// count, as long as Elixir's random ones.
    fn generated_id(&self) -> Vec<u8> {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(&self.scope);
        hasher.update(self.generated.saturating_add(1).to_be_bytes());
        let digest = hasher.finalize();
        crate::routing::base64url_nopad(digest.get(..16).unwrap_or(&digest)).into_bytes()
    }

    fn reserve(
        &mut self,
        amount: u64,
        id: Option<&Vec<u8>>,
        now: u64,
    ) -> Result<Reservation, FlowError> {
        let id = id.cloned().unwrap_or_else(|| self.generated_id());
        if let Some(active) = self.reservations.get(&id) {
            return if active.amount == amount {
                Ok(active.clone())
            } else {
                Err(rejected("ERR flow budget reservation id already exists"))
            };
        }
        if self.settled.iter().any(|held| held.id == id) {
            return Err(rejected("ERR flow budget reservation id already exists"));
        }
        if self.reservations.len() >= MAX_ACTIVE {
            return Err(rejected("ERR flow budget has too many active reservations"));
        }
        if self.used.saturating_add(amount) > self.limit {
            let retry = self
                .window_ms
                .saturating_sub(now.saturating_sub(self.window_start_ms));
            let denial = Denial::new(
                "GOVERNANCE_BUDGET_EXHAUSTED",
                b"Governance budget exhausted".to_vec(),
                vec![("scope", Some(self.scope.clone()))],
            )
            .with("requested", Field::Int(amount))
            .with("used", Field::Int(self.used))
            .with("limit", Field::Int(self.limit))
            .with("window_ms", Field::Int(self.window_ms))
            .with("retry_after_ms", Field::Int(retry));
            return Err(FlowError::Governance(Box::new(denial)));
        }
        let reservation = Reservation {
            id: id.clone(),
            amount,
            actual_amount: None,
            status: Status::Reserved,
            usage: None,
            overage_amount: 0,
            reserved_at_ms: now,
            settled_at_ms: None,
            window_start_ms: self.window_start_ms,
        };
        self.used = self.used.saturating_add(amount);
        if self.generated_id() == id {
            self.generated = self.generated.saturating_add(1);
        }
        self.reservations.insert(id, reservation.clone());
        Ok(reservation)
    }

    fn settled_result(&self, id: &[u8], actual: Option<u64>) -> Result<Reservation, FlowError> {
        match self.settled.iter().find(|held| held.id == id) {
            Some(held)
                if actual.is_some()
                    && held.status == Status::Committed
                    && held.actual_amount == actual =>
            {
                Ok(held.clone())
            }
            Some(held) if actual.is_none() && held.status == Status::Released => Ok(held.clone()),
            Some(held) => Err(FlowError::rejected(format!(
                "ERR flow budget reservation already {}",
                held.status.name()
            ))),
            None => Err(rejected("ERR flow budget reservation not found")),
        }
    }

    /// `committed_used/3`: a commit in the current window moves the usage
    /// by what it used over or under its hold; one from an earlier window
    /// leaves it. Returns the overage.
    fn charge(&mut self, held: &Reservation, actual: u64, current: bool) -> Result<u64, FlowError> {
        if !current {
            return Ok(actual.saturating_sub(held.amount));
        }
        let used = self.used.saturating_add(actual).saturating_sub(held.amount);
        if used > MAX_EXACT {
            return Err(rejected(
                "ERR flow budget aggregate usage exceeds durable integer range",
            ));
        }
        self.used = used;
        Ok(used.saturating_sub(self.limit))
    }

    fn settle_one(
        &mut self,
        id: &[u8],
        (actual, usage): (Option<u64>, Option<&Term>),
        now: u64,
    ) -> Result<Reservation, FlowError> {
        let Some(active) = self.reservations.get(id).cloned() else {
            return self.settled_result(id, actual);
        };
        if now < active.reserved_at_ms {
            return Err(rejected(
                "ERR flow budget now_ms cannot precede reservation",
            ));
        }
        let current = active.window_start_ms == self.window_start_ms;
        let mut settled = active;
        settled.settled_at_ms = Some(now);
        match actual {
            None => {
                if current {
                    self.used = self.used.saturating_sub(settled.amount);
                }
                settled.actual_amount = Some(0);
                settled.status = Status::Released;
            }
            Some(actual) => {
                if usage.is_some_and(|usage| !request::portable(usage)) {
                    return Err(rejected(
                        "ERR flow budget usage must be a bounded portable term",
                    ));
                }
                settled.overage_amount = self.charge(&settled, actual, current)?;
                settled.actual_amount = Some(actual);
                settled.status = Status::Committed;
                settled.usage = usage.cloned();
            }
        }
        self.settle(settled.clone());
        Ok(settled)
    }
}
