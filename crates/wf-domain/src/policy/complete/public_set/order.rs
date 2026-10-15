//! Reached capture correspondence, without native-order or payment authority.

use super::{Error, PolicySchemaCharge, PolicySchemaError, charge_schema};

/// Borrowed server-supplied map-order facts for one original public request.
///
/// Permutations prove correspondence only. Their origin and agreement with the
/// captured VM remain caller obligations, freshly checked where represented.
pub struct OriginalPolicySetMapOrders<'capture> {
    patch_identity: [u8; 32],
    current_encoded: Option<&'capture [u8]>,
    patch: &'capture [(u32, &'capture [u32], &'capture [u32])],
    current: &'capture [(u32, &'capture [u32], &'capture [u32])],
    merged: &'capture [(u32, &'capture [u32], &'capture [u32])],
}

impl<'capture> OriginalPolicySetMapOrders<'capture> {
    /// Borrows mathematical facts without validating, paying or attesting them.
    #[must_use]
    pub const fn new(
        patch_identity: [u8; 32],
        current_encoded: Option<&'capture [u8]>,
        patch: &'capture [(u32, &'capture [u32], &'capture [u32])],
        current: &'capture [(u32, &'capture [u32], &'capture [u32])],
        merged: &'capture [(u32, &'capture [u32], &'capture [u32])],
    ) -> Self {
        Self {
            patch_identity,
            current_encoded,
            patch,
            current,
            merged,
        }
    }

    pub(in crate::policy::complete) const fn patch_identity(&self) -> [u8; 32] {
        self.patch_identity
    }

    pub(in crate::policy::complete) const fn patch_count(&self) -> usize {
        self.patch.len()
    }

    pub(in crate::policy::complete) fn bind_patch(
        &self,
        actual: [u8; 32],
        charge: &mut PolicySchemaCharge,
    ) -> Result<(), Error> {
        pay(charge, 192)?;
        if self.patch_identity != actual {
            return Err(authority());
        }
        Ok(())
    }

    pub(in crate::policy::complete) fn admit_patch(
        &self,
        bound: usize,
        charge: &mut PolicySchemaCharge,
    ) -> Result<usize, Error> {
        pay(charge, 128)?;
        pay(charge, 16)?;
        if self.patch.len() > bound {
            return Err(authority());
        }
        let mut ranks = 0_usize;
        for (index, &(id, forward, inverse)) in self.patch.iter().enumerate() {
            pay(charge, 128)?;
            pay(charge, 16)?;
            if usize::try_from(id).map_err(|_| overflow())? != index {
                return Err(authority());
            }
            pay(charge, 16)?;
            if forward.len() != inverse.len() {
                return Err(authority());
            }
            pay(charge, 16)?;
            ranks = ranks.checked_add(forward.len()).ok_or_else(overflow)?;
            if ranks > bound {
                return Err(authority());
            }
        }
        backing(self.patch.len(), ranks)
    }

    pub(in crate::policy::complete) fn admit_current(
        &self,
        bound: usize,
        charge: &mut PolicySchemaCharge,
    ) -> Result<usize, Error> {
        admit_maps(self.current, bound, charge)
    }

    pub(in crate::policy::complete) fn admit_merged(
        &self,
        bound: usize,
        charge: &mut PolicySchemaCharge,
    ) -> Result<usize, Error> {
        admit_maps(self.merged, bound, charge)
    }

    pub(in crate::policy::complete) fn bind_current_bytes(
        &self,
        actual: Option<&[u8]>,
        charge: &mut PolicySchemaCharge,
    ) -> Result<(), Error> {
        pay(charge, 128)?;
        match (self.current_encoded, actual) {
            (None, None) => Ok(()),
            (Some(expected), Some(actual)) => {
                pay(
                    charge,
                    128_usize
                        .checked_add(expected.len())
                        .and_then(|n| n.checked_add(actual.len()))
                        .ok_or_else(overflow)?,
                )?;
                if expected == actual {
                    Ok(())
                } else {
                    Err(authority())
                }
            }
            _ => Err(authority()),
        }
    }

