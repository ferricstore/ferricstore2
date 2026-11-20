//! `PolicyPatch.policy_attrs/3`: a patch replaces the stored policy, or is
//! deep-merged into its stored form. Nested maps merge key by key; under
//! `states` a nil state deletes it.

use super::super::term::Term;
use super::FlowPolicy;

type Entries = Vec<(Vec<u8>, Term)>;

fn deep_merge(mut left: Entries, right: &[(Vec<u8>, Term)], top: bool) -> Entries {
    for (key, new) in right {
        let slot = left.iter_mut().find(|(name, _)| name == key);
        let value = match (slot.as_deref().map(|(_, old)| old), new) {
            (Some(Term::Map(old)), Term::Map(patch)) if top && key.as_slice() == b"states" => {
                Term::Map(merge_states(old.clone(), patch))
            }
            (Some(Term::Map(old)), Term::Map(patch)) => {
                Term::Map(deep_merge(old.clone(), patch, false))
            }
            _ => new.clone(),
        };
        match slot {
            Some((_, old)) => *old = value,
            None => left.push((key.clone(), value)),
        }
    }
    left
}

fn merge_states(mut states: Entries, patch: &[(Vec<u8>, Term)]) -> Entries {
    for (state, policy) in patch {
        if matches!(policy, Term::Nil) {
            states.retain(|(name, _)| name != state);
            continue;
        }
        match states.iter_mut().find(|(name, _)| name == state) {
            Some((_, old)) => {
                *old = match (&*old, policy) {
                    (Term::Map(stored), Term::Map(fields)) => {
                        Term::Map(deep_merge(stored.clone(), fields, false))
                    }
                    _ => policy.clone(),
                };
            }
            None => states.push((state.clone(), policy.clone())),
        }
    }
    states
}

/// The attributes a set normalizes: the patch alone when replacing or when
/// no policy is stored, otherwise the patch merged into the stored form.
#[must_use]
pub fn merge(stored: Option<&FlowPolicy>, patch: &[(Vec<u8>, Term)], replace: bool) -> Entries {
    match stored {
        Some(policy) if !replace => match policy.term() {
            Term::Map(entries) => deep_merge(entries, patch, true),
            _ => patch.to_vec(),
        },
        _ => patch.to_vec(),
    }
}
