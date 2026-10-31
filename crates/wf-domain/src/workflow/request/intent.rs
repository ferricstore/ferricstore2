//! Pure intent-version constructors and borrowed identity access.

use super::{ClientIntent, RequestValueError};

impl ClientIntent {
    /// Public V1 Claim digest with a separate internal funded-parent profile.
    #[must_use]
    pub const fn public_claim(digest: [u8; 32]) -> Self {
        Self {
            version: 21,
            digest,
        }
    }
    /// Atomic Claim original-caller identity, independent of generated payment inputs.
    #[must_use]
    pub const fn atomic_claim(digest: [u8; 32]) -> Self {
        Self {
            version: 20,
            digest,
        }
    }
    /// Canonical private release maintenance; supplies no remote-release authority.
    #[must_use]
    pub const fn workflow_release_maintenance(digest: [u8; 32]) -> Self {
        Self {
            version: 18,
            digest,
        }
    }
    /// Private paid Claim identity, excluding generated payment IDs and epoch.
    #[must_use]
    pub const fn governed_claim(digest: [u8; 32]) -> Self {
        Self {
            version: 19,
            digest,
        }
    }
    /// Canonical public credit caller identity, without prepared inputs or authority.
    #[must_use]
    pub const fn public_credit(digest: [u8; 32]) -> Self {
        Self {
            version: 17,
            digest,
        }
    }
    /// Canonical conditional prune identity; no verification or authority.
    #[must_use]
    pub const fn governance_catalog_prune(digest: [u8; 32]) -> Self {
        Self {
            version: 16,
            digest,
        }
    }
    /// Canonical derived catalog progress identity; no admission or authority.
    #[must_use]
    pub const fn governance_catalog_progress(digest: [u8; 32]) -> Self {
        Self {
            version: 15,
            digest,
        }
    }
    /// Canonical neutral catalog membership identity; supplies no authority.
    #[must_use]
    pub const fn governance_catalog(digest: [u8; 32]) -> Self {
        Self {
            version: 14,
            digest,
        }
    }
    /// Admits the selected canonical intent version and explicit verified digest.
    ///
    /// # Errors
    /// Rejects an unsupported canonical default/field-normalization version.
    pub const fn new(version: u16, digest: [u8; 32]) -> Result<Self, RequestValueError> {
        if version == 1 {
            Ok(Self { version, digest })
        } else {
            Err(RequestValueError::UnsupportedVersion)
        }
    }
    /// Selects the mathematical complete-core-Patch grammar for an explicit digest.
    ///
    /// Performs no verification, admission, payment or authorization and proves
    /// no committed outcome. Frame callers must verify the actual original bytes;
    /// persisted-outcome callers must validate their full framing and response.
    #[must_use]
    pub const fn complete_core_policy_patch(digest: [u8; 32]) -> Self {
        Self { version: 2, digest }
    }

    /// Selects the complete-Replace registry after same-frame digest comparison.
    #[must_use]
    pub(crate) const fn complete_policy_replace(digest: [u8; 32]) -> Self {
        Self { version: 2, digest }
    }

    /// Selects the original whole-flow registry after same-frame digest comparison.
    #[must_use]
    pub(crate) const fn original_flow_policy_replace(digest: [u8; 32]) -> Self {
        Self { version: 3, digest }
    }

    /// Selects the mathematical original public-SET grammar for an explicit digest.
    /// Constructs the original SET mathematical digest identity only.
    /// No verification, admission, payment, runtime origin or apply authority is supplied.
    #[must_use]
    pub const fn original_policy_set(digest: [u8; 32]) -> Self {
        Self { version: 4, digest }
    }

    /// Selects the prepared mutation-batch grammar; grants no apply authority.
    #[must_use]
    pub const fn record_batch(digest: [u8; 32]) -> Self {
        Self { version: 5, digest }
    }

    /// Selects ordered independent mutations; grants no apply authority.
    #[must_use]
    pub const fn independent_record_batch(digest: [u8; 32]) -> Self {
        Self { version: 6, digest }
    }

    /// Selects the private atomic-start digest identity only.
    /// No digest verification, framing, admission, payment, runtime origin or
    /// apply authority is supplied by this primitive registry selection.
    #[must_use]
    pub const fn atomic_start(digest: [u8; 32]) -> Self {
        Self { version: 7, digest }
    }

    /// Selects value-batch caller identity without verification or apply authority.
    #[must_use]
    pub const fn workflow_value_batch(digest: [u8; 32]) -> Self {
        Self { version: 8, digest }
    }

    /// Selects explicit stored-byte caller identity without apply authority.
    #[must_use]
    pub const fn workflow_stored_value_batch(digest: [u8; 32]) -> Self {
        Self { version: 9, digest }
    }

    /// Selects the private singleton Step identity without verification/authority.
    /// The explicit codec must derive and verify the actual represented fields;
    /// this mathematical registry selection supplies no admission or submission.
    #[must_use]
    pub const fn step_continue(digest: [u8; 32]) -> Self {
        Self {
            version: 10,
            digest,
        }
    }

    /// Selects stored-payload Step identity without verification or authority.
    #[must_use]
    pub const fn stored_step_continue(digest: [u8; 32]) -> Self {
        Self {
            version: 11,
            digest,
        }
    }

    /// Selects private identified-credit identity without verification/authority.
    /// The explicit codec must bind the complete actual frame before apply.
    #[must_use]
    pub const fn governance_credit(digest: [u8; 32]) -> Self {
        Self {
            version: 12,
            digest,
        }
    }

    /// Selects private catalog-ACK identity without verification or authority.
    /// Only its explicit codec may bind/admit the complete represented frame.
    #[must_use]
    pub const fn credit_catalog_ack(digest: [u8; 32]) -> Self {
        Self {
            version: 13,
            digest,
        }
    }

    /// Reads canonical intent normalization version.
    #[must_use]
    pub const fn version(self) -> u16 {
        self.version
    }
    /// Borrows stable client intent, independently of prepared identity.
    #[must_use]
    pub const fn digest(&self) -> &[u8; 32] {
        &self.digest
    }
}
