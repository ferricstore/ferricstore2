//! One bounded source per call; idempotent publication must precede ACK.

use std::fmt;
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::CreditCatalogOwnerState;
use wf_ports::{
    consensus::CreditCatalogPublicationPort, log::CodecLimits, state::CreditCatalogReadLimits,
};

mod dispatch;
mod plan;

/// Immutable configured namespaces; none of these values grants routing authority.
#[derive(Clone, Copy)]
pub struct CreditCatalogPublicationRequest<'a> {
    /// Server-configured source namespaces, preserving the scheduler order.
    pub sources: &'a [RequestScope],
    /// Original nonnegative exact source cursor; selects one source modulo count.
    pub source_cursor: u64,
    /// Separately verified target namespace, in the same tenant as the source.
    pub target: &'a RequestScope,
    /// Exact target catalog identity.
    pub catalog: &'a [u8],
}

/// Positive explicit logical bounds; native engine/cache allocation is separate.
#[derive(Clone, Copy, Debug)]
pub struct CreditCatalogPublicationLimits {
    /// Complete private request/response profile.
    pub codec: CodecLimits,
    /// One coherent source guard's cumulative read/adapter limits.
    pub read: CreditCatalogReadLimits,
    /// At most256 source entries, before any deduplication.
    pub page_size: u16,
    /// Flat member/range/reference staging ceiling.
    pub maximum_members_bytes: usize,
    /// Supervised submission owner budget, held through actual completion.
    pub execution_workspace_bytes: usize,
    /// Enclosing call ceiling, including reads, staging and execution.
    pub workspace_bytes: usize,
}
impl Default for CreditCatalogPublicationLimits {
    fn default() -> Self {
        Self {
            codec: CodecLimits::default(),
            read: CreditCatalogReadLimits::default(),
            page_size: 256,
            maximum_members_bytes: 16 * 1_048_576,
            execution_workspace_bytes: 256 * 1_048_576,
            workspace_bytes: 1_024 * 1_048_576,
        }
    }
}

/// Source-compatible accounting, independent of unique membership inserts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CreditCatalogPublicationStats {
    /// Processed prefix entries, reported only after confirmed ACK.
    pub published: u16,
    /// Operational/prefix diagnoses; a write failure adds to an existing prefix error.
    pub errors: u16,
    /// Source convention: zero for empty pages, two for nonempty owner batches.
    pub read_batches: u8,
    /// Zero for missing-only prefixes, otherwise one attempted membership batch.
    pub write_batches: u8,
    /// Exactly one selected source for every valid call.
    pub sources_scanned: u8,
    /// Next modulo source index, including after operational refusal.
    pub next_source: u64,
}

/// Whole-call preflight failures, without retaining any business reply.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreditCatalogPublicationError {
    /// Invalid cursor/count/namespace or required positive profile.
    InvalidOptions,
    /// Checked aggregate ownership/workspace admission failed before effects.
    Limit,
    /// A prospectively admitted host reservation failed.
    Allocation,
}
impl fmt::Display for CreditCatalogPublicationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidOptions => "ERR invalid flow limit reconciliation options",
            Self::Limit => "catalog publication workspace limit",
            Self::Allocation => "catalog publication allocation failed",
        })
    }
}
impl std::error::Error for CreditCatalogPublicationError {}

/// Publishes one valid contiguous source prefix, then ACKs only confirmed work.
/// # Errors
/// Refuses whole-call options/resources before source reads or preparation.
pub fn publish_credit_catalog<P: CreditCatalogPublicationPort>(
    request: CreditCatalogPublicationRequest<'_>,
    port: &mut P,
    limits: CreditCatalogPublicationLimits,
) -> Result<CreditCatalogPublicationStats, CreditCatalogPublicationError> {
    let (source, next_source) = plan::select(request, limits)?;
    let mut scopes = Vec::new();
    scopes
        .try_reserve_exact(usize::from(limits.page_size))
        .map_err(|_| CreditCatalogPublicationError::Allocation)?;
    let mut stats = CreditCatalogPublicationStats {
        published: 0,
        errors: 0,
        read_batches: 0,
        write_batches: 0,
        sources_scanned: 1,
        next_source,
    };
    let page = match port.credit_catalog_page(source, limits.page_size, limits.read) {
        Ok(page) if plan::valid_page(&page, limits) => page,
        _ => {
            stats.errors = 1;
            return Ok(stats);
        }
    };
    if page.entries.is_empty() {
        return Ok(stats);
    }
    stats.read_batches = 2;
    let owners = match port.credit_catalog_owners(source, &page.entries, limits.read) {
        Ok(owners) if plan::valid_owners(&owners, page.entries.len(), limits) => owners,
        _ => {
            stats.errors = 1;
            return Ok(stats);
        }
    };
    let mut processed = 0_u16;
    let mut up_to = None;
    for (entry, state) in page.entries.iter().zip(&owners) {
        match state {
            CreditCatalogOwnerState::Corrupt => {
                stats.errors = 1;
                break;
            }
            CreditCatalogOwnerState::Missing => (),
            CreditCatalogOwnerState::Present => {
                let scope = entry.credit_scope.as_slice();
                if !scopes.contains(&scope) {
                    scopes.push(scope);
                }
            }
        }
        processed += 1;
        up_to = Some(entry.sequence);
    }
    drop(owners);
    let Some(up_to) = up_to else {
        return Ok(stats);
    };
    if !scopes.is_empty() {
        stats.write_batches = 1;
        if !dispatch::members(
            port,
            source,
            request.target,
            request.catalog,
            &scopes,
            limits,
        ) {
            stats.errors += 1;
            return Ok(stats);
        }
    }
    drop(scopes);
    let command = wf_domain::governance::catalog::CreditCatalogAckRequest {
        expected_head: page.meta.head(),
        up_to,
    };
    if dispatch::ack(port, source, command, limits) {
        stats.published = processed;
    } else {
        stats.errors += 1;
    }
    Ok(stats)
}