    pub(in crate::policy::complete) fn patch_record(
        &self,
        id: u32,
        pairs: usize,
        charge: &mut PolicySchemaCharge,
    ) -> Result<(&'capture [u32], &'capture [u32]), Error> {
        pay(charge, 128)?;
        let &(actual, forward, inverse) = self
            .patch
            .get(usize::try_from(id).map_err(|_| overflow())?)
            .ok_or_else(authority)?;
        pay(charge, 16)?;
        if actual != id {
            return Err(authority());
        }
        permutation(forward, inverse, pairs, charge)
    }

    pub(in crate::policy::complete) fn current_record(
        &self,
        id: u32,
        pairs: usize,
        charge: &mut PolicySchemaCharge,
    ) -> Result<(&'capture [u32], &'capture [u32]), Error> {
        find(self.current, id, pairs, charge)
    }

    pub(in crate::policy::complete) fn merged_record(
        &self,
        id: u32,
        pairs: usize,
        charge: &mut PolicySchemaCharge,
    ) -> Result<(&'capture [u32], &'capture [u32]), Error> {
        find(self.merged, id, pairs, charge)
    }
}

fn admit_maps(
    records: &[(u32, &[u32], &[u32])],
    bound: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<usize, Error> {
    pay(charge, 128)?;
    pay(charge, 16)?;
    if records.len() > bound {
        return Err(authority());
    }
    let (mut ranks, mut previous) = (0_usize, None);
    for &(id, forward, inverse) in records {
        pay(charge, 128)?;
        pay(charge, 16)?;
        if usize::try_from(id).map_err(|_| overflow())? >= bound {
            return Err(authority());
        }
        if let Some(previous) = previous {
            pay(charge, 16)?;
            if previous >= id {
                return Err(authority());
            }
        }
        previous = Some(id);
        pay(charge, 16)?;
        if forward.len() != inverse.len() {
            return Err(authority());
        }
        pay(charge, 16)?;
        ranks = ranks.checked_add(forward.len()).ok_or_else(overflow)?;
        if ranks > bound {
            return Err(authority());
        }
    }
    backing(records.len(), ranks)
}

fn find<'capture>(
    records: &'capture [(u32, &'capture [u32], &'capture [u32])],
    id: u32,
    pairs: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<(&'capture [u32], &'capture [u32]), Error> {
    pay(charge, 128)?;
    let (mut lo, mut hi) = (0, records.len());
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        pay(charge, 16)?;
        let &(actual, forward, inverse) = records.get(mid).ok_or_else(authority)?;
        match actual.cmp(&id) {
            std::cmp::Ordering::Less => lo = mid + 1,
            std::cmp::Ordering::Greater => hi = mid,
            std::cmp::Ordering::Equal => return permutation(forward, inverse, pairs, charge),
        }
    }
    Err(authority())
}

fn permutation<'capture>(
    forward: &'capture [u32],
    inverse: &'capture [u32],
    pairs: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<(&'capture [u32], &'capture [u32]), Error> {
    pay(charge, 16)?;
    if forward.len() != pairs {
        return Err(authority());
    }
    for source in 0..pairs {
        pay(charge, 4)?;
        let rank =
            usize::try_from(*forward.get(source).ok_or_else(authority)?).map_err(|_| overflow())?;
        pay(charge, 16)?;
        if rank >= pairs {
            return Err(authority());
        }
        pay(charge, 4)?;
        if usize::try_from(*inverse.get(rank).ok_or_else(authority)?).map_err(|_| overflow())?
            != source
        {
            return Err(authority());
        }
    }
    Ok((forward, inverse))
}

fn backing(records: usize, ranks: usize) -> Result<usize, Error> {
    records
        .checked_mul(64)
        .and_then(|n| n.checked_add(ranks.checked_mul(8)?))
        .ok_or_else(overflow)
}
fn pay(charge: &mut PolicySchemaCharge, amount: usize) -> Result<(), Error> {
    charge_schema(charge, u64::try_from(amount).map_err(|_| overflow())?, 0).map_err(Error::Schema)
}
fn overflow() -> Error {
    Error::Schema(PolicySchemaError::LengthOverflow)
}
fn authority() -> Error {
    Error::Schema(PolicySchemaError::InvalidAuthority)
}
