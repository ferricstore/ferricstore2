//! One continuous work and cumulative requested-owner account.

#[derive(Debug)]
pub(super) struct HistoryValueCharge {
    pub(super) work_units: usize,
    pub(super) requested_bytes: usize,
}
