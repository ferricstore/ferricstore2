//! `FLOW.SIGNAL` (ADR-014 step 7), read as `Ferricstore.Flow.Signal` reads
//! it.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use super::FlowError;
use super::create::{partition_key, values};
use super::opts::Opts;
use super::term::Term;

/// One signal to one Flow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signal {
    /// Flow id.
    pub id: Vec<u8>,
    /// Partition, if sent.
    pub partition_key: Option<Vec<u8>>,
    /// Signal name, recorded in history.
    pub signal: Vec<u8>,
    /// States the Flow must be in for `transition_to`.
    pub if_state: Vec<Vec<u8>>,
    /// State to move to, releasing any lease.
    pub transition_to: Option<Vec<u8>>,
    /// A repeat under this key with the same content changes nothing.
    pub idempotency_key: Option<Vec<u8>>,
    /// Client clock.
    pub now_ms: Option<u64>,
    /// Due time after a transition; the command time when absent.
    pub run_at_ms: Option<u64>,
    /// Named values to write.
    pub values: BTreeMap<Vec<u8>, Vec<u8>>,
    /// References attached by name (audit gap G9).
    pub value_refs: BTreeMap<Vec<u8>, Vec<u8>>,
    /// Named values to drop.
    pub drop_values: Vec<Vec<u8>>,
    /// Named values whose bytes may change.
    pub override_values: Vec<Vec<u8>>,
}

impl Signal {
    /// The digest an idempotency key remembers: every field that decides
    /// what the signal does.
    #[must_use]
    pub fn digest(&self) -> Vec<u8> {
        let mut hasher = Sha256::new();
        let mut part = |bytes: &[u8]| {
            hasher.update(u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_be_bytes());
            hasher.update(bytes);
        };
        part(&self.signal);
        for state in &self.if_state {
            part(state);
        }
        part(b"|");
        part(self.transition_to.as_deref().unwrap_or_default());
        part(
            &self
                .run_at_ms
                .map_or_else(Vec::new, |at| at.to_be_bytes().to_vec()),
        );
        for (name, bytes) in &self.values {
            part(name);
            part(bytes);
        }
        part(b"|");
        for name in self.drop_values.iter().chain(&self.override_values) {
            part(name);
        }
        part(b"|");
        for (name, reference) in &self.value_refs {
            part(name);
            part(reference);
        }
        hasher.finalize().to_vec()
    }
}

fn if_states(opts: &Opts<'_>) -> Result<Vec<Vec<u8>>, FlowError> {
    let invalid = || FlowError::flow("if_state", "must be a non-empty string");
    match opts.get("if_state") {
        None | Some(Term::Nil) => Ok(Vec::new()),
        Some(Term::Bin(state)) if !state.is_empty() => Ok(vec![state.clone()]),
        Some(Term::List(states)) => states
            .iter()
            .map(|state| match state {
                Term::Bin(state) if !state.is_empty() => Ok(state.clone()),
                _ => Err(invalid()),
            })
            .collect(),
        Some(_) => Err(invalid()),
    }
}

/// Reads `FLOW.SIGNAL`.
///
/// # Errors
/// The Elixir server's refusal text for an invalid option.
pub fn parse(args: &Term) -> Result<Signal, FlowError> {
    let opts = Opts::new(
        "FLOW.SIGNAL",
        args,
        &[
            "id",
            "signal",
            "if_state",
            "transition_to",
            "idempotency_key",
            "partition_key",
            "now_ms",
            "run_at_ms",
            "values",
            "value_refs",
            "drop_values",
            "override_values",
        ],
    )?;
    let id = match opts.get("id") {
        Some(Term::Bin(id)) if !id.is_empty() => id.clone(),
        _ => return Err(FlowError::flow("id", "must be a non-empty string")),
    };
    let signal = opts.required_binary("signal")?;
    let if_state = if_states(&opts)?;
    let transition_to = opts.binary_or_nil("transition_to")?;
    if transition_to.as_deref() == Some(b"running".as_slice()) {
        return Err(FlowError::rejected(
            "ERR flow running state is only entered by FLOW.CLAIM_DUE".to_owned(),
        ));
    }
    let idempotency_key = opts.binary_or_nil("idempotency_key")?;
    if idempotency_key
        .as_ref()
        .is_some_and(|key| key.len() > 4_096)
    {
        return Err(FlowError::rejected(
            "ERR flow idempotency_key too large (max 4096 bytes)".to_owned(),
        ));
    }
    Ok(Signal {
        id,
        partition_key: partition_key(&opts)?,
        signal,
        if_state,
        transition_to,
        idempotency_key,
        now_ms: opts.non_neg_integer("now_ms")?,
        run_at_ms: opts.non_neg_integer("run_at_ms")?,
        values: values(&opts)?,
        value_refs: super::create::value_refs(&opts)?,
        drop_values: opts.binary_list("drop_values")?,
        override_values: opts.binary_list("override_values")?,
    })
}
