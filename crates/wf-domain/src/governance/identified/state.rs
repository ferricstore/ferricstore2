//! Exact sparse observations and effects; none certify their storage origin.

/// Durable membership status; absence is represented independently.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreditReservationStatus {
    /// Credit remains held in this generation.
    Active,
    /// Already released; retained to prevent another spend of the same ID.
    Released,
}

/// Actual point observation for one exact original ID in one lease generation.
#[derive(Clone, Copy)]
pub struct CreditReservationObservation<'a> {
    /// Exact requested opaque ID.
    pub id: &'a [u8],
    /// Exact queried shard.
    pub shard_id: u64,
    /// Exact queried generation.
    pub epoch: u64,
    /// Known status, or observed point absence.
    pub status: Option<CreditReservationStatus>,
}

/// One original-ID point effect, without copying its payload owner.
#[derive(Clone)]
pub struct CreditReservationChange<'a> {
    /// Actual original ID bytes.
    pub id: std::borrow::Cow<'a, [u8]>,
    /// Actual target shard.
    pub shard_id: u64,
    /// Actual target generation.
    pub epoch: u64,
    /// Replacement status, or deletion.
    pub status: Option<CreditReservationStatus>,
}

/// One ordered detached page, bounded to256 original IDs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreditPageData {
    /// Actual owner shard.
    pub shard_id: u64,
    /// Actual lease generation.
    pub epoch: u64,
    /// Positive page number, at most256.
    pub page: u16,
    /// Original insertion order, without lexicographic reordering.
    pub reservation_ids: Vec<Vec<u8>>,
}

/// One proposed page replacement or deletion.
pub enum CreditPageChange {
    /// Actual page contents.
    Put(CreditPageData),
    /// Delete one exact retired page.
    Delete {
        /// Actual owner shard.
        shard_id: u64,
        /// Actual retired generation.
        epoch: u64,
        /// Actual page number.
        page: u16,
    },
}

/// One exact remaining retired page range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CreditCleanupTask {
    /// Actual retired shard.
    pub shard_id: u64,
    /// Actual retired generation.
    pub epoch: u64,
    /// First unprocessed page.
    pub next_page: u16,
    /// Last page in the retired generation.
    pub last_page: u16,
}

/// One proposed queue entry replacement/deletion.
#[derive(Clone, Copy)]
pub struct CreditCleanupChange {
    /// Exact monotone queue sequence.
    pub sequence: u64,
    /// Replacement task, or deletion.
    pub task: Option<CreditCleanupTask>,
}

/// Exact next read; the same outer guard must establish its actual result.
#[derive(Clone, Copy)]
pub enum CreditProtocolQuery<'a> {
    /// Classify every original requested ID, preserving its order.
    Reservations {
        /// Actual selected shard.
        shard_id: u64,
        /// Actual selected generation.
        epoch: u64,
        /// Exact original IDs.
        ids: &'a [Vec<u8>],
    },
    /// Read one exact append or cleanup page.
    Page {
        /// Actual selected shard.
        shard_id: u64,
        /// Actual selected generation.
        epoch: u64,
        /// Exact selected page.
        page: u16,
    },
    /// Read an existing queue head; newly derived tasks need no invented read.
    CleanupTask {
        /// Exact existing queue sequence.
        sequence: u64,
    },
}

/// Explicit actual read result; omitted observations cannot establish absence.
pub enum CreditProtocolObservation<'a> {
    /// Complete requested point observations in query order.
    Reservations(Vec<CreditReservationObservation<'a>>),
    /// Actual exact page or observed absence.
    Page(Option<CreditPageData>),
    /// Actual exact task or observed absence.
    CleanupTask(Option<CreditCleanupTask>),
}
