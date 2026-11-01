//! Compact sealed scalar arena; the existing ETF encoder owns wire semantics.
use super::budget::{Budget, add};
use super::{OriginalMetadataInput as I, OriginalScopeError as E};
use crate::value::{OriginalValueInput, OriginalValueNode as N};

pub(super) struct Arena {
    bytes: Vec<u8>,
    nodes: Vec<N>,
    edges: Vec<usize>,
    root: usize,
    node_limit: usize,
    edge_limit: usize,
}

impl Arena {
    pub(super) fn new(bytes: usize, rows: usize, budget: &mut Budget) -> Result<Self, E> {
        if rows > 16 {
            return Err(E::Limit);
        }
        let node_limit = rows * 7 + 4;
        let edge_limit = rows * 7 + 2;
        Ok(Self {
            bytes: budget.reserve(bytes)?,
            nodes: budget.reserve(node_limit)?,
            edges: budget.reserve(edge_limit)?,
            root: 0,
            node_limit,
            edge_limit,
        })
    }
    pub(super) fn input(&self) -> OriginalValueInput<'_> {
        OriginalValueInput {
            bytes: &self.bytes,
            nodes: &self.nodes,
            edges: &self.edges,
            root: self.root,
        }
    }
    pub(super) fn scalar(&mut self, value: I<'_, '_>, budget: &mut Budget) -> Result<usize, E> {
        budget.spend(1)?;
        let node = match value {
            I::Integer(value) => self.integer(value, budget)?,
            I::BigInteger {
                negative,
                little_endian,
            } => self.big(negative, little_endian, budget)?,
            I::Binary(bytes) => N::Binary {
                bytes: self.put(bytes, budget)?,
            },
            I::Atom(atom) => N::Atom {
                bytes: self.put(atom.as_bytes(), budget)?,
                utf8: true,
            },
            I::Boolean(value) => N::Atom {
                bytes: self.put(if value { b"true" } else { b"false" }, budget)?,
                utf8: true,
            },
            _ => return Err(E::InvalidSystemMetadata),
        };
        self.push(node)
    }
    fn integer(&mut self, value: i128, budget: &mut Budget) -> Result<N, E> {
        if let Ok(value) = i64::try_from(value) {
            return Ok(N::Integer(value));
        }
        let bytes = value.unsigned_abs().to_le_bytes();
        self.big(value < 0, &bytes, budget)
    }
    fn big(&mut self, negative: bool, bytes: &[u8], budget: &mut Budget) -> Result<N, E> {
        budget.spend(add(bytes.len(), 1)?)?;
        let length = bytes.iter().rposition(|b| *b != 0).map_or(0, |i| i + 1);
        let bytes = bytes.get(..length).ok_or(E::InvalidSystemMetadata)?;
        if length <= 8 {
            let mut raw = [0; 8];
            raw.get_mut(..length)
                .ok_or(E::InvalidSystemMetadata)?
                .copy_from_slice(bytes);
            let magnitude = u64::from_le_bytes(raw);
            let signed = if negative {
                -i128::from(magnitude)
            } else {
                i128::from(magnitude)
            };
            if let Ok(value) = i64::try_from(signed) {
                return Ok(N::Integer(value));
            }
        }
        Ok(N::BigInteger {
            negative,
            digits: self.put(bytes, budget)?,
        })
    }
    fn put(&mut self, bytes: &[u8], budget: &mut Budget) -> Result<std::ops::Range<usize>, E> {
        budget.spend(add(bytes.len(), 1)?)?;
        let start = self.bytes.len();
        let end = add(start, bytes.len())?;
        if end > self.bytes.capacity() {
            return Err(E::Limit);
        }
        self.bytes.extend_from_slice(bytes);
        Ok(start..end)
    }
    fn push(&mut self, node: N) -> Result<usize, E> {
        if self.nodes.len() == self.node_limit {
            return Err(E::Limit);
        }
        let id = self.nodes.len();
        self.nodes.push(node);
        Ok(id)
    }
    pub(super) fn tuple(&mut self, children: &[usize]) -> Result<usize, E> {
        let start = self.edges.len();
        if add(start, children.len())? > self.edge_limit {
            return Err(E::Limit);
        }
        self.edges.extend_from_slice(children);
        self.push(N::Tuple {
            children: start..self.edges.len(),
        })
    }
    pub(super) fn finish(&mut self, rows: &[usize], budget: &mut Budget) -> Result<(), E> {
        let tag = self.scalar(I::Atom("flow_system_metadata"), budget)?;
        let tail = self.push(N::EmptyList)?;
        let list = if rows.is_empty() {
            tail
        } else {
            let start = self.edges.len();
            if add(start, rows.len())? > self.edge_limit.checked_sub(2).ok_or(E::Limit)? {
                return Err(E::Limit);
            }
            self.edges.extend_from_slice(rows);
            self.push(N::List {
                heads: start..self.edges.len(),
                tail,
            })?
        };
        self.root = self.tuple(&[tag, list])?;
        Ok(())
    }
    pub(super) fn select_scope(&mut self, rows: &[usize], budget: &mut Budget) -> Result<(), E> {
        // Same-length ASCII tags can replace bytes without changing descriptors.
        let tag_start = self.bytes.len().checked_sub(20).ok_or(E::Limit)?;
        self.bytes
            .get_mut(tag_start..)
            .ok_or(E::Limit)?
            .copy_from_slice(b"flow_isolation_scope");
        budget.spend(20)?;
        let root = self.nodes.len().checked_sub(1).ok_or(E::Limit)?;
        let list = root.checked_sub(1).ok_or(E::Limit)?;
        let N::List { heads, .. } = self.nodes.get_mut(list).ok_or(E::Limit)? else {
            return Err(E::Limit);
        };
        let start = heads.start;
        heads.end = add(start, rows.len())?;
        self.edges
            .get_mut(start..heads.end)
            .ok_or(E::Limit)?
            .copy_from_slice(rows);
        Ok(())
    }
}
