//! Encounter-order raw adapter sharing the original paid deprecated selectors.

use super::super::original_retry_keys::{OriginalMapVisit, run, run_with_node_limit};
use super::{Error, EtfMap, PolicySchemaCharge, PolicySchemaLimits, compound, overflow, pay};

impl OriginalMapVisit for EtfMap<'_, '_, '_> {
    type Key = u32;
    fn len(self) -> usize {
        self.spans[self.root as usize].datum() as usize
    }
    fn key(self, previous: Option<u32>) -> u32 {
        previous.map_or(self.root + 1, |key| {
            self.spans[self.spans[key as usize].next as usize].next
        })
    }
    fn value_map(self, key: u32) -> Option<Self> {
        let value = compound::resolve(self.spans, self.spans[key as usize].next);
        if self.spans[value as usize].tag() == 116 {
            Some(Self {
                root: value,
                ..self
            })
        } else {
            None
        }
    }
    fn name_len(self, key: u32) -> Option<usize> {
        let span = self.spans[compound::resolve(self.spans, key) as usize];
        if self.is_atom(key) {
            Some(self.safety.atoms[span.datum() as usize].len())
        } else {
            self.name(key).map(<[u8]>::len)
        }
    }
    fn is_named(self, key: u32) -> bool {
        self.name_len(key).is_some()
    }
    fn is_atom(self, key: u32) -> bool {
        matches!(
            self.spans[compound::resolve(self.spans, key) as usize].tag(),
            100 | 115 | 118 | 119
        )
    }
    fn equals_ascii(self, key: u32, target: &[u8]) -> bool {
        if self.is_atom(key) {
            let span = self.spans[compound::resolve(self.spans, key) as usize];
            self.safety.atoms[span.datum() as usize].as_bytes() == target
        } else {
            self.name(key) == Some(target)
        }
    }
}

impl<'raw> EtfMap<'raw, '_, '_> {
    fn name(self, key: u32) -> Option<&'raw [u8]> {
        let span = self.spans[compound::resolve(self.spans, key) as usize];
        match span.tag() {
            109 | 77 if span.keys.is_multiple_of(8) => self
                .raw
                .get(span.canonical as usize..(span.canonical + span.keys / 8) as usize),
            _ => None,
        }
    }
}

pub(super) fn old_keys(
    map: EtfMap<'_, '_, '_>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<bool, Error> {
    run(map, limits, 2 * 263_168, charge)
}

pub(super) fn old_keys_with_node_limit(
    map: EtfMap<'_, '_, '_>,
    limits: PolicySchemaLimits,
    nodes: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<bool, Error> {
    run_with_node_limit(
        map,
        limits,
        2 * 263_168,
        u64::try_from(nodes).map_err(|_| overflow())?,
        charge,
    )
}

pub(super) fn workflow_type<'raw>(
    map: EtfMap<'raw, '_, '_>,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<&'raw [u8]>, Error> {
    pay(charge, 128)?;
    let mut previous = None;
    for _ in 0..map.len() {
        let key = map.key(previous);
        previous = Some(key);
        let span = map.spans[compound::resolve(map.spans, key) as usize];
        let width = if map.is_atom(key) {
            map.safety.atoms[span.datum() as usize].len()
        } else {
            map.name_len(key).unwrap_or(0)
        };
        pay(charge, 128 + u64::try_from(width).map_err(|_| overflow())?)?;
        if !map.is_atom(key) {
            continue;
        }
        let name = map.safety.atoms[span.datum() as usize];
        pay(
            charge,
            128 + u64::try_from(name.len()).map_err(|_| overflow())? + 4,
        )?;
        if name == "type" {
            pay(charge, 128)?;
            let value = compound::resolve(map.spans, map.spans[key as usize].next);
            return Ok(map.name(value));
        }
    }
    Ok(None)
}
