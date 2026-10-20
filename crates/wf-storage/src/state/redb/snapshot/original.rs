//! Explicit original-policy layout and portable server profile dispatch.

pub(in crate::state::redb) const LAYOUT: u16 = 6;
pub(in crate::state::redb) const PROFILE: u32 = 1;
pub(in crate::state::redb) const CATALOG_DOMAIN: &[u8] =
    b"ferricstore-workflow-original-policy-catalog-v1\0";
