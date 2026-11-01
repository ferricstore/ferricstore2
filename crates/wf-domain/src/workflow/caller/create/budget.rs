//! Continuous prospective capture, helper and final-owner accounting.
use super::{I, OriginalCreateError as E, OriginalCreateLimits as L};
use crate::metadata::OriginalMetadataError;
use std::{borrow::Cow, mem::size_of};

pub(super) enum Failure {
    Source(Cow<'static, str>),
    Raised,
    Operational(E),
}
impl From<E> for Failure {
    fn from(e: E) -> Self {
        Self::Operational(e)
    }
}
impl From<OriginalMetadataError> for Failure {
    fn from(e: OriginalMetadataError) -> Self {
        match e {
            OriginalMetadataError::SourceReturned(s) => Self::Source(s.into()),
            OriginalMetadataError::RaisedArgumentError => Self::Raised,
            OriginalMetadataError::ReserveFailed(e) => Self::Operational(E::ReserveFailed(e)),
        }
    }
}
pub(super) fn source(s: impl Into<Cow<'static, str>>) -> Failure {
    Failure::Source(s.into())
}
pub(super) fn add(a: usize, b: usize) -> Result<usize, E> {
    a.checked_add(b).ok_or(E::Limit)
}
pub(super) fn mul(a: usize, b: usize) -> Result<usize, E> {
    a.checked_mul(b).ok_or(E::Limit)
}
pub(super) struct Budget {
    pub(super) limits: L,
    pub(super) work: usize,
    pub(super) peak: usize,
    pub(super) live: usize,
    raw: usize,
}
impl Budget {
    pub(super) fn new(limits: L) -> Result<Self, E> {
        if [
            limits.maximum_input_bytes,
            limits.maximum_output_bytes,
            limits.maximum_work_units,
            limits.maximum_temporary_bytes,
        ]
        .contains(&0)
        {
            return Err(E::InvalidLimits);
        }
        let mut b = Self {
            limits,
            work: 0,
            peak: 0,
            live: 0,
            raw: 0,
        };
        b.spend(16_384)?;
        b.admit(16_384)?;
        Ok(b)
    }
    pub(super) fn spend(&mut self, n: usize) -> Result<(), E> {
        self.work = add(self.work, n)?;
        if self.work > self.limits.maximum_work_units {
            Err(E::Limit)
        } else {
            Ok(())
        }
    }
    pub(super) fn temporary(&mut self, n: usize) -> Result<(), E> {
        if n > self.limits.maximum_temporary_bytes {
            return Err(E::Limit);
        }
        self.peak = self.peak.max(n);
        Ok(())
    }
    pub(super) fn admit(&mut self, n: usize) -> Result<(), E> {
        let next = add(self.live, n)?;
        self.temporary(next)?;
        self.live = next;
        Ok(())
    }
    pub(super) fn output(&self, n: usize) -> Result<(), E> {
        if n > self.limits.maximum_output_bytes {
            Err(E::Limit)
        } else {
            Ok(())
        }
    }
    fn raw(&mut self, n: usize) -> Result<(), E> {
        self.raw = add(self.raw, n)?;
        if self.raw > self.limits.maximum_input_bytes {
            return Err(E::Limit);
        }
        self.spend(mul(n, 512)?)
    }
    fn push<'r, 'a>(&mut self, stack: &mut Vec<I<'r, 'a>>, value: I<'r, 'a>) -> Result<(), E> {
        if stack.len() == stack.capacity() {
            let old = mul(stack.capacity(), size_of::<I<'_, '_>>())?;
            let extra = stack.capacity().max(1);
            let bytes = mul(add(stack.capacity(), extra)?, size_of::<I<'_, '_>>())?;
            self.admit(bytes)?;
            stack.try_reserve_exact(extra).map_err(E::ReserveFailed)?;
            self.live = self.live.checked_sub(old).ok_or(E::Limit)?;
        }
        stack.push(value);
        Ok(())
    }
    pub(super) fn input(&mut self, roots: &[I<'_, '_>]) -> Result<(), E> {
        let before = self.live;
        let mut stack = Vec::new();
        for &root in roots {
            self.push(&mut stack, root)?;
            while let Some(value) = stack.pop() {
                self.scan(value, &mut stack)?;
            }
        }
        drop(stack);
        self.live = before;
        Ok(())
    }
    fn scan<'r, 'a>(&mut self, value: I<'r, 'a>, stack: &mut Vec<I<'r, 'a>>) -> Result<(), E> {
        self.raw(size_of::<I<'_, '_>>())?;
        match value {
            I::Binary(b)
            | I::BigInteger {
                little_endian: b, ..
            } => self.raw(b.len())?,
            I::Atom(s) => self.raw(s.len())?,
            I::Map(rows) => {
                for &(key, value) in rows {
                    self.push(stack, key)?;
                    self.push(stack, value)?;
                }
            }
            I::List(values) | I::Tuple(values) => {
                for &v in values {
                    self.push(stack, v)?;
                }
            }
            I::ImproperList { prefix, tail } => {
                self.push(stack, *tail)?;
                for &v in prefix {
                    self.push(stack, v)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}
