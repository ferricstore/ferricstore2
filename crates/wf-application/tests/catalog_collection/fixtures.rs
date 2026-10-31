use super::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use wf_ports::state::{GovernanceCatalogPage, GovernanceCatalogRead, GovernanceCatalogReadPort};
use wf_types::identity::{GroupId, RecoveryId, TenantId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CatalogFault {
    None,
    DeleteAnchor,
    InsertBeforeAnchor,
    Unavailable,
    RepeatedPage,
    ShortFull,
    Unsorted,
}
pub(super) struct CatalogFixture {
    pub(super) scope: RequestScope,
    rows: RefCell<Vec<Vec<u8>>>,
    pub(super) calls: RefCell<Vec<usize>>,
    pub(super) fault: Cell<CatalogFault>,
}
impl CatalogFixture {
    pub(super) fn new(count: u32) -> TestResult<Self> {
        Ok(Self {
            scope: RequestScope::new(
                TenantId::new(b"t".to_vec())?,
                GroupId::new(b"g".to_vec())?,
                RecoveryId::new(b"r".to_vec())?,
            ),
            rows: RefCell::new((0..count).map(|n| n.to_be_bytes().to_vec()).collect()),
            calls: RefCell::new(Vec::new()),
            fault: Cell::new(CatalogFault::None),
        })
    }
    pub(super) fn request(&self, limit: usize) -> Request<'_> {
        Request {
            scope: &self.scope,
            catalog: b"limit",
            limit,
        }
    }
    pub(super) fn load(key: &[u8], _: ReadLimits) -> Result<Option<Loaded<Vec<u8>>>, S> {
        Ok(Some(Loaded {
            record: key.to_vec(),
            owned_bytes: std::mem::size_of::<Vec<u8>>() + key.len(),
        }))
    }
}
impl GovernanceCatalogReadPort for CatalogFixture {
    fn governance_catalog_page(
        &self,
        request: GovernanceCatalogRead<'_>,
        _: ReadLimits,
    ) -> Result<GovernanceCatalogPage, S> {
        self.calls.borrow_mut().push(usize::from(request.limit));
        if self.fault.get() == CatalogFault::Unavailable {
            return Err(S::Unavailable);
        }
        let mut rows = self.rows.borrow_mut();
        if let Some(cursor) = request.cursor {
            if self.fault.get() == CatalogFault::DeleteAnchor {
                rows.retain(|key| key.as_slice() != cursor);
            }
            if self.fault.get() == CatalogFault::InsertBeforeAnchor {
                rows.insert(0, Vec::new());
                self.fault.set(CatalogFault::None);
            }
        }
        let start = match request.cursor {
            Some(cursor) if self.fault.get() != CatalogFault::RepeatedPage => {
                rows.iter()
                    .position(|key| key.as_slice() == cursor)
                    .ok_or(S::CatalogChanged)?
                    + 1
            }
            _ => 0,
        };
        let mut members = rows
            .iter()
            .skip(start)
            .take(usize::from(request.limit))
            .cloned()
            .collect::<Vec<_>>();
        if self.fault.get() == CatalogFault::Unsorted && members.len() >= 2 {
            members.swap(0, 1);
        }
        let full = members.len() == usize::from(request.limit)
            || self.fault.get() == CatalogFault::ShortFull;
        Ok(GovernanceCatalogPage { members, full })
    }
    fn governance_catalog_contains(
        &self,
        _: &RequestScope,
        _: &[u8],
        member: &[u8],
        _: ReadLimits,
    ) -> Result<bool, S> {
        Ok(self.rows.borrow().iter().any(|key| key == member))
    }
}

pub(super) struct TrackedRecord {
    pub(super) key: Vec<u8>,
    live: Rc<Cell<usize>>,
}
impl TrackedRecord {
    pub(super) fn load(key: &[u8], live: &Rc<Cell<usize>>, peak: &Cell<usize>) -> Loaded<Self> {
        live.set(live.get() + 1);
        peak.set(peak.get().max(live.get()));
        Loaded {
            record: Self {
                key: key.to_vec(),
                live: Rc::clone(live),
            },
            owned_bytes: std::mem::size_of::<Self>() + key.len(),
        }
    }
}
impl Drop for TrackedRecord {
    fn drop(&mut self) {
        self.live.set(self.live.get() - 1);
    }
}
